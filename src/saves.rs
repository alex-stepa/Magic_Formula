// saves.rs — uložené buildy do player data (player/builds/*.json).
// Build = laděná výbava (motor spec, zavěšení, převodovka, brzdy, layout, diff, auto).
// Serializuje se přes serde; std::fs jako zbytek projektu (žádný extra dep).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::vehicle::{Brakes, CarChoice, Differential, DriveLayout, EngineSpec, Gearbox, Loadout, SuspensionKind};

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedBuild {
    pub name: String,
    pub car: CarChoice,
    pub engine: EngineSpec,
    #[serde(default = "default_susp")]
    pub front_susp: SuspensionKind,
    #[serde(default = "default_susp")]
    pub rear_susp: SuspensionKind,
    pub gearbox: Gearbox,
    pub brakes: Brakes,
    pub layout: DriveLayout,
    pub front_lock: f32,
    pub rear_lock: f32,
    #[serde(default)]
    pub front_offset: f32,
    #[serde(default)]
    pub rear_offset: f32,
}

impl SavedBuild {
    pub fn from_loadout(l: &Loadout, name: String) -> Self {
        Self {
            name,
            car: l.car,
            engine: l.engine_spec,
            front_susp: l.front_susp,
            rear_susp: l.rear_susp,
            gearbox: l.gearbox.clone(),
            brakes: l.brakes.clone(),
            layout: l.layout,
            front_lock: l.front_diff.lock,
            rear_lock: l.rear_diff.lock,
            front_offset: l.front_offset,
            rear_offset: l.rear_offset,
        }
    }
    // Namontuj uložený build do výbavy (motor se přegeneruje z spec).
    pub fn apply(&self, l: &mut Loadout) {
        l.car = self.car;
        l.engine_spec = self.engine;
        l.engine = self.engine.build();
        l.front_susp = self.front_susp;
        l.rear_susp = self.rear_susp;
        l.gearbox = self.gearbox.clone();
        l.brakes = self.brakes.clone();
        l.layout = self.layout;
        l.front_diff = Differential { lock: self.front_lock };
        l.rear_diff = Differential { lock: self.rear_lock };
        l.front_offset = self.front_offset;
        l.rear_offset = self.rear_offset;
    }
    // Automatický popisný název: objem + plnění + zavěšení (např. "1.4L NA · Coil").
    pub fn auto_name(l: &Loadout) -> String {
        let asp = if l.engine_spec.boost > 0.05 { "Turbo" } else { "NA" };
        let code = |k: SuspensionKind| match k {
            SuspensionKind::Rigid => "Rigid",
            SuspensionKind::Coil { .. } => "Coil",
            SuspensionKind::McPherson { .. } => "McP",
            SuspensionKind::DoubleWishbone { .. } => "SLA",
            SuspensionKind::TorsionBeam { .. } => "TB",
            SuspensionKind::LeafSpring { .. } => "Leaf",
        };
        // Stejné vpředu i vzadu → jeden kód; jinak "předek/zadek".
        let susp = if l.front_susp.same_variant(l.rear_susp) {
            code(l.front_susp).to_string()
        } else {
            format!("{}/{}", code(l.front_susp), code(l.rear_susp))
        };
        format!("{:.1}L {asp} · {susp} · {}", l.engine_spec.displacement, l.layout.label())
    }
}

fn dir() -> PathBuf {
    PathBuf::from("player/builds")
}

// Fallback pro staré save soubory bez front_susp/rear_susp polí.
fn default_susp() -> SuspensionKind {
    SuspensionKind::COIL
}

// Název → bezpečné jméno souboru (alfanumerika + _).
fn file_stem(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect()
}

pub fn save(b: &SavedBuild) -> std::io::Result<()> {
    std::fs::create_dir_all(dir())?;
    let path = dir().join(format!("{}.json", file_stem(&b.name)));
    std::fs::write(path, serde_json::to_string_pretty(b).unwrap())
}

pub fn delete(name: &str) {
    let _ = std::fs::remove_file(dir().join(format!("{}.json", file_stem(name))));
}

fn export_dir() -> PathBuf {
    PathBuf::from("export")
}

// Export mimo player/builds — do export/<jméno>.json, sdílitelné/kopírovatelné ručně.
pub fn export(b: &SavedBuild) -> std::io::Result<()> {
    std::fs::create_dir_all(export_dir())?;
    let path = export_dir().join(format!("{}.json", file_stem(&b.name)));
    std::fs::write(path, serde_json::to_string_pretty(b).unwrap())
}

// Načte všechny buildy z export/ a uloží je do player/builds (import). Vrací počet.
pub fn import_all() -> usize {
    std::fs::read_dir(export_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| serde_json::from_str::<SavedBuild>(&s).ok())
        .filter(|b| save(b).is_ok())
        .count()
}

// Načte všechny uložené buildy (seřazené podle jména). Chybný soubor přeskočí.
pub fn list() -> Vec<SavedBuild> {
    let mut builds: Vec<SavedBuild> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| serde_json::from_str::<SavedBuild>(&s).ok())
        .collect();
    builds.sort_by(|a, b| a.name.cmp(&b.name));
    builds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_build() {
        let l = Loadout::default();
        let b = SavedBuild::from_loadout(&l, "test".into());
        let json = serde_json::to_string(&b).unwrap();
        let back: SavedBuild = serde_json::from_str(&json).unwrap();
        let mut l2 = Loadout::default();
        l2.engine_spec.displacement = 9.9; // rozbij, ať apply prokazatelně vrátí
        back.apply(&mut l2);
        assert_eq!(l2.engine_spec, l.engine_spec);
        assert_eq!(l2.front_susp, l.front_susp);
        assert_eq!(l2.rear_susp, l.rear_susp);
    }
}
