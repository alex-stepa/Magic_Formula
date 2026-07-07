// world.rs — terén jako trait: solver se ptá jen na výšku/normálu povrchu, konkrétní
// implementace (rovina, procedurální kopce, časem OSM+SRTM heightfield) je vyměnitelná.

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

// Aktivní terén jako resource — solver i stavba render meshe čtou přes trait objekt.
#[derive(Resource)]
pub struct Terrain(pub Box<dyn World>);

impl Default for Terrain {
    fn default() -> Self {
        Self(Box::new(Hills::default()))
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
