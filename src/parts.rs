// parts.rs — katalog dílů z JSON (assets/parts/*.json).
// Díl = data: PartData nese jednu z existujících herních struktur (SuspensionKind,
// Engine, …). Garáž nabídne PartOption karty, klik `apply` zapíše do Loadout, spawn
// postaví. Nový díl = nový JSON entry; nová KATEGORIE = 1 arm v apply/is_selected/category.

use bevy::prelude::*;
use serde::Deserialize;

use crate::vehicle::{Gearbox, Loadout, SuspensionKind};

// Jedna položka katalogu = zobrazitelný díl + jeho data.
#[derive(Deserialize)]
pub struct PartOption {
    pub name: String,
    pub note: String,
    pub data: PartData,
}

// Data dílu. Varianta určuje KATEGORII (tab). Externí tag: {"suspension":{"coil":{…}}}.
// Motor/brzdy NEJSOU díly — jsou parametrické (slidery). Zavěšení = topologie karta +
// slidery. Katalog drží diskrétní díly (dnes převodovka + zavěšení kindy).
#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum PartData {
    Suspension(SuspensionKind),
    Gearbox(Gearbox),
}

// Kategorie = tab v garáži. Pořadí = pořadí tabů.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartCategory {
    Suspension,
    Gearbox,
}

impl PartData {
    pub fn category(&self) -> PartCategory {
        match self {
            PartData::Suspension(_) => PartCategory::Suspension,
            PartData::Gearbox(_) => PartCategory::Gearbox,
        }
    }

    // Namontuj díl do výbavy. ponytail: match per kategorie je záměr — kategorií je pár,
    // dílů hodně; nová kategorie = 1 arm, nový díl = 0 Rustu (jen JSON).
    pub fn apply(&self, loadout: &mut Loadout) {
        match self {
            PartData::Suspension(s) => loadout.suspension = *s,
            PartData::Gearbox(g) => loadout.gearbox = g.clone(),
        }
    }

    // Je tenhle díl právě namontovaný? (zvýraznění karty v garáži)
    pub fn is_selected(&self, loadout: &Loadout) -> bool {
        match self {
            // topologie (kind), ne přesná tuhost — tu ladí slidery
            PartData::Suspension(s) => loadout.suspension.same_variant(*s),
            PartData::Gearbox(g) => &loadout.gearbox == g,
        }
    }
}

// Všechny načtené díly (napříč kategoriemi); garáž filtruje dle category().
#[derive(Resource)]
pub struct PartCatalog(pub Vec<PartOption>);

// Načte assets/parts/*.json (každý = pole PartOption). Chybný soubor warn! + přeskoč
// (jako available_vehs toleruje). Stejný vzor: std::fs + serde_json, žádný Bevy loader.
pub fn load_part_catalog() -> PartCatalog {
    let mut parts = Vec::new();
    let Ok(dir) = std::fs::read_dir("assets/parts") else {
        warn!("assets/parts nejde číst — katalog dílů prázdný");
        return PartCatalog(parts);
    };
    let mut files: Vec<_> = dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    for path in files {
        match std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<Vec<PartOption>>(&s).map_err(|e| warn!("{}: {e}", path.display())).ok()) {
            Some(mut v) => parts.append(&mut v),
            None => warn!("{}: přeskočeno", path.display()),
        }
    }
    info!("Katalog dílů: {} položek", parts.len());
    PartCatalog(parts)
}

// Serde tagy jsou snadné rozbít (nested externí tag u zavěšení). Kontrakt: struct
// varianta {"coil":{…}}, unit varianta "rigid", kategorie sedí.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_category() {
        let json = r#"[
            {"name":"S","note":"","data":{"suspension":{"coil":{"arm_k":150000,"coil_k":100000}}}},
            {"name":"R","note":"","data":{"suspension":"rigid"}},
            {"name":"G","note":"","data":{"gearbox":{"ratios":[3.5,2.1],"final_drive":3.9}}}
        ]"#;
        let v: Vec<PartOption> = serde_json::from_str(json).unwrap();
        assert_eq!(v.len(), 3);
        let cats: Vec<_> = v.iter().map(|p| p.data.category()).collect();
        assert_eq!(cats, vec![
            PartCategory::Suspension, PartCategory::Suspension, PartCategory::Gearbox,
        ]);
        assert!(matches!(v[0].data, PartData::Suspension(SuspensionKind::Coil { .. })));
        assert!(matches!(v[1].data, PartData::Suspension(SuspensionKind::Rigid)));
    }
}
