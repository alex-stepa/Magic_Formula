// Termální model brzd a pneumatik — per kolo. Konstanty jsou kalibrační knoby na
// jednom místě (jako TIRE_* v solveru), ne magie v kódu. Teploty se mění pomalu
// (sekundy) → integrujeme JEDNOU ZA SNÍMEK ve `wheel_thermal` (Update), ne per-substep.
//
// Fyzika = lumped kapacita: T_dot = (heat_in − cooling) / C.
//   brake heat = brzdný moment × úhlová rychlost = wheel.brake · |spin_vel|  [W]
//   tire heat  = síla × SKLUZOVÁ rychlost kontaktu (z kola, ne z rychlosti auta →
//                burnout/zámek hřeje i při stojícím autě)
//   cooling    = (h0 + h1·v) · (T − ambient)  — pasivní odvod + konvekce ∝ rychlost
// Zamčené kolo (spin_vel ≈ 0) negeneruje brake teplo — třecí práce jde do skluzu pneu.
//
// Pneu je DVOUVRSTVÁ: povrch (contact patch) hřeje/chladne rychle (malá kapacita), kostra
// (carcass) je pomalá tepelná setrvačnost. Povrch krmí grip; kostra ho drží mezi zatáčkami.
// Vedení povrch↔kostra + oba chladnou do vzduchu.

use bevy::prelude::*;
use crate::solver::{SoftBody, WheelTelem};
use crate::vehicle::{DrivetrainState, Loadout};

/// Výchozí teplota okolí (°C). Spawn teploty + cíl chlazení. Runtime přepisuje Ambient.
pub const T_AMBIENT: f32 = 30.0;

/// Teplota okolí za běhu (°C) — mění se sliderem (studený den = díly hůř drží teplo).
/// Díly chladnou k téhle hodnotě. Slider v SOUND/settings ji přepisuje.
#[derive(Resource)]
pub struct Ambient(pub f32);
impl Default for Ambient {
    fn default() -> Self {
        Self(T_AMBIENT)
    }
}

// --- MOTOR (chladicí okruh) ---
// Přehřátí (dlouhá zátěž bez airflow) = ztráta výkonu. Cooler jako part časem zvedne
// chlazení (ENGINE_H0/H1) — ponytail: až bude díl, čti hodnoty z Loadoutu, ne konstanty.
pub const ENGINE_C_CAP: f32 = 8000.0; // tepelná kapacita bloku+chladiče (J/°C)
pub const ENGINE_HEAT: f32 = 55000.0; // W při plné zátěži (rpm_frac × throttle)
pub const ENGINE_IDLE_HEAT: f32 = 4000.0; // W baseline běžícího motoru
pub const ENGINE_H0: f32 = 67.0; // pasivní odvod (W/°C) — laděno na idle ~90 °C; default Cooler.h0
pub const ENGINE_H1: f32 = 25.0; // radiátor: konvekce ∝ rychlost (W/°C na m/s); default Cooler.h1
pub const ENGINE_OVERHEAT: f32 = 115.0; // °C — nad tím klesá výkon
pub const ENGINE_COOK: f32 = 135.0; // °C — plná ztráta
pub const ENGINE_POWER_FLOOR: f32 = 0.6; // zbytkový výkon při přehřátí

/// Podíl výkonu motoru dle teploty: 1.0 pod ENGINE_OVERHEAT, lineárně na floor nad COOK.
pub fn engine_power_factor(engine_c: f32) -> f32 {
    let t = ((engine_c - ENGINE_OVERHEAT) / (ENGINE_COOK - ENGINE_OVERHEAT)).clamp(0.0, 1.0);
    1.0 - t * (1.0 - ENGINE_POWER_FLOOR)
}

/// Krok teploty motoru (čistá funkce). `load` = rpm_frac × throttle (0..1); chcíplý motor
/// = running false (nedělá teplo, jen chladne). `v` = m/s. `h0`/`h1` = chlazení z dílu Cooler
/// (default ENGINE_H0/H1). Vrací novou teplotu (clamp).
pub fn engine_step(engine_c: f32, running: bool, load: f32, v: f32, ambient: f32, dt: f32, h0: f32, h1: f32) -> f32 {
    let heat = if running { ENGINE_IDLE_HEAT + ENGINE_HEAT * load.clamp(0.0, 1.0) } else { 0.0 };
    let cool = (h0 + h1 * v.abs()) * (engine_c - ambient);
    (engine_c + (heat - cool) / ENGINE_C_CAP * dt).clamp(ambient, 160.0)
}

// --- BRZDY ---
// Tepelná kapacita kotouče+destiček (J/°C ≈ kg × ~500 J/kg°C). Přední kotouče větší →
// větší kapacita (a brzdí víc, viz Brakes.front > rear). ~10 kg vpředu, ~7 kg vzadu.
pub const BRAKE_C_FRONT: f32 = 5000.0;
pub const BRAKE_C_REAR: f32 = 3500.0;
// Chlazení: h0 pasivní (stojící auto), h1 konvekce úměrná rychlosti (W/°C na m/s).
// Kontrola řádu: při 28 m/s a ΔT 500 °C odvod ≈ (8 + 3·28)·500 ≈ 46 kW — srovnatelné
// s brzdnou špičkou (1000 Nm · 84 rad/s ≈ 84 kW) → ustálí kolem fade pásma, nechladí
// donekonečna. Časová konstanta chladnutí τ = C/(h0 + h1·v) ≈ 53 s (přední) při 28 m/s.
pub const BRAKE_H0: f32 = 8.0;
pub const BRAKE_H1: f32 = 3.0;
// Fade: pod prahem plný moment, nad ním lineárně dolů na podlahu (přehřáté destičky).
pub const FADE_START: f32 = 450.0; // °C — začátek slábnutí
pub const FADE_FULL: f32 = 700.0; // °C — plný fade
pub const FADE_FLOOR: f32 = 0.4; // zbytkový podíl momentu při plném fade

/// Efektivní podíl brzdného momentu podle teploty: 1.0 pod `fade_start`, lineárně na
/// FADE_FLOOR nad FADE_FULL. `fade_start` = práh z dílu Brakes (default FADE_START).
/// Násobí se na wheel.brake ve vehicle_drive.
pub fn brake_fade(brake_c: f32, fade_start: f32) -> f32 {
    let t = ((brake_c - fade_start) / (FADE_FULL - fade_start)).clamp(0.0, 1.0);
    1.0 - t * (1.0 - FADE_FLOOR)
}

// --- PNEUMATIKY (dvouvrstvé) ---
pub const TIRE_C_SURF: f32 = 800.0; // kapacita povrchu (J/°C) — malá = svižná reakce na skluz
pub const TIRE_C_CORE: f32 = 6000.0; // kapacita kostry — velká = pomalá setrvačnost (minuty)
pub const TIRE_COND: f32 = 40.0; // vedení povrch↔kostra (W/°C)
pub const SURF_H0: f32 = 8.0; // povrch → vzduch, pasivní (W/°C)
pub const SURF_H1: f32 = 4.0; // povrch → vzduch, konvekce ∝ rychlost
pub const CORE_H: f32 = 4.0; // kostra → vzduch, slabé (kostra je uvnitř)
pub const SLIP_SPEED_CAP: f32 = 30.0; // strop skluzové rychlosti (m/s) — brzdí runaway při burnoutu
// Valivý odpor: pneu se hřeje i bez hrubého skluzu (deformace boků při valení pod
// zátěží). Bez toho by cold-start na klidné jízdě zůstal studený navždy. W na N·(m/s).
pub const ROLL_HEAT: f32 = 0.05;
// Provozní okno (°C): uvnitř grip ×1.00 → NEUTRÁLNÍ, Pacejka beze změny. Mimo klesá.
pub const WIN_LO: f32 = 75.0;
pub const WIN_HI: f32 = 105.0;
pub const TIRE_COLD: f32 = 40.0; // ≤ = studená guma (podlaha GRIP_COLD)
pub const TIRE_HOT: f32 = 140.0; // ≥ = přehřátá (podlaha GRIP_HOT)
pub const GRIP_COLD: f32 = 0.90; // multiplikátor peaku studená
pub const GRIP_HOT: f32 = 0.88; // multiplikátor peaku přehřátá

// --- TLAK PNEU ---
// Horká pneu = vyšší tlak (plyn se ohřeje). Lineární aproximace: tlak roste s teplotou
// JÁDRA (pomalá masa vzduchu, ne povrch). COEF ~ 0.004/°C ≈ +0.1 bar na 25 °C nad ambient.
// ponytail: kalibrační knob — reálná pneu drift dle plynu/objemu, tady jeden lineární člen.
pub const PRESS_TEMP_COEF: f32 = 0.004; // podíl base tlaku na °C nad ambient
pub const PRESS_GRIP_FLOOR: f32 = 0.85; // strop ztráty gripu z odchylky tlaku

/// Provozní tlak pneu (bar) z teploty jádra. `base` = studený tlak z dílu Compound.
/// Nad ambient roste (teplo → tlak). VRSTVA — sama o sobě grip nemění (viz pressure_grip).
pub fn tire_pressure(core_c: f32, base: f32, ambient: f32) -> f32 {
    base * (1.0 + PRESS_TEMP_COEF * (core_c - ambient).max(0.0))
}

/// Grip multiplikátor z tlaku — VRSTVA nad Pacejkou. Peak ×1.00 v cílovém tlaku (ten, který
/// pneu má ve STŘEDU provozního okna); odchylka (přehuštěná horká / podhuštěná studená) grip
/// snižuje úměrně `sens`. `sens = 0` (default kompound) → ×1.00 VŽDY = neutrální (grip zaparkovaný).
pub fn pressure_grip(core_c: f32, base: f32, ambient: f32, win_lo: f32, win_hi: f32, sens: f32) -> f32 {
    let p = tire_pressure(core_c, base, ambient);
    let p_target = tire_pressure(0.5 * (win_lo + win_hi), base, ambient); // ideál = tlak v okně
    (1.0 - sens * (p - p_target).abs()).clamp(PRESS_GRIP_FLOOR, 1.0)
}

// --- OPOTŘEBENÍ PNEU ---
pub const WEAR_RATE: f32 = 2.0e-7; // wear za J skluzové práce (kalibrační; burnout ~min = prask)
pub const WEAR_GRIP_FLOOR: f32 = 0.7; // grip ojeté (wear=1) vs nové (1.0)
pub const BURST_WEAR: f32 = 1.0; // wear ≥ = pneu praskne
pub const BURST_GRIP: f32 = 0.3; // grip prasklé pneu (flat)

/// Teplotní multiplikátor peaku pneumatiky — VRSTVA nad Pacejkou (needituje křivku).
/// Čte se teplota POVRCHU. Plató ×1.00 v okně [WIN_LO, WIN_HI] (žádná změna gripu).
/// Studená (≤ TIRE_COLD) = GRIP_COLD, přehřátá (≥ TIRE_HOT) = GRIP_HOT, mezi lineárně.
/// Násobí `friction` v solveru (jedna hodnota krmí peak i kruh adheze → konzistentní).
/// `win_lo`/`win_hi` = provozní okno z dílu Compound (default WIN_LO/WIN_HI). Uvnitř okna
/// plató ×1.00; rampy k TIRE_COLD/HOT (fyzika gumy, nezávislá na kompoundu).
pub fn grip_mult(surf_c: f32, win_lo: f32, win_hi: f32) -> f32 {
    if surf_c < win_lo {
        // studená rampa: TIRE_COLD→win_lo ~ GRIP_COLD→1.0
        let t = ((surf_c - TIRE_COLD) / (win_lo - TIRE_COLD)).clamp(0.0, 1.0);
        GRIP_COLD + t * (1.0 - GRIP_COLD)
    } else if surf_c > win_hi {
        // horká rampa: win_hi→TIRE_HOT ~ 1.0→GRIP_HOT
        let t = ((surf_c - win_hi) / (TIRE_HOT - win_hi)).clamp(0.0, 1.0);
        1.0 - t * (1.0 - GRIP_HOT)
    } else {
        1.0 // provozní okno — neutrální
    }
}

/// Teplotní zesílení skřípotu pneu (amplituda). V provozním okně 1.0, studená/přehřátá
/// guma protestuje víc (hlasitější screech). Strop +0.6 = kalibrační knob.
pub fn tire_squeal_gain(surf_c: f32) -> f32 {
    let d = if surf_c < WIN_LO {
        WIN_LO - surf_c
    } else if surf_c > WIN_HI {
        surf_c - WIN_HI
    } else {
        0.0
    };
    1.0 + (d / 75.0).clamp(0.0, 0.6)
}

/// Grip multiplikátor z opotřebení: nová (wear 0) = 1.0, ojetá (wear 1) = WEAR_GRIP_FLOOR.
/// Vrstva nad Pacejkou jako temps — default (nové pneu) NEUTRÁLNÍ.
pub fn wear_grip(wear: f32) -> f32 {
    1.0 - wear.clamp(0.0, 1.0) * (1.0 - WEAR_GRIP_FLOOR)
}

/// Jeden krok integrace teplot jednoho kola (čistá funkce → testovatelná bez Bevy).
/// `v` = podélná rychlost auta (m/s, se znaménkem), `ambient` = teplota okolí (°C),
/// `dt` = délka snímku (s).
pub fn integrate(
    th: &mut crate::solver::WheelThermal,
    brake_cap: f32,
    brake_nm: f32,
    spin_vel: f32,
    radius: f32,
    tm: WheelTelem,
    v: f32,
    ambient: f32,
    dt: f32,
    win_hi: f32,
    wear_rate: f32,
) {
    // BRZDA (jednovrstvá): ohřev z brzdné práce, chlazení konvekcí + pasivně.
    // `brake_cap` = tepelná kapacita kotouče z dílu Brakes (default BRAKE_C_FRONT/REAR).
    let bheat = brake_nm * spin_vel.abs();
    let bcool = (BRAKE_H0 + BRAKE_H1 * v.abs()) * (th.brake_c - ambient);
    th.brake_c = (th.brake_c + (bheat - bcool) / brake_cap * dt).max(ambient);

    // PNEU: skluzová rychlost z KOLA (obvodová − podélná auta), ne z rychlosti auta →
    // burnout z místa (v≈0, kolo se točí) i zámek (kolo stojí, auto jede) hřejí povrch.
    let roll = spin_vel * radius;
    let slip_long = (roll - v).abs().min(SLIP_SPEED_CAP);
    let slip_lat = (v * tm.slip_angle.sin()).abs().min(SLIP_SPEED_CAP);
    let roll_heat = ROLL_HEAT * tm.fz * v.abs(); // valivý odpor (i bez skluzu)
    let theat = tm.f_long.abs() * slip_long + tm.f_lat.abs() * slip_lat + roll_heat; // W do povrchu
    let cond = TIRE_COND * (th.tire_surf_c - th.tire_core_c); // povrch → kostra
    let surf_air = (SURF_H0 + SURF_H1 * v.abs()) * (th.tire_surf_c - ambient);
    th.tire_surf_c = (th.tire_surf_c + (theat - cond - surf_air) / TIRE_C_SURF * dt).max(ambient);
    let core_air = CORE_H * (th.tire_core_c - ambient);
    th.tire_core_c = (th.tire_core_c + (cond - core_air) / TIRE_C_CORE * dt).max(ambient);

    // OPOTŘEBENÍ: skluzová (třecí) práce obrušuje gumu; přehřátá pneu se dře rychleji.
    // Nad BURST_WEAR pneu praskne (burst) — grip spadne (řeší solver + wheel_thermal zvuk).
    let slip_work = (tm.f_long.abs() * slip_long + tm.f_lat.abs() * slip_lat) * dt; // J
    let heat_accel = 1.0 + (th.tire_surf_c - win_hi).max(0.0) / 40.0;
    th.wear = (th.wear + slip_work * wear_rate * heat_accel).min(1.2);
    if th.wear >= BURST_WEAR {
        th.burst = true;
    }
}

/// Jednou za snímek: integruje teploty všech kol ze stavu (brake, spin_vel, telem). Běží
/// v Update po `vehicle_drive`, takže čte už fade-nutý `wheel.brake` — míň brzdy = míň
/// tepla (self-limiting). 1-frame zpoždění teploty proti fade je zanedbatelné (děj pomalý).
pub fn wheel_thermal(
    mut sb: ResMut<SoftBody>,
    state: Res<DrivetrainState>,
    loadout: Res<Loadout>,
    ambient: Res<Ambient>,
    time: Res<Time>,
    mut burst_ev: EventWriter<crate::solver::CrashImpact>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let v = state.speed_kmh / 3.6; // podélná rychlost auta (m/s, se znaménkem)
    let compound = loadout.compound; // pneu díl (okno + wear rate) — společný všem kolům
    for wheel in &mut sb.wheels {
        let was_burst = wheel.thermal.burst;
        // kapacita kotouče z dílu Brakes dle nápravy
        let brake_cap = if wheel.front { loadout.brakes.c_front } else { loadout.brakes.c_rear };
        integrate(
            &mut wheel.thermal,
            brake_cap,
            wheel.brake,
            wheel.spin_vel,
            wheel.radius,
            wheel.telem,
            v,
            ambient.0,
            dt,
            compound.win_hi,
            compound.wear_rate,
        );
        if !was_burst && wheel.thermal.burst {
            // prasknutí = rána; recykluju crash zvuk z PR #8 (bang) místo nového voice
            burst_ev.send(crate::solver::CrashImpact(4.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brake_fade_curve() {
        let approx = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // pod prahem = plný moment
        assert_eq!(brake_fade(300.0, FADE_START), 1.0);
        assert_eq!(brake_fade(FADE_START, FADE_START), 1.0);
        // v pásmu = slábne, ale ne pod podlahu
        let mid = brake_fade(600.0, FADE_START);
        assert!(mid < 1.0 && mid > FADE_FLOOR);
        // nad plným fade = podlaha (a zůstane, neklesá dál)
        assert!(approx(brake_fade(FADE_FULL, FADE_START), FADE_FLOOR));
        assert!(approx(brake_fade(2000.0, FADE_START), FADE_FLOOR));
        // nižší práh (měkčí brzda) fadne dřív: při 500 °C už slábne, default drží plný
        assert!(brake_fade(500.0, 400.0) < brake_fade(500.0, FADE_START));
    }

    #[test]
    fn grip_mult_window() {
        let approx = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // v provozním okně = neutrální (žádná změna Pacejky)
        assert_eq!(grip_mult(90.0, WIN_LO, WIN_HI), 1.0);
        assert_eq!(grip_mult(WIN_LO, WIN_LO, WIN_HI), 1.0);
        assert_eq!(grip_mult(WIN_HI, WIN_LO, WIN_HI), 1.0);
        // studená i přehřátá < 1.0
        assert!(grip_mult(TIRE_COLD - 10.0, WIN_LO, WIN_HI) < 1.0);
        assert!(grip_mult(TIRE_HOT + 20.0, WIN_LO, WIN_HI) < 1.0);
        // cold-start ambient je pod oknem → < 1.0 (zahřeje se do okna při jízdě)
        assert!(grip_mult(T_AMBIENT, WIN_LO, WIN_HI) < 1.0);
        // podlahy
        assert!(approx(grip_mult(0.0, WIN_LO, WIN_HI), GRIP_COLD));
        assert!(approx(grip_mult(300.0, WIN_LO, WIN_HI), GRIP_HOT));
        // posunuté okno (kompound): při 108 °C default už fadne, širší okno drží plató
        assert_eq!(grip_mult(108.0, WIN_LO, 115.0), 1.0);
        assert!(grip_mult(108.0, WIN_LO, WIN_HI) < 1.0);
    }

    #[test]
    fn tire_squeal_temp() {
        assert_eq!(tire_squeal_gain(90.0), 1.0); // v okně = beze změny
        assert!(tire_squeal_gain(30.0) > 1.0); // studená = hlasitější screech
        assert!(tire_squeal_gain(160.0) > 1.0); // přehřátá = hlasitější
    }

    #[test]
    fn engine_thermal() {
        let approx = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // pod overheat = plný výkon; nad = klesá na floor
        assert_eq!(engine_power_factor(90.0), 1.0);
        assert_eq!(engine_power_factor(ENGINE_OVERHEAT), 1.0);
        assert!(engine_power_factor(125.0) < 1.0);
        assert!(approx(engine_power_factor(200.0), ENGINE_POWER_FLOOR));
        let dt = 1.0 / 60.0;
        // idle stojící se ustálí kolem ~90 °C (laděno)
        let mut idle = 90.0;
        for _ in 0..60 * 300 {
            idle = engine_step(idle, true, 0.0, 0.0, 30.0, dt, ENGINE_H0, ENGINE_H1);
        }
        assert!((80.0..100.0).contains(&idle), "idle={idle}");
        // 2 min plné zátěže bez airflow (v=0) → přehřátí
        let mut hot = 90.0;
        for _ in 0..60 * 120 {
            hot = engine_step(hot, true, 1.0, 0.0, 30.0, dt, ENGINE_H0, ENGINE_H1);
        }
        assert!(hot > ENGINE_OVERHEAT, "sustained load nepřehřálo: {hot}");
        // chcíplý (running=false) chladne k ambient
        let mut cool = 120.0;
        for _ in 0..60 * 300 {
            cool = engine_step(cool, false, 0.0, 20.0, 30.0, dt, ENGINE_H0, ENGINE_H1);
        }
        assert!(cool < 40.0, "nevychladlo: {cool}");
        // lepší chladič (vyšší h0/h1) drží nižší ustálenou teplotu při stejné zátěži
        let (mut stock, mut race) = (90.0, 90.0);
        for _ in 0..60 * 120 {
            stock = engine_step(stock, true, 0.6, 20.0, 30.0, dt, ENGINE_H0, ENGINE_H1);
            race = engine_step(race, true, 0.6, 20.0, 30.0, dt, 110.0, 50.0);
        }
        assert!(race < stock, "race cooler nechladí líp: race={race} stock={stock}");
    }

    #[test]
    fn integrate_heats_and_cools() {
        let dt = 1.0 / 60.0;
        // skluz: velká podélná síla + protáčení (roll ≠ v) → povrch hřeje
        let slipping = WheelTelem { fz: 4000.0, slip_ratio: 0.5, slip_angle: 0.0, f_long: 3000.0, f_lat: 0.0 };
        let mut th = crate::solver::WheelThermal::default();
        let start = th.brake_c;
        // 10 s brzdění + skluz při 28 m/s
        for _ in 0..600 {
            integrate(&mut th, BRAKE_C_FRONT, 1000.0, 80.0, 0.33, slipping, 28.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE);
        }
        assert!(th.brake_c > start + 50.0, "brake_c={}", th.brake_c);
        assert!(th.tire_surf_c > start + 20.0, "surf={}", th.tire_surf_c);
        // povrch teplejší než kostra (setrvačnost kostry)
        assert!(th.tire_surf_c > th.tire_core_c, "surf={} core={}", th.tire_surf_c, th.tire_core_c);

        // burnout z MÍSTA (v≈0, kolo se točí) MUSÍ hřát povrch — dřív gate na rychlost auta
        // to zabíjel. Bez brzdy, bez pohybu auta.
        let mut bo = crate::solver::WheelThermal::default();
        let before = bo.tire_surf_c;
        for _ in 0..120 {
            integrate(&mut bo, BRAKE_C_REAR, 0.0, 100.0, 0.33, slipping, 0.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE);
        }
        assert!(bo.tire_surf_c > before + 30.0, "burnout surf={}", bo.tire_surf_c);

        // samotné valení pod zátěží (bez skluzu, fz>0) pneu POSTUPNĚ zahřeje valivým
        // odporem → cold-start se dostane k oknu i bez smyku.
        let mut warm = crate::solver::WheelThermal::default();
        let cruise = WheelTelem { fz: 3500.0, slip_ratio: 0.0, slip_angle: 0.0, f_long: 0.0, f_lat: 0.0 };
        for _ in 0..3600 {
            integrate(&mut warm, BRAKE_C_FRONT, 0.0, 20.0 / 0.33, 0.33, cruise, 20.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE); // roll = v → 0 skluz
        }
        assert!(warm.tire_surf_c > 55.0, "warm-up surf={}", warm.tire_surf_c);

        // coasting: brake=0, čisté valení (žádný skluz) → obojí chladne
        let hot_b = th.brake_c;
        let hot_s = th.tire_surf_c;
        let rolling = WheelTelem::default();
        for _ in 0..3000 {
            integrate(&mut th, BRAKE_C_FRONT, 0.0, 80.0, 0.33, rolling, 28.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE);
        }
        assert!(th.brake_c < hot_b, "brzda nechladne: {}", th.brake_c);
        assert!(th.tire_surf_c < hot_s, "povrch nechladne: {}", th.tire_surf_c);
    }

    // ZÁRUKA no-change: default díly (Fáze 1 thermal→parts) musí nést PŘESNĚ current
    // konstanty → osazení stock dílu = beze změny feelu. Kdyby někdo hnul defaultem, spadne.
    #[test]
    fn default_parts_equal_constants() {
        use crate::vehicle::{Brakes, Compound, Cooler};
        let c = Compound::default();
        assert_eq!(c.grip, 1.0);
        assert_eq!(c.win_lo, WIN_LO);
        assert_eq!(c.win_hi, WIN_HI);
        assert_eq!(c.wear_rate, WEAR_RATE);
        assert_eq!(c.press_sens, 0.0); // tlak grip nemění na stock (neutrální vrstva)
        let cool = Cooler::default();
        assert_eq!(cool.h0, ENGINE_H0);
        assert_eq!(cool.h1, ENGINE_H1);
        // Brakes default (přes Loadout) = kapacity + fade práh z konstant
        let b = crate::vehicle::Loadout::default().brakes;
        assert_eq!(b.c_front, BRAKE_C_FRONT);
        assert_eq!(b.c_rear, BRAKE_C_REAR);
        assert_eq!(b.fade_start, FADE_START);
    }

    #[test]
    fn tire_pressure_and_grip() {
        let approx = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // horká pneu = vyšší tlak než studená (v ambient = base)
        assert!(approx(tire_pressure(T_AMBIENT, 2.0, T_AMBIENT), 2.0));
        assert!(tire_pressure(120.0, 2.0, T_AMBIENT) > 2.0);
        // default kompound (sens 0) = grip ×1.00 VŽDY, i horká/studená → grip zaparkovaný
        assert_eq!(pressure_grip(T_AMBIENT, 2.0, T_AMBIENT, WIN_LO, WIN_HI, 0.0), 1.0);
        assert_eq!(pressure_grip(140.0, 2.0, T_AMBIENT, WIN_LO, WIN_HI, 0.0), 1.0);
        // citlivý kompound (sens>0): v okně ~ideál (×1.0), přehřátý mimo okno ztrácí
        let win_mid = 0.5 * (WIN_LO + WIN_HI);
        assert!(approx(pressure_grip(win_mid, 2.0, T_AMBIENT, WIN_LO, WIN_HI, 0.15), 1.0));
        assert!(pressure_grip(150.0, 2.0, T_AMBIENT, WIN_LO, WIN_HI, 0.15) < 1.0);
        // strop ztráty
        assert!(pressure_grip(1000.0, 2.0, T_AMBIENT, WIN_LO, WIN_HI, 5.0) >= PRESS_GRIP_FLOOR);
    }

    #[test]
    fn asymmetric_heating_from_fz() {
        // Přenos váhy: vnější kolo v zatáčce nese víc Fz → víc boční síly → hřeje víc.
        // Per-kolo integrate krmené SKUTEČNÝM telem.fz (a silami ∝ Fz) → asymetrie sama.
        let dt = 1.0 / 60.0;
        let alpha = 0.1_f32; // stejný skluzový úhel obou kol
        // vnější: velký Fz + velká boční síla; vnitřní: půlka
        let outer = WheelTelem { fz: 5000.0, slip_ratio: 0.0, slip_angle: alpha, f_long: 0.0, f_lat: 4500.0 };
        let inner = WheelTelem { fz: 2500.0, slip_ratio: 0.0, slip_angle: alpha, f_long: 0.0, f_lat: 2250.0 };
        let (mut o, mut i) = (crate::solver::WheelThermal::default(), crate::solver::WheelThermal::default());
        for _ in 0..600 {
            integrate(&mut o, BRAKE_C_FRONT, 0.0, 20.0 / 0.33, 0.33, outer, 20.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE);
            integrate(&mut i, BRAKE_C_FRONT, 0.0, 20.0 / 0.33, 0.33, inner, 20.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE);
        }
        assert!(o.tire_surf_c > i.tire_surf_c, "outer={} inner={}", o.tire_surf_c, i.tire_surf_c);
    }

    #[test]
    fn tire_wear_and_burst() {
        let approx = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert_eq!(wear_grip(0.0), 1.0); // nová = neutrální
        assert!(approx(wear_grip(1.0), WEAR_GRIP_FLOOR)); // ojetá = podlaha
        let dt = 1.0 / 60.0;
        // dlouhý burnout (velký skluz + síla) obrousí a prasne
        let slip = WheelTelem { fz: 4000.0, slip_ratio: 1.0, slip_angle: 0.0, f_long: 3000.0, f_lat: 0.0 };
        let mut th = crate::solver::WheelThermal::default();
        for _ in 0..60 * 120 {
            integrate(&mut th, BRAKE_C_REAR, 0.0, 120.0, 0.33, slip, 0.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE);
        }
        assert!(th.burst, "neprasklo: wear={}", th.wear);
        // měkčí kompound (vyšší wear_rate) se sdře rychleji za stejný skluz. Krátce (1 s),
        // ať oba zůstanou pod stropem wear (1.2) a je vidět poměr.
        let mut soft = crate::solver::WheelThermal::default();
        let mut hard = crate::solver::WheelThermal::default();
        for _ in 0..60 {
            integrate(&mut soft, BRAKE_C_REAR, 0.0, 120.0, 0.33, slip, 0.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE * 1.5);
            integrate(&mut hard, BRAKE_C_REAR, 0.0, 120.0, 0.33, slip, 0.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE * 0.6);
        }
        assert!(soft.wear < 1.2 && hard.wear < 1.2, "saturace: soft={} hard={}", soft.wear, hard.wear);
        assert!(soft.wear > hard.wear, "soft={} hard={}", soft.wear, hard.wear);
        // klidná jízda (bez skluzu) skoro neobrušuje
        let cruise = WheelTelem { fz: 3500.0, slip_ratio: 0.0, slip_angle: 0.0, f_long: 0.0, f_lat: 0.0 };
        let mut easy = crate::solver::WheelThermal::default();
        for _ in 0..60 * 60 {
            integrate(&mut easy, BRAKE_C_FRONT, 0.0, 20.0 / 0.33, 0.33, cruise, 20.0, T_AMBIENT, dt, WIN_HI, WEAR_RATE);
        }
        assert!(easy.wear < 0.05, "klidná jízda moc obrušuje: {}", easy.wear);
    }
}
