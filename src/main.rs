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

use solver::*;
use vehicle::*;

fn main() {
    App::new()
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
        .init_resource::<Drivetrain>()
        .init_resource::<CarBuild>()
        .add_systems(Startup, setup)
        // FixedUpdate (60 Hz) = náš solver; uvnitř si dělá vlastní substepping.
        .add_systems(FixedUpdate, softbody_solve)
        // Update = spustí se každý snímek (reaguje na vstup, zapisuje pozice do meshe).
        // Pořadí: nejdřív řazení + výpočet RPM, pak pohon (čte RPM), pak zbytek.
        .add_systems(Update, (shift_gears, update_drivetrain, vehicle_drive, vehicle_steer, update_hud, camera_control, spawn_buttons, spawn_shortcuts, toggle_menu, highlight_build_buttons, softbody_to_transforms, update_softbody_mesh).chain())
        .run();
}

// Postaví scénu: kamera, světlo, zem a auto. Běží jednou při startu.
// Commands = fronta příkazů na vytváření/úpravu entit.
// ResMut<Assets<...>> = zapisovatelný přístup do úložiště meshů a materiálů.
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
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

    // Zem: obří plochý plane s mřížkovou texturou (záchytné body na vnímání rychlosti).
    // Fyzika je stejně nekonečná — solver kontaktuje rovinu y = GROUND_TOP bez hranic.
    let ground_size = 5000.0;
    let cell = 5.0; // velikost jedné buňky mřížky v metrech
    let ground_mat = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(grid_texture())),
        // Dlaždice se opakuje (ground_size/cell)× přes celý plane → mřížka po 5 m.
        uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(ground_size / cell)),
        perceptual_roughness: 0.95,
        ..default()
    });
    commands
        .spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(ground_size, ground_size))), MeshMaterial3d(ground_mat)))
        .insert(physics::ground_physics());

    // Auto se na startu NEspawnuje — vybereš ho v UI (tlačítka) nebo klávesou 1/2/3.
    spawn_ui(&mut commands);
    spawn_hud(&mut commands);
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

// HUD vpravo dole: rychlost / kvalt / otáčky. Text přepisuje update_hud každý snímek.
fn spawn_hud(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(16.0),
                right: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("—"),
                TextFont { font_size: 26.0, ..default() },
                TextColor(Color::srgb(0.9, 0.95, 1.0)),
                Node { margin: UiRect::all(Val::Px(10.0)), ..default() },
                HudText,
            ));
        });
}

// Co dané tlačítko / klávesa udělá.
#[derive(Component, Clone)]
enum SpawnAction {
    RigidCar,                     // tuhá node-beam konfigurace (kvádr + 4 kola), NE rapier rigid-body
    SoftBody(String),             // node-beam soft-body z .veh souboru na téhle cestě
    SetSuspension(SuspensionKind), // vybere díl zavěšení do CarBuild; přestaví stojící auto
    Clear,                        // smaže všechna naspawnovaná auta
}

// Jediné tlačítko, co otevírá/zavírá menu.
#[derive(Component)]
struct MenuToggleButton;

// Panel s volbami (rozbalovací) — přepíná se jeho zobrazení.
#[derive(Component)]
struct MenuPanel;

// Marker textu HUDu (rychlost/kvalt/otáčky) — podle něj ho update systém najde a přepíše.
#[derive(Component)]
struct HudText;

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

// UI vlevo nahoře: jedno tlačítko „Menu", které rozbalí/sbalí panel s volbami.
fn spawn_ui(commands: &mut Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|root| {
            // Toggle tlačítko — vždy viditelné.
            root.spawn((
                Button,
                Node {
                    width: Val::Px(260.0),
                    height: Val::Px(34.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.2, 0.28)),
                MenuToggleButton,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("\u{2630} Menu"),
                    TextFont { font_size: 16.0, ..default() },
                    TextColor(Color::WHITE),
                ));
            });

            // Panel s volbami — na startu OTEVŘENÝ (toggle „☰ Menu" ho sbalí/rozbalí).
            root.spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                MenuPanel,
            ))
            .with_children(|panel| {
                spawn_button(panel, "1 — Testovací vůz", SpawnAction::RigidCar);
                // Výběr dílu zavěšení (build systém): přepne CarBuild + přestaví stojící auto.
                spawn_button(panel, &format!("  Zavěšení: {}", SuspensionKind::RIGID.label()), SpawnAction::SetSuspension(SuspensionKind::RIGID));
                spawn_button(panel, &format!("  Zavěšení: {}", SuspensionKind::COIL.label()), SpawnAction::SetSuspension(SuspensionKind::COIL));
                spawn_button(panel, &format!("  Zavěšení: {}", SuspensionKind::MCPHERSON.label()), SpawnAction::SetSuspension(SuspensionKind::MCPHERSON));
                spawn_button(panel, &format!("  Zavěšení: {}", SuspensionKind::DOUBLE_WISHBONE.label()), SpawnAction::SetSuspension(SuspensionKind::DOUBLE_WISHBONE));
                for (i, path) in available_vehs().iter().enumerate() {
                    let file = path.file_name().unwrap().to_string_lossy();
                    let label = format!("{} — Soft-body: {file}", i + 2);
                    spawn_button(panel, &label, SpawnAction::SoftBody(path.to_string_lossy().into_owned()));
                }
                spawn_button(panel, "Backspace — Clear (smazat auta)", SpawnAction::Clear);
            });
        });
}

// Toggle menu: klik na „Menu" přepne zobrazení panelu.
fn toggle_menu(
    toggle: Query<&Interaction, (Changed<Interaction>, With<MenuToggleButton>)>,
    mut panel: Query<&mut Node, With<MenuPanel>>,
) {
    for interaction in &toggle {
        if *interaction == Interaction::Pressed {
            for mut node in &mut panel {
                node.display = if node.display == Display::None {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        }
    }
}

// Pomocník: jedno tlačítko s popiskem a akcí.
fn spawn_button(parent: &mut ChildBuilder, label: &str, action: SpawnAction) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(260.0),
                height: Val::Px(34.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.15, 0.15, 0.18)),
            action,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::WHITE),
            ));
        });
}

// Reakce na kliknutí tlačítka → spawne příslušné auto.
fn spawn_buttons(
    interactions: Query<(&Interaction, &SpawnAction), (Changed<Interaction>, With<Button>)>,
    spawned: Query<Entity, With<Spawned>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut softbody: ResMut<SoftBody>,
    mut build: ResMut<CarBuild>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            SpawnAction::RigidCar => spawn_solver_car(&mut commands, &mut meshes, &mut materials, &mut softbody, &build),
            SpawnAction::SoftBody(path) => spawn_softbody(&mut commands, &mut meshes, &mut materials, &mut softbody, path),
            SpawnAction::SetSuspension(kind) => {
                build.suspension = *kind;
                // Stojí-li solver auto (chassis = jen ono), přestav ho hned ať je rozdíl vidět.
                if softbody.chassis.is_some() {
                    clear_spawned(&mut commands, &spawned, &mut softbody);
                    spawn_solver_car(&mut commands, &mut meshes, &mut materials, &mut softbody, &build);
                }
            }
            SpawnAction::Clear => clear_spawned(&mut commands, &spawned, &mut softbody),
        }
    }
}

// Zvýrazní aktivní díl zavěšení v menu (selektor musí ukázat, co je vybráno).
fn highlight_build_buttons(build: Res<CarBuild>, mut buttons: Query<(&SpawnAction, &mut BackgroundColor)>) {
    for (action, mut bg) in &mut buttons {
        if let SpawnAction::SetSuspension(kind) = action {
            *bg = if *kind == build.suspension {
                BackgroundColor(Color::srgb(0.2, 0.45, 0.3)) // aktivní = zelené
            } else {
                BackgroundColor(Color::srgb(0.15, 0.15, 0.18))
            };
        }
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
    build: Res<CarBuild>,
) {
    // Clear = Backspace (Shift+C nešlo: levý Shift je spojka, C kamera — kolize).
    if keys.just_pressed(KeyCode::Backspace) {
        clear_spawned(&mut commands, &spawned, &mut softbody);
    }
    if keys.just_pressed(KeyCode::Digit1) {
        spawn_solver_car(&mut commands, &mut meshes, &mut materials, &mut softbody, &build);
    }
    // Klávesy 2,3,4… = .veh soubory ve stejném pořadí jako UI tlačítka.
    let digits = [KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5];
    if digits.iter().any(|d| keys.just_pressed(*d)) {
        for (i, path) in available_vehs().iter().enumerate() {
            if i < digits.len() && keys.just_pressed(digits[i]) {
                spawn_softbody(&mut commands, &mut meshes, &mut materials, &mut softbody, &path.to_string_lossy());
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

// Přepíše HUD text aktuálními hodnotami z Drivetrain.
fn update_hud(dt: Res<Drivetrain>, mut hud: Query<&mut Text, With<HudText>>) {
    let gear = match dt.gear {
        -1 => "R".to_string(),
        0 => "N".to_string(),
        g => g.to_string(),
    };
    let diff = if dt.rear_diff.lock > 0.5 { "ZAMČENÝ" } else { "otevřený" };
    let clutch = if dt.stalled {
        "  CHCÍPLO (Shift+R)"
    } else if dt.clutch < 0.95 {
        "  SPOJKA"
    } else {
        ""
    };
    if let Ok(mut text) = hud.get_single_mut() {
        text.0 = format!(
            "{:>3.0} km/h\nKvalt {}{clutch}\n{:>5.0} ot/min\n{}  diff: {}",
            dt.speed_kmh, gear, dt.rpm, dt.layout.label(), diff
        );
    }
}

// Kamera: dva módy přepínané klávesou C — chase (za autem) a FPV (z kokpitu).
// Stav auta čteme přímo ze SoftBody (centroid + směr z rohových nodů), auto nemá Transform.
// `fpv` je Local<bool> — drží si ho systém sám mezi snímky (žádný extra resource netřeba).
fn camera_control(
    keys: Res<ButtonInput<KeyCode>>,
    sb: Res<SoftBody>,
    mut fpv: Local<bool>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
) {
    if keys.just_pressed(KeyCode::KeyC) {
        *fpv = !*fpv; // překlop mód
    }

    // Bez auta (žádný chassis rám) kameru nehýbeme.
    let Some(cf) = &sb.chassis else { return };
    let Ok(mut cam) = camera.get_single_mut() else { return };
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
