// world.rs — terén jako trait: solver se ptá jen na výšku/normálu povrchu, konkrétní
// implementace (rovina, procedurální kopce, časem OSM+SRTM heightfield) je vyměnitelná.

use std::path::Path;

use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;

use crate::solver::GROUND_TOP;

pub trait World: Send + Sync {
    /// Výška povrchu v bodě (x, z) — světové souřadnice, y nahoru.
    fn height(&self, x: f32, z: f32) -> f32;

    /// Normála povrchu. Default = konečné diference přes height() — stačí implementovat
    /// výšku a normála „je zdarma" (analytickou dodej jen když je levná).
    fn normal(&self, x: f32, z: f32) -> Vec3 {
        let e = 0.1;
        let hx = self.height(x + e, z) - self.height(x - e, z);
        let hz = self.height(x, z + e) - self.height(x, z - e);
        Vec3::new(-hx, 2.0 * e, -hz).normalize()
    }
}

// Placatá zem — chová se přesně jako starý solver s konstantou GROUND_TOP.
pub struct FlatWorld;

impl World for FlatWorld {
    fn height(&self, _x: f32, _z: f32) -> f32 {
        GROUND_TOP
    }
    fn normal(&self, _x: f32, _z: f32) -> Vec3 {
        Vec3::Y
    }
}

// Procedurální kopce: rovina okolo spawnu (ať se auto ladí na rovině), dál zvlněná
// krajina sinus×sinus. Max sklon ≈ amplitude·2π/wavelength — drž pod ~20°, jinak
// auta kloužou (tangenciální tření v solveru je zatím hrubé).
pub struct Hills {
    pub amplitude: f32,   // výška kopců (m)
    pub wavelength: f32,  // vzdálenost mezi kopci (m)
    pub flat_radius: f32, // poloměr roviny okolo spawnu (m)
    pub blend: f32,       // šířka přechodu rovina→kopce (m)
}

impl Default for Hills {
    fn default() -> Self {
        Self { amplitude: 5.0, wavelength: 80.0, flat_radius: 120.0, blend: 150.0 }
    }
}

impl World for Hills {
    fn height(&self, x: f32, z: f32) -> f32 {
        let r = (x * x + z * z).sqrt();
        let t = ((r - self.flat_radius) / self.blend).clamp(0.0, 1.0);
        let fade = t * t * (3.0 - 2.0 * t); // smoothstep: hladký nájezd kopců
        GROUND_TOP + fade * self.amplitude * (x / self.wavelength).sin() * (z / self.wavelength).sin()
    }
}

// Mřížka výšek s bilineární interpolací. Zdroj dat může být cokoliv: testovací data,
// načtený SRTM .hgt soubor, generovaný terén…
//
// Přepnutí: v Terrain::default() nahraď Hills::default() za HeightField::test_hill()
pub struct HeightField {
    heights: Vec<f32>, // row-major: heights[row * cols + col]
    cols: usize,
    rows: usize,
    origin_x: f32, // světové X pro col = 0
    origin_z: f32, // světové Z pro row = 0
    spacing: f32,  // metry na buňku
}

impl HeightField {
    /// 11×11 grid, spacing 30 m → 300×300 m. Kopeček 8 m uprostřed, okraje na GROUND_TOP.
    pub fn test_hill() -> Self {
        let cols = 11usize;
        let rows = 11usize;
        let spacing = 30.0f32;
        let cx = (cols / 2) as f32;
        let cz = (rows / 2) as f32;
        let heights = (0..rows * cols)
            .map(|i| {
                let col = (i % cols) as f32;
                let row = (i / cols) as f32;
                let d = ((col - cx).powi(2) + (row - cz).powi(2)).sqrt();
                let h = (1.0 - (d / 4.0).clamp(0.0, 1.0)) * 8.0;
                GROUND_TOP + h
            })
            .collect();
        let half = (cols - 1) as f32 * spacing / 2.0;
        Self { heights, cols, rows, origin_x: -half, origin_z: -half, spacing }
    }

    /// Načte SRTM .hgt soubor → HeightField.
    ///
    /// Detekuje typ z velikosti souboru:
    ///   SRTM1 = 3601×3601 × 2 B = ~25.9 MB (1 arc-sec ≈ 30 m)
    ///   SRTM3 = 1201×1201 × 2 B = ~2.9 MB  (3 arc-sec ≈ 90 m)
    ///
    /// `origin_x/z`: světové souřadnice (X, Z) pro nejzápadnější-nejjižnější roh.
    /// Pro centrování na spawnpoint: -side * spacing / 2.0.
    ///
    /// Void hodnoty (-32768) jsou nahrazeny 0 m (nerelevantní nad mořem).
    pub fn from_hgt(path: &Path, origin_x: f32, origin_z: f32) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() % 2 != 0 {
            return Err(format!("Lichý počet bajtů: {} — není to .hgt?", bytes.len()));
        }
        let samples = bytes.len() / 2;
        let side = (samples as f64).sqrt().round() as usize;
        if side * side != samples {
            return Err(format!("Soubor není čtvercový grid (samples={})", samples));
        }

        // Spacing v metrech — zeměpisná šířka: 1 arc-sec ≈ 30.87 m.
        // Zeměpisná délka závisí na šířce (cos(lat)), ale pro renderování
        // hry bereme izotropní aproximaci (přijatelné do ~70° šířky).
        let spacing = match side {
            3601 => 30.87_f32,
            1201 => 92.6_f32,
            _    => 30.87_f32 * 3601.0 / side as f32, // jiné rozlišení
        };

        // SRTM: řádky jdou Sever→Jih (row 0 = nejsevernější).
        // HeightField: row 0 = nejmenší Z (= jih v naší konvenci). Přetočíme řádky.
        let raw: Vec<f32> = bytes
            .chunks_exact(2)
            .map(|b| {
                let v = i16::from_be_bytes([b[0], b[1]]);
                if v == -32768 { 0.0 } else { v as f32 }
            })
            .collect();

        let mut heights = vec![0.0f32; side * side];
        for r in 0..side {
            let src = (side - 1 - r) * side; // přetoč pořadí řádků
            heights[r * side..r * side + side].copy_from_slice(&raw[src..src + side]);
        }

        Ok(Self { heights, cols: side, rows: side, origin_x, origin_z, spacing })
    }

    fn sample(&self, col: usize, row: usize) -> f32 {
        self.heights[row * self.cols + col]
    }
}

impl World for HeightField {
    fn height(&self, x: f32, z: f32) -> f32 {
        let cf = ((x - self.origin_x) / self.spacing).clamp(0.0, (self.cols - 1) as f32);
        let rf = ((z - self.origin_z) / self.spacing).clamp(0.0, (self.rows - 1) as f32);
        // .min(…-2) zajistí že ci+1/ri+1 jsou vždy platné indexy
        let ci = (cf.floor() as usize).min(self.cols - 2);
        let ri = (rf.floor() as usize).min(self.rows - 2);
        let u = cf - ci as f32;
        let v = rf - ri as f32;
        self.sample(ci,     ri    ) * (1.0 - u) * (1.0 - v)
            + self.sample(ci + 1, ri    ) * u         * (1.0 - v)
            + self.sample(ci,     ri + 1) * (1.0 - u) * v
            + self.sample(ci + 1, ri + 1) * u         * v
    }
}

// Aktivní terén jako resource — solver i stavba render meshe čtou přes trait objekt.
#[derive(Resource)]
pub struct Terrain(pub Box<dyn World>);

impl Default for Terrain {
    fn default() -> Self {
        // SRTM1, 3601×3601, spacing ~30.87 m → dlaždice je 3600×30.87 ≈ 111 km na stranu.
        // Centrujeme na střed dlaždice (polovinu strany v každém směru).
        let half = 1800.0_f32 * 30.87;
        let hf = HeightField::from_hgt(
            Path::new("assets/N50E014.hgt"),
            -half, -half,
        )
        .unwrap_or_else(|e| {
            eprintln!("SRTM chyba: {e}");
            HeightField::test_hill()
        });
        Self(Box::new(hf))
    }
}

// Render mesh terénu: pravidelná mřížka vzorkuje height() a normal(). UV jde 0..1 přes
// celou plochu (stejně jako Plane3d) — mřížkovou texturu škáluje uv_transform v materiálu.
pub fn terrain_mesh(world: &dyn World, size: f32, step: f32) -> Mesh {
    let n = (size / step) as u32; // počet buněk na stranu
    let verts_per_side = n + 1;
    let half = size / 2.0;

    let mut positions = Vec::with_capacity((verts_per_side * verts_per_side) as usize);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    for zi in 0..verts_per_side {
        for xi in 0..verts_per_side {
            let x = -half + xi as f32 * step;
            let z = -half + zi as f32 * step;
            positions.push([x, world.height(x, z), z]);
            normals.push(world.normal(x, z).to_array());
            uvs.push([xi as f32 / n as f32, zi as f32 / n as f32]);
        }
    }

    let mut indices = Vec::with_capacity((n * n * 6) as usize);
    for zi in 0..n {
        for xi in 0..n {
            let a = zi * verts_per_side + xi;
            let b = a + 1;
            let c = a + verts_per_side;
            let d = c + 1;
            // dva trojúhelníky na buňku, CCW při pohledu shora (y nahoru)
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
