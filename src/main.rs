// Magic Formula — vstupní bod hry.
// Tady se sestaví Bevy App, zapnou pluginy a zaregistrují systémy.

use bevy::prelude::*;          // základ Bevy: App, Commands, Transform, Component...
use bevy_rapier3d::prelude::*; // fyzika: RapierPhysicsPlugin, RigidBody, Collider...

mod physics; // načte modul z src/physics.rs (fyzikální komponenty)

// Resource = globální data sdílená celou hrou (jen jedna instance).
// Sem si držíme, v jakém režimu je kamera.
#[derive(Resource)]
enum CameraMode {
    Free,   // volná kamera (poletíš si kam chceš)
    Follow, // kamera připnutá na auto
}

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
        // Vloží náš resource do hry s počáteční hodnotou Free.
        .insert_resource(CameraMode::Free)
        // Startup = spustí se jednou na začátku (postaví scénu).
        .add_systems(Startup, setup)
        // Update = spustí se každý snímek (reaguje na vstup).
        .add_systems(Update, camera_mode)
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

    // Auto (placeholder): červený kvádr 2×0.5×4 + fyzika. Spuštěné z výšky, ať je vidět pád.
    // .id() vrátí Entity (identitu auta). Uložíme si ji — joints kol ji potřebují.
    let chassis = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(2.0, 0.5, 4.0))),
            MeshMaterial3d(materials.add(Color::srgb(0.8, 0.2, 0.2))),
            Transform::from_translation(car_pos),
        ))
        .insert(physics::car_physics())
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
            // Joint připne kolo k chassis. `offset` je bod uchycení na autě.
            .insert(physics::wheel_joint(physics::Suspension::Rigid, chassis, offset))
            .with_children(|parent| {
                // Mesh válce jako DÍTĚ, otočené o 90° → kolo vizuálně "leží" (osa X).
                // Otáčíme jen vzhled, ne tělo, takže osa jointu zůstane správná.
                parent.spawn((
                    Mesh3d(meshes.add(Cylinder::new(wheel_radius, wheel_width))),
                    MeshMaterial3d(materials.add(Color::srgb(0.1, 0.1, 0.1))),
                    Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                ));
            });
    }
}

// Přepíná režim kamery při stisku C. Běží každý snímek (Update).
// ResMut<CameraMode> = můžeme režim měnit. Res<ButtonInput<KeyCode>> = stav kláves.
fn camera_mode(mut camera_mode: ResMut<CameraMode>, keys: Res<ButtonInput<KeyCode>>) {
    // just_pressed = reaguje jen na okamžik stisku, ne na držení.
    if keys.just_pressed(KeyCode::KeyC) {
        // Podle aktuálního režimu přepneme na ten druhý.
        match *camera_mode {
            CameraMode::Free => {
                *camera_mode = CameraMode::Follow;
                println!("Follow");
            }
            CameraMode::Follow => {
                *camera_mode = CameraMode::Free;
                println!("Free");
            }
        }
    }
}
