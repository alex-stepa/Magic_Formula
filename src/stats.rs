// stats.rs — odvozené parametry buildu pro garáž (KPI: výkon, hmotnost, rozložení, 0–100).
// Poctivý výpočet z REÁLNÝCH dat Loadoutu, ne fabrikace: výkon i hmotnost jsou přesné,
// 0–100 je deterministická zjednodušená integrace (bez aera/prokluzu spojky/času řazení).
// Sdílené s budoucí Fází 4.2 (dyno, weight distribution) — čisté funkce, žádný Bevy stav.

use crate::vehicle::{ChassisParams, DriveLayout, Engine, Gearbox};
use crate::{veh, vehicle};

// Špičkový výkon (kW) z momentové křivky: P = M·ω, ω = rpm·2π/60. Vzorkuje po 50 rpm
// do omezovače (interpolovaný vrchol křivky, ne jen v zadaných bodech).
pub fn peak_power_kw(engine: &Engine) -> f32 {
    let mut peak = 0.0_f32;
    let mut rpm = engine.torque_curve.first().map(|p| p.0).unwrap_or(0.0);
    while rpm < engine.max_rpm {
        let p = vehicle::engine_torque(rpm, engine) * rpm * std::f32::consts::TAU / 60.0;
        peak = peak.max(p);
        rpm += 50.0;
    }
    peak / 1000.0
}

// Hmotnost testovacího auta (kg) = 8 rohů karoserie + 4 huby (viz spawn_solver_car).
pub fn test_car_mass(chassis: &ChassisParams) -> f32 {
    8.0 * chassis.body_mass + 4.0 * chassis.hub_mass
}

// Hmotnost (kg) a rozložení přední osy (0..1) z .veh dat. Axle nody kol (do solveru
// nejdou) se přeskočí, místo nich 4× hub_mass. Rozložení podle podélné osy (game +Z =
// dopředu = −rawY, viz veh_pos): nody před těžištěm = přední.
pub fn veh_mass_and_front(vehicle: &veh::Vehicle, hub_mass: f32) -> (f32, f32) {
    let axle: std::collections::HashSet<&str> =
        vehicle.wheels.iter().flat_map(|w| [w.node1.as_str(), w.node2.as_str()]).collect();
    let nodes: Vec<(f32, f32)> = vehicle
        .nodes
        .iter()
        .filter(|n| !axle.contains(n.id.as_str()))
        .map(|n| (n.mass, -n.pos[1])) // (hmota, podélná pozice v game space)
        .collect();
    let body_mass: f32 = nodes.iter().map(|(m, _)| m).sum();
    let mass = body_mass + 4.0 * hub_mass;
    if nodes.is_empty() || body_mass <= 0.0 {
        return (mass, 0.5);
    }
    // těžiště podél podélné osy, vážené hmotou
    let cg = nodes.iter().map(|(m, z)| m * z).sum::<f32>() / body_mass;
    let front: f32 = nodes.iter().filter(|(_, z)| *z >= cg).map(|(m, _)| m).sum();
    (mass, front / body_mass)
}

// Odhad 0–100 km/h (s). Deterministická integrace: síla na kole = moment motoru přes
// převod / poloměr, omezená trakcí (μ·hmotnost·g·podíl na hnané ose). Řadí na omezovači.
// ponytail: bez aera, prokluzu spojky a času řazení — hrubý, ale z REÁLNÝCH dílů; upgrade
// přijde s test-loop simulací (4.2). Vrací None, když auto nezrychlí (přetížené / bez tahu).
pub fn estimate_0_100_s(
    engine: &Engine,
    gearbox: &Gearbox,
    layout: DriveLayout,
    mass: f32,
    wheel_radius: f32,
    friction: f32,
    front_frac: f32,
) -> Option<f32> {
    const G: f32 = 9.81;
    const TARGET: f32 = 100.0 / 3.6; // m/s
    const DT: f32 = 0.01;
    const EFF: f32 = 0.9; // účinnost hnacího ústrojí
    // Podíl statické váhy na hnané ose (trakční strop).
    let driven = match layout {
        DriveLayout::Fwd => front_frac,
        DriveLayout::Rwd => 1.0 - front_frac,
        DriveLayout::Awd { .. } => 1.0,
    };
    let f_max = friction * mass * G * driven;
    let mut v = 0.0_f32;
    let mut t = 0.0_f32;
    while v < TARGET {
        // vyber nejnižší kvalt, kde motor není přes omezovač (jinak drž nejvyšší)
        let mut wheel_force = 0.0;
        for (gi, &ratio) in gearbox.ratios.iter().enumerate() {
            let total = ratio * gearbox.final_drive;
            let rpm = v / wheel_radius * total * 60.0 / std::f32::consts::TAU;
            if rpm < engine.max_rpm || gi == gearbox.ratios.len() - 1 {
                wheel_force = vehicle::engine_torque(rpm.min(engine.max_rpm), engine) * total * EFF / wheel_radius;
                break;
            }
        }
        let f = wheel_force.min(f_max);
        let a = f / mass;
        if a <= 0.01 {
            return None; // nezrychluje
        }
        v += a * DT;
        t += DT;
        if t > 60.0 {
            return None; // limit
        }
    }
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle::Loadout;

    #[test]
    fn test_car_mass_is_1100() {
        assert_eq!(test_car_mass(&ChassisParams::default()), 1100.0);
    }

    #[test]
    fn stock_engine_power_plausible() {
        let l = Loadout::default();
        let kw = peak_power_kw(&l.engine);
        // Default 1.4 L atmosférický → reálných ~65 kW (~90 hp).
        assert!((40.0..100.0).contains(&kw), "výkon {kw} kW mimo očekávání");
    }

    #[test]
    fn stock_0_100_plausible() {
        let l = Loadout::default();
        let t = estimate_0_100_s(&l.engine, &l.gearbox, l.layout, 1100.0, 0.34, 1.0, 0.5)
            .expect("mělo zrychlit");
        // Arkádní křivka ~100 kW / 1100 kg → řádově 6–14 s.
        assert!((4.0..20.0).contains(&t), "0–100 = {t} s mimo očekávání");
    }
}
