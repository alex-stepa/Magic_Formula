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
        .init_resource::<SoftBody>()
        .add_systems(Startup, setup)
        // FixedUpdate (60 Hz) = náš solver; uvnitř si dělá vlastní substepping.
        .add_systems(FixedUpdate, softbody_solve)
        // Update = spustí se každý snímek (reaguje na vstup, zapisuje pozice do meshe).
        .add_systems(Update, (vehicle_drive, vehicle_steer, follow_camera, spawn_buttons, spawn_shortcuts, toggle_menu, softbody_to_transforms, update_softbody_mesh).chain())
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

// --- VLASTNÍ SOFT-BODY SOLVER (Fáze 2) ---
// Stav soft-body držíme my, ne rapier. Structure-of-Arrays: index i = jeden nod,
// pole pos/vel/inv_mass/force jsou paralelní (cache-friendly, beamy odkazují indexem).
#[derive(Resource, Default)]
struct SoftBody {
    pos: Vec<Vec3>,        // pozice nodu i (světové)
    vel: Vec<Vec3>,        // rychlost nodu i
    inv_mass: Vec<f32>,    // 1/m (předpočítané; 0 = ukotvený nod)
    force: Vec<Vec3>,      // akumulátor sil (čistí se každý substep)
    beams: Vec<BeamC>,     // pružiny mezi indexy
    wheels: Vec<WheelC>,   // kola (každé = hub-node + stav pneumatiky)
    entities: Vec<Entity>, // node index i ↔ render entita (zápis Transformu)
    substeps: u32,         // kolik integračních kroků na snímek (adaptivně dle nejtužšího beamu)
}

// Beam ve solveru — indexy do polí výše (ne Entity ani String), REÁLNÁ tuhost z .veh.
struct BeamC {
    i: usize,
    j: usize,
    rest: f32, // klidová délka
    k: f32,    // tuhost (bez škálování)
    d: f32,    // tlumení
}

// --- KOLA NA SOLVERU (jednotný model: kolo = node v SoftBody s bohatším kontaktem) ---
// Princip rozšiřitelnosti: všechno tady jsou DATA, ne konstanty v kódu. Tuning part / custom
// díly časem jen přepíšou tahle čísla; solver se nemění. Pérování NENÍ tady — to jsou beamy,
// které postaví builder zavěšení (viz SuspensionKind) → různé geometrie bez zásahu do solveru.
// allow(dead_code): spin_vel/driven/steered/grip zatím jen zapisujeme při spawnu —
// číst je bude TIRE MODEL (krok 2: pohon, brzda, řízení, boční grip). Schválně už teď,
// ať je datový tvar kola hotový a builder se příště nemusí měnit.
#[allow(dead_code)]
struct WheelC {
    hub: usize,        // index hub-nodu v SoftBody (na něm visí pérovací beamy do těla)
    render: Entity,    // entita disku kola (zápis Transformu: pozice hubu + spin + steer)
    radius: f32,
    spin: f32,         // úhel natočení kola (rad) — pro render i výpočet kontaktu
    spin_vel: f32,     // úhlová rychlost valení (rad/s) — pohon/brzda (krok 2)
    steer: f32,        // aktuální natočení řízení (rad), nastaví vehicle_steer (krok 2)
    driven: bool,      // pohání ho motor? (krok 2)
    steered: bool,     // natáčí se s ním? (krok 2)
    tire: TireParams,  // model pneumatiky — izolovaná data, ať jde vyměnit za Pacejku
}

// Parametry pneumatiky = data. Teď jen svislá tuhost/tlumení (auto sedí na kolech);
// `grip` použije boční/podélný tire model v kroku 2. Časem sem přijdou Pacejka koeficienty.
#[allow(dead_code)] // grip čte až tire model (krok 2)
#[derive(Clone, Copy)]
struct TireParams {
    normal_stiffness: f32, // svislá tuhost kontaktu (pneumatika tlačí auto nahoru)
    normal_damping: f32,   // tlumení svislého kontaktu
    grip: f32,             // lineární grip (boční/podélná síla na jednotku skluzu)
}

// Typ zavěšení = JAK builder propojí hub kola s tělem (topologie + tuhost beamů).
// Solver o typu neví. Zatím jen Rigid; další varianty přidají mezi-nody a jiné beamy.
#[derive(Clone, Copy)]
#[allow(dead_code)] // další varianty přijdou; držíme šev pro rozšíření
enum SuspensionKind {
    Rigid, // hub připojen tuhými beamy přímo k nejbližším nodům těla
    // McPherson { stiffness, damping, travel },
    // DoubleWishbone { ... },
    // LeafSpring { ... },
}

// FixedUpdate jede defaultně na 64 Hz — délka jednoho snímku solveru.
const FRAME_DT: f32 = 1.0 / 64.0;
// Rezerva pod CFL hranicí (stabilita explicitní integrace = dt < 2/ω; bereme jen část).
const CFL_SAFETY: f32 = 0.4;
// Strop substepů, ať patologicky tuhý .veh nezmrazí hru.
const MAX_SUBSTEPS: u32 = 512;
// Horní hrana zemního kvádru (Collider::cuboid half_height 0.1 v počátku) — kontaktní rovina.
const GROUND_TOP: f32 = 0.1;
// Poměr tlumení beamů: 0 = netlumené (zvoní/poskakuje), 1 = kriticky tlumené (mrtvé).
// Tlumení škálujeme s tuhostí: d = ζ·d_crit = 2·ζ·√(k·μ) — jinak tuhé beamy jen zvoní.
const DAMPING_RATIO: f32 = 0.25;

// Pomocné metody pro STAVBU soft-body. Jediný zdroj pravdy — používá je builder .veh
// i builder auta, ať se logika (tlumení, substepy) nikde nerozjede.
impl SoftBody {
    // Přidá node a vrátí jeho index. `entity` = render kulička pro zápis Transformu.
    fn add_node(&mut self, pos: Vec3, mass: f32, entity: Entity) -> usize {
        let i = self.pos.len();
        self.pos.push(pos);
        self.vel.push(Vec3::ZERO);
        self.inv_mass.push(1.0 / mass);
        self.force.push(Vec3::ZERO);
        self.entities.push(entity);
        i
    }

    // Přidá beam mezi dva nody. Klidová délka = aktuální vzdálenost, tlumení dle ζ a tuhosti.
    fn add_beam(&mut self, i: usize, j: usize, k: f32) {
        let mu = 1.0 / (self.inv_mass[i] + self.inv_mass[j]); // redukovaná hmotnost pružiny
        self.beams.push(BeamC {
            i,
            j,
            rest: (self.pos[i] - self.pos[j]).length(),
            k,
            d: 2.0 * DAMPING_RATIO * (k * mu).sqrt(),
        });
    }

    // Dopočítá počet substepů z CFL (Gershgorin: ω²ₘₐₓ ≤ 2·max_i(Σk_i/m_i)). Volá se po
    // přidání všech beamů. Tuhý uzel (součet tuhostí incidentních beamů) určuje frekvenci.
    fn compute_substeps(&mut self) {
        let mut k_sum = vec![0.0_f32; self.pos.len()];
        for beam in &self.beams {
            k_sum[beam.i] += beam.k;
            k_sum[beam.j] += beam.k;
        }
        let mut max_w2 = 0.0_f32;
        for i in 0..self.pos.len() {
            if self.inv_mass[i] > 0.0 {
                max_w2 = max_w2.max(2.0 * k_sum[i] * self.inv_mass[i]);
            }
        }
        let max_omega = max_w2.sqrt();
        self.substeps = if max_omega > 0.0 {
            let sub_dt_max = CFL_SAFETY * 2.0 / max_omega;
            ((FRAME_DT / sub_dt_max).ceil() as u32).clamp(1, MAX_SUBSTEPS)
        } else {
            1
        };
    }
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
    mut softbody: ResMut<SoftBody>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            SpawnAction::RigidCar => spawn_solver_car(&mut commands, &mut meshes, &mut materials, &mut softbody),
            SpawnAction::SoftBody(path) => spawn_softbody(&mut commands, &mut meshes, &mut materials, &mut softbody, path),
            SpawnAction::Clear => clear_spawned(&mut commands, &spawned, &mut softbody),
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

// Klávesové zkratky: 1 = rigid auto, 2 = test.veh, 3 = fabia.veh (pokud existuje).
fn spawn_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    spawned: Query<Entity, With<Spawned>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut softbody: ResMut<SoftBody>,
) {
    // Clear = Shift+C (samotné C je obsazené kamerou).
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if shift && keys.just_pressed(KeyCode::KeyC) {
        clear_spawned(&mut commands, &spawned, &mut softbody);
    }
    if keys.just_pressed(KeyCode::Digit1) {
        spawn_solver_car(&mut commands, &mut meshes, &mut materials, &mut softbody);
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

// Postaví zavěšení = beamy mezi hubem kola a tělem. JAK je propojí, závisí na typu.
// Solver o typu neví — tady se „vlévá" celá variabilita podvozku (custom suspension).
fn build_suspension(sb: &mut SoftBody, hub: usize, body: &[usize], kind: SuspensionKind) {
    match kind {
        // Rigid = hub tuze připojen ke všem nodům těla → kolo drží napevno (zatím žádný zdvih).
        // McPherson/DoubleWishbone časem přidají mezi-nody a měkčí beamy podél zdvihu.
        SuspensionKind::Rigid => {
            let k = 150_000.0;
            for &b in body {
                sb.add_beam(hub, b, k);
            }
        }
    }
}

// Spawne JEZDÍCÍ auto na vlastním solveru (jednotný model): tělo = node-beam kvádr,
// kola = hub-nody zavěšené beamy + render disky. Minimální test rig pro odladění
// drive/steer/grip (krok 2). Žádný rapier.
fn spawn_solver_car(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    sb: &mut SoftBody,
) {
    *sb = SoftBody::default(); // nový spawn = čistý solver
    let origin = Vec3::new(0.0, 2.0, 0.0);

    let node_mesh = meshes.add(Sphere::new(0.06));
    let node_material = materials.add(Color::srgb(0.9, 0.3, 0.2));

    // --- Tělo: 8 rohů kvádru, PLNĚ provázané tuhými beamy (rám se nedá zkroutit) ---
    let (hx, hy, hz) = (0.7, 0.25, 1.8);
    let body_mass = 15.0; // na jeden roh
    let chassis_k = 300_000.0;
    let mut body: Vec<usize> = Vec::with_capacity(8);
    for sx in [-1.0_f32, 1.0] {
        for sy in [-1.0_f32, 1.0] {
            for sz in [-1.0_f32, 1.0] {
                let p = origin + Vec3::new(sx * hx, sy * hy, sz * hz);
                let e = commands
                    .spawn((
                        Mesh3d(node_mesh.clone()),
                        MeshMaterial3d(node_material.clone()),
                        Transform::from_translation(p),
                        Spawned,
                    ))
                    .id();
                body.push(sb.add_node(p, body_mass, e));
            }
        }
    }
    // plný graf přes 8 rohů = 28 beamů → tuhý rám
    for a in 0..body.len() {
        for b in (a + 1)..body.len() {
            sb.add_beam(body[a], body[b], chassis_k);
        }
    }

    // --- 4 kola: hub-node (zavěšený na tělo) + render disk ---
    let wheel_radius = 0.34;
    let wheel_width = 0.22;
    let hub_mass = 20.0;
    let wheel_mesh = meshes.add(Cylinder::new(wheel_radius, wheel_width));
    let wheel_material = materials.add(Color::srgb(0.05, 0.05, 0.05));
    let (wx, wy, wz) = (0.85, -0.15, 1.4);
    let layout = [
        (-1.0_f32, 1.0_f32, true, false),  // FL: řízené
        (1.0, 1.0, true, false),           // FR: řízené
        (-1.0, -1.0, false, true),         // RL: hnané
        (1.0, -1.0, false, true),          // RR: hnané
    ];
    for (sx, sz, steered, driven) in layout {
        let hub_pos = origin + Vec3::new(sx * wx, wy, sz * wz);
        // debug kulička v ose hubu (drží Transform hubu); samotné kolo = disk níž
        let hub_dbg = commands
            .spawn((
                Mesh3d(node_mesh.clone()),
                MeshMaterial3d(node_material.clone()),
                Transform::from_translation(hub_pos),
                Spawned,
            ))
            .id();
        let hub = sb.add_node(hub_pos, hub_mass, hub_dbg);
        build_suspension(sb, hub, &body, SuspensionKind::Rigid);

        let render = commands
            .spawn((
                Mesh3d(wheel_mesh.clone()),
                MeshMaterial3d(wheel_material.clone()),
                Transform::from_translation(hub_pos),
                Spawned,
            ))
            .id();
        sb.wheels.push(WheelC {
            hub,
            render,
            radius: wheel_radius,
            spin: 0.0,
            spin_vel: 0.0,
            steer: 0.0,
            driven,
            steered,
            tire: TireParams { normal_stiffness: 120_000.0, normal_damping: 4_000.0, grip: 8_000.0 },
        });
    }

    sb.compute_substeps();
    info!(
        "Solver auto: {} nodů, {} beamů, {} kol, {} substepů",
        sb.pos.len(),
        sb.beams.len(),
        sb.wheels.len(),
        sb.substeps
    );
}

// BeamNG používá z = nahoru, Bevy y = nahoru. Přehodíme osy, ať auto stojí správně
// (a zachováme pravotočivost: dopředné +y BeamNG → -z Bevy).
fn veh_pos(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1])
}

// Načte .veh a naplní vlastní solver: nody = render kuličky + záznam v `SoftBody`,
// beamy = pružiny (`BeamC`) mezi indexy nodů, triangly = deformující povrch.
// Reálné tuhosti z .veh; tlumení a počet substepů dopočítají helpery `SoftBody`.
fn spawn_softbody(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    sb: &mut SoftBody,
    path: &str,
) {
    let raw = std::fs::read_to_string(path).expect("nelze číst .veh");
    let vehicle: veh::Vehicle = serde_json::from_str(&raw).expect("nevalidní .veh");
    info!("Načteno vozidlo z .veh: {} nodů, {} beamů", vehicle.nodes.len(), vehicle.beams.len());

    let origin = Vec3::new(0.0, 4.0, 6.0); // PŘED auto (+Z) a ve vzduchu, ať to chase kamera vidí

    // Nový spawn = čistý solver (jinak by se sčítala dvě auta do jednoho stavu).
    *sb = SoftBody::default();

    // Sdílený mesh + materiál pro všechny nody (1067× nepřidávat nový asset = výkon).
    let node_mesh = meshes.add(Sphere::new(0.04));
    let node_material = materials.add(Color::srgb(0.2, 0.6, 1.0));

    // Mapa id → (render Entity, index do polí SoftBody). Beamy i triangly z ní berou.
    let mut node_idx: std::collections::HashMap<&str, (Entity, usize)> = std::collections::HashMap::new();

    // Nody → render kuličky (BEZ rapieru) + záznam do solveru přes helper.
    for node in &vehicle.nodes {
        let entity = commands
            .spawn((
                Mesh3d(node_mesh.clone()),
                MeshMaterial3d(node_material.clone()),
                Transform::from_translation(origin + veh_pos(node.pos)),
                Spawned, // ať to umí Clear smazat
            ))
            .id();
        let i = sb.add_node(origin + veh_pos(node.pos), node.mass, entity);
        node_idx.insert(node.id.as_str(), (entity, i));
    }

    // Beamy → BeamC do solveru. REÁLNÁ tuhost z .veh — žádný scale, žádný strop.
    for beam in &vehicle.beams {
        let (Some(&(_, i)), Some(&(_, j))) = (node_idx.get(beam.a.as_str()), node_idx.get(beam.b.as_str())) else {
            continue; // beam odkazuje na neexistující nod → přeskoč
        };
        let k = if beam.stiffness > 0.0 { beam.stiffness } else { 1000.0 };
        sb.add_beam(i, j, k);
    }

    sb.compute_substeps();
    info!("Soft-body: {} substepů (~{} Hz solver)", sb.substeps, sb.substeps * 64);

    // --- Povrch z trojúhelníků: deformující se mesh, jehož vrcholy sledují nody ---
    // Vrchol i = node v pořadí vehicle.nodes; mapa id → index vrcholu.
    let mut vert_of: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    let mut vertex_positions: Vec<[f32; 3]> = Vec::with_capacity(vehicle.nodes.len());
    let mut vertex_nodes: Vec<Entity> = Vec::with_capacity(vehicle.nodes.len());
    for (i, node) in vehicle.nodes.iter().enumerate() {
        vert_of.insert(node.id.as_str(), i as u32);
        let p = origin + veh_pos(node.pos);
        vertex_positions.push([p.x, p.y, p.z]);
        vertex_nodes.push(node_idx[node.id.as_str()].0);
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

// --- SOLVER: semi-implicitní Euler se substeppingem (běží ve FixedUpdate, 64 Hz) ---
// Jeden snímek rozdělíme na `sb.substeps` malých kroků (adaptivně, viz compute_substeps),
// aby tuhé beamy zůstaly stabilní (CFL: dt < 2/ω). Reálné tuhosti, žádné škálování.
fn softbody_solve(sb: ResMut<SoftBody>, time: Res<Time<Fixed>>) {
    if sb.beams.is_empty() {
        return; // nic nespawnuto
    }
    let sb = sb.into_inner(); // rozbalíme ResMut → můžeme půjčovat víc polí najednou
    let gravity = Vec3::new(0.0, -9.81, 0.0);
    let ground_y = GROUND_TOP + 0.04; // horní hrana zemního kvádru + poloměr nodu
    let sub_dt = time.delta_secs() / sb.substeps as f32;

    for _ in 0..sb.substeps {
        // 1) vynuluj síly a přidej gravitaci (jen pohyblivým nodům)
        for i in 0..sb.pos.len() {
            sb.force[i] = if sb.inv_mass[i] > 0.0 { gravity / sb.inv_mass[i] } else { Vec3::ZERO };
        }

        // 2) síly beamů: pružina (Hooke) + tlumení podél osy
        for beam in &sb.beams {
            let delta = sb.pos[beam.j] - sb.pos[beam.i];
            let len = delta.length();
            if len < 1e-6 {
                continue; // zdegenerovaný beam → nedělitelný směr
            }
            let dir = delta / len;
            let spring = beam.k * (len - beam.rest);
            let rel_v = (sb.vel[beam.j] - sb.vel[beam.i]).dot(dir);
            let f = (spring + beam.d * rel_v) * dir;
            sb.force[beam.i] += f; // přitahuje i k j
            sb.force[beam.j] -= f;
        }

        // 2.5) svislá síla pneumatik: kontakt jako KRUH o poloměru kola (ne bod jako u nodů).
        // Tím auto sedí na kolech místo zaboření hubu. Podélná/boční síla přijde v kroku 2.
        // Indexový loop + kopie skalárů — abychom mohli zapsat do sb.force[hub] bez aliasingu.
        for wi in 0..sb.wheels.len() {
            let hub = sb.wheels[wi].hub;
            let radius = sb.wheels[wi].radius;
            let ns = sb.wheels[wi].tire.normal_stiffness;
            let nd = sb.wheels[wi].tire.normal_damping;
            let pen = (GROUND_TOP + radius) - sb.pos[hub].y; // >0 = pneumatika stlačená
            if pen > 0.0 {
                let fy = ns * pen - nd * sb.vel[hub].y; // pružina nahoru − tlumení
                if fy > 0.0 {
                    sb.force[hub].y += fy;
                }
            }
        }

        // 3) integrace (semi-implicitní Euler) + kolize se zemí
        for i in 0..sb.pos.len() {
            if sb.inv_mass[i] == 0.0 {
                continue; // ukotvený nod
            }
            sb.vel[i] += sb.force[i] * sb.inv_mass[i] * sub_dt;
            sb.pos[i] += sb.vel[i] * sub_dt;

            // Zem: vytlač nad rovinu a utlum odraz (normála) i tření (tangenta).
            if sb.pos[i].y < ground_y {
                sb.pos[i].y = ground_y;
                if sb.vel[i].y < 0.0 {
                    sb.vel[i].y = 0.0; // žádný skok zpět (nepružná zem)
                }
                sb.vel[i].x *= 0.9; // hrubé tření, než přijde pořádný model
                sb.vel[i].z *= 0.9;
            }
        }
    }
}

// Zápis stavu solveru → Transformy render entit (kuličky + disky kol). Mesh už pak
// deformuje update_softbody_mesh, který čte tyhle Transformy. Běží v Update (po solveru).
fn softbody_to_transforms(sb: Res<SoftBody>, mut transforms: Query<&mut Transform>) {
    // Nody → kuličky.
    for (i, &entity) in sb.entities.iter().enumerate() {
        if let Ok(mut tf) = transforms.get_mut(entity) {
            tf.translation = sb.pos[i];
        }
    }
    // Kola → disky: pozice = hub, rotace = řízení(Y) · valení(X) · base(válec Y→osa X).
    let base = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    for wheel in &sb.wheels {
        if let Ok(mut tf) = transforms.get_mut(wheel.render) {
            tf.translation = sb.pos[wheel.hub];
            tf.rotation = Quat::from_rotation_y(wheel.steer) * Quat::from_rotation_x(wheel.spin) * base;
        }
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
