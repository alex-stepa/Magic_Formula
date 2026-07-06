# Magic Formula

Rally + freeroam hra s modulární car fyzikální architekturou inspirovanou BeamNG.
Vlastní formát, vlastní implementace, vlastní identita. Potenciálně Steam release.

- **Název:** Magic Formula
- **Repozitáře:** `magic-formula` (hra) + `jbeam-rs` (konvertor)

---

## Dva repozitáře

### 1. `jbeam-rs` — standalone konvertor
Parsuje BeamNG `.jbeam` soubory a konvertuje je do vlastního `.veh` formátu.
Oddělený od hry, není součástí buildu.

### 2. `magic-formula` — hlavní hra
Rust + Bevy, vlastní `.veh` formát, node-beam fyzika na **vlastním solveru**
(rapier zůstal jen pro statický svět/zem).

---

## Tech Stack

| Co | Čím |
|---|---|
| Jazyk | Rust (edition 2024) |
| Engine | Bevy 0.15 |
| Fyzika vozidel | **vlastní node-beam solver** (semi-implicitní Euler + adaptivní substepping) |
| Fyzika světa | bevy_rapier3d 0.28 — jen statická zem / kolize světa |
| Tire model | Magic Formula (Pacejka) — podélně i bočně, friction circle |
| Formát vozidla | vlastní `.veh` (JSON) |
| Mapa (základ) | flat plane (placatá zem) |
| Mapa (budoucnost) | OpenStreetMap + SRTM elevation |
| Serializace | serde + serde_json |
| CLI (jbeam-rs) | clap |

---

## Vlastní formát `.veh`

Inspirovaný .jbeam, ale úplně vlastní. Čitelný JSON. **Tohle je formát, který hra dnes
reálně načítá** (`src/veh.rs`):

```json
{
  "name": "skoda_fabia",
  "version": 1,
  "nodes": [
    { "id": "t5", "pos": [0.0, 1.33, 0.98], "mass": 1.1 }
  ],
  "beams": [
    { "a": "fh1r", "b": "fs1r", "stiffness": 20000.0, "damping": 0.0 }
  ],
  "wheels": [
    { "name": "FR", "node1": "fw1rr", "node2": "fw1r", "dir": 1 }
  ],
  "triangles": [
    { "a": "fx2r", "b": "fh6r", "c": "fx2l" }
  ]
}
```

- `pos` je v BeamNG osách (z = nahoru); hra je při načtení překlápí na Bevy (y = nahoru).
- `wheels.node1/node2` = nody osy kola, `dir` = strana/orientace.
- `damping` z `.veh` se **nepoužívá** (bývá nespolehlivé) — hra si tlumení dopočítá z tuhosti.

**Plánováno (zatím NENÍ ve formátu):** sekce `parts` (engine, gearbox, …) a `break_force`
na beamech. Přijde s drivetrainem a build systémem.

---

## Architektura

### Klíčové principy
- **Solver je hloupý integrátor.** Fyzikální stav celého auta žije v JEDNOM resource
  `SoftBody` jako Structure-of-Arrays (`pos`/`vel`/`inv_mass`/`force`/`beams`/`wheels`).
  Beamy a kola odkazují na nody **indexem**, ne přes Entity.
- **Render = entity.** Každý node (kulička), kolo (disk) i trup je Bevy entita; každý
  snímek se jí přepíše `Transform` z pozic v solveru.
- **Variabilita patří do builderů**, ne do solveru. Custom díly, tuning a typy zavěšení
  se „vlévají" při stavbě auta (`spawn_solver_car`, `build_suspension`), solver se nemění.
- **World bude trait** — abstrakce nad terénem pro budoucí OSM swap (zatím placatá zem).
- **Směr (cíl):** build/tuning systém půjde k **per-part datové vrstvě / entitám** nad
  `SoftBody` (parts = engine, gearbox…). SoA solver je základ, parts se staví nad ním.

### Stav v paměti (resource `SoftBody`)

```rust
struct SoftBody {
    pos: Vec<Vec3>, vel: Vec<Vec3>, inv_mass: Vec<f32>, force: Vec<Vec3>, // nody (SoA)
    beams: Vec<BeamC>,                 // pružiny mezi indexy nodů
    wheels: Vec<WheelC>,               // kola = hub-node + stav pneumatiky
    entities: Vec<Entity>,             // node index ↔ render entita
    chassis: Option<ChassisFrame>,     // referenční nody pro směr/orientaci auta
    substeps: u32,                     // adaptivně z CFL
}
```

### Klíčové typy

```rust
struct BeamC { i: usize, j: usize, rest: f32, k: f32, d: f32 }
struct WheelC { hub: usize, radius: f32, spin: f32, spin_vel: f32, steer: f32,
                drive_torque: f32, brake: f32, driven: bool, steered: bool, tire: TireParams }
struct TireParams { normal_stiffness: f32, normal_damping: f32, long: Pacejka, lat: Pacejka }
struct Pacejka { b: f32, c: f32, e: f32 } // Magic Formula koeficienty
enum SuspensionKind { Rigid /* McPherson, DoubleWishbone, LeafSpring… */ }
```

---

## Struktura projektu

```
src/
├── main.rs    ← App setup, solver, tire model, builder aut, UI, kamera (zatím vše tady)
├── physics.rs ← jen statický svět (zem) jako impl Bundle pro rapier
└── veh.rs     ← načtení vlastního .veh (serde)
```

**Plán:** `main.rs` rozdělit — solver/tire/vozidlo do vlastního modulu (`solver.rs` / `vehicle.rs`).

---

## Co je hotové

### `jbeam-rs` (konvertor)
- [x] Parsování `.jbeam` (vlastní preprocess — jbeam má chybějící čárky, není čistý json5)
- [x] Konverze nodes, beams, **wheels**, triangles do `.veh`
- [x] CLI: `jbeam-rs input.jbeam -o output.veh`
- [x] Test na Škoda Fabia (`assets/fabia.veh`: 500 nodů, 5871 beamů, 4 kola, 791 triangů)

### `magic-formula`
- [x] Bevy setup, okno, kamera, světlo, placatá zem
- [x] `.veh` loader (deserializace + stavba soft-body)
- [x] **Vlastní node-beam solver** (nahradil rapier soft-body i rapier rigid auto)
- [x] Adaptivní substepping z CFL (stabilita reálných tuhostí, žádný škálovací hack)
- [x] Tire model: Pacejka podélně i bočně, friction circle, dynamika spinu
- [x] Pohon / brzda / plynulé řízení (vstup zapisuje záměr, sílu řeší solver)
- [x] Chase + FPV kamera (přepínání klávesou C)
- [x] Spawn menu UI (klávesy 1/2/3, Shift+C = clear)

### Zatím NENÍ
- ❌ Řiditelné `.veh` auto (Fabia se načte jako blob — kola/chassis se ještě nenapojují) → **krok A**
- ❌ Opravdové zavěšení (kola jsou tuze přibita) → **krok B**
- ❌ Drivetrain (motor s křivkou + převodovka; teď placatý moment) → **krok C**
- ❌ Terén (jen placatá rovina) → **krok D**
- ❌ Doladěný tire model (hodnoty se ladí)
- ❌ Real OSM lokace, build system, rally checkpointy, multiplayer

---

## Roadmap

### Fáze 2 — Fyzika (PRÁVĚ TEĎ)
Pořadí dalších kroků:
- **A) Rozjet `.veh` auto** — napojit kola (`wheels` sekce) na hub-nody, odvodit `ChassisFrame`
  z pozic nodů, postavit zavěšení → řídí se skutečná Fabia, ne hardcoded kvádr.
- **B) Opravdové zavěšení** — `SuspensionKind` dál než `Rigid` (McPherson / wishbone se zdvihem).
- **C) Drivetrain** — motor (momentová křivka, RPM) + převodovka (ratios, final drive).
- **D) Terén** — `World` trait, heightfield kolize, weight transfer na svazích.
- Průběžně: **ladění tire modelu** (Pacejka `b/c/e`, `MU`, grip, `V_LOW`).

### Fáze 3 — Svět
- OSM data loader, SRTM elevation → terrain mesh, road mesh z OSM ways, jedna reálná lokace

### Fáze 4 — Build system
- Part inventory, assembly UI (My Summer Car styl), kompatibilita, tuning (Automation styl)

### Fáze 5 — Game modes
- Freeroam, rally checkpointy + timer, ghost replay

### Fáze 6 — Steam
- Steam Direct, achievements, leaderboards, Workshop pro vlastní `.veh`

### Možný budoucí scope (nezávazné)
- **Vizuální deformace / damage** — plastická deformace beamů při nárazu + přepočet
  render meshe (trvalé promáčkliny, ne jen pružné zhoupnutí).
- **Alternativní rapier rigid-body mód vozidla** — tuhá tělesa místo node-beamu jako
  volitelná varianta (rychlejší, bez deformace) VEDLE stávajícího solveru.
  Node-beam solver zůstává výchozí; rapier by byl jen druhá cesta.

---

## Učení paralelně

| Kdy | Co |
|---|---|
| Během Bevy | ECS pattern, Bevy docs |
| Fyzika | Pacejka papers, iforce2d.net vehicle physics, explicitní integrátory + CFL stabilita |
| Před fází 3 | GeoJSON spec, OSM data format, SRTM format |
| Konvertor | serde, error handling, clap, json5 |

---

## Zdroje

- **Bevy**: bevyengine.org/learn
- **bevy_rapier3d**: rapier.rs/docs
- **Vehicle physics**: iforce2d.net/b2dtut/vehicle-physics (2D, ale principy platí)
- **Pacejka**: hledej "Magic Formula tire model explained"
- **OSM data**: download.geofabrik.de (free OSM dumps)
- **SRTM elevation**: earthexplorer.usgs.gov
