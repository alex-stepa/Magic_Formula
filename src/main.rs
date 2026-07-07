// Magic Formula — vstupní bod hry.
// Tady se sestaví Bevy App, zapnou pluginy a zaregistrují systémy.

use bevy::prelude::*;          // základ Bevy: App, Commands, Transform, Component...
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor}; // opakující se textura zem
use bevy::render::mesh::Indices; // indexy trojúhelníků pro mesh
use bevy::render::render_asset::RenderAssetUsages; // mesh zůstane i v main světě (ať ho updatujeme)
use bevy::render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat}; // TriangleList + tvorba textury
use bevy::render::view::NoFrustumCulling; // deformující mesh: vypnout culling (Aabb se nepřepočítává)
use bevy_rapier3d::prelude::*; // fyzika: RapierPhysicsPlugin, RigidBody, Collider...

mod physics; // načte modul z src/physics.rs (fyzikální komponenty)
mod veh; // načítání .veh formátu (Fáze 2 — node-beam soft-body)

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

// Marker na všech naspawnovaných herních entitách (chassis, kola, nody) — podle něj Clear maže.
#[derive(Component)]
struct Spawned;

// Viditelný trup auta (kvádr). Nemá vlastní fyziku — jeho Transform každý snímek
// přepíšeme z orientace rámu (chassis), ať „sedí" na uzlech soft-body.
#[derive(Component)]
struct CarBody;

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
    chassis: Option<ChassisFrame>, // referenční nody pro směr auta (None u .veh — nemá kola)
}

// Referenční nody těla, ze kterých solver dopočítá dopředný/boční směr auta (yaw rámu).
// Forward = mean(front) − mean(rear), Right = mean(right) − mean(left). Plní ho builder auta;
// solver je tak pořád data-driven (nezná layout, jen čte indexy).
struct ChassisFrame {
    front: Vec<usize>,
    rear: Vec<usize>,
    left: Vec<usize>,
    right: Vec<usize>,
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
struct WheelC {
    hub: usize,        // index hub-nodu v SoftBody (na něm visí pérovací beamy do těla)
    render: Entity,    // entita disku kola (zápis Transformu: pozice hubu + spin + steer)
    radius: f32,
    spin: f32,         // úhel natočení kola (rad) — pro render
    spin_vel: f32,     // úhlová rychlost valení (rad/s)
    steer: f32,        // aktuální natočení řízení (rad), nastaví vehicle_steer
    drive_torque: f32, // moment na tomhle kole (Nm), nastaví vehicle_drive (z motoru+převodů)
    brake: f32,        // brzdný moment (Nm), nastaví vehicle_drive
    front: bool,       // je na PŘEDNÍ ose? (pohon rozhoduje DriveLayout, ne kolo samo)
    steered: bool,     // natáčí se s ním?
    tire: TireParams,  // model pneumatiky — izolovaná data, ať jde vyměnit za Pacejku
}

// Koeficienty Magic Formula (Pacejka) pro jednu osu (podélnou nebo boční).
// F = D·sin(C·atan(B·x − E·(B·x − atan(B·x)))), kde D = μ·Fz se dopočítá za běhu.
#[derive(Clone, Copy)]
struct Pacejka {
    b: f32, // stiffness factor — jak strmě křivka stoupá ke špičce
    c: f32, // shape factor — tvar (podélně ~1.65, bočně ~1.3)
    e: f32, // curvature factor — ostrost špičky (~0.97)
}

// Parametry pneumatiky = data. Svislý kontakt (auto sedí na kolech) + dvě Pacejka křivky.
#[derive(Clone, Copy)]
struct TireParams {
    normal_stiffness: f32, // svislá tuhost kontaktu (pneumatika tlačí auto nahoru)
    normal_damping: f32,   // tlumení svislého kontaktu
    long: Pacejka,         // podélná křivka (slip ratio → tah/brzda)
    lat: Pacejka,          // boční křivka (slip angle → zatáčení)
}

// --- LADICÍ HODNOTY PNEUMATIKY (vše na jednom místě) ---
// Změň čísla a `cargo run` — solver je nesahá, jen čte. D (špička) = MU·Fz dynamicky.
const TIRE_LONG: Pacejka = Pacejka { b: 10.0, c: 1.65, e: 0.97 };
const TIRE_LAT: Pacejka = Pacejka { b: 8.0, c: 1.3, e: 0.97 };
// Spodní mez referenční rychlosti pro normalizaci skluzu (m/s). Brání dělení nulou
// při rozjezdu z místa — bez ní κ vystřelí a auto exploduje.
const V_LOW: f32 = 2.0;

// --- LADICÍ HODNOTY ZAVĚŠENÍ (Coil) ---
// Ramena tuhá jako Rigid (drží kolo v ose), pružina měkčí (svislý zdvih). Sag na roh:
// ~2350 N / (coil_k·cos²θ ≈ 0.47·k) → při 100k ≈ 5 cm. Zdvih končí ~40 cm níž, kde se
// pružina překlopí do vodorovna (snap-through) — kapacita ½·k_v·0.4² musí přežít dopad
// z 2 m (~3.8 kJ na roh); při 100k + tlumiči (COIL_DAMP_MULT) vychází s rezervou.
const SUSP_ARM_K: f32 = 150_000.0;
const SUSP_COIL_K: f32 = 100_000.0;
// Násobek tlumení coil beamu: globální DAMPING_RATIO 0.25 je proti zvonění rámu,
// skutečný tlumič zavěšení má ζ ≈ 0.6 — dopad má sežrat tlumič, ne konec zdvihu.
const COIL_DAMP_MULT: f32 = 2.5;

// Typ zavěšení = JAK builder propojí hub kola s tělem (topologie + tuhost beamů).
// Solver o typu neví.
#[derive(Clone, Copy, PartialEq)]
enum SuspensionKind {
    Rigid, // hub připojen tuhými beamy přímo k nejbližším nodům těla
    // Svislý zdvih trikem geometrie: 2 tuhá SKORO VODOROVNÁ ramena drží kolo podélně
    // i příčně (svisle skoro nekladou odpor — délka se mění jen ~Δ²/2L), 1 měkčí SKORO
    // SVISLÁ pružina nese váhu a pruží. Klidová délka = spawn vzdálenost → auto si po
    // spawnu sedne o sag ≈ váha/k_svislá níž.
    Coil { arm_k: f32, coil_k: f32 },
    // McPherson { ... }, DoubleWishbone { ... }, LeafSpring { ... } — časem
}

impl SuspensionKind {
    // Presety pro build menu (výběr „dílu" bez ladicích čísel v UI).
    const RIGID: Self = Self::Rigid;
    const COIL: Self = Self::Coil { arm_k: SUSP_ARM_K, coil_k: SUSP_COIL_K };
    fn label(self) -> &'static str {
        match self {
            Self::Rigid => "Rigid (tuhé)",
            Self::Coil { .. } => "Coil (zdvih)",
        }
    }
}

// Build config = jaké DÍLY auto má. Menu píše sem, spawn_solver_car čte → výměna dílu
// bez zásahu do solveru (stejný vzor jako Drivetrain: díl = data, resource drží volbu).
// Zatím jen zavěšení; motor/převodovka žijí v Drivetrain, časem se sem sloučí.
#[derive(Resource)]
struct CarBuild {
    suspension: SuspensionKind,
}
impl Default for CarBuild {
    fn default() -> Self {
        Self { suspension: SuspensionKind::COIL }
    }
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
// Koeficient tření pneumatika↔zem. Strop celkové vodorovné síly = MU·Fz (kruh adheze):
// víc, než drží guma, nedá podélnou ani boční sílu (skluz/smyk).
const MU: f32 = 1.0;

// --- DRIVETRAIN ---
// Part-shaped (připraveno na build systém): DÍLY jsou datové structy, RESOURCE drží,
// které díly jsou v autě + runtime stav. Build systém díl vymění, tuning přepíše pole,
// `.veh parts` se sem časem jen deserializují. Žádné magické konstanty v systémech.

// Motor = díl. Moment podle RPM dá momentová křivka (body (rpm, Nm), lineární interpolace).
struct Engine {
    torque_curve: Vec<(f32, f32)>, // (rpm, Nm) — vzestupně podle rpm
    max_rpm: f32,                  // omezovač otáček
    idle_rpm: f32,                 // volnoběh (pod něj RPM neklesne)
}

// Převodovka = díl. Poměry kvaltů + stálý převod (rozvodovka).
struct Gearbox {
    ratios: Vec<f32>, // poměry pro kvalty 1..N
    final_drive: f32, // stálý převod
}

// Rozvod pohonu = nastavení (data). Které osy táhnou + jak se mezi ně dělí moment.
#[derive(Clone, Copy)]
enum DriveLayout {
    Fwd,                // přední pohon
    Rwd,                // zadní pohon
    Awd { front: f32 }, // 4×4: podíl momentu na přední osu (0..1), zadní = 1−front
}

impl DriveLayout {
    // Podíl celkového momentu jdoucí na osu daného kola.
    fn axle_fraction(self, front: bool) -> f32 {
        match self {
            DriveLayout::Fwd => if front { 1.0 } else { 0.0 },
            DriveLayout::Rwd => if front { 0.0 } else { 1.0 },
            DriveLayout::Awd { front: f } => if front { f } else { 1.0 - f },
        }
    }
    fn label(self) -> &'static str {
        match self {
            DriveLayout::Fwd => "FWD",
            DriveLayout::Rwd => "RWD",
            DriveLayout::Awd { .. } => "AWD",
        }
    }
}

// Diferenciál mezi dvěma koly osy. `lock` 0 = otevřený (kola různě rychlá v zatáčce),
// 1 = uzamčený (stejná rychlost → trakce), mezi = samosvorný (LSD). Data i runtime
// (uzamykatelný diff = za běhu se `lock` přepne). Solver podle něj sváže rychlosti kol.
struct Differential {
    lock: f32, // 0..1
}

#[derive(Resource)]
struct Drivetrain {
    engine: Engine,         // namontovaný motor
    gearbox: Gearbox,       // namontovaná převodovka
    layout: DriveLayout,    // FWD/RWD/AWD (data, přepínatelné)
    front_diff: Differential, // diff přední osy
    rear_diff: Differential,  // diff zadní osy
    gear: i32,              // runtime: -1 = zpátečka, 0 = neutrál, 1..=N = kvalty
    rpm: f32,               // runtime: dopočítané otáčky (pro HUD i moment)
    speed_kmh: f32,         // runtime: dopočítaná rychlost (pro HUD)
}

impl Default for Drivetrain {
    fn default() -> Self {
        Self {
            engine: Engine {
                // Hrubá benzínová křivka ~1.4 (vrchol kolem 3500–5000). Doladí se s motorem.
                torque_curve: vec![(850.0, 90.0), (2000.0, 140.0), (3500.0, 175.0), (5000.0, 180.0), (6500.0, 150.0)],
                max_rpm: 6500.0,
                idle_rpm: 850.0,
            },
            gearbox: Gearbox { ratios: vec![3.5, 2.1, 1.4, 1.0, 0.8], final_drive: 3.9 },
            layout: DriveLayout::Rwd,                 // start: zadní pohon
            front_diff: Differential { lock: 0.0 },   // start: otevřené
            rear_diff: Differential { lock: 0.0 },
            gear: 1,
            rpm: 0.0,
            speed_kmh: 0.0,
        }
    }
}

// Marker textu HUDu (rychlost/kvalt/otáčky) — podle něj ho update systém najde a přepíše.
#[derive(Component)]
struct HudText;

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

// Klávesové zkratky: 1 = rigid auto, 2 = test.veh, 3 = fabia.veh (pokud existuje).
fn spawn_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    spawned: Query<Entity, With<Spawned>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut softbody: ResMut<SoftBody>,
    build: Res<CarBuild>,
) {
    // Clear = Shift+C (samotné C je obsazené kamerou).
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if shift && keys.just_pressed(KeyCode::KeyC) {
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

// Postaví zavěšení = beamy mezi hubem kola a tělem. JAK je propojí, závisí na typu.
// Solver o typu neví — tady se „vlévá" celá variabilita podvozku (custom suspension).
fn build_suspension(sb: &mut SoftBody, hub: usize, anchors: &[usize], kind: SuspensionKind) {
    match kind {
        // Rigid = hub tuze připojen ke všem předaným úchytům → kolo drží napevno (žádný zdvih).
        SuspensionKind::Rigid => {
            let k = 150_000.0;
            for &a in anchors {
                sb.add_beam(hub, a, k);
            }
        }
        // Coil čeká anchors = [příčné rameno, podélné rameno, horní úchyt pružiny]
        // (výběr rohů dělá volající — viz spawn_solver_car). Tlumení dopočítá add_beam
        // z DAMPING_RATIO: měkčí k → úměrně menší d, poměrově tlumené stejně.
        SuspensionKind::Coil { arm_k, coil_k } => {
            sb.add_beam(hub, anchors[0], arm_k);
            sb.add_beam(hub, anchors[1], arm_k);
            sb.add_beam(hub, anchors[2], coil_k);
            // Tlumič sedí na pružině: zesil d posledního beamu (builder-side, solver netknutý).
            sb.beams.last_mut().unwrap().d *= COIL_DAMP_MULT;
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
    build: &CarBuild,
) {
    *sb = SoftBody::default(); // nový spawn = čistý solver
    // Spawn z 2 m = drop test zavěšení při každém spawnu (Coil ho musí ustát:
    // tužší pružina + tlumič, viz SUSP_COIL_K / COIL_DAMP_MULT).
    let origin = Vec3::new(0.0, 2.0, 0.0);

    let node_mesh = meshes.add(Sphere::new(0.06));
    let node_material = materials.add(Color::srgb(0.9, 0.3, 0.2));

    // --- Tělo: 8 rohů kvádru, PLNĚ provázané tuhými beamy (rám se nedá zkroutit) ---
    let (hx, hy, hz) = (0.7, 0.25, 1.8);
    let body_mass = 120.0; // na jeden roh (8× = 960 kg karoserie — realistický kvádr ~auto)
    let chassis_k = 300_000.0;
    let mut body: Vec<usize> = Vec::with_capacity(8);
    // Klasifikace rohů pro směrový rám: front = +z, right = +x (viz veh_pos: forward auta = +Z).
    let (mut front, mut rear, mut left, mut right) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
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
                let idx = sb.add_node(p, body_mass, e);
                if sz > 0.0 { front.push(idx) } else { rear.push(idx) }
                if sx > 0.0 { right.push(idx) } else { left.push(idx) }
                body.push(idx);
            }
        }
    }
    sb.chassis = Some(ChassisFrame { front, rear, left, right });
    // plný graf přes 8 rohů = 28 beamů → tuhý rám
    for a in 0..body.len() {
        for b in (a + 1)..body.len() {
            sb.add_beam(body[a], body[b], chassis_k);
        }
    }

    // --- 4 kola: hub-node (zavěšený na tělo) + render disk ---
    let wheel_radius = 0.34;
    let wheel_width = 0.22;
    let hub_mass = 35.0; // 4× = 140 kg (kolo+hub+brzda) → celkem ~1100 kg
    let wheel_mesh = meshes.add(Cylinder::new(wheel_radius, wheel_width));
    let wheel_material = materials.add(Color::srgb(0.05, 0.05, 0.05));
    // Sdílená značka u ráfku — kontrastní kostička, ať je vidět otáčení kola.
    let marker_mesh = meshes.add(Cuboid::new(0.08, 0.08, 0.08));
    let marker_material = materials.add(Color::srgb(1.0, 0.9, 0.1));
    let (wx, wy, wz) = (0.85, -0.15, 1.4);
    // (sx, sz, steered) — front/řízení; KTERÁ kola táhnou rozhoduje DriveLayout, ne builder.
    let wheel_layout = [
        (-1.0_f32, 1.0_f32, true),  // FL: přední, řízené
        (1.0, 1.0, true),           // FR: přední, řízené
        (-1.0, -1.0, false),        // RL: zadní
        (1.0, -1.0, false),         // RR: zadní
    ];
    for (sx, sz, steered) in wheel_layout {
        let front = sz > 0.0; // přední osa = +z
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
        // Úchyty Coil z rohů rámu. Index rohu = 4·(sx>0) + 2·(horní) + 1·(sz>0), dané
        // pořadím stavby těla výš. Ramena do SPODNÍCH rohů (y=1.75 ≈ výška hubu → skoro
        // vodorovná), pružina do horního rohu nad kolem (skoro svislá).
        let xi = if sx > 0.0 { 4 } else { 0 };
        let zi = if sz > 0.0 { 1 } else { 0 };
        let anchors = [
            body[(4 - xi) + zi], // spodní roh PROTĚJŠÍ strany, stejné z → příčné rameno (drží bok)
            body[xi + 1 - zi],   // spodní roh stejné strany, OPAČNÉ z → podélné rameno (drží tah/brzdu)
            body[xi + 2 + zi],   // horní roh stejné strany, stejné z → pružina (nese váhu)
        ];
        // Rigid potřebuje VŠECH 8 nodů (šikmé beamy do všech směrů = tuhý zámek);
        // se 3 skoro-vodorovnými úchyty by svisle skoro nedržel a houpal se.
        let rigid_anchors: &[usize] = match build.suspension {
            SuspensionKind::Rigid => &body,
            _ => &anchors,
        };
        build_suspension(sb, hub, rigid_anchors, build.suspension);

        let render = commands
            .spawn((
                Mesh3d(wheel_mesh.clone()),
                MeshMaterial3d(wheel_material.clone()),
                Transform::from_translation(hub_pos),
                Spawned,
            ))
            .with_children(|w| {
                // Značka kouká kousek ZA běhoun (z > radius), ať není zabořená ve válci.
                // Obíhá se spinem → vidíš otáčení. y = 0: jinak by seděla na ose (X) a netočila se.
                w.spawn((
                    Mesh3d(marker_mesh.clone()),
                    MeshMaterial3d(marker_material.clone()),
                    Transform::from_xyz(0.0, 0.0, wheel_radius + 0.03),
                ));
            })
            .id();
        sb.wheels.push(WheelC {
            hub,
            render,
            radius: wheel_radius,
            spin: 0.0,
            spin_vel: 0.0,
            steer: 0.0,
            drive_torque: 0.0,
            brake: 0.0,
            front,
            steered,
            tire: TireParams { normal_stiffness: 120_000.0, normal_damping: 4_000.0, long: TIRE_LONG, lat: TIRE_LAT },
        });
    }

    // Viditelný trup: kvádr velikosti rámu. Transform mu přepisuje softbody_to_transforms.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(2.0 * hx, 2.0 * hy, 2.0 * hz))),
        MeshMaterial3d(materials.add(Color::srgb(0.6, 0.6, 0.7))),
        Transform::from_translation(origin),
        CarBody,
        Spawned,
    ));

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

    // --- A1+A2: KOLA z .veh ---
    // `.veh` má `wheels: [{name, node1, node2, dir}]`, kde node1/node2 = AXLE nody kola.
    // POZOR: v BeamNG mají hub-nody jen LOKÁLNÍ souřadnice kola (všechny ~[x,0,0]), takže
    // jejich střed NENÍ skutečná poloha kola (přední i zadní by splynuly). Reálnou pozici
    // dostaneme z jejich beam-SOUSEDŮ (= suspension nody fh*/rh*…), co globální pozice mají.
    // Tytéž sousedy = úchyty zavěšení (build_suspension), takže hub na nich rovnou visí.
    let wheel_radius = 0.34;
    let wheel_width = 0.20;
    let hub_mass = 35.0;
    let wheel_mesh = meshes.add(Cylinder::new(wheel_radius, wheel_width));
    let wheel_material = materials.add(Color::srgb(0.05, 0.05, 0.05));
    let marker_mesh = meshes.add(Cuboid::new(0.08, 0.08, 0.08));
    let marker_material = materials.add(Color::srgb(1.0, 0.9, 0.1));

    // Sousedství přes beamy (index nodu → indexy spojené beamem). sb.pos teď = jen nody .veh.
    let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); sb.pos.len()];
    for beam in &vehicle.beams {
        if let (Some(&(_, i)), Some(&(_, j))) =
            (node_idx.get(beam.a.as_str()), node_idx.get(beam.b.as_str()))
        {
            neighbors[i].push(j);
            neighbors[j].push(i);
        }
    }

    // 1. průchod: pro každé kolo poskládej úchyty (sousedi axle nodů, bez nich samých)
    // a z jejich průměru spočti REÁLNOU pozici hubu.
    let mut wheels: Vec<(Vec<usize>, Vec3)> = Vec::new(); // (úchyty zavěšení, hub_pos)
    for wheel in &vehicle.wheels {
        let (Some(&(_, n1)), Some(&(_, n2))) =
            (node_idx.get(wheel.node1.as_str()), node_idx.get(wheel.node2.as_str()))
        else {
            continue; // kolo odkazuje na neexistující nod → přeskoč
        };
        let mut anchors: Vec<usize> = Vec::new();
        for &axle in &[n1, n2] {
            for &nb in &neighbors[axle] {
                if nb != n1 && nb != n2 && !anchors.contains(&nb) {
                    anchors.push(nb);
                }
            }
        }
        if anchors.is_empty() {
            continue; // bez úchytů neumíme kolo umístit ani zavěsit
        }
        let hub_pos = anchors.iter().map(|&i| sb.pos[i]).sum::<Vec3>() / anchors.len() as f32;
        wheels.push((anchors, hub_pos));
    }
    // Průměrné z hubů → hranice mezi přední (větší z) a zadní (menší z) osou.
    let mean_z = wheels.iter().map(|(_, p)| p.z).sum::<f32>() / wheels.len().max(1) as f32;

    // 2. průchod: hub-node + render disk + zavěšení (Rigid) + WheelC.
    let mut hub_idxs: Vec<usize> = Vec::new(); // pro tuhý rám mezi koly (níž)
    for (anchors, hub_pos) in &wheels {
        let mut hub_pos = *hub_pos;
        // .veh dá kola moc inboard (hub = průměr zatažených suspension nodů). Vytlač je ven
        // na rozumný rozchod, ať nejsou zastrčená pod karoserií. Rest-length beamů zavěšení
        // (build_suspension níž) tuhle polohu zafixuje, i když anchory zůstanou inboard.
        hub_pos.x = hub_pos.x.signum() * 0.80;
        // Posaď kolo NÍŽ pod suspension nody = světlá výška. Dno Fabie je nízko (y≈0, hub≈0.32);
        // s realistickým kolem by dřelo o zem (tření přilepí auto). Spuštěním hubu se karoserie
        // zvedne nad zem. Rest-length beamů zavěšení (build_suspension) tuhle polohu zafixuje.
        hub_pos.y -= 0.14;
        let front = hub_pos.z > mean_z; // přední osa = +z (přední kola Fabie vyšla na +z)
        let render = commands
            .spawn((
                Mesh3d(wheel_mesh.clone()),
                MeshMaterial3d(wheel_material.clone()),
                Transform::from_translation(hub_pos),
                Spawned,
            ))
            .with_children(|w| {
                w.spawn((
                    Mesh3d(marker_mesh.clone()),
                    MeshMaterial3d(marker_material.clone()),
                    Transform::from_xyz(0.0, 0.0, wheel_radius + 0.03),
                ));
            })
            .id();
        // Hub = nový node; jeho render entita = disk kola (softbody_to_transforms mu
        // dá pozici i rotaci, takže žádná zvlášť debug kulička není potřeba).
        let hub = sb.add_node(hub_pos, hub_mass, render);
        build_suspension(sb, hub, anchors, SuspensionKind::Rigid); // přitáhni hub na suspension nody
        hub_idxs.push(hub);
        sb.wheels.push(WheelC {
            hub,
            render,
            radius: wheel_radius,
            spin: 0.0,
            spin_vel: 0.0,
            steer: 0.0,
            drive_torque: 0.0,
            brake: 0.0,
            front,
            steered: front, // přední = řízená (A4 to případně doladí)
            tire: TireParams { normal_stiffness: 120_000.0, normal_damping: 4_000.0, long: TIRE_LONG, lat: TIRE_LAT },
        });
    }

    // Tuhý rám mezi koly (jako plně provázaný rám test auta): drží karoserii, ať se
    // neprověsí mezi nápravami až na zem, a přenáší hnací sílu jako CELEK (ne jen lokální
    // deformace u hnaných kol). 4 huby → 6 beamů, tuhé jako rám test auta.
    for a in 0..hub_idxs.len() {
        for b in (a + 1)..hub_idxs.len() {
            sb.add_beam(hub_idxs[a], hub_idxs[b], 300_000.0);
        }
    }

    // --- A3: ChassisFrame z nodů Fabie (směr auta pro solver/kameru/render) ---
    // Klasifikace jako u test auta: front = +z, right = +x (handedness, ať up = fwd×right = +y).
    // POZOR: pravá kola Fabie jsou fyzicky na −x → „right" rám může vyjít zrcadlově. Když A/D
    // zatáčí na opačnou stranu, otoč znaménko steeru (nebo zde prohoď left↔right).
    let body_count = vehicle.nodes.len(); // nody Fabie = indexy 0..body_count (huby jsou až za nimi)
    let center = (0..body_count).map(|i| sb.pos[i]).sum::<Vec3>() / body_count as f32;
    let (mut front, mut rear, mut left, mut right) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for i in 0..body_count {
        if sb.pos[i].z > center.z { front.push(i) } else { rear.push(i) }
        if sb.pos[i].x > center.x { right.push(i) } else { left.push(i) }
    }
    sb.chassis = Some(ChassisFrame { front, rear, left, right });

    // POŘADÍ: kola/zavěšení přidávají beamy (suspension), proto až TEĎ počítáme substepy.
    sb.compute_substeps();
    info!(
        "Soft-body: {} kol, {} substepů (~{} Hz solver)",
        sb.wheels.len(),
        sb.substeps,
        sb.substeps * 64
    );

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
            NoFrustumCulling, // mesh se deformuje (vrcholy se mění, Transform pevný) → culling ho jinak občas schová
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

// Magic Formula (Pacejka): skluz `x` → síla. `d` = špička (μ·Fz). Křivka roste ke špičce
// a ZA ní zase klesá — to je ten realismus oproti lineárnímu gripu (přebrzdění, smyk).
fn magic_formula(x: f32, p: Pacejka, d: f32) -> f32 {
    let bx = p.b * x;
    d * (p.c * (bx - p.e * (bx - bx.atan())).atan()).sin()
}

// --- SOLVER: semi-implicitní Euler se substeppingem (běží ve FixedUpdate, 64 Hz) ---
// Jeden snímek rozdělíme na `sb.substeps` malých kroků (adaptivně, viz compute_substeps),
// aby tuhé beamy zůstaly stabilní (CFL: dt < 2/ω). Reálné tuhosti, žádné škálování.
fn softbody_solve(sb: ResMut<SoftBody>, dt: Res<Drivetrain>, time: Res<Time<Fixed>>) {
    if sb.beams.is_empty() {
        return; // nic nespawnuto
    }
    let sb = sb.into_inner(); // rozbalíme ResMut → můžeme půjčovat víc polí najednou
    let gravity = Vec3::new(0.0, -9.81, 0.0);
    let ground_y = GROUND_TOP + 0.04; // horní hrana zemního kvádru + poloměr nodu
    let sub_dt = time.delta_secs() / sb.substeps as f32;
    // Tangenciální tření země je per-substep násobení rychlosti. To se ALE umocňuje počtem
    // substepů: 0.9^4 (test auto) je mírné, ale 0.9^382 (Fabia) = ~0 → každý dotyk přilepí.
    // Přepočítáme faktor tak, aby výsledek za SNÍMEK byl stejný (~0.7) bez ohledu na substepy.
    let tang_keep = 0.7_f32.powf(1.0 / sb.substeps as f32);
    let (front_lock, rear_lock) = (dt.front_diff.lock, dt.rear_diff.lock); // diff osy

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

        // 2.5) směrový rám auta (yaw): dopředný a boční jednotkový vektor z referenčních nodů.
        // Počítá se jednou za substep (před koly). U .veh (chassis None) zůstane nula → kola nejedou.
        let (fwd, right) = match &sb.chassis {
            Some(cf) => {
                let mean = |idx: &[usize]| idx.iter().map(|&i| sb.pos[i]).sum::<Vec3>() / idx.len() as f32;
                let mut f = mean(&cf.front) - mean(&cf.rear);
                let mut r = mean(&cf.right) - mean(&cf.left);
                f.y = 0.0; // bereme jen vodorovný směr (náklon neřeší tire model)
                r.y = 0.0;
                (f.normalize_or_zero(), r.normalize_or_zero())
            }
            None => (Vec3::ZERO, Vec3::ZERO),
        };

        // 2.6) TIRE MODEL na každém kole: svislá (kruh o radius) + boční/podélný grip + spin.
        // Indexový loop + kopie skalárů — ať můžeme zapsat do sb.force[hub]/sb.wheels bez aliasingu.
        for wi in 0..sb.wheels.len() {
            let hub = sb.wheels[wi].hub;
            let radius = sb.wheels[wi].radius;
            let ns = sb.wheels[wi].tire.normal_stiffness;
            let nd = sb.wheels[wi].tire.normal_damping;
            let long = sb.wheels[wi].tire.long;
            let lat = sb.wheels[wi].tire.lat;
            let steer = sb.wheels[wi].steer;
            let m_wheel = 1.0 / sb.inv_mass[hub];
            let i_wheel = 0.5 * m_wheel * radius * radius; // moment setrvačnosti disku

            // -- svislá síla (Fz) + vodorovná reakce pneumatiky, jen když se kolo dotýká --
            let pen = (GROUND_TOP + radius) - sb.pos[hub].y; // >0 = pneumatika stlačená
            let mut f_long = 0.0; // reakce zpět na spin (mimo zem = 0)
            if pen > 0.0 {
                let fy = ns * pen - nd * sb.vel[hub].y; // pružina nahoru − tlumení
                if fy > 0.0 {
                    sb.force[hub].y += fy;

                    // směr kola po natočení řízení (kolem svislé osy Y)
                    let turn = Quat::from_rotation_y(steer);
                    let wf = turn * fwd; // dopředný směr kola
                    let wl = turn * right; // boční směr kola

                    let v = sb.vel[hub];
                    let v_long = v.dot(wf); // jak rychle se kontakt veze dopředu
                    let v_lat = v.dot(wl); // boční skluz
                    let roll = sb.wheels[wi].spin_vel * radius; // obvodová rychlost kola

                    // Normalizace skluzu pro Pacejku (v_ref s dolní mezí = ochrana proti dělení 0).
                    let v_ref = v_long.abs().max(V_LOW);
                    let kappa = (roll - v_long) / v_ref; // slip ratio (podélně)
                    let alpha = v_lat.atan2(v_ref); // slip angle v rad (bočně)

                    let d = MU * fy; // špička křivky = μ·Fz (víc guma neudrží)
                    let mut fl = magic_formula(kappa, long, d); // podélná síla (tah/brzda)
                    let mut fc = magic_formula(-alpha, lat, d); // boční síla (brzdí smyk)

                    // kruh adheze jako pojistka na kombinovaný skluz: |(podélná, boční)| ≤ MU·Fz
                    let fmax = MU * fy;
                    let mag = (fl * fl + fc * fc).sqrt();
                    if mag > fmax {
                        let s = fmax / mag;
                        fl *= s;
                        fc *= s;
                    }
                    sb.force[hub] += fl * wf + fc * wl;
                    f_long = fl;
                }
            }

            // -- dynamika otáčení kola: motor + brzda − reakce od podélné síly --
            let spin_vel = sb.wheels[wi].spin_vel;
            let brake_torque = -spin_vel.signum() * sb.wheels[wi].brake; // proti směru otáčení
            let net = sb.wheels[wi].drive_torque + brake_torque - f_long * radius;
            let new_spin_vel = spin_vel + net / i_wheel * sub_dt;
            sb.wheels[wi].spin_vel = new_spin_vel;
            sb.wheels[wi].spin += new_spin_vel * sub_dt; // úhel pro render
        }

        // 2.7) DIFERENCIÁL: sváž rychlosti kol stejné osy k jejich průměru podle `lock`.
        // 0 = otevřený (kola volně různě rychlá → zatáčí, ale jedno protáčí), 1 = uzamčený
        // (stejná rychlost → trakce, ale škube v zatáčce), mezi = samosvorný (LSD).
        for (is_front, lock) in [(true, front_lock), (false, rear_lock)] {
            if lock <= 0.0 {
                continue; // otevřený diff = nic nesvazujeme
            }
            let (mut sum, mut c) = (0.0, 0.0);
            for w in &sb.wheels {
                if w.front == is_front {
                    sum += w.spin_vel;
                    c += 1.0;
                }
            }
            if c > 0.0 {
                let mean = sum / c;
                for w in &mut sb.wheels {
                    if w.front == is_front {
                        w.spin_vel += (mean - w.spin_vel) * lock; // lerp k průměru
                    }
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
                sb.vel[i].x *= tang_keep; // hrubé tření (substep-nezávislé), než přijde pořádný model
                sb.vel[i].z *= tang_keep;
            }
        }
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

    // Kola → disky: pozice = hub, rotace = orientace auta · řízení(Y) · valení(X) · base(válec Y→osa X).
    let base = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    for wheel in &sb.wheels {
        if let Ok(mut tf) = transforms.get_mut(wheel.render) {
            tf.translation = sb.pos[wheel.hub];
            tf.rotation =
                car_rot * Quat::from_rotation_y(wheel.steer) * Quat::from_rotation_x(wheel.spin) * base;
        }
    }
}

// Moment motoru při daných otáčkách — lineární interpolace momentové křivky.
// Nad max_rpm = omezovač (moment 0 → kolo se nemůže protáčet do nekonečna).
fn engine_torque(rpm: f32, engine: &Engine) -> f32 {
    if rpm >= engine.max_rpm {
        return 0.0; // rev limiter
    }
    let c = &engine.torque_curve;
    if rpm <= c[0].0 {
        return c[0].1; // pod prvním bodem → držíme první hodnotu
    }
    for w in c.windows(2) {
        let (r0, t0) = w[0];
        let (r1, t1) = w[1];
        if rpm <= r1 {
            let f = (rpm - r0) / (r1 - r0); // 0..1 mezi body
            return t0 + f * (t1 - t0);
        }
    }
    c.last().unwrap().1 // nad poslední bod → držíme poslední hodnotu
}

// Pohon auta. W = plyn, S = brzda, Space = ruční brzda; zpátečka = zařadit R (Q) a dát plyn.
// Nepočítá fyziku — jen zapíše ZÁMĚR (moment na kola) z motoru přes převody; sílu řeší solver.
fn vehicle_drive(keys: Res<ButtonInput<KeyCode>>, dt: Res<Drivetrain>, mut sb: ResMut<SoftBody>) {
    let throttle = if keys.pressed(KeyCode::KeyW) { 1.0 } else { 0.0 };
    let brake = if keys.pressed(KeyCode::KeyS) { 800.0 } else { 0.0 }
        + if keys.pressed(KeyCode::Space) { 1500.0 } else { 0.0 }; // ruční brzda silnější

    // Převod aktuálního kvaltu (R = opačný směr). Neutrál → 0 (motor točí, kola ne).
    let ratio = match dt.gear {
        0 => 0.0,
        -1 => -dt.gearbox.ratios[0], // zpátečka: opačný smysl než 1. kvalt
        g => dt.gearbox.ratios[(g - 1) as usize],
    };
    // Celkový moment z motoru: moment × plyn → přes převod a stálý převod.
    let total = engine_torque(dt.rpm, &dt.engine) * throttle * ratio * dt.gearbox.final_drive;

    // Rozdělení podle DriveLayoutu: každá osa dostane svůj podíl, ten se rozpočítá mezi
    // její kola. Diferenciál (sváz rychlostí) řeší solver dle Drivetrain.{front,rear}_diff.
    let front_n = sb.wheels.iter().filter(|w| w.front).count().max(1) as f32;
    let rear_n = sb.wheels.iter().filter(|w| !w.front).count().max(1) as f32;

    for wheel in &mut sb.wheels {
        let axle = dt.layout.axle_fraction(wheel.front); // podíl pro osu tohoto kola
        let n = if wheel.front { front_n } else { rear_n };
        wheel.drive_torque = total * axle / n;
        wheel.brake = brake; // brzdí všechna kola
    }
}

// Řízení: A/D natáčí přední kola kolem svislé osy. Plynule lerpne k cíli (ne skokem),
// ať řízení nešvihne. Solver z `wheel.steer` odvodí směr pneumatiky (sekce 2.6).
fn vehicle_steer(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut sb: ResMut<SoftBody>) {
    let max_angle = 0.5; // ~28°
    let mut target = 0.0;
    if keys.pressed(KeyCode::KeyA) {
        target = max_angle; // doleva
    } else if keys.pressed(KeyCode::KeyD) {
        target = -max_angle; // doprava
    }
    let rate = 4.0 * time.delta_secs(); // max přírůstek úhlu za snímek

    for wheel in &mut sb.wheels {
        if wheel.steered {
            wheel.steer += (target - wheel.steer).clamp(-rate, rate);
        }
    }
}

// Ovládání drivetrainu: E/Q = řazení, L = zámek diffu, K = přepnout pohon (FWD/RWD/AWD).
// Počet kvaltů bere z namontované převodovky (díl), ne z konstanty.
fn shift_gears(keys: Res<ButtonInput<KeyCode>>, mut dt: ResMut<Drivetrain>) {
    let top = dt.gearbox.ratios.len() as i32;
    if keys.just_pressed(KeyCode::KeyE) {
        dt.gear = (dt.gear + 1).min(top); // nahoru, max nejvyšší kvalt
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        dt.gear = (dt.gear - 1).max(-1); // dolů, min zpátečka
    }
    // L = uzamykatelný diff: přepne lock obou os mezi 0 (otevřený) a 1 (uzamčený).
    if keys.just_pressed(KeyCode::KeyL) {
        let new = if dt.rear_diff.lock > 0.5 { 0.0 } else { 1.0 };
        dt.front_diff.lock = new;
        dt.rear_diff.lock = new;
    }
    // K = přepnout rozvod pohonu RWD → FWD → AWD(50:50) → …
    if keys.just_pressed(KeyCode::KeyK) {
        dt.layout = match dt.layout {
            DriveLayout::Rwd => DriveLayout::Fwd,
            DriveLayout::Fwd => DriveLayout::Awd { front: 0.5 },
            DriveLayout::Awd { .. } => DriveLayout::Rwd,
        };
    }
}

// Dopočítá rychlost a otáčky z aktuálního stavu solveru (pro HUD).
// Rychlost = vodorovná rychlost těžiště rámu; RPM = otáčky hnaných kol × převod × stálý.
fn update_drivetrain(sb: Res<SoftBody>, mut dt: ResMut<Drivetrain>) {
    let idle = dt.engine.idle_rpm; // kopie předem (čteme díl, pak píšeme runtime pole)
    let layout = dt.layout;
    let Some(cf) = &sb.chassis else {
        dt.speed_kmh = 0.0;
        dt.rpm = idle;
        return;
    };

    // Rychlost: průměr rychlostí rohových nodů, jen vodorovná složka (m/s → km/h).
    let nodes = cf.front.iter().chain(&cf.rear);
    let n = (cf.front.len() + cf.rear.len()).max(1) as f32;
    let mut v: Vec3 = nodes.map(|&i| sb.vel[i]).sum::<Vec3>() / n;
    v.y = 0.0;
    dt.speed_kmh = v.length() * 3.6;

    // Otáčky kola: průměr |spin_vel| hnaných kol (dle layoutu) (rad/s).
    let mut omega = 0.0;
    let mut cnt = 0.0;
    for w in &sb.wheels {
        if layout.axle_fraction(w.front) > 0.0 {
            omega += w.spin_vel.abs();
            cnt += 1.0;
        }
    }
    if cnt > 0.0 {
        omega /= cnt;
    }

    // RPM = ω_kola × |převod| × stálý × (60/2π). Neutrál → jen volnoběh.
    let final_drive = dt.gearbox.final_drive;
    let ratio = match dt.gear {
        0 => 0.0,                                       // neutrál: kola nespojená s motorem
        -1 => dt.gearbox.ratios[0],                     // zpátečka: zatím poměr jako 1. kvalt
        g => dt.gearbox.ratios[(g - 1) as usize],       // 1..=N
    };
    let rpm = omega * ratio * final_drive * (60.0 / std::f32::consts::TAU);
    dt.rpm = rpm.max(idle); // pod volnoběh nikdy
}

// Přepíše HUD text aktuálními hodnotami z Drivetrain.
fn update_hud(dt: Res<Drivetrain>, mut hud: Query<&mut Text, With<HudText>>) {
    let gear = match dt.gear {
        -1 => "R".to_string(),
        0 => "N".to_string(),
        g => g.to_string(),
    };
    let diff = if dt.rear_diff.lock > 0.5 { "ZAMČENÝ" } else { "otevřený" };
    if let Ok(mut text) = hud.get_single_mut() {
        text.0 = format!(
            "{:>3.0} km/h\nKvalt {}\n{:>5.0} ot/min\n{}  diff: {}",
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
