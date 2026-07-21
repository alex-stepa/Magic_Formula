// solver.rs — jádro fyziky: SoftBody (SoA stav), node-beam solver, tire model.
// Solver je HLOUPÝ integrátOR nad indexy — o typech zavěšení ani dílech nic neví
// (variabilita patří do builderů, viz vehicle.rs).

use bevy::prelude::*;

use crate::vehicle::Loadout;
use crate::world::Terrain;

// --- VLASTNÍ SOFT-BODY SOLVER (Fáze 2) ---
// Stav soft-body držíme my, ne rapier. Structure-of-Arrays: index i = jeden nod,
// pole pos/vel/inv_mass/force jsou paralelní (cache-friendly, beamy odkazují indexem).
#[derive(Resource, Default)]
pub struct SoftBody {
    pub pos: Vec<Vec3>,        // pozice nodu i (světové)
    pub vel: Vec<Vec3>,        // rychlost nodu i
    pub inv_mass: Vec<f32>,    // 1/m (předpočítané; 0 = ukotvený nod)
    pub force: Vec<Vec3>,      // akumulátor sil (čistí se každý substep)
    pub beams: Vec<BeamC>,     // pružiny mezi indexy
    pub wheels: Vec<WheelC>,   // kola (každé = hub-node + stav pneumatiky)
    pub entities: Vec<Entity>, // node index i ↔ render entita (zápis Transformu)
    pub substeps: u32,         // kolik integračních kroků na snímek (adaptivně dle nejtužšího beamu)
    pub chassis: Option<ChassisFrame>, // referenční nody pro směr auta (None u .veh — nemá kola)
}

// Referenční nody těla, ze kterých solver dopočítá dopředný/boční směr auta (yaw rámu).
// Forward = mean(front) − mean(rear), Right = mean(right) − mean(left). Plní ho builder auta;
// solver je tak pořád data-driven (nezná layout, jen čte indexy).
pub struct ChassisFrame {
    pub front: Vec<usize>,
    pub rear: Vec<usize>,
    pub left: Vec<usize>,
    pub right: Vec<usize>,
}

// Beam ve solveru — indexy do polí výše (ne Entity ani String), REÁLNÁ tuhost z .veh.
pub struct BeamC {
    pub i: usize,
    pub j: usize,
    pub rest: f32, // klidová délka
    pub k: f32,    // tuhost (bez škálování)
    pub d: f32,    // tlumení při stlačování (bump)
    pub d_rebound: f32, // tlumení při natahování (rebound) — reál má ~2× bump
    // Dorazy zdvihu (bump stop / odskok): mimo <min, max> délku nastoupí dorazová
    // pružina stop_k NAVÍC ke k. stop_k = 0 → beam bez dorazů (default).
    pub min: f32,
    pub max: f32,
    pub stop_k: f32,
}

// Síla pružiny beamu vč. dorazů, kladná = přitahuje (čistá funkce kvůli testu).
// Doraz je jen tužší pružina od prahu — spojitá síla, žádný skok (stabilita).
pub fn beam_spring_force(beam: &BeamC, len: f32) -> f32 {
    let mut s = beam.k * (len - beam.rest);
    if beam.stop_k > 0.0 {
        if len < beam.min {
            s += beam.stop_k * (len - beam.min); // stlačeno pod doraz → tlačí od sebe
        } else if len > beam.max {
            s += beam.stop_k * (len - beam.max); // nataženo přes odskok → táhne zpět
        }
    }
    s
}

// --- KOLA NA SOLVERU (jednotný model: kolo = node v SoftBody s bohatším kontaktem) ---
// Princip rozšiřitelnosti: všechno tady jsou DATA, ne konstanty v kódu. Tuning part / custom
// díly časem jen přepíšou tahle čísla; solver se nemění. Pérování NENÍ tady — to jsou beamy,
// které postaví builder zavěšení (viz SuspensionKind) → různé geometrie bez zásahu do solveru.
pub struct WheelC {
    pub hub: usize,        // index hub-nodu v SoftBody (na něm visí pérovací beamy do těla)
    pub top: Option<usize>, // top nod těhlice (DoubleWishbone): hub→top = osa kola → camber v renderu; None = kolo vzpřímené s rámem
    pub render: Entity,    // entita disku kola (zápis Transformu: pozice hubu + spin + steer)
    pub radius: f32,
    pub spin: f32,         // úhel natočení kola (rad) — pro render
    pub spin_vel: f32,     // úhlová rychlost valení (rad/s)
    pub steer: f32,        // aktuální natočení řízení (rad), nastaví vehicle_steer
    pub drive_torque: f32, // moment na tomhle kole (Nm), nastaví vehicle_drive (z motoru+převodů)
    pub brake: f32,        // brzdný moment (Nm), nastaví vehicle_drive
    pub front: bool,       // je na PŘEDNÍ ose? (pohon rozhoduje DriveLayout, ne kolo samo)
    pub steered: bool,     // natáčí se s ním?
    pub tire: TireParams,  // model pneumatiky — izolovaná data, ať jde vyměnit za Pacejku
    pub telem: WheelTelem, // poslední hodnoty kontaktu (jen čtení pro HUD/log, solver je píše)
}

// Telemetrie kola za poslední substep — pro HUD a CSV log při ladění gripu.
// Nepočítá nic; jen okno do toho, co tire model uvnitř solveru dělá.
#[derive(Clone, Copy, Default)]
pub struct WheelTelem {
    pub fz: f32,          // normálová síla (N); 0 = kolo ve vzduchu
    pub slip_ratio: f32,  // podélný skluz κ
    pub slip_angle: f32,  // boční skluz α (rad)
    pub f_long: f32,      // podélná síla pneumatiky (N)
    pub f_lat: f32,       // boční síla pneumatiky (N)
}

// Koeficienty Magic Formula (Pacejka) pro jednu osu (podélnou nebo boční).
// F = D·sin(C·atan(B·x − E·(B·x − atan(B·x)))), kde D = μ·Fz se dopočítá za běhu.
#[derive(Clone, Copy)]
pub struct Pacejka {
    pub b: f32, // stiffness factor — jak strmě křivka stoupá ke špičce
    pub c: f32, // shape factor — tvar (podélně ~1.65, bočně ~1.3)
    pub e: f32, // curvature factor — ostrost špičky (~0.97)
}

// Parametry pneumatiky = data. Svislý kontakt (auto sedí na kolech) + dvě Pacejka křivky.
#[derive(Clone, Copy)]
pub struct TireParams {
    pub normal_stiffness: f32, // svislá tuhost kontaktu (pneumatika tlačí auto nahoru)
    pub normal_damping: f32,   // tlumení svislého kontaktu
    pub long: Pacejka,         // podélná křivka (slip ratio → tah/brzda)
    pub lat: Pacejka,          // boční křivka (slip angle → zatáčení)
    // Koeficient tření pneumatika↔zem. Strop celkové vodorovné síly = friction·Fz (kruh
    // adheze): víc, než drží guma, nedá podélnou ani boční sílu (skluz/smyk). Per-pneu →
    // sportovní kompound zvedne strop gripu, ne jen tvar Pacejka křivky.
    pub friction: f32,
}

// --- LADICÍ HODNOTY PNEUMATIKY (vše na jednom místě) ---
// Změň čísla a `cargo run` — solver je nesahá, jen čte. D (špička) = MU·Fz dynamicky.
pub const TIRE_LONG: Pacejka = Pacejka { b: 10.0, c: 1.65, e: 0.97 };
pub const TIRE_LAT: Pacejka = Pacejka { b: 18.0, c: 1.3, e: 0.97 };
// Výchozí pneumatika (svislý kontakt + obě křivky) — jediné místo; buildery jen kopírují.
pub const TIRE_DEFAULT: TireParams = TireParams {
    normal_stiffness: 120_000.0,
    normal_damping: 4_000.0,
    long: TIRE_LONG,
    lat: TIRE_LAT,
    friction: 1.0,
};
// Spodní mez referenční rychlosti pro normalizaci skluzu (m/s). Brání dělení nulou
// při rozjezdu z místa — bez ní κ vystřelí a auto exploduje.
pub const V_LOW: f32 = 2.0;
// Camber thrust: nakloněné kolo táhne do strany náklonu, F = CAMBER_THRUST·Fz·sin(γ).
// Působí jen u kol s těhlicí (WheelC.top — DoubleWishbone); ostatní mají γ trvale 0.
// Reálná guma ~0.5–1.0·Fz na radián; started konzervativně.
pub const CAMBER_THRUST: f32 = 0.5;

// FixedUpdate jede defaultně na 64 Hz — délka jednoho snímku solveru.
pub const FRAME_DT: f32 = 1.0 / 64.0;
// Rezerva pod CFL hranicí (stabilita explicitní integrace = dt < 2/ω; bereme jen část).
pub const CFL_SAFETY: f32 = 0.4;
// Strop substepů, ať patologicky tuhý .veh nezmrazí hru.
pub const MAX_SUBSTEPS: u32 = 512;
// Horní hrana zemního kvádru (Collider::cuboid half_height 0.1 v počátku) — kontaktní rovina.
pub const GROUND_TOP: f32 = 0.1;
// Poměr tlumení beamů: 0 = netlumené (zvoní/poskakuje), 1 = kriticky tlumené (mrtvé).
// Tlumení škálujeme s tuhostí: d = ζ·d_crit = 2·ζ·√(k·μ) — jinak tuhé beamy jen zvoní.
pub const DAMPING_RATIO: f32 = 0.25;
// Max stlačení pneumatiky (metry) — pak „ráfek": tvrdá projekce nad terén. Boční stěna
// 175/65R14 má ~11 cm, plné sražení už je náraz do ráfku; ~70 % je realistický strop.
pub const TIRE_MAX_SQUASH: f32 = 0.08;

// --- DRIVETRAIN ---
// Part-shaped (připraveno na build systém): DÍLY jsou datové structy, RESOURCE drží,
// které díly jsou v autě + runtime stav. Build systém díl vymění, tuning přepíše pole,
// `.veh parts` se sem časem jen deserializují. Žádné magické konstanty v systémech.

// Pomocné metody pro STAVBU soft-body. Jediný zdroj pravdy — používá je builder .veh
// i builder auta, ať se logika (tlumení, substepy) nikde nerozjede.
impl SoftBody {
    // Přidá node a vrátí jeho index. `entity` = render kulička pro zápis Transformu.
    pub fn add_node(&mut self, pos: Vec3, mass: f32, entity: Entity) -> usize {
        let i = self.pos.len();
        self.pos.push(pos);
        self.vel.push(Vec3::ZERO);
        self.inv_mass.push(1.0 / mass);
        self.force.push(Vec3::ZERO);
        self.entities.push(entity);
        i
    }

    // Přidá beam mezi dva nody. Klidová délka = aktuální vzdálenost, tlumení dle ζ a tuhosti.
    pub fn add_beam(&mut self, i: usize, j: usize, k: f32) {
        let mu = 1.0 / (self.inv_mass[i] + self.inv_mass[j]); // redukovaná hmotnost pružiny
        self.add_beam_damped(i, j, k, 2.0 * DAMPING_RATIO * (k * mu).sqrt());
    }

    // Beam s EXPLICITNÍM tlumením (např. z .veh: pružina k>0 d=0, tlumič k=0 d>0).
    // add_beam tlumení dopočítává z DAMPING_RATIO — tohle ho bere jak je.
    pub fn add_beam_damped(&mut self, i: usize, j: usize, k: f32, d: f32) {
        self.beams.push(BeamC {
            i,
            j,
            rest: (self.pos[i] - self.pos[j]).length(),
            k,
            d,
            d_rebound: d, // default symetrické; asymetrii nastaví builder (finish_spring)
            min: 0.0,
            max: f32::INFINITY,
            stop_k: 0.0,
        });
    }

    // Dopočítá počet substepů z CFL (Gershgorin: ω²ₘₐₓ ≤ 2·max_i(Σk_i/m_i)). Volá se po
    // přidání všech beamů. Tuhý uzel (součet tuhostí incidentních beamů) určuje frekvenci.
    pub fn compute_substeps(&mut self) {
        let mut k_sum = vec![0.0_f32; self.pos.len()];
        for beam in &self.beams {
            // + stop_k: na dorazu je efektivní tuhost k+stop_k — substepy musí stačit
            // i v tom stavu, jinak dopad na doraz exploduje
            k_sum[beam.i] += beam.k + beam.stop_k;
            k_sum[beam.j] += beam.k + beam.stop_k;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_stop_engages_outside_travel() {
        let b = BeamC { i: 0, j: 1, rest: 0.5, k: 20_000.0, d: 0.0, d_rebound: 0.0, min: 0.44, max: 0.56, stop_k: 200_000.0 };
        let close = |a: f32, b: f32| (a - b).abs() < 0.1; // floaty: tolerance 0.1 N
        assert!(close(beam_spring_force(&b, 0.5), 0.0)); // klid
        assert!(close(beam_spring_force(&b, 0.45), 20_000.0 * -0.05)); // ve zdvihu: jen pružina
        // na dorazu (0.42 < min): pružina + doraz tlačí od sebe (záporná = odpuzuje)
        assert!(close(beam_spring_force(&b, 0.42), 20_000.0 * -0.08 + 200_000.0 * -0.02));
        // přes odskok (0.58 > max): táhne zpět (kladná = přitahuje)
        assert!(close(beam_spring_force(&b, 0.58), 20_000.0 * 0.08 + 200_000.0 * 0.02));
        // bez dorazů (stop_k = 0) žádný zlom
        let plain = BeamC { stop_k: 0.0, ..b };
        assert!(close(beam_spring_force(&plain, 0.42), 20_000.0 * -0.08));
    }
}

// Magic Formula (Pacejka): skluz `x` → síla. `d` = špička (μ·Fz). Křivka roste ke špičce
// a ZA ní zase klesá — to je ten realismus oproti lineárnímu gripu (přebrzdění, smyk).
pub fn magic_formula(x: f32, p: Pacejka, d: f32) -> f32 {
    let bx = p.b * x;
    d * (p.c * (bx - p.e * (bx - bx.atan())).atan()).sin()
}

// --- SOLVER: semi-implicitní Euler se substeppingem (běží ve FixedUpdate, 64 Hz) ---
// Jeden snímek rozdělíme na `sb.substeps` malých kroků (adaptivně, viz compute_substeps),
// aby tuhé beamy zůstaly stabilní (CFL: dt < 2/ω). Reálné tuhosti, žádné škálování.
pub fn softbody_solve(sb: ResMut<SoftBody>, loadout: Res<Loadout>, time: Res<Time<Fixed>>, terrain: Res<Terrain>) {
    if sb.beams.is_empty() {
        return; // nic nespawnuto
    }
    let sb = sb.into_inner(); // rozbalíme ResMut → můžeme půjčovat víc polí najednou
    let world = terrain.0.as_ref(); // terén: solver se ptá jen na height/normal
    let gravity = Vec3::new(0.0, -9.81, 0.0);
    let node_r = 0.04; // poloměr nodu nad povrchem
    let sub_dt = time.delta_secs() / sb.substeps as f32;
    // Tangenciální tření země je per-substep násobení rychlosti. To se ALE umocňuje počtem
    // substepů: 0.9^4 (test auto) je mírné, ale 0.9^382 (velké .veh) = ~0 → každý dotyk přilepí.
    // Přepočítáme faktor tak, aby výsledek za SNÍMEK byl stejný (~0.7) bez ohledu na substepy.
    let tang_keep = 0.7_f32.powf(1.0 / sb.substeps as f32);
    let (front_lock, rear_lock) = (loadout.front_diff.lock, loadout.rear_diff.lock); // diff osy

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
            let spring = beam_spring_force(beam, len);
            let rel_v = (sb.vel[beam.j] - sb.vel[beam.i]).dot(dir);
            // rel_v > 0 = beam se natahuje → rebound tlumení, jinak bump
            let d = if rel_v > 0.0 { beam.d_rebound } else { beam.d };
            let f = (spring + d * rel_v) * dir;
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
            let friction = sb.wheels[wi].tire.friction;
            let steer = sb.wheels[wi].steer;
            let m_wheel = 1.0 / sb.inv_mass[hub];
            let i_wheel = 0.5 * m_wheel * radius * radius; // moment setrvačnosti disku

            // -- normálová síla (Fz) + reakce pneumatiky v tečné rovině, jen při dotyku --
            // Výška a normála terénu pod kolem; penetrace se měří svisle (u sklonů do
            // ~20° je chyba proti měření podél normály zanedbatelná).
            let h = world.height(sb.pos[hub].x, sb.pos[hub].z);
            let n = world.normal(sb.pos[hub].x, sb.pos[hub].z);
            let mut pen = (h + radius) - sb.pos[hub].y; // >0 = pneumatika stlačená
            // Tvrdý strop průniku (jako projekce nodů nad terén): rychlý nájezd do svahu
            // pružina nezbrzdí a kolo by se viditelně propadlo do terénu. Přes
            // TIRE_MAX_SQUASH už guma nemá kam — dál je „ráfek": poloha + útlum dopadu.
            let max_pen = TIRE_MAX_SQUASH.min(0.5 * radius); // .min: pojistka pro mini kola
            if pen > max_pen {
                sb.pos[hub].y = h + radius - max_pen;
                pen = max_pen;
                let vn = sb.vel[hub].dot(n);
                if vn < 0.0 {
                    sb.vel[hub] -= vn * n; // nepružná zem — žádný odraz navíc
                }
            }
            let mut f_long = 0.0; // reakce zpět na spin (mimo zem = 0)
            let mut telem = WheelTelem::default(); // ve vzduchu zůstanou nuly
            if pen > 0.0 {
                let fy = ns * pen - nd * sb.vel[hub].dot(n); // pružina podél normály − tlumení
                if fy > 0.0 {
                    sb.force[hub] += fy * n;

                    // směr kola po natočení řízení (kolem svislé osy Y), promítnutý do
                    // tečné roviny terénu — na svahu jede síla „po povrchu", ne vodorovně
                    let turn = Quat::from_rotation_y(steer);
                    let wf = (turn * fwd - n * (turn * fwd).dot(n)).normalize_or_zero(); // dopředný směr kola
                    let wl = (turn * right - n * (turn * right).dot(n)).normalize_or_zero(); // boční směr kola

                    let v = sb.vel[hub];
                    let v_long = v.dot(wf); // jak rychle se kontakt veze dopředu
                    let v_lat = v.dot(wl); // boční skluz
                    let roll = sb.wheels[wi].spin_vel * radius; // obvodová rychlost kola

                    // Normalizace skluzu pro Pacejku (v_ref s dolní mezí = ochrana proti dělení 0).
                    let v_ref = v_long.abs().max(V_LOW);
                    let kappa = (roll - v_long) / v_ref; // slip ratio (podélně)
                    let alpha = v_lat.atan2(v_ref); // slip angle v rad (bočně)

                    let d = friction * fy; // špička křivky = μ·Fz (víc guma neudrží)
                    let mut fl = magic_formula(kappa, long, d); // podélná síla (tah/brzda)
                    let mut fc = magic_formula(-alpha, lat, d); // boční síla (brzdí smyk)

                    // Camber thrust: kolo s těhlicí (DW) nakloněné o γ táhne do strany
                    // náklonu. sin γ = průmět osy těhlice do bočního směru kola; síla
                    // jde PŘED kruh adheze — víc než μ·Fz guma nedá ani camberem.
                    if let Some(t) = sb.wheels[wi].top {
                        let up_w = (sb.pos[t] - sb.pos[hub]).normalize_or_zero();
                        fc += CAMBER_THRUST * fy * up_w.dot(wl);
                    }

                    // kruh adheze jako pojistka na kombinovaný skluz: |(podélná, boční)| ≤ MU·Fz
                    let fmax = friction * fy;
                    let mag = (fl * fl + fc * fc).sqrt();
                    if mag > fmax {
                        let s = fmax / mag;
                        fl *= s;
                        fc *= s;
                    }
                    sb.force[hub] += fl * wf + fc * wl;
                    f_long = fl;
                    telem = WheelTelem { fz: fy, slip_ratio: kappa, slip_angle: alpha, f_long: fl, f_lat: fc };
                }
            }
            sb.wheels[wi].telem = telem;

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

            // Zem: vytlač nad povrch terénu a utlum odraz (normála) i tření (tangenta).
            let h = world.height(sb.pos[i].x, sb.pos[i].z) + node_r;
            if sb.pos[i].y < h {
                sb.pos[i].y = h;
                let n = world.normal(sb.pos[i].x, sb.pos[i].z);
                let vn = sb.vel[i].dot(n);
                if vn < 0.0 {
                    sb.vel[i] -= vn * n; // žádný skok zpět (nepružná zem)
                }
                // hrubé tření v tečné rovině (substep-nezávislé), než přijde pořádný model
                let vn = sb.vel[i].dot(n);
                sb.vel[i] = n * vn + (sb.vel[i] - n * vn) * tang_keep;
            }
        }
    }
}
