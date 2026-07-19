// vehicle.rs — všechno „auto": typy zavěšení, drivetrain (motor/převodovka/spojka),
// buildery (spawn test auta i .veh), ovládání (plyn/brzda/řízení/řazení).
// Princip: variabilita (díly, tuning, geometrie zavěšení) žije TADY v builderech,
// solver (solver.rs) je jen hloupý integrátor nad indexy.

use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::view::NoFrustumCulling;

use crate::solver::*;
use crate::veh;

// Marker na všech naspawnovaných herních entitách (chassis, kola, nody) — podle něj Clear maže.
#[derive(Component)]
pub struct Spawned;

// Viditelný trup auta (kvádr). Nemá vlastní fyziku — jeho Transform každý snímek
// přepíšeme z orientace rámu (chassis), ať „sedí" na uzlech soft-body.
#[derive(Component)]
pub struct CarBody;

// Povrchový mesh soft-body. Drží pro každý vrchol odpovídající node entitu,
// aby update systém mohl vrcholy posouvat za pohybem nodů (deformace).
#[derive(Component)]
pub struct SoftbodyMesh {
    pub nodes: Vec<Entity>, // vertex i ↔ node entita
}

// --- LADICÍ HODNOTY ZAVĚŠENÍ (Coil) ---
// Ramena tuhá jako Rigid (drží kolo v ose), pružina měkčí (svislý zdvih). Sag na roh:
// ~2350 N / (coil_k·cos²θ ≈ 0.47·k) → při 100k ≈ 5 cm. Zdvih končí ~40 cm níž, kde se
// pružina překlopí do vodorovna (snap-through) — kapacita ½·k_v·0.4² musí přežít dopad
// z 2 m (~3.8 kJ na roh); při 100k + tlumiči (COIL_DAMP_MULT) vychází s rezervou.
pub const SUSP_ARM_K: f32 = 150_000.0;
pub const SUSP_COIL_K: f32 = 100_000.0;
// Násobek tlumení coil beamu: globální DAMPING_RATIO 0.25 je proti zvonění rámu,
// skutečný tlumič zavěšení má ζ ≈ 0.6–0.9 — dopad i kolébání má sežrat tlumič.
pub const COIL_DAMP_MULT: f32 = 3.5;
// Dorazy zdvihu pružiny: ±STOP_TRAVEL od klidové délky, pak nastoupí dorazová pružina.
// Hodnoty dle BeamNG (shortBoundRange ~0.065 m, beamLimitSpring ~200k).
pub const STOP_TRAVEL: f32 = 0.06;
pub const STOP_K: f32 = 200_000.0;
// Rebound tlumení = REBOUND_MULT × bump (jbeam Fabia: 7500/3500 ≈ 2.1). Tužší rebound
// drží kolo na zemi po nerovnosti — auto se nehoupe („neplave").
pub const REBOUND_MULT: f32 = 2.0;
// Stabilizátor (V-nod nad osou): roll napíná ramena V (svislá složka ~0.45 → roll
// tuhost ~0.2·k na kolo), bounce V-nod jen sveze s koly → žádný odpor. Viz build_antiroll.
pub const ARB_K: f32 = 100_000.0;
// --- McPherson ---
// Vzpěra jde z hubu na VĚŽ (nový nod nad hubem, tuze svázaný s rámem) → ~11° od svislice,
// skoro celé k nese váhu (cos²θ ≈ 0.96) → stačí měkčí pružina než Coil (jehož pružina do
// rohu rámu má jen ~0.55·k svisle). Sag: 2350 N / (0.96·80k) ≈ 3 cm.
pub const SUSP_STRUT_K: f32 = 80_000.0;
pub const STRUT_TOWER_K: f32 = 200_000.0; // věž = pevná část rámu (4 tuhé beamy do rohů)
pub const STRUT_TOWER_UP: f32 = 0.75; // věž nad hubem (délka vzpěry = rezerva do překlopení)
pub const STRUT_TOWER_IN: f32 = 0.15; // věž kousek dovnitř → vzpěra šikmá jako v reálu
// --- Double wishbone ---
// Kolo = hub + TOP nod (těhlice). Dolní ramena na hub (dlouhá, do protějších rohů),
// horní A-rameno na top (krátké, do rohů STEJNÉ strany) = SLA geometrie: při propružení
// opisuje top menší oblouk než hub → kolo mění odklon (camber). Orientaci disku dává
// vektor hub→top (softbody_to_transforms), solver o camberu nic neví.
pub const KNUCKLE_H: f32 = 0.55; // top nod nad hubem — vyjde ve výšce horních rohů (ramena ~vodorovná)
pub const KNUCKLE_K: f32 = 300_000.0; // těhlice hub↔top = tuhá ocel jako rám (definuje rovinu kola)
pub const SUSP_DW_SPRING_K: f32 = 100_000.0; // pružina šikmo do rohu jako Coil (~0.55·k svisle)

// Typ zavěšení = JAK builder propojí hub kola s tělem (topologie + tuhost beamů).
// Solver o typu neví.
#[derive(Clone, Copy, PartialEq)]
pub enum SuspensionKind {
    Rigid, // hub připojen tuhými beamy přímo k nejbližším nodům těla
    // Svislý zdvih trikem geometrie: 2 tuhá SKORO VODOROVNÁ ramena drží kolo podélně
    // i příčně (svisle skoro nekladou odpor — délka se mění jen ~Δ²/2L), 1 měkčí SKORO
    // SVISLÁ pružina nese váhu a pruží. Klidová délka = spawn vzdálenost → auto si po
    // spawnu sedne o sag ≈ váha/k_svislá níž.
    Coil { arm_k: f32, coil_k: f32 },
    // Jako Coil, ale pružina+tlumič = VZPĚRA na věž nad kolem (~svislá, viz STRUT_TOWER_*).
    // Věž staví builder v spawn_solver_car; build_suspension jen dostane její index v anchors[2].
    McPherson { arm_k: f32, strut_k: f32 },
    // Dvě roviny ramen + těhlice (top nod nad hubem): dolní ramena na hub, horní na top,
    // pružina na hub. Kolo má orientaci z hub→top = skutečný camber (viz KNUCKLE_*).
    DoubleWishbone { arm_k: f32, spring_k: f32 },
    // LeafSpring { ... } — časem
}

impl SuspensionKind {
    // Presety pro build menu (výběr „dílu" bez ladicích čísel v UI).
    pub const RIGID: Self = Self::Rigid;
    pub const COIL: Self = Self::Coil { arm_k: SUSP_ARM_K, coil_k: SUSP_COIL_K };
    pub const MCPHERSON: Self = Self::McPherson { arm_k: SUSP_ARM_K, strut_k: SUSP_STRUT_K };
    pub const DOUBLE_WISHBONE: Self = Self::DoubleWishbone { arm_k: SUSP_ARM_K, spring_k: SUSP_DW_SPRING_K };
    pub fn label(self) -> &'static str {
        match self {
            Self::Rigid => "Rigid (tuhé)",
            Self::Coil { .. } => "Coil (zdvih)",
            Self::McPherson { .. } => "McPherson (vzpěra)",
            Self::DoubleWishbone { .. } => "Double wishbone (camber)",
        }
    }
}

// Vybrané auto: vestavěný testovací vůz, nebo n-tý .veh soubor z assets
// (index do seznamu available_vehs — dnes jen fabia).
#[derive(Clone, Copy, PartialEq)]
pub enum CarChoice {
    TestCar,
    Veh(usize),
}

// Build config = jaké DÍLY auto má. Garáž píše sem, spawn čte → výměna dílu
// bez zásahu do solveru (stejný vzor jako Drivetrain: díl = data, resource drží volbu).
// Zatím auto + zavěšení; motor/převodovka žijí v Drivetrain, časem se sem sloučí.
#[derive(Resource)]
pub struct CarBuild {
    pub car: CarChoice,
    pub suspension: SuspensionKind,
}

impl Default for CarBuild {
    fn default() -> Self {
        Self { car: CarChoice::TestCar, suspension: SuspensionKind::COIL }
    }
}

// Motor = díl. Moment podle RPM dá momentová křivka (body (rpm, Nm), lineární interpolace).
// Deserialize: díl se dá načíst přímo z .veh (sekce "engine"). Debug chce veh::Vehicle.
#[derive(Debug, serde::Deserialize)]
pub struct Engine {
    pub torque_curve: Vec<(f32, f32)>, // (rpm, Nm) — vzestupně podle rpm
    pub max_rpm: f32,                  // omezovač otáček
    pub idle_rpm: f32,                 // volnoběh (pod něj RPM neklesne)
}

// Převodovka = díl. Poměry kvaltů + stálý převod (rozvodovka).
#[derive(Debug, serde::Deserialize)]
pub struct Gearbox {
    pub ratios: Vec<f32>, // poměry pro kvalty 1..N
    pub final_drive: f32, // stálý převod
}

// Brzdy = díl. Brzdný moment NA KOLO (Nm) při plném sešlápnutí, per náprava — přední
// brzdí víc (přenos zátěže). Ruční brzda působí jen na zadní (jako v reálném autě).
#[derive(Debug, serde::Deserialize)]
pub struct Brakes {
    pub front: f32,
    pub rear: f32,
    pub parking: f32,
}

// Rozvod pohonu = nastavení (data). Které osy táhnou + jak se mezi ně dělí moment.
// V .veh: "fwd"/"rwd" string, AWD objekt {"awd":{"front":0.5}} (serde externí tag).
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriveLayout {
    Fwd,                // přední pohon
    Rwd,                // zadní pohon
    Awd { front: f32 }, // 4×4: podíl momentu na přední osu (0..1), zadní = 1−front
}

impl DriveLayout {
    // Podíl celkového momentu jdoucí na osu daného kola.
    pub fn axle_fraction(self, front: bool) -> f32 {
        match self {
            DriveLayout::Fwd => if front { 1.0 } else { 0.0 },
            DriveLayout::Rwd => if front { 0.0 } else { 1.0 },
            DriveLayout::Awd { front: f } => if front { f } else { 1.0 - f },
        }
    }
    pub fn label(self) -> &'static str {
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
pub struct Differential {
    pub lock: f32, // 0..1
}

#[derive(Resource)]
pub struct Drivetrain {
    pub engine: Engine,         // namontovaný motor
    pub gearbox: Gearbox,       // namontovaná převodovka
    pub brakes: Brakes,         // namontované brzdy (moment per náprava)
    pub layout: DriveLayout,    // FWD/RWD/AWD (data, přepínatelné)
    pub front_diff: Differential, // diff přední osy
    pub rear_diff: Differential,  // diff zadní osy
    pub gear: i32,              // runtime: -1 = zpátečka, 0 = neutrál, 1..=N = kvalty
    pub rpm: f32,               // runtime: STAV motoru (integruje se, není derivovaný z kol)
    pub clutch: f32,            // runtime: 0 = rozpojená (levý Shift), 1 = sepnutá
    pub stalled: bool,          // runtime: motor chcíplý (start = R)
    pub speed_kmh: f32,         // runtime: dopočítaná rychlost (pro HUD)
}

// --- SPOJKA + MOTOR JAKO STAV ---
// Setrvačnost motoru (setrvačník, kg·m²) — jak rychle plyn vytočí volný motor.
pub const ENGINE_INERTIA: f32 = 0.25;
// Viskózní spojka: přenášený moment = VISC × rozdíl rychlostí (motor vs kola přes převod),
// škálováno sepnutím (0..1) a omezeno kapacitou CAP (víc lamely neudrží → prokluz).
pub const CLUTCH_VISC: f32 = 2.0;   // Nm / (rad/s) skluzu
pub const CLUTCH_CAP: f32 = 300.0;  // Nm na straně motoru
// Regulátor volnoběhu: moment úměrný deficitu pod idle. Záměrně SLABÝ — sepnutá
// spojka proti stojícím kolům ho přetlačí (rovnováha ~360 rpm < STALL_RPM) a motor
// CHCÍPNE; rozjezd bez plynu tedy chce cit v noze jako ve skutečném autě.
pub const IDLE_GAIN: f32 = 1.5; // Nm / (rad/s) deficitu
// Pod tyhle otáčky motor chcípne (stalled). Start zpět: klávesa R (+ Shift, ať znovu nechcípne).
pub const STALL_RPM: f32 = 400.0;
// Vnitřní tření motoru (Nm na rad/s): chcíplý motor dotočí do nuly a při puštěném
// plynu brzdí přes převod (brzdění motorem). Bez něj otáčky mrtvého motoru zamrznou.
pub const ENGINE_FRICTION: f32 = 0.05;

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
            // ~65/35 dopředu (přenos zátěže při brzdění); parking dle dřívějšího Space=1500
            brakes: Brakes { front: 1000.0, rear: 550.0, parking: 1500.0 },
            layout: DriveLayout::Rwd,                 // start: zadní pohon
            front_diff: Differential { lock: 0.0 },   // start: otevřené
            rear_diff: Differential { lock: 0.0 },
            gear: 1,
            rpm: 850.0,   // motor běží od startu na volnoběh
            clutch: 1.0,  // sepnutá (Shift drž při startu/řazení)
            stalled: false,
            speed_kmh: 0.0,
        }
    }
}

// Poslední přidaný beam = pružina zavěšení: tlumič (COIL_DAMP_MULT) + dorazy zdvihu
// (gumový doraz při stlačení, odskokový popruh při natažení).
fn finish_spring(sb: &mut SoftBody) {
    let b = sb.beams.last_mut().unwrap();
    b.d *= COIL_DAMP_MULT;
    b.d_rebound = b.d * REBOUND_MULT; // natahování tlumí tvrději než stlačování
    b.min = b.rest - STOP_TRAVEL;
    b.max = b.rest + STOP_TRAVEL;
    b.stop_k = STOP_K;
}

// Postaví zavěšení = beamy mezi hubem kola a tělem. JAK je propojí, závisí na typu.
// Solver o typu neví — tady se „vlévá" celá variabilita podvozku (custom suspension).
pub fn build_suspension(sb: &mut SoftBody, hub: usize, top: Option<usize>, anchors: &[usize], kind: SuspensionKind) {
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
        // McPherson = stejná topologie, jen anchors[2] je VĚŽ (staví ji spawn_solver_car)
        // místo rohu rámu → vzpěra ~svislá, celé strut_k nese váhu.
        SuspensionKind::Coil { arm_k, coil_k }
        | SuspensionKind::McPherson { arm_k, strut_k: coil_k } => {
            sb.add_beam(hub, anchors[0], arm_k);
            sb.add_beam(hub, anchors[1], arm_k);
            sb.add_beam(hub, anchors[2], coil_k);
            // Tlumič sedí na pružině + dorazy zdvihu (builder-side, solver netknutý).
            finish_spring(sb);
        }
        // Double wishbone čeká anchors = [.., .., pružina, horní A přední, horní A zadní]
        // a top = těhlice (staví ji spawn_solver_car). Dolní ramena dlouhá (protější rohy),
        // horní krátká (rohy stejné strany) = SLA → camber při propružení.
        SuspensionKind::DoubleWishbone { arm_k, spring_k } => {
            let top = top.expect("DoubleWishbone potřebuje top nod (těhlici)");
            sb.add_beam(hub, anchors[0], arm_k); // dolní příčné rameno
            sb.add_beam(hub, anchors[1], arm_k); // dolní podélné rameno
            sb.add_beam(top, anchors[3], arm_k); // horní A-rameno, přední úchyt (krátké → camber)
            sb.add_beam(top, anchors[4], arm_k); // horní A-rameno, zadní úchyt
            // Příčná výztuha topu na PROTĚJŠÍ horní roh: bez ní drží top bočně jen
            // x-složka krátkého ramene → dopad ho přesmykne do zrcadlové polohy a kolo
            // zůstane křivé. Dlouhé ~vodorovné rameno zdvih nebrzdí (Δ²/2L) a camber
            // jen zmírní (oblouky 0.5 m vs 1.75 m se přetlačí ~napůl).
            sb.add_beam(top, anchors[5], arm_k);
            sb.add_beam(hub, top, KNUCKLE_K);    // těhlice (tuhá — drží rovinu kola)
            sb.add_beam(hub, anchors[2], spring_k);
            finish_spring(sb); // tlumič na pružině + dorazy zdvihu
        }
    }
}

// Stabilizátor (anti-roll bar) jako V-nod: lehký nod nad středem osy, tuhá ramena
// na oba huby. Obě kola spolu (bounce) → nod se sveze s nimi, beamy bez napětí,
// žádný odpor. Kola PROTI sobě (roll) → nod drží střed a ramena se napnou → brání
// náklonu. Links na horní rohy těla drží nod podélně/příčně (svisle ~vodorovné = nic).
pub fn build_antiroll(
    commands: &mut Commands,
    sb: &mut SoftBody,
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
    hub_l: usize,
    hub_r: usize,
    link_l: usize,
    link_r: usize,
) {
    let apex = (sb.pos[hub_l] + sb.pos[hub_r]) / 2.0 + Vec3::Y * 0.5;
    let e = commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(apex),
            Spawned,
        ))
        .id();
    // 12 kg ~ skutečný stabilizátor; lehčí nod = tužší uzel vůči hmotě = víc substepů (CFL).
    let a = sb.add_node(apex, 12.0, e);
    sb.add_beam(a, hub_l, ARB_K);
    sb.add_beam(a, hub_r, ARB_K);
    sb.add_beam(a, link_l, 30_000.0);
    sb.add_beam(a, link_r, 30_000.0);
}

// Spawne JEZDÍCÍ auto na vlastním solveru (jednotný model): tělo = node-beam kvádr,
// kola = hub-nody zavěšené beamy + render disky. Minimální test rig pro odladění
// drive/steer/grip (krok 2). Žádný rapier.
pub fn spawn_solver_car(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    sb: &mut SoftBody,
    dt: &mut Drivetrain,
    build: &CarBuild,
    ground_y: f32,
) {
    // Testovací auto jezdí na sériových dílech — vrátit je (po .veh autě by tu zůstaly jeho).
    let stock = Drivetrain::default();
    dt.engine = stock.engine;
    dt.gearbox = stock.gearbox;
    dt.layout = stock.layout;
    dt.gear = dt.gear.min(dt.gearbox.ratios.len() as i32);
    *sb = SoftBody::default(); // nový spawn = čistý solver
    // Spawn 2 m nad terénem = drop test zavěšení při každém spawnu (Coil ho musí ustát:
    // tužší pružina + tlumič, viz SUSP_COIL_K / COIL_DAMP_MULT).
    let origin = Vec3::new(0.0, ground_y + 2.0, 0.0);

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
    // wx: kolo (r 0.34, šířka 0.22) se při plném rejdu 28° natočí ~0.26 m do stran —
    // rozchod 1.0 ho drží mimo kvádr těla (hx 0.7); širší rozchod = i menší kolébání.
    // wy: kola 0.30 pod středem těla → spodek těla ~28 cm nad zemí (s wy −0.15 škrtal
    // o zem při zhoupnutí v rozjezdu). Ramena vyjdou v klidu ~vodorovně (střed zdvihu)
    // a pružina svisleji (víc rezervy do překlopení).
    let (wx, wy, wz) = (1.0, -0.30, 1.4);
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
        // Horní úchyt pružiny: Coil → roh rámu; McPherson → VĚŽ = nový nod nad hubem,
        // tuze svázaný 4 beamy s rohy (jede s rámem jako pevný úchyt). Vzpěra na věž
        // vyjde ~11° od svislice → celé strut_k nese váhu (roh rámu dá jen ~0.55·k).
        let top = if let SuspensionKind::McPherson { .. } = build.suspension {
            let tp = hub_pos + Vec3::new(-sx * STRUT_TOWER_IN, STRUT_TOWER_UP, 0.0);
            let te = commands
                .spawn((
                    Mesh3d(node_mesh.clone()),
                    MeshMaterial3d(node_material.clone()),
                    Transform::from_translation(tp),
                    Spawned,
                ))
                .id();
            let t = sb.add_node(tp, 8.0, te);
            sb.add_beam(t, body[xi + 2 + zi], STRUT_TOWER_K);       // horní roh stejné strany, stejné z
            sb.add_beam(t, body[xi + 3 - zi], STRUT_TOWER_K);       // horní roh stejné strany, opačné z
            sb.add_beam(t, body[(4 - xi) + 2 + zi], STRUT_TOWER_K); // horní roh protější strany (drží bok)
            sb.add_beam(t, body[xi + zi], STRUT_TOWER_K);           // spodní roh pod věží (drží svisle)
            t
        } else {
            body[xi + 2 + zi] // horní roh stejné strany, stejné z
        };
        // Těhlice (jen DoubleWishbone): top nod nad hubem — na něm visí horní A-rameno
        // a jeho poloha vůči hubu dává disku odklon (camber) v renderu.
        let knuckle = if let SuspensionKind::DoubleWishbone { .. } = build.suspension {
            let kp = hub_pos + Vec3::Y * KNUCKLE_H;
            let ke = commands
                .spawn((
                    Mesh3d(node_mesh.clone()),
                    MeshMaterial3d(node_material.clone()),
                    Transform::from_translation(kp),
                    Spawned,
                ))
                .id();
            Some(sb.add_node(kp, 10.0, ke))
        } else {
            None
        };
        let anchors = [
            body[(4 - xi) + zi],     // spodní roh PROTĚJŠÍ strany, stejné z → příčné rameno (drží bok)
            body[xi + 1 - zi],       // spodní roh stejné strany, OPAČNÉ z → podélné rameno (drží tah/brzdu)
            top,                     // pružina/vzpěra (nese váhu)
            body[xi + 2 + zi],       // horní roh stejné strany, stejné z → horní A-rameno (jen DW, krátké)
            body[xi + 3 - zi],       // horní roh stejné strany, opačné z → horní A-rameno (jen DW)
            body[(4 - xi) + 2 + zi], // horní roh PROTĚJŠÍ strany, stejné z → příčná výztuha topu (jen DW)
        ];
        // Rigid potřebuje VŠECH 8 nodů (šikmé beamy do všech směrů = tuhý zámek);
        // se 3 skoro-vodorovnými úchyty by svisle skoro nedržel a houpal se.
        let rigid_anchors: &[usize] = match build.suspension {
            SuspensionKind::Rigid => &body,
            _ => &anchors,
        };
        build_suspension(sb, hub, knuckle, rigid_anchors, build.suspension);

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
            top: knuckle,
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
            telem: WheelTelem::default(),
        });
    }

    // Stabilizátory předek + zadek (vše se zdvihem — Rigid ho nemá, V-nod by byl balast).
    // Pořadí wheels = wheel_layout: [0] FL, [1] FR, [2] RL, [3] RR.
    if !matches!(build.suspension, SuspensionKind::Rigid) {
        let (fl, fr) = (sb.wheels[0].hub, sb.wheels[1].hub);
        let (rl, rr) = (sb.wheels[2].hub, sb.wheels[3].hub);
        build_antiroll(commands, sb, &node_mesh, &node_material, fl, fr, body[3], body[7]);
        build_antiroll(commands, sb, &node_mesh, &node_material, rl, rr, body[2], body[6]);
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
pub fn veh_pos(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1])
}

// Načte .veh a naplní vlastní solver: nody = render kuličky + záznam v `SoftBody`,
// beamy = pružiny (`BeamC`) mezi indexy nodů, triangly = deformující povrch.
// Reálné tuhosti z .veh; tlumení a počet substepů dopočítají helpery `SoftBody`.
pub fn spawn_softbody(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    sb: &mut SoftBody,
    dt: &mut Drivetrain,
    path: &str,
    ground_y: f32,
) {
    let raw = std::fs::read_to_string(path).expect("nelze číst .veh");
    let mut vehicle: veh::Vehicle = serde_json::from_str(&raw).expect("nevalidní .veh");
    info!("Načteno vozidlo z .veh: {} nodů, {} beamů", vehicle.nodes.len(), vehicle.beams.len());

    // Motor/převodovka z .veh; starý .veh bez nich → díly testovacího auta (Default).
    let stock = Drivetrain::default();
    dt.engine = vehicle.engine.take().unwrap_or(stock.engine);
    dt.gearbox = vehicle.gearbox.take().unwrap_or(stock.gearbox);
    dt.layout = vehicle.layout.unwrap_or(stock.layout);
    // Nová skříň může mít míň kvaltů, než je zařazeno → srovnat, ať se neindexuje mimo.
    dt.gear = dt.gear.min(dt.gearbox.ratios.len() as i32);

    let origin = Vec3::new(0.0, ground_y + 4.0, 6.0); // PŘED auto (+Z) a ve vzduchu, ať to chase kamera vidí

    // Nový spawn = čistý solver (jinak by se sčítala dvě auta do jednoho stavu).
    *sb = SoftBody::default();
    sb.from_veh = true; // menu dílů (SetSuspension) tohle auto nepřestavuje

    // AXLE nody kol (fw1r…) mají v jbeam jen LOKÁLNÍ souřadnice kola (~[±x,0,0]) —
    // v globálním prostoru nesmysl: visely by na mega-tuhých beamech pod podlahou,
    // drhly o zem a sebraly kolům zátěž (= žádný grip). Do solveru NEJDOU; slouží
    // jen jako reference, přes jejich beamy se najdou suspension nody (úchyty).
    let axle_ids: std::collections::HashSet<&str> = vehicle.wheels.iter()
        .flat_map(|w| [w.node1.as_str(), w.node2.as_str()])
        .collect();

    // Sdílený mesh + materiál pro všechny nody (1067× nepřidávat nový asset = výkon).
    let node_mesh = meshes.add(Sphere::new(0.04));
    let node_material = materials.add(Color::srgb(0.2, 0.6, 1.0));

    // Mapa id → (render Entity, index do polí SoftBody). Beamy i triangly z ní berou.
    let mut node_idx: std::collections::HashMap<&str, (Entity, usize)> = std::collections::HashMap::new();

    // Nody → render kuličky (BEZ rapieru) + záznam do solveru přes helper.
    for node in &vehicle.nodes {
        if axle_ids.contains(node.id.as_str()) {
            continue; // axle nod kola — viz výše, do solveru nepatří
        }
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

    // Beamy → BeamC do solveru. REÁLNÁ tuhost i tlumení z .veh — pružina (k>0, d=0)
    // i čistý tlumič (k=0, d>0) projdou beze změny. Fallback jen pro starý .veh bez materiálů.
    for beam in &vehicle.beams {
        let (Some(&(_, i)), Some(&(_, j))) = (node_idx.get(beam.a.as_str()), node_idx.get(beam.b.as_str())) else {
            continue; // beam odkazuje na neexistující nod → přeskoč
        };
        if beam.stiffness == 0.0 && beam.damping == 0.0 {
            sb.add_beam(i, j, 1000.0); // starý formát: dopočítej d z DAMPING_RATIO
        } else {
            sb.add_beam_damped(i, j, beam.stiffness, beam.damping);
        }
        // Předpětí (beam.rest) jen pro čisté pružiny (k>0, d=0): rest > spawn délka
        // = pružina tlačí ven a nese statickou zátěž → auto sedí v jízdní výšce.
        // Mount-beamy (motor/výfuk, k obří, d>0) rest ignorují — jejich rest je
        // kratší než spawn délka a stáhl by geometrii (kola k sobě, viz historie).
        if let Some(rest) = beam.rest {
            if beam.stiffness > 0.0 && beam.damping == 0.0 {
                sb.beams.last_mut().unwrap().rest = rest;
            }
        }
    }

    // --- A1+A2: KOLA z .veh ---
    // `.veh` má `wheels: [{name, node1, node2, dir}]`, kde node1/node2 = AXLE nody kola.
    // POZOR: v BeamNG mají hub-nody jen LOKÁLNÍ souřadnice kola (všechny ~[x,0,0]), takže
    // jejich střed NENÍ skutečná poloha kola (přední i zadní by splynuly). Reálnou pozici
    // dostaneme z jejich beam-SOUSEDŮ (= suspension nody fh*/rh*…), co globální pozice mají.
    // Tytéž sousedy = úchyty zavěšení (build_suspension), takže hub na nich rovnou visí.
    let hub_mass = 35.0;
    let wheel_material = materials.add(Color::srgb(0.05, 0.05, 0.05));
    let marker_mesh = meshes.add(Cuboid::new(0.08, 0.08, 0.08));
    let marker_material = materials.add(Color::srgb(1.0, 0.9, 0.1));

    // Sousedství přes beamy PODLE ID (axle nody v solveru nejsou, ale jejich beamy
    // v surových datech ano — právě přes ně se dojde k suspension nodům).
    let mut neighbors: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for beam in &vehicle.beams {
        neighbors.entry(beam.a.as_str()).or_default().push(beam.b.as_str());
        neighbors.entry(beam.b.as_str()).or_default().push(beam.a.as_str());
    }

    // 1. průchod: pro každé kolo poskládej úchyty (sousedi axle nodů, co v solveru
    // existují) a z jejich průměru spočti REÁLNOU pozici hubu.
    let mut wheels: Vec<(Vec<usize>, Vec3, f32, f32)> = Vec::new(); // (úchyty, hub_pos, radius, width)
    for wheel in &vehicle.wheels {
        let mut anchors: Vec<usize> = Vec::new();
        for axle in [wheel.node1.as_str(), wheel.node2.as_str()] {
            for &nb in neighbors.get(axle).map(|v| v.as_slice()).unwrap_or(&[]) {
                // node_idx nezná axle nody ani neexistující id → projde jen skutečný nod
                if let Some(&(_, i)) = node_idx.get(nb) {
                    if !anchors.contains(&i) {
                        anchors.push(i);
                    }
                }
            }
        }
        if anchors.is_empty() {
            continue; // bez úchytů neumíme kolo umístit ani zavěsit
        }
        let mut hub_pos = anchors.iter().map(|&i| sb.pos[i]).sum::<Vec3>() / anchors.len() as f32;
        // Přesný střed kola z .veh (nodeOffset slotu): průměr úchytů ho jen aproximuje —
        // u zadní vlečné nápravy lže o ~0.3 m dopředu (rameno táhne průměr k předku).
        // x necháváme z úchytů: nese znaménko strany a stejně se tlačí na rozchod níž.
        if let Some(c) = wheel.center {
            let p = origin + veh_pos(c);
            hub_pos.y = p.y;
            hub_pos.z = p.z;
        }
        // Rozměry z .veh; 0 = data chyběla (tire party mimo mod) → fallback níž.
        let radius = if wheel.radius > 0.0 { wheel.radius } else { 0.34 };
        let width = if wheel.width > 0.0 { wheel.width } else { 0.20 };
        wheels.push((anchors, hub_pos, radius, width));
    }
    // Průměrné z hubů → hranice mezi přední (větší z) a zadní (menší z) osou.
    let mean_z = wheels.iter().map(|(_, p, _, _)| p.z).sum::<f32>() / wheels.len().max(1) as f32;

    // 2. průchod: hub-node + render disk + zavěšení (Rigid) + WheelC.
    for &(ref anchors, hub_pos, radius, width) in &wheels {
        let mut hub_pos = hub_pos;
        // .veh dá kola moc inboard (hub = průměr zatažených suspension nodů). Vytlač je ven
        // na rozumný rozchod, ať nejsou zastrčená pod karoserií. Rest-length beamů zavěšení
        // (build_suspension níž) tuhle polohu zafixuje, i když anchory zůstanou inboard.
        // ponytail: konstanta — reálný rozchod je v jbeam nodeOffset jako $výraz
        // ($trackwidth…), vyhodnocení proměnných až bude potřeba víc aut.
        hub_pos.x = hub_pos.x.signum() * 0.80;
        let front = hub_pos.z > mean_z; // přední osa = +z (přední kola Fabie vyšla na +z)
        let render = commands
            .spawn((
                Mesh3d(meshes.add(Cylinder::new(radius, width))),
                MeshMaterial3d(wheel_material.clone()),
                Transform::from_translation(hub_pos),
                Spawned,
            ))
            .with_children(|w| {
                w.spawn((
                    Mesh3d(marker_mesh.clone()),
                    MeshMaterial3d(marker_material.clone()),
                    Transform::from_xyz(0.0, 0.0, radius + 0.03),
                ));
            })
            .id();
        // Hub = nový node; jeho render entita = disk kola (softbody_to_transforms mu
        // dá pozici i rotaci, takže žádná zvlášť debug kulička není potřeba).
        let hub = sb.add_node(hub_pos, hub_mass, render);
        build_suspension(sb, hub, None, anchors, SuspensionKind::Rigid); // přitáhni hub na suspension nody
        sb.wheels.push(WheelC {
            hub,
            top: None, // .veh zavěšení zatím Rigid — kolo vzpřímené s rámem
            render,
            radius,
            spin: 0.0,
            spin_vel: 0.0,
            steer: 0.0,
            drive_torque: 0.0,
            brake: 0.0,
            front,
            steered: front, // přední = řízená (A4 to případně doladí)
            tire: TireParams { normal_stiffness: 120_000.0, normal_damping: 4_000.0, long: TIRE_LONG, lat: TIRE_LAT },
            telem: WheelTelem::default(),
        });
    }

    // --- A3: ChassisFrame z nodů Fabie (směr auta pro solver/kameru/render) ---
    // Klasifikace jako u test auta: front = +z, right = +x (handedness, ať up = fwd×right = +y).
    // POZOR: pravá kola Fabie jsou fyzicky na −x → „right" rám může vyjít zrcadlově. Když A/D
    // zatáčí na opačnou stranu, otoč znaménko steeru (nebo zde prohoď left↔right).
    let body_count = node_idx.len(); // spawnuté nody Fabie = indexy 0..body_count (huby jsou až za nimi)
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
    for node in &vehicle.nodes {
        let Some(&(entity, _)) = node_idx.get(node.id.as_str()) else {
            continue; // axle nod — v solveru není, vrchol pro něj nemá kdo hýbat
        };
        vert_of.insert(node.id.as_str(), vertex_positions.len() as u32);
        let p = origin + veh_pos(node.pos);
        vertex_positions.push([p.x, p.y, p.z]);
        vertex_nodes.push(entity);
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

// Moment motoru při daných otáčkách — lineární interpolace momentové křivky.
// Nad max_rpm = omezovač (moment 0 → kolo se nemůže protáčet do nekonečna).
pub fn engine_torque(rpm: f32, engine: &Engine) -> f32 {
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
pub fn vehicle_drive(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut dt: ResMut<Drivetrain>,
    mut sb: ResMut<SoftBody>,
) {
    let throttle = if keys.pressed(KeyCode::KeyW) { 1.0 } else { 0.0 };
    let braking = keys.pressed(KeyCode::KeyS);
    let parking = keys.pressed(KeyCode::Space); // ruční brzda — jen zadní náprava

    // Spojka: levý Shift drží rozpojeno, puštění plynule sepne (skokové sepnutí = škubnutí).
    let clutch_target = if keys.pressed(KeyCode::ShiftLeft) { 0.0 } else { 1.0 };
    let clutch_rate = 4.0 * time.delta_secs();
    dt.clutch += (clutch_target - dt.clutch).clamp(-clutch_rate, clutch_rate);

    // Starter: R nahodí chcíplý motor na volnoběh. MUSÍ být před čtením omega_e —
    // jinak integrace níž jede ze starých otáček a motor hned zase chcípne.
    if dt.stalled && keys.just_pressed(KeyCode::KeyR) {
        dt.stalled = false;
        dt.rpm = dt.engine.idle_rpm;
    }

    // Převod aktuálního kvaltu (R = opačný směr). Neutrál → 0 (motor točí, kola ne).
    let ratio = match dt.gear {
        0 => 0.0,
        -1 => -dt.gearbox.ratios[0], // zpátečka: opačný smysl než 1. kvalt
        g => dt.gearbox.ratios[(g - 1) as usize],
    };
    let total_ratio = ratio * dt.gearbox.final_drive;

    // Moment spojky: viskózní vazba motor ↔ kola (úměrná skluzu, se stropem kapacity).
    // Neutrál nebo plně rozpojeno → nula (motor běží naprázdno).
    let omega_e = dt.rpm * std::f32::consts::TAU / 60.0; // otáčky motoru (rad/s)
    let t_clutch = if total_ratio != 0.0 && dt.clutch > 0.0 {
        // průměrná rychlost hnaných kol (rad/s), přepočtená na stranu motoru
        let (mut sum, mut n) = (0.0, 0.0);
        for w in &sb.wheels {
            if dt.layout.axle_fraction(w.front) > 0.0 {
                sum += w.spin_vel;
                n += 1.0;
            }
        }
        let omega_w = if n > 0.0 { sum / n } else { 0.0 };
        let slip = omega_e - omega_w * total_ratio;
        (CLUTCH_VISC * slip * dt.clutch).clamp(-CLUTCH_CAP, CLUTCH_CAP)
    } else {
        0.0
    };

    // Motor jako stav: plyn ho roztáčí, spojka brzdí proti kolům, vnitřní tření ho
    // stahuje k nule, regulátor volnoběhu dotláčí k idle (slabě — jde přetlačit
    // a motor chcípne). Rev limiter v engine_torque.
    let idle_omega = dt.engine.idle_rpm * std::f32::consts::TAU / 60.0;
    let (t_engine, t_idle) = if dt.stalled {
        (0.0, 0.0) // chcíplý motor nedává nic, jen tření a spojka
    } else {
        (
            engine_torque(dt.rpm, &dt.engine) * throttle,
            IDLE_GAIN * (idle_omega - omega_e).max(0.0),
        )
    };
    // Chcíplý motor má navíc konstantní kompresní odpor — samotné viskózní tření je
    // exponenciála (τ=5 s), otáčky by k nule lezly desítky sekund a na HUDu „zamrzly".
    let t_drag = ENGINE_FRICTION * omega_e + if dt.stalled { 20.0 } else { 0.0 };
    let t_net = t_engine + t_idle - t_clutch - t_drag;
    let omega_new = (omega_e + t_net / ENGINE_INERTIA * time.delta_secs()).max(0.0);
    if dt.stalled && dt.clutch > 0.5 && omega_new > idle_omega {
        dt.stalled = false; // roztlačení: kola protočila mrtvý motor nad volnoběh → chytl
    } else if !dt.stalled && omega_new < STALL_RPM * std::f32::consts::TAU / 60.0 {
        dt.stalled = true; // zátěž stáhla otáčky pod práh → chcíplo (start: Shift+R)
    }
    dt.rpm = omega_new * 60.0 / std::f32::consts::TAU;

    // Na kola jde moment SPOJKY (ne motoru) × převod — co spojka nepřenese, neexistuje.
    let total = t_clutch * total_ratio;

    // Rozdělení podle DriveLayoutu: každá osa dostane svůj podíl, ten se rozpočítá mezi
    // její kola. Diferenciál (sváz rychlostí) řeší solver dle Drivetrain.{front,rear}_diff.
    let front_n = sb.wheels.iter().filter(|w| w.front).count().max(1) as f32;
    let rear_n = sb.wheels.iter().filter(|w| !w.front).count().max(1) as f32;

    let v = dt.speed_kmh / 3.6; // rychlost auta (m/s) pro odhad skluzu kola
    for wheel in &mut sb.wheels {
        let axle = dt.layout.axle_fraction(wheel.front); // podíl pro osu tohoto kola
        let n = if wheel.front { front_n } else { rear_n };
        wheel.drive_torque = total * axle / n;
        // „Spojka/trakčka": nad κ ≈ 0.3 stáhni moment úměrně skluzu. Bez toho plný
        // moment (víc než pneumatika udrží, μFz·r) roztočí kolo do omezovače; za
        // špičkou Pacejky klesne podélná síla I boční vedení (kruh adheze) → auto
        // tancuje bokem a nerozjede se. Ustálí se kousek za špičkou (κ ~0.45).
        // ponytail: TC natvrdo bez vypínače; vypínač až s modelem spojky.
        let slip = (wheel.spin_vel.abs() * wheel.radius - v).max(0.0) / v.max(V_LOW);
        if slip > 0.3 {
            wheel.drive_torque *= 0.3 / slip;
        }
        // Brzdný moment per náprava z dílu Brakes; ručka přičítá jen na zadní.
        wheel.brake = if braking {
            if wheel.front { dt.brakes.front } else { dt.brakes.rear }
        } else {
            0.0
        } + if parking && !wheel.front { dt.brakes.parking } else { 0.0 };
    }
}

// Řízení: A/D natáčí přední kola kolem svislé osy. Plynule lerpne k cíli (ne skokem),
// ať řízení nešvihne. Solver z `wheel.steer` odvodí směr pneumatiky (sekce 2.6).
// Feel (úhel dle rychlosti, Ackermann, samovracení) až s volantem/ovladačem.
pub fn vehicle_steer(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut sb: ResMut<SoftBody>) {
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
pub fn shift_gears(keys: Res<ButtonInput<KeyCode>>, mut dt: ResMut<Drivetrain>) {
    let top = dt.gearbox.ratios.len() as i32;
    // Kvalt jde zařadit jen s rozpojenou spojkou (drž levý Shift) — jako ve skutečném autě.
    let clutch_free = dt.clutch < 0.5;
    if keys.just_pressed(KeyCode::KeyE) && clutch_free {
        dt.gear = (dt.gear + 1).min(top); // nahoru, max nejvyšší kvalt
    }
    if keys.just_pressed(KeyCode::KeyQ) && clutch_free {
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

// Dopočítá rychlost z aktuálního stavu solveru (pro HUD).
// RPM tady NEpočítáme — motor je stav, integruje ho vehicle_drive (spojka).
pub fn update_drivetrain(sb: Res<SoftBody>, mut dt: ResMut<Drivetrain>) {
    let Some(cf) = &sb.chassis else {
        dt.speed_kmh = 0.0;
        return;
    };

    // Rychlost: průměr rychlostí rohových nodů, jen vodorovná složka (m/s → km/h).
    let nodes = cf.front.iter().chain(&cf.rear);
    let n = (cf.front.len() + cf.rear.len()).max(1) as f32;
    let mut v: Vec3 = nodes.map(|&i| sb.vel[i]).sum::<Vec3>() / n;
    v.y = 0.0;
    dt.speed_kmh = v.length() * 3.6;
}
