// veh.rs — načtení vlastního .veh formátu do hry.
// Tady jen Deserialize (umí se NAČÍST z JSON). Zápis dělá konvertor jbeam-rs.
// Structy se musí shodovat s těmi v jbeam-rs/src/veh.rs (sdílená "smlouva" o formátu).

// allow(dead_code): některá pole (name, version, wheels) zatím nečteme.
#![allow(dead_code)]

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Vehicle {
    pub name: String,
    pub version: u32,
    pub nodes: Vec<Node>,
    pub beams: Vec<Beam>,
    #[serde(default)] // kdyby .veh sekci wheels neměl, bude prázdná
    pub wheels: Vec<Wheel>,
    #[serde(default)]
    pub triangles: Vec<Triangle>,
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
}

#[derive(Debug, Deserialize)]
pub struct Wheel {
    pub name: String,
    pub node1: String,
    pub node2: String,
    pub dir: i32,
    // Rozměry z jbeam pressureWheels (radius/tireWidth). 0 = v datech nebyly
    // (tire party bývají v BeamNG common obsahu mimo mod) → hra dosadí fallback.
    #[serde(default)]
    pub radius: f32,
    #[serde(default)]
    pub width: f32,
}
