// Magic Formula — vstupní bod hry.
// Tady se sestaví Bevy App, zapnou pluginy a zaregistrují systémy.

use bevy::prelude::*;          // základ Bevy: App, Commands, Transform, Component...
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor}; // opakující se textura zem
use bevy::render::render_asset::RenderAssetUsages; // mesh zůstane i v main světě (ať ho updatujeme)
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat}; // tvorba textury země
use bevy_rapier3d::prelude::*; // fyzika: RapierPhysicsPlugin, RigidBody, Collider...

mod physics; // načte modul z src/physics.rs (fyzikální komponenty)
mod veh; // načítání .veh formátu (Fáze 2 — node-beam soft-body)
mod solver; // jádro fyziky: SoftBody, node-beam solver, tire model
mod vehicle; // auto: zavěšení, drivetrain, buildery, ovládání
mod parts; // katalog dílů z JSON (assets/parts) — build systém
mod saves; // uložené buildy do player data (player/builds)
mod stats; // odvozené KPI buildu (výkon/hmotnost/rozložení/0–100) — čisté funkce
mod world; // terén jako trait (rovina / kopce / časem OSM heightfield)
mod ui; // obrazovky: title / garáž / HUD + pauza (stavový automat)
mod audio; // procedurální zvuk motoru (reaguje na rpm/válce/config)
mod thermal; // termální model brzd/pneu (teploty → fade + grip okno)
mod effects; // vizuální efekty pneu (skid marks + kouř)
mod keybinds; // přemapovatelné ovládání (Action → klávesa, defaulty = dnešní natvrdo)

use solver::*;
use vehicle::*;
use world::{MapKind, Terrain};

fn main() {
    let mut app = App::new();
    app
        // DefaultPlugins = okno, renderer, input, čas... všechno základní.
        // .set(WindowPlugin {...}) jen přenastaví okno (titulek, rozlišení).
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Magic Formula".into(),
                resolution: (1280.0, 720.0).into(),
                ..default() // zbytek nastavení necháme výchozí
            }),
            ..default()
        }))
        // Zapne fyzikální simulaci (gravitace, srážky). Od teď rapier hýbe tělesy.
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        // Startup = spustí se jednou na začátku (postaví scénu).
        .init_resource::<SoftBody>()
        .init_resource::<Loadout>()
        .init_resource::<DrivetrainState>()
        .init_resource::<thermal::Ambient>()
        .init_resource::<DriverAids>()
        .init_resource::<FuelState>()
        .init_resource::<TestDrive>()
        .init_resource::<ChassisParams>()
        .insert_resource(parts::load_part_catalog())
        .init_resource::<Terrain>()
        .init_resource::<WorldColliders>()
        .add_event::<CrashImpact>()
        .init_resource::<Telemetry>()
        .add_plugins(ui::UiPlugin)
        .add_plugins(audio::EngineSoundPlugin)
        .add_plugins(effects::TireFxPlugin)
        .add_plugins(keybinds::plugin)
        .add_systems(Startup, setup)
        // Plná nádrž při každém vjezdu do hry (garáž → jízda). ponytail: respawn klávesou 1
        // uprostřed jízdy nádrž neresetuje — okrajové, netřeba.
        .add_systems(OnEnter(ui::AppState::Game), |mut fuel: ResMut<FuelState>| fuel.kg = fuel.capacity_kg)
        // FixedUpdate (60 Hz) = náš solver; uvnitř si dělá vlastní substepping.
        // Běží jen ve hře a mimo pauzu — na title/garáži/pauze svět stojí.
        .add_systems(FixedUpdate, softbody_solve.run_if(ui::in_game_unpaused))
        // Update = spustí se každý snímek (reaguje na vstup, zapisuje pozice do meshe).
        // Pořadí: nejdřív řazení + výpočet RPM, pak pohon (čte RPM), pak zbytek.
        // Render sync (softbody_to_transforms, update_softbody_mesh) běží vždy —
        // auto musí být vidět i pod pauzou.
        .add_systems(
            Update,
            (
                (shift_gears, update_drivetrain, vehicle_drive, thermal::wheel_thermal, vehicle_steer, telemetry_log, camera_control, spawn_shortcuts)
                    .chain()
                    .run_if(ui::in_game_unpaused),
                (softbody_to_transforms, update_softbody_mesh).chain(),
            )
                .chain(),
        );
    app.run();
}

// Postaví scénu: kamera, světlo, zem a auto. Běží jednou při startu.
// Commands = fronta příkazů na vytváření/úpravu entit.
// ResMut<Assets<...>> = zapisovatelný přístup do úložiště meshů a materiálů.
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    terrain: Res<Terrain>,
    mut colliders: ResMut<WorldColliders>,
) {
    // Kamera ve výšce 8, kus dozadu (z=15), kouká do středu světa (0,0,0).
    commands.spawn(Camera3d::default()).insert(Transform::from_xyz(0.0, 8.0, 15.0).looking_at(Vec3::ZERO, Vec3::Y));

    // Směrové světlo (jako slunce) — svítí pod úhlem daným rotací.
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,   // jak silně svítí
            shadows_enabled: true,  // vrhá stíny
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.0, -0.5, 0.0)),
    ));

    // Zem + (na testovací ploše) testovací dráha. Default mapa = TestPad (viz Terrain::default).
    spawn_ground(&mut commands, &mut meshes, &mut materials, &mut images, terrain.0.as_ref());
    build_test_track(&mut commands, &mut meshes, &mut materials, &mut colliders);

    // Auto se na startu NEspawnuje — vybere se v garáži / titulní obrazovce.
}

// Zem: mesh terénu (mřížka vzorkuje Terrain) s mřížkovou texturou. FYZIKA jede z Terrain
// přímo (height/normal v solveru) a je nekonečná — mesh je jen render. Marker GroundMesh:
// picker ho najde a přestaví. Sdílí setup i map_buttons, ať je konfigurace na jednom místě.
fn spawn_ground(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    world: &dyn world::World,
) {
    let ground_size = 4000.0;
    let mesh_step = 8.0; // hustota mřížky meshe (m) — kopce (λ 80 m) vzorkuje ~10× na vlnu
    let cell = 5.0; // velikost jedné buňky mřížkové textury v metrech
    let ground_mat = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(grid_texture())),
        // Dlaždice se opakuje (ground_size/cell)× přes celý mesh → mřížka po 5 m.
        uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(ground_size / cell)),
        perceptual_roughness: 0.95,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(world::terrain_mesh(world, ground_size, mesh_step))),
        MeshMaterial3d(ground_mat),
        GroundMesh,
        physics::ground_physics(),
    ));
}

// Testovací dráha na rovině: SKIDPAD (kruh značek známého poloměru — jezdíš po něm
// ustálenou zatáčku a zvyšuješ rychlost, sbírá se boční grip) + ROVINKA (rozjezd/brzdění
// = podélný grip). Značky jsou jen vizuál (fyzika = plochá zem); poloměr 20 m je „pravda"
// pro binování dat. Volá se jen na TestPad — proto GROUND_TOP stačí místo world.height.
fn build_test_track(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, colliders: &mut WorldColliders) {
    // Smashable zeď (crash target) — 12 m široká, 3 m vysoká, 1 m tlustá, 30 m severně od spawnu.
    // Sloupky skidpadu zůstávají ghost schválně (jezdí se mezi nimi); tvrdý hitbox má jen zeď.
    let wall_half = Vec3::new(6.0, 1.5, 0.5);
    let wall_center = Vec3::new(0.0, GROUND_TOP + wall_half.y, -30.0);
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(wall_half.x * 2.0, wall_half.y * 2.0, wall_half.z * 2.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.55, 0.55, 0.58))),
        Transform::from_translation(wall_center),
        TrackProp,
    ));
    colliders.0.push(solver::Collider::Box { min: wall_center - wall_half, max: wall_center + wall_half });

    // Pár barelů (válcové hitboxy) před zdí — varied crash target, mimo skidpad (R=20 kolem 0).
    let barrel_r = 0.5;
    let barrel_h = 1.2;
    let barrel = meshes.add(Cylinder::new(barrel_r, barrel_h));
    let barrel_mat = materials.add(Color::srgb(0.75, 0.35, 0.1));
    for bx in [-3.0f32, 0.0, 3.0] {
        let c = Vec3::new(bx, GROUND_TOP + barrel_h * 0.5, -24.0);
        commands.spawn((Mesh3d(barrel.clone()), MeshMaterial3d(barrel_mat.clone()), Transform::from_translation(c), TrackProp));
        colliders.0.push(solver::Collider::Cylinder { x: c.x, z: c.z, r: barrel_r, y0: GROUND_TOP, y1: GROUND_TOP + barrel_h });
    }

    let post = meshes.add(Cylinder::new(0.2, 0.6));
    let orange = materials.add(Color::srgb(0.95, 0.45, 0.05));
    let y = GROUND_TOP + 0.3; // značka stojí na zemi (půlka výšky nad povrchem)
    let post_at = |commands: &mut Commands, x: f32, z: f32| {
        commands.spawn((
            Mesh3d(post.clone()),
            MeshMaterial3d(orange.clone()),
            Transform::from_xyz(x, y, z),
            TrackProp,
        ));
    };
    // Skidpad: kruh R=20 m okolo spawnu (auto startuje uprostřed → vyjeď a krouži).
    const SKIDPAD_R: f32 = 20.0;
    let n = 24;
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        post_at(commands, SKIDPAD_R * a.cos(), SKIDPAD_R * a.sin());
    }
    // Rovinka: pruh značek 40 m východně od skidpadu (ať se nekříží), lane šířky 4 m,
    // délka 160 m (rozjezd + brzdná zóna). Přejeď k ní pro podélné testy.
    let lane_x = 40.0;
    let mut z = -80.0;
    while z <= 80.0 {
        post_at(commands, lane_x - 2.0, z);
        post_at(commands, lane_x + 2.0, z);
        z += 10.0;
    }
}

// Vyrobí dlaždici mřížky: zelená výplň + světlejší čára na dvou okrajích. Sampler je
// nastaven na opakování (Repeat), takže se přes celý plane vykreslí souvislá mřížka.
fn grid_texture() -> Image {
    let size = 64u32; // rozlišení dlaždice v px
    let line = 2u32;  // tloušťka čáry v px
    let fill = [70u8, 100, 70, 255];   // tráva
    let lines = [120u8, 150, 120, 255]; // čára mřížky
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            // čára na levém a horním okraji dlaždice → po složení vznikne souvislá mříž
            let on_line = x < line || y < line;
            data.extend_from_slice(if on_line { &lines } else { &fill });
        }
    }
    let mut img = Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Bez Repeat by se textura jen roztáhla; takhle se dlaždí donekonečna.
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..default()
    });
    img
}

// Render mesh terénu — podle markeru ho picker najde, smaže a postaví z nové mapy.
#[derive(Component)]
struct GroundMesh;

// Značka testovací dráhy (skidpad, rovinka) — žije jen na testovací ploše, picker ji
// smaže při přepnutí na jinou mapu. Zvlášť od `Spawned` (auta), ať je Backspace nemaže.
#[derive(Component)]
struct TrackProp;

// Seznam dostupných .veh souborů ve složce assets (seřazený) — UI i klávesy berou stejný.
fn available_vehs() -> Vec<std::path::PathBuf> {
    let mut vehs = Vec::new();
    if let Ok(dir) = std::fs::read_dir("assets") {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|x| x == "veh") {
                vehs.push(path);
            }
        }
    }
    vehs.sort();
    vehs
}

// Přepne mapu za běhu: nový Terrain + rebuild render meshe země + testovací dráha
// (jen na TestPad). Auta zůstanou — solver čte height/normal z Terrain živě.
// Volá UI (map picker na title screenu).
fn rebuild_map(
    kind: MapKind,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    terrain: &mut Terrain,
    colliders: &mut WorldColliders,
    ground: impl IntoIterator<Item = Entity>,
    track: impl IntoIterator<Item = Entity>,
) {
    *terrain = Terrain(kind.build());
    for e in ground {
        commands.entity(e).despawn_recursive(); // starý mesh + collider pryč
    }
    spawn_ground(commands, meshes, materials, images, terrain.0.as_ref());
    for e in track {
        commands.entity(e).despawn_recursive(); // dráha jen na TestPad
    }
    colliders.0.clear(); // hitboxy dráhy pryč (zeď); TestPad je znovu naplní
    if kind == MapKind::TestPad {
        build_test_track(commands, meshes, materials, colliders);
    }
}

// Smaže všechny naspawnované entity (i s dětmi: mesh kol, joint pružin) a vyčistí solver.
fn clear_spawned(commands: &mut Commands, spawned: &Query<Entity, With<Spawned>>, softbody: &mut SoftBody) {
    for entity in spawned {
        commands.entity(entity).despawn_recursive();
    }
    *softbody = SoftBody::default(); // ať po smazání nezůstanou viset nody bez entit
}

// Klávesové zkratky: 1 = testovací vůz, 2+ = .veh soubory z assets (pokud existují).
fn spawn_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    spawned: Query<Entity, With<Spawned>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut softbody: ResMut<SoftBody>,
    mut loadout: ResMut<Loadout>,
    mut state: ResMut<DrivetrainState>,
    chassis: Res<ChassisParams>,
    terrain: Res<Terrain>,
) {
    // Clear = Backspace (Shift+C nešlo: levý Shift je spojka, C kamera — kolize).
    if keys.just_pressed(KeyCode::Backspace) {
        clear_spawned(&mut commands, &spawned, &mut softbody);
    }
    let ground_y = terrain.0.height(0.0, 0.0);
    if keys.just_pressed(KeyCode::Digit1) {
        spawn_solver_car(&mut commands, &mut meshes, &mut materials, &mut softbody, &mut loadout, &mut state, &chassis, ground_y);
    }
    // Klávesy 2,3,4… = .veh soubory ve stejném pořadí jako UI tlačítka.
    let digits = [KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5];
    if digits.iter().any(|d| keys.just_pressed(*d)) {
        for (i, path) in available_vehs().iter().enumerate() {
            if i < digits.len() && keys.just_pressed(digits[i]) {
                spawn_softbody(&mut commands, &mut meshes, &mut materials, &mut softbody, &mut loadout, &mut state, &chassis, &path.to_string_lossy(), ground_y);
            }
        }
    }
}

// Každý snímek přepíše vrcholy povrchu na aktuální pozice nodů → mesh se deformuje s soft-body.
fn update_softbody_mesh(
    surfaces: Query<(&SoftbodyMesh, &Mesh3d)>,
    transforms: Query<&Transform>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (surface, mesh3d) in &surfaces {
        let Some(mesh) = meshes.get_mut(&mesh3d.0) else {
            continue;
        };
        let positions: Vec<[f32; 3]> = surface
            .nodes
            .iter()
            .map(|&e| {
                let p = transforms.get(e).map(|t| t.translation).unwrap_or(Vec3::ZERO);
                [p.x, p.y, p.z]
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.compute_normals();
    }
}

// Zápis stavu solveru → Transformy render entit (kuličky + disky kol). Mesh už pak
// deformuje update_softbody_mesh, který čte tyhle Transformy. Běží v Update (po solveru).
fn softbody_to_transforms(
    sb: Res<SoftBody>,
    mut transforms: Query<&mut Transform>,
    car_body: Query<Entity, With<CarBody>>,
) {
    // Nody → kuličky.
    for (i, &entity) in sb.entities.iter().enumerate() {
        if let Ok(mut tf) = transforms.get_mut(entity) {
            tf.translation = sb.pos[i];
        }
    }

    // Orientace auta z chassis rámu (X=right, Y=up, Z=forward). Identita, když rám není (.veh).
    let (car_rot, center) = match &sb.chassis {
        Some(cf) => {
            let mean = |idx: &[usize]| idx.iter().map(|&i| sb.pos[i]).sum::<Vec3>() / idx.len() as f32;
            let fwd = (mean(&cf.front) - mean(&cf.rear)).normalize_or_zero();
            let r0 = (mean(&cf.right) - mean(&cf.left)).normalize_or_zero();
            let up = fwd.cross(r0).normalize_or_zero();
            let right = up.cross(fwd).normalize_or_zero(); // re-ortogonalizace
            let center = (mean(&cf.front) + mean(&cf.rear)) * 0.5;
            if up == Vec3::ZERO {
                (Quat::IDENTITY, center) // zdegenerovaný rám
            } else {
                (Quat::from_mat3(&Mat3::from_cols(right, up, fwd)), center)
            }
        }
        None => (Quat::IDENTITY, Vec3::ZERO),
    };

    // Trup → kvádr: sedí na těžišti rámu, natočený s autem.
    for entity in &car_body {
        if let Ok(mut tf) = transforms.get_mut(entity) {
            tf.translation = center;
            tf.rotation = car_rot;
        }
    }

    // Kola → disky: pozice = hub, rotace = orientace kola · řízení(Y) · valení(X) · base(válec Y→osa X).
    let base = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    for wheel in &sb.wheels {
        if let Ok(mut tf) = transforms.get_mut(wheel.render) {
            tf.translation = sb.pos[wheel.hub];
            // S těhlicí (top nod) je „up" kola skutečný vektor hub→top → disk se naklání
            // s těhlicí = camber. Forward se bere z rámu auta a re-ortogonalizuje. Bez
            // těhlice kolo vzpřímené s rámem (Rigid/Coil/McPherson — hub je bod).
            let wheel_rot = match wheel.top {
                Some(t) => {
                    let up = (sb.pos[t] - sb.pos[wheel.hub]).normalize_or_zero();
                    let right = up.cross(car_rot * Vec3::Z).normalize_or_zero();
                    if right == Vec3::ZERO {
                        car_rot // zdegenerované (top≈hub nebo up ∥ forward)
                    } else {
                        Quat::from_mat3(&Mat3::from_cols(right, up, right.cross(up)))
                    }
                }
                None => car_rot,
            };
            tf.rotation =
                wheel_rot * Quat::from_rotation_y(wheel.steer) * Quat::from_rotation_x(wheel.spin) * base;
        }
    }
}

// Telemetrie: log per-kolo dat kontaktu do CSV pro ladění Pacejky. Klávesa T přepíná
// záznam. Nový soubor telemetry_<epoch>.csv v cwd; drží se writer + čas od startu.
#[derive(Resource, Default)]
struct Telemetry {
    writer: Option<std::io::BufWriter<std::fs::File>>,
    t: f32,
    loadout_id: String, // signatura buildu (auto_name) — konstantní per jízda, diff buildů
}

fn telemetry_log(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<keybinds::KeyBindings>,
    time: Res<Time>,
    sb: Res<SoftBody>,
    state: Res<DrivetrainState>,
    loadout: Res<Loadout>,
    mut tel: ResMut<Telemetry>,
) {
    use std::io::Write;
    if bindings.just_pressed(&keys, keybinds::Action::Telemetry) {
        if tel.writer.is_some() {
            tel.writer = None; // Drop = flush + zavření souboru
            info!("Telemetrie: záznam zastaven");
        } else {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = std::fs::create_dir_all("telemetry"); // složka v projektu (idempotentní)
            let path = format!("telemetry/telemetry_{stamp}.csv");
            match std::fs::File::create(&path) {
                Ok(f) => {
                    let mut w = std::io::BufWriter::new(f);
                    let _ = writeln!(w, "loadout_id,t,wheel,front,speed_kmh,fz,slip_ratio,slip_angle_deg,f_long,f_lat,brake_c,tire_surf_c,tire_core_c,engine_c,wear");
                    tel.writer = Some(w);
                    tel.t = 0.0;
                    // signatura buildu (bez čárek, ať nerozbije CSV)
                    tel.loadout_id = saves::SavedBuild::auto_name(&loadout).replace(',', " ");
                    info!("Telemetrie: záznam do {path} (build: {})", tel.loadout_id);
                }
                Err(e) => warn!("Telemetrie: nelze otevřít soubor ({e})"),
            }
        }
    }
    if tel.writer.is_none() {
        return;
    }
    tel.t += time.delta_secs();
    let (t, speed, eng) = (tel.t, state.speed_kmh, state.engine_c);
    // Kola nakopírovat před půjčením writeru (ResMut vs Res<SoftBody> jsou nezávislé).
    let rows: Vec<_> = sb.wheels.iter().enumerate()
        .map(|(i, w)| (i, w.front, w.telem, w.thermal.brake_c, w.thermal.tire_surf_c, w.thermal.tire_core_c, w.thermal.wear)).collect();
    let id = tel.loadout_id.clone();
    let w = tel.writer.as_mut().unwrap();
    for (i, front, tm, brake_c, surf_c, core_c, wear) in rows {
        let _ = writeln!(w, "{id},{t:.3},{i},{},{speed:.2},{:.1},{:.4},{:.3},{:.1},{:.1},{:.1},{:.1},{:.1},{eng:.1},{:.3}",
            front as u8, tm.fz, tm.slip_ratio, tm.slip_angle.to_degrees(), tm.f_long, tm.f_lat, brake_c, surf_c, core_c, wear);
    }
}

// Kamera: dva módy přepínané klávesou C — chase (za autem) a FPV (z kokpitu).
// Stav auta čteme přímo ze SoftBody (centroid + směr z rohových nodů), auto nemá Transform.
// `fpv` je Local<bool> — drží si ho systém sám mezi snímky (žádný extra resource netřeba).
fn camera_control(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<keybinds::KeyBindings>,
    sb: Res<SoftBody>,
    mut fpv: Local<bool>,
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera3d>>,
) {
    if bindings.just_pressed(&keys, keybinds::Action::Camera) {
        *fpv = !*fpv; // překlop mód
        // FPV širší záběr (periferie jako v kokpitu), chase zpět na default 45°.
        if let Ok((_, mut proj)) = camera.get_single_mut() {
            if let Projection::Perspective(p) = proj.as_mut() {
                p.fov = if *fpv { 75.0_f32.to_radians() } else { 45.0_f32.to_radians() };
            }
        }
    }

    // Bez auta (žádný chassis rám) kameru nehýbeme.
    let Some(cf) = &sb.chassis else { return };
    let Ok((mut cam, _)) = camera.get_single_mut() else { return };
    let mean = |idx: &[usize]| idx.iter().map(|&i| sb.pos[i]).sum::<Vec3>() / idx.len() as f32;

    let center = (mean(&cf.front) + mean(&cf.rear)) * 0.5; // těžiště rámu
    let fwd = (mean(&cf.front) - mean(&cf.rear)).normalize_or_zero();
    if fwd == Vec3::ZERO {
        return; // zdegenerovaný rám → směr neznámý
    }

    if *fpv {
        // FPV: oko kousek nad a před středem, koukáme po směru jízdy.
        let eye = center + Vec3::Y * 0.5 + fwd * 0.3;
        cam.translation = eye;
        cam.look_at(eye + fwd, Vec3::Y);
    } else {
        // Chase: za auto (jen yaw, ať kamera nepodlétne zem při náklonu) a nahoru.
        let mut heading = fwd;
        heading.y = 0.0;
        let heading = heading.normalize_or_zero();
        cam.translation = center + (-heading * 12.0 + Vec3::Y * 5.0);
        cam.look_at(center, Vec3::Y);
    }
}
