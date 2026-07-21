// veh.rs — načtení vlastního .veh formátu do hry.
// Tady jen Deserialize (umí se NAČÍST z JSON). Zápis dělá konvertor.
// Jen pole, co hra ČTE — ostatní klíče formátu (version, wheels.name/dir…)
// serde tiše ignoruje; plný formát dokumentuje README.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Vehicle {
    pub name: String,
    pub nodes: Vec<Node>,
    pub beams: Vec<Beam>,
    #[serde(default)] // kdyby .veh sekci wheels neměl, bude prázdná
    pub wheels: Vec<Wheel>,
    #[serde(default)]
    pub triangles: Vec<Triangle>,
    // Motor a převodovka (volitelné — starý .veh je nemá). Přímo structy hry:
    // .veh formát je psaný na míru, zrcadlové structy by byly jen opisování.
    pub engine: Option<crate::vehicle::Engine>,
    pub gearbox: Option<crate::vehicle::Gearbox>,
    pub brakes: Option<crate::vehicle::Brakes>,
    pub layout: Option<crate::vehicle::DriveLayout>,
}

/// Trojúhelník povrchu — tři node id. Vrcholy meshe sledují pozice těchto nodů.
#[derive(Debug, Deserialize)]
pub struct Triangle {
    pub a: String,
    pub b: String,
    pub c: String,
}

#[derive(Debug, Deserialize)]
pub struct Node {
    pub id: String,
    pub pos: [f32; 3],
    pub mass: f32,
}

#[derive(Debug, Deserialize)]
pub struct Beam {
    pub a: String,
    pub b: String,
    #[serde(default)] // starší .veh bez materiálů → 0 (loader pak dosadí fallback)
    pub stiffness: f32,
    #[serde(default)]
    pub damping: f32,
    // Tlumení při natahování (jbeam beamDampRebound); 0 = symetrické (= damping).
    #[serde(default)]
    pub rebound: f32,
    // Klidová délka s předpětím (jbeam beamPrecompression/precompressionRange).
    // None = klidová délka je vzdálenost nodů při spawnu (většina beamů).
    pub rest: Option<f32>,
}

// Kontrakt formátu: starý .veh bez engine/gearbox musí projít (None), nový je načte.
#[cfg(test)]
mod tests {
    #[test]
    fn engine_gearbox_optional() {
        let old = r#"{"name":"x","version":1,"nodes":[],"beams":[]}"#;
        let v: super::Vehicle = serde_json::from_str(old).unwrap();
        assert!(v.engine.is_none() && v.gearbox.is_none());

        let new = r#"{"name":"x","version":1,"nodes":[],"beams":[],
            "engine":{"torque_curve":[[0.0,0.0],[5000.0,159.5]],"idle_rpm":800.0,"max_rpm":6700.0},
            "gearbox":{"ratios":[3.25,1.65,1.03,0.82],"final_drive":3.89},
            "layout":"fwd"}"#;
        let v: super::Vehicle = serde_json::from_str(new).unwrap();
        assert_eq!(v.engine.unwrap().torque_curve[1], (5000.0, 159.5));
        assert_eq!(v.gearbox.unwrap().ratios.len(), 4);
        assert!(matches!(v.layout, Some(crate::vehicle::DriveLayout::Fwd)));

        let awd = r#"{"name":"x","version":1,"nodes":[],"beams":[],"layout":{"awd":{"front":0.5}}}"#;
        let v: super::Vehicle = serde_json::from_str(awd).unwrap();
        assert!(matches!(v.layout, Some(crate::vehicle::DriveLayout::Awd { front }) if front == 0.5));
    }
}

#[derive(Debug, Deserialize)]
pub struct Wheel {
    pub node1: String,
    pub node2: String,
    // Rozměry z jbeam pressureWheels (radius/tireWidth). 0 = v datech nebyly
    // (tire party bývají v BeamNG common obsahu mimo mod) → hra dosadí fallback.
    #[serde(default)]
    pub radius: f32,
    #[serde(default)]
    pub width: f32,
    // Skutečný střed kola (nodeOffset slotu v jbeam). x bývá $výraz → konvertor dá 0
    // a hra stranu odvodí z úchytů. Bez něj se střed aproximuje průměrem úchytů.
    pub center: Option<[f32; 3]>,
}
