// Magic Formula — vstupní bod hry.
// Tady se sestaví Bevy App, zapnou pluginy a zaregistrují systémy.

use bevy::prelude::*;          // základ Bevy: App, Commands, Transform, Component...
use bevy::render::mesh::Indices; // indexy trojúhelníků pro mesh
use bevy::render::render_asset::RenderAssetUsages; // mesh zůstane i v main světě (ať ho updatujeme)
use bevy::render::render_resource::PrimitiveTopology; // TriangleList
use bevy_rapier3d::prelude::*; // fyzika: RapierPhysicsPlugin, RigidBody, Collider...

mod physics; // načte modul z src/physics.rs (fyzikální komponenty)
mod veh; // načítání .veh formátu (Fáze 2 — node-beam soft-body)

// Marker komponenta (bez dat) — jen označí, která entita je tělo auta.
// Podle ní si v systému řízení najdeme auto a otáčíme s ním.
#[derive(Component)]
struct Chassis;

// Komponenta = "štítek s daty" na entitě. Tahle značí, že entita je kolo.
// Podle ní půjde kola najít a build system bude měnit jejich parametry.
// allow(dead_code): pole zatím jen zapisujeme; číst je budou systémy v dalších krocích.
#[allow(dead_code)]
#[derive(Component)]
struct Wheel {
    radius: f32,   // poloměr kola
    driven: bool,  // je poháněné motorem?
    steered: bool, // zatáčí se s ním?
}

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
        .add_systems(Startup, setup)
        // Update = spustí se každý snímek (reaguje na vstup).
        .add_systems(Update, (vehicle_drive, vehicle_steer, follow_camera, spawn_buttons, spawn_shortcuts, toggle_menu, update_softbody_mesh))
        .run();
}

// Postaví scénu: kamera, světlo, zem a auto. Běží jednou při startu.
// Commands = fronta příkazů na vytváření/úpravu entit.
// ResMut<Assets<...>> = zapisovatelný přístup do úložiště meshů a materiálů.
fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
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

    // Zem: plochý plane 50×50 + fyzika (pevná podlaha, do které věci narážejí).
    commands.spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))), MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))))).insert(physics::ground_physics());

    // Auto se na startu NEspawnuje — vybereš ho v UI (tlačítka) nebo klávesou 1/2/3.
    spawn_ui(&mut commands);
}

// Co dané tlačítko / klávesa udělá.
#[derive(Component, Clone)]
enum SpawnAction {
    RigidCar,         // jezdící kvádr + 4 kola
    SoftBody(String), // node-beam soft-body z .veh souboru na téhle cestě
    Clear,            // smaže všechna naspawnovaná auta
}

// Marker na všech naspawnovaných herních entitách (chassis, kola, nody) — podle něj Clear maže.
#[derive(Component)]
struct Spawned;

// Jediné tlačítko, co otevírá/zavírá menu.
#[derive(Component)]
struct MenuToggleButton;

// Panel s volbami (rozbalovací) — přepíná se jeho zobrazení.
#[derive(Component)]
struct MenuPanel;

// Povrchový mesh soft-body. Drží pro každý vrchol odpovídající node entitu,
// aby update systém mohl vrcholy posouvat za pohybem nodů (deformace).
#[derive(Component)]
struct SoftbodyMesh {
    nodes: Vec<Entity>, // vertex i ↔ node entita
}

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

            // Panel s volbami — na startu SKRYTÝ (Display::None), toggle ho ukáže.
            root.spawn((
                Node {
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                MenuPanel,
            ))
            .with_children(|panel| {
                spawn_button(panel, "1 — Rigid auto (jezdící)", SpawnAction::RigidCar);
                for (i, path) in available_vehs().iter().enumerate() {
                    let file = path.file_name().unwrap().to_string_lossy();
                    let label = format!("{} — Soft-body: {file}", i + 2);
                    spawn_button(panel, &label, SpawnAction::SoftBody(path.to_string_lossy().into_owned()));
                }
                spawn_button(panel, "Shift+C — Clear (smazat auta)", SpawnAction::Clear);
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
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            SpawnAction::RigidCar => spawn_rigid_car(&mut commands, &mut meshes, &mut materials),
            SpawnAction::SoftBody(path) => spawn_softbody(&mut commands, &mut meshes, &mut materials, path),
            SpawnAction::Clear => clear_spawned(&mut commands, &spawned),
        }
    }
}

// Smaže všechny naspawnované entity (i s dětmi: mesh kol, joint pružin).
fn clear_spawned(commands: &mut Commands, spawned: &Query<Entity, With<Spawned>>) {
    for entity in spawned {
        commands.entity(entity).despawn_recursive();
    }
}

// Klávesové zkratky: 1 = rigid auto, 2 = test.veh, 3 = fabia.veh (pokud existuje).
fn spawn_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    spawned: Query<Entity, With<Spawned>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Clear = Shift+C (samotné C je obsazené kamerou).
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if shift && keys.just_pressed(KeyCode::KeyC) {
        clear_spawned(&mut commands, &spawned);
    }
    if keys.just_pressed(KeyCode::Digit1) {
        spawn_rigid_car(&mut commands, &mut meshes, &mut materials);
    }
    // Klávesy 2,3,4… = .veh soubory ve stejném pořadí jako UI tlačítka.
    let digits = [KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5];
    if digits.iter().any(|d| keys.just_pressed(*d)) {
        for (i, path) in available_vehs().iter().enumerate() {
            if i < digits.len() && keys.just_pressed(digits[i]) {
                spawn_softbody(&mut commands, &mut meshes, &mut materials, &path.to_string_lossy());
            }
        }
    }
}

// Spawne RIGID jezdící auto: kvádr (chassis) + 4 kola s joints (z dřívějška).
fn spawn_rigid_car(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let car_pos = Vec3::new(0.0, 3.0, 0.0);

    let chassis = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.5, 0.5, 4.0))),
            MeshMaterial3d(materials.add(Color::srgb(0.8, 0.2, 0.2))),
            Transform::from_translation(car_pos),
        ))
        .insert(physics::car_physics())
        .insert(Chassis)
        .insert(Velocity::default())
        .insert(Spawned) // ať to umí Clear smazat
        .id();

    let wheel_radius = 0.32;
    let wheel_width = 0.2;
    let wheels = [
        ("FL", Vec3::new(-1.15, -0.3, 1.5), true, false),
        ("FR", Vec3::new(1.15, -0.3, 1.5), true, false),
        ("RL", Vec3::new(-1.15, -0.3, -1.5), false, true),
        ("RR", Vec3::new(1.15, -0.3, -1.5), false, true),
    ];
    for (name, offset, steered, driven) in wheels {
        commands
            .spawn((
                Transform::from_translation(car_pos + offset),
                Visibility::default(),
                Name::new(format!("Wheel {name}")),
            ))
            .insert(physics::wheel_physics(wheel_radius, wheel_width))
            .insert(Wheel { radius: wheel_radius, driven, steered })
            .insert(physics::Suspension::Rigid)
            .insert(Spawned)
            .insert(physics::wheel_joint(physics::Suspension::Rigid, chassis, offset, steered))
            .with_children(|builder| {
                builder.spawn((
                    Mesh3d(meshes.add(Cylinder::new(wheel_radius, wheel_width))),
                    MeshMaterial3d(materials.add(Color::srgb(0.1, 0.1, 0.1))),
                    Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                ));
            });
    }
}

// BeamNG používá z = nahoru, Bevy y = nahoru. Přehodíme osy, ať auto stojí správně
// (a zachováme pravotočivost: dopředné +y BeamNG → -z Bevy).
fn veh_pos(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1])
}

// Načte .veh a postaví soft-body: nody = malé koule (tělesa), beamy = pružiny (SpringJoint).
// Víc beamů na jeden nod → joint dáme na DÍTĚ nodu (bevy_rapier druhý konec najde přes rodiče).
fn spawn_softbody(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    path: &str,
) {
    let raw = std::fs::read_to_string(path).expect("nelze číst .veh");
    let vehicle: veh::Vehicle = serde_json::from_str(&raw).expect("nevalidní .veh");
    info!("Načteno vozidlo z .veh: {} nodů, {} beamů", vehicle.nodes.len(), vehicle.beams.len());

    let origin = Vec3::new(0.0, 4.0, 6.0); // PŘED auto (+Z) a ve vzduchu, ať to chase kamera vidí

    // Sdílený mesh + materiál pro všechny nody (1067× nepřidávat nový asset = výkon).
    let node_mesh = meshes.add(Sphere::new(0.04));
    let node_material = materials.add(Color::srgb(0.2, 0.6, 1.0));

    // Mapy pro propojení beamů: id → Entity a id → pozice (na klidovou délku pružiny).
    let mut node_entity: std::collections::HashMap<&str, Entity> = std::collections::HashMap::new();
    let positions: std::collections::HashMap<&str, Vec3> =
        vehicle.nodes.iter().map(|n| (n.id.as_str(), veh_pos(n.pos))).collect();

    // Nody → malé koule jako dynamická tělesa.
    for node in &vehicle.nodes {
        let entity = commands
            .spawn((
                Mesh3d(node_mesh.clone()),
                MeshMaterial3d(node_material.clone()),
                Transform::from_translation(origin + veh_pos(node.pos)),
                RigidBody::Dynamic,
                Collider::ball(0.04),
                AdditionalMassProperties::Mass(node.mass), // hmotnost nodu z .veh
                Spawned,                                   // ať to umí Clear smazat
            ))
            .id();
        node_entity.insert(node.id.as_str(), entity);
    }

    // Beamy → pružiny mezi nody. Klidová délka = počáteční vzdálenost.
    // Reálné BeamNG tuhosti (tisíce až miliony) počítají s 2000 Hz solverem; rapier běží
    // na 60 Hz, takže je musíme ŠKÁLOVAT DOLŮ a STROPOVAT, jinak to ustřelí. Plus tlumení.
    // (Plná věrnost = vlastní solver — to je teprve cíl Fáze 2.)
    let stiffness_scale = 0.01;
    let max_stiffness = 500.0;
    for beam in &vehicle.beams {
        let (Some(&a), Some(&b)) = (node_entity.get(beam.a.as_str()), node_entity.get(beam.b.as_str())) else {
            continue; // beam odkazuje na neexistující nod → přeskoč
        };
        let rest = (positions[beam.a.as_str()] - positions[beam.b.as_str()]).length();
        // Materiál z .veh; když chybí (0), dosadíme fallback. Pak škála + strop.
        let raw = if beam.stiffness > 0.0 { beam.stiffness } else { 1000.0 };
        let stiffness = (raw * stiffness_scale).min(max_stiffness);
        let damping = (if beam.damping > 0.0 { beam.damping } else { 50.0 }).clamp(10.0, 100.0);
        let spring = SpringJointBuilder::new(rest, stiffness, damping);
        commands.entity(a).with_children(|child| {
            child.spawn(ImpulseJoint::new(b, spring));
        });
    }

    // --- Povrch z trojúhelníků: deformující se mesh, jehož vrcholy sledují nody ---
    // Vrchol i = node v pořadí vehicle.nodes; mapa id → index vrcholu.
    let mut vert_of: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    let mut vertex_positions: Vec<[f32; 3]> = Vec::with_capacity(vehicle.nodes.len());
    let mut vertex_nodes: Vec<Entity> = Vec::with_capacity(vehicle.nodes.len());
    for (i, node) in vehicle.nodes.iter().enumerate() {
        vert_of.insert(node.id.as_str(), i as u32);
        let p = origin + veh_pos(node.pos);
        vertex_positions.push([p.x, p.y, p.z]);
        vertex_nodes.push(node_entity[node.id.as_str()]);
    }
    // Indexy z trojúhelníků (přeskoč ty, co odkazují na neznámý nod).
    let mut indices: Vec<u32> = Vec::new();
    for tri in &vehicle.triangles {
        if let (Some(&a), Some(&b), Some(&c)) = (
            vert_of.get(tri.a.as_str()),
            vert_of.get(tri.b.as_str()),
            vert_of.get(tri.c.as_str()),
        ) {
            indices.extend([a, b, c]);
        }
    }
    if !indices.is_empty() {
        // RenderAssetUsages::all() = mesh zůstane i v main světě, ať ho můžeme každý snímek měnit.
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertex_positions);
        mesh.insert_indices(Indices::U32(indices));
        mesh.compute_normals();
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(Color::srgb(0.75, 0.75, 0.8))),
            Transform::IDENTITY,
            SoftbodyMesh { nodes: vertex_nodes },
            Spawned,
        ));
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

// Pohon auta: plyn (W), zpátečka (S), brzda (mezerník). Běží každý snímek.
// Query nám dá VŠECHNA kola: pár (data Wheel, jeho joint k autu).
// Motor jointu roztáčí volnou osu kola na zadanou cílovou rychlost.
fn vehicle_drive(keys: Res<ButtonInput<KeyCode>>, mut wheels: Query<(&Wheel, &mut ImpulseJoint)>) {
    // Cílová úhlová rychlost otáčení kola (rad/s). Znaménko určuje směr.
    let mut target_speed = 0.0;
    if keys.pressed(KeyCode::KeyW) {
        target_speed = 80.0; // plyn vpřed (vyšší = vyšší maximálka)
    } else if keys.pressed(KeyCode::KeyS) {
        target_speed = -80.0; // zpátečka
    }
    let braking = keys.pressed(KeyCode::Space);

    // Síla motoru = jak silně
    // tlačí kola na cílovou rychlost (~tah / zrychlení).
    let engine_power = 100.0;

    for (wheel, mut joint) in &mut wheels {
        if !wheel.driven {
            continue; // motor dáváme jen na hnaná (zadní) kola
        }
        // (cílová_rychlost, factor) — factor = jak silně motor tlačí kolo na cíl.
        let (vel, factor) = if braking {
            (0.0, 8.0) // brzda: drž rychlost na nule, silně
        } else {
            (target_speed, engine_power)
        };
        // joint.data je TypedJoint (enum). Vytáhneme revolute variantu a roztočíme ji.
        if let TypedJoint::RevoluteJoint(revolute) = &mut joint.data {
            revolute.set_motor_velocity(vel, factor);
        }
    }
}

// Řízení (kinematické): A/D natáčí RÁM jointu řízených kol kolem Y.
// Tím se natočí osa valení → kolo se s ní natočí. Je to pevná geometrie, ne síla,
// takže se točí kola (ne trup) a nic se neklepe.
fn vehicle_steer(keys: Res<ButtonInput<KeyCode>>, mut wheels: Query<(&Wheel, &mut ImpulseJoint)>) {
    // Maximální úhel zatočení v radiánech (~0.5 rad ≈ 28°).
    let max_angle = 0.5;
    let mut steer_angle = 0.0;
    if keys.pressed(KeyCode::KeyA) {
        steer_angle = max_angle; // doleva
    } else if keys.pressed(KeyCode::KeyD) {
        steer_angle = -max_angle; // doprava
    }

    for (wheel, mut joint) in &mut wheels {
        if !wheel.steered {
            continue; // natáčíme jen přední (řízená) kola
        }
        // Řízené kolo má generic joint — otočíme jeho rám (local_basis1) kolem Y.
        if let TypedJoint::GenericJoint(generic) = &mut joint.data {
            generic.set_local_basis1(Quat::from_rotation_y(steer_angle));
        }
    }
}

// Chase kamera: drží se za autem podle jeho směru a kouká na něj.
// Dva dotazy na Transform: auto čteme, kameru měníme. `Without<Chassis>` u kamery
// říká Bevy, že to NEJSOU stejné entity (jinak by dva přístupy k Transform neprošly).
fn follow_camera(
    car: Query<&Transform, With<Chassis>>,
    mut camera: Query<&mut Transform, (With<Camera3d>, Without<Chassis>)>,
) {
    // get_single = čekáme přesně jedno auto a jednu kameru; jinak nic neděláme.
    let Ok(car_tf) = car.get_single() else { return; };
    let Ok(mut cam_tf) = camera.get_single_mut() else { return; };

    // Směr, kam auto míří (jeho lokální +Z ve světě). Vynulujeme Y → bereme jen
    // otočení kolem svislé osy (yaw), aby kamera nepodlétla zem při náklonu/skoku.
    let mut heading = car_tf.rotation * Vec3::Z;
    heading.y = 0.0;
    let heading = heading.normalize_or_zero();

    // Kamera za auto (proti směru jízdy) a nahoru, pak namíříme na auto.
    let offset = -heading * 12.0 + Vec3::Y * 5.0;
    cam_tf.translation = car_tf.translation + offset;
    cam_tf.look_at(car_tf.translation, Vec3::Y);
}
