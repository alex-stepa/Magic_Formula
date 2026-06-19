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
Rust + Bevy, vlastní `.veh` formát, node-beam fyzika přes bevy_rapier3d.

---

## Tech Stack

| Co | Čím |
|---|---|
| Jazyk | Rust |
| Engine | Bevy 0.15 |
| Fyzika | bevy_rapier3d 0.28 |
| Formát vozidla | vlastní `.veh` (JSON) |
| Mapa (základ) | flat procedural terrain |
| Mapa (budoucnost) | OpenStreetMap + SRTM elevation |
| Serializace | serde + serde_json |
| CLI (jbeam-rs) | clap |

---

## Vlastní formát `.veh`

Inspirovaný .jbeam ale úplně vlastní. Čitelný JSON.

```json
{
  "name": "skoda_fabia",
  "version": 1,
  "nodes": [
    { "id": "n_chassis_fl", "pos": [-0.7, 0.3, 1.4], "mass": 15.0 },
    { "id": "n_chassis_fr", "pos": [0.7, 0.3, 1.4], "mass": 15.0 },
    { "id": "n_chassis_rl", "pos": [-0.7, 0.3, -1.4], "mass": 15.0 },
    { "id": "n_chassis_rr", "pos": [0.7, 0.3, -1.4], "mass": 15.0 }
  ],
  "beams": [
    { "a": "n_chassis_fl", "b": "n_chassis_fr", "stiffness": 8000.0, "damping": 300.0, "break_force": 999999.0 }
  ],
  "wheels": [
    { "id": "w_fl", "node": "n_chassis_fl", "radius": 0.32, "width": 0.2, "driven": false, "steered": true }
  ],
  "parts": [
    { "id": "engine", "type": "engine", "torque": 180.0, "max_rpm": 6500.0, "node": "n_chassis_fr" },
    { "id": "gearbox", "type": "gearbox", "ratios": [3.5, 2.1, 1.4, 1.0, 0.8], "final_drive": 3.9 }
  ]
}
```

---

## Architektura — Bevy ECS

### Klíčové principy
- **Auto není jedna entita** — každý part je vlastní Bevy entita s komponentami
- **Physics per part** — rapier body per wheel/chassis node
- **World je trait** — abstrakce nad terrainem pro budoucí OSM swap

### Entity struktura

```
VehicleRoot (entita)
├── Chassis (entita) — rapier RigidBody
│   ├── Node entita × N — mass points
│   └── Beam entita × N — spring/damper constraints
├── Wheel (entita) × 4 — rapier RigidBody + Joint
├── Engine (entita) — torque, RPM
└── Gearbox (entita) — gear ratios
```

### Klíčové Bevy komponenty

```rust
#[derive(Component)] struct VehicleNode { id: String, mass: f32 }
#[derive(Component)] struct VehicleBeam { node_a: Entity, node_b: Entity, stiffness: f32, damping: f32 }
#[derive(Component)] struct Wheel { radius: f32, driven: bool, steered: bool }
#[derive(Component)] struct Engine { torque: f32, max_rpm: f32, current_rpm: f32 }
```

---

## Struktura projektu

```
src/
├── main.rs        ← App setup, WindowPlugin, systémy
├── physics.rs     ← Fyzika, collidery, RigidBody
└── (další moduly přibudou)
```

---

## Základ na léto (scope)

### `jbeam-rs` (2–3 týdny)
- [ ] Parsování `.jbeam` souboru (json5 crate)
- [ ] Konverze nodes, beams, wheels do `.veh`
- [ ] CLI: `jbeam-rs input.jbeam -o output.veh`
- [ ] Test na Škoda Fabia souboru

### `magic-formula` základ (3–4 týdny)
- [x] Bevy projekt setup + bevy_rapier3d
- [x] Okno, kamera, světlo, ground plane, auto placeholder
- [x] CameraMode enum (Free/Follow) s přepínáním na C
- [x] Modulární struktura (physics.rs)
- [x] Collider na zem + RigidBody na auto
- [ ] `.veh` loader (deserializace do ECS entit)
- [ ] Chassis jako rapier rigid body
- [x] 4 kola s rapier joints
- [x] Základní input (plyn, brzda, řízení)
- [ ] Volná kamera (WASD + myš) + follow kamera

### Co základ NENÍ
- ❌ Real OSM lokace
- ❌ Build system
- ❌ Rally checkpointy
- ❌ Soft-body node-beam (zatím rapier rigid body)
- ❌ Multiplayer

---

## Roadmap po prázdninách

### Fáze 2 — Fyzika
- Vlastní soft-body solver (node-beam nad rapier nebo místo něj)
- Pacejka tire model
- Suspension geometry (wishbone, McPherson)
- Weight transfer

### Fáze 3 — Svět
- OSM data loader (openstreetmap-rs nebo vlastní parser)
- SRTM elevation → terrain mesh
- Jedna reálná lokace (Praha okolí?)
- Road mesh generátor z OSM ways

### Fáze 4 — Build system
- Part inventory systém
- Assembly UI (My Summer Car styl)
- Part kompatibilita
- Tuning parametry (Automation styl)

### Fáze 5 — Game modes
- Freeroam
- Rally checkpointy + timer
- Ghost replay systém

### Fáze 6 — Steam
- Steam Direct ($100 fee)
- Achievements, leaderboards
- Workshop pro vlastní `.veh` soubory

---

## Učení paralelně

| Kdy | Co |
|---|---|
| Během Bevy | ECS pattern, Bevy docs, bevy_rapier3d examples |
| Před fází 2 | Pacejka papers, iforce2d.net vehicle physics |
| Před fází 3 | GeoJSON spec, OSM data format, SRTM format |
| Během parseru | serde, error handling, clap, json5 |

---

## Zdroje

- **Bevy**: bevyengine.org/learn
- **bevy_rapier3d**: rapier.rs/docs
- **Vehicle physics**: iforce2d.net/b2dtut/vehicle-physics (2D ale principy platí)
- **Pacejka**: hledej "Magic Formula tire model explained"
- **OSM data**: download.geofabrik.de (free OSM dumps)
- **SRTM elevation**: earthexplorer.usgs.gov
