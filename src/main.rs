// Magic Formula — vstupní bod hry.
// Tady se sestaví Bevy App, zapnou pluginy a zaregistrují systémy.

use bevy::prelude::*;          // základ Bevy: App, Commands, Transform, Component...
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
        .add_systems(Update, (vehicle_drive, vehicle_steer, follow_camera))
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

    // Pozici auta si uložíme, ať ji použijeme pro auto i pro výpočet pozic kol.
    let car_pos = Vec3::new(0.0, 3.0, 0.0);

    // Auto (placeholder): červený kvádr 1.5×0.5×4 + fyzika. Užší než stopa kol,
    // aby přední (i zadní) kola stála volně vedle těla = prostor místo "výřezu".
    // .id() vrátí Entity (identitu auta). Uložíme si ji — joints kol ji potřebují.
    let chassis = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.5, 0.5, 4.0))),
            MeshMaterial3d(materials.add(Color::srgb(0.8, 0.2, 0.2))),
            Transform::from_translation(car_pos),
        ))
        .insert(physics::car_physics())
        .insert(Chassis)            // marker, ať auto najdeme v řízení
        .insert(Velocity::default()) // umožní číst/nastavit rychlost auta
        .id();

    // Společné parametry kol — později přijdou z .veh souboru (build system).
    let wheel_radius = 0.32;
    let wheel_width = 0.2;

    // Seznam kol: (název, offset od středu auta, řízené, hnané).
    // Iterujeme přes něj smyčkou → přidat/změnit kolo = změnit data, ne kód.
    // X = ±1.15 (kraje, kousek mimo tělo, ať se collidery na startu nepřekrývají),
    // Z = ±1.5 (přední/zadní náprava), Y = -0.3 (kousek pod střed auta).
    let wheels = [
        ("FL", Vec3::new(-1.15, -0.3, 1.5), true, false),  // přední levé: řízené
        ("FR", Vec3::new(1.15, -0.3, 1.5), true, false),   // přední pravé: řízené
        ("RL", Vec3::new(-1.15, -0.3, -1.5), false, true), // zadní levé: hnané
        ("RR", Vec3::new(1.15, -0.3, -1.5), false, true),  // zadní pravé: hnané
    ];

    // Pro každý záznam v seznamu spawneme jedno kolo.
    for (name, offset, steered, driven) in wheels {
        commands
            .spawn((
                // Tělo kola: BEZ rotace (kvůli ose jointu). Pozice = auto + offset.
                Transform::from_translation(car_pos + offset),
                Visibility::default(), // ať se dědí viditelnost na mesh-dítě
                Name::new(format!("Wheel {name}")), // jméno pro snazší ladění
            ))
            .insert(physics::wheel_physics(wheel_radius, wheel_width))
            .insert(Wheel { radius: wheel_radius, driven, steered })
            .insert(physics::Suspension::Rigid) // typ zavěšení (data pro build system)
            // Joint: kolo → chassis. U řízených kol půjde rám jointu za běhu natáčet.
            .insert(physics::wheel_joint(physics::Suspension::Rigid, chassis, offset, steered))
            .with_children(|builder| {
                // Mesh válce jako DÍTĚ, otočené o 90° → kolo vizuálně "leží" (osa X).
                // Otáčíme jen vzhled, ne tělo, takže osa jointu zůstane správná.
                builder.spawn((
                    Mesh3d(meshes.add(Cylinder::new(wheel_radius, wheel_width))),
                    MeshMaterial3d(materials.add(Color::srgb(0.1, 0.1, 0.1))),
                    Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                ));
            });
    }

    // --- Fáze 2: načti .veh a postav z něj node-beam SOFT-BODY (testovací čtyřstěn) ---
    spawn_softbody(&mut commands, &mut meshes, &mut materials);
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
) {
    // Když je k dispozici převedená Fabia (lokální, gitignored), načti ji; jinak testovací čtyřstěn.
    let path = if std::path::Path::new("assets/fabia.veh").exists() {
        "assets/fabia.veh"
    } else {
        "assets/test.veh"
    };
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
