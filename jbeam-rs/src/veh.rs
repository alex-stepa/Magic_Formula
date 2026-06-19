// veh.rs — datové structy vlastního formátu .veh (výstup konvertoru).
//
// Serialize  = struct umí ZAPSAT do JSON (to potřebuje konvertor jbeam → veh).
// Deserialize = struct umí NAČÍST z JSON (to využije později hra při loadu auta).
// Díky tomu je tenhle soubor "smlouva" o formátu sdílená oběma stranami.

// allow(dead_code): pole zatím jen plníme/serializujeme; číst je bude až hra.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Celé vozidlo — kořen .veh souboru.
#[derive(Debug, Serialize, Deserialize)]
pub struct Vehicle {
    pub name: String,    // jméno auta (např. "fabia")
    pub version: u32,    // verze formátu (teď 1)
    pub nodes: Vec<Node>, // hmotné body
    pub beams: Vec<Beam>, // pružné spoje mezi nody
}

/// Jeden hmotný bod (node).
#[derive(Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: String,    // unikátní jméno nodu (např. "f1rr")
    pub pos: [f32; 3], // pozice [x, y, z]
    pub mass: f32,     // hmotnost bodu
}

/// Jeden spoj (beam) mezi dvěma nody — odkazuje na ně přes jejich id.
#[derive(Debug, Serialize, Deserialize)]
pub struct Beam {
    pub a: String, // id prvního nodu
    pub b: String, // id druhého nodu
}
