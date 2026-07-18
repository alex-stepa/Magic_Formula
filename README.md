# Magic Formula

Rally + freeroam game with a modular car physics architecture inspired by BeamNG.
Own format, own implementation, own identity. Potential Steam release.

- **Name:** Magic Formula
- **Parts:** `magic-formula` (game) + [`jbeam2veh`](https://github.com/alex-stepa/jbeam2veh) (`.jbeam` → `.veh` converter, separate repo)

---

## Project parts

### 1. `magic-formula` — the main game
Rust + Bevy, custom `.veh` format, node-beam physics on a **custom solver**
(rapier remains only for the static world/ground).

### 2. [`jbeam2veh`](https://github.com/alex-stepa/jbeam2veh) — the converter
Parses BeamNG `.jbeam` files and converts them into the custom `.veh` format.
Standalone crate in its **own repository**, no dependency on the game.

---

## Tech Stack

| What | With |
|---|---|
| Language | Rust (edition 2024) |
| Engine | Bevy 0.15 |
| Vehicle physics | **custom node-beam solver** (semi-implicit Euler + adaptive substepping) |
| World physics | bevy_rapier3d 0.28 — static ground / world collisions only |
| Tire model | Magic Formula (Pacejka) — longitudinal and lateral, friction circle |
| Vehicle format | custom `.veh` (JSON) |
| Map (base) | flat plane |
| Map (future) | OpenStreetMap + SRTM elevation |
| Serialization | serde + serde_json |
| CLI (jbeam-rs) | clap |

---

## Custom `.veh` format

Inspired by .jbeam, but fully our own. Readable JSON. **This is the format the game
actually loads today** (`src/veh.rs`):

```json
{
  "name": "example_car",
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

- `pos` is in BeamNG axes (z = up); the game flips them to Bevy (y = up) on load.
- `wheels.node1/node2` = wheel axle nodes, `dir` = side/orientation.
- `damping` from `.veh` is **not used** (tends to be unreliable) — the game derives
  damping from stiffness.

**Planned (NOT in the format yet):** a `parts` section (engine, gearbox, …) and
`break_force` on beams. Coming with the drivetrain and build system.

---

## Architecture

### Key principles
- **The solver is a dumb integrator.** The physics state of the whole car lives in ONE
  resource `SoftBody` as Structure-of-Arrays (`pos`/`vel`/`inv_mass`/`force`/`beams`/`wheels`).
  Beams and wheels reference nodes **by index**, not via Entity.
- **Render = entities.** Every node (ball), wheel (disk) and body is a Bevy entity; each
  frame its `Transform` is overwritten from solver positions.
- **Variability belongs in builders**, not in the solver. Custom parts, tuning and
  suspension types are "poured in" during car construction (`spawn_solver_car`,
  `build_suspension`); the solver never changes.
- **World will be a trait** — an abstraction over terrain for the future OSM swap
  (flat ground for now).
- **Direction (goal):** the build/tuning system moves toward a **per-part data layer /
  entities** above `SoftBody` (parts = engine, gearbox…). The SoA solver is the
  foundation; parts are built on top of it.

### In-memory state (resource `SoftBody`)

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

### Key types

```rust
struct BeamC { i: usize, j: usize, rest: f32, k: f32, d: f32 }
struct WheelC { hub: usize, radius: f32, spin: f32, spin_vel: f32, steer: f32,
                drive_torque: f32, brake: f32, front: bool, steered: bool, tire: TireParams }
struct TireParams { normal_stiffness: f32, normal_damping: f32, long: Pacejka, lat: Pacejka }
struct Pacejka { b: f32, c: f32, e: f32 } // Magic Formula koeficienty
enum SuspensionKind { Rigid, Coil { arm_k, coil_k }, McPherson { arm_k, strut_k },
                      DoubleWishbone { arm_k, spring_k } /* LeafSpring… */ }
struct CarBuild { suspension: SuspensionKind } // resource: vybrané díly (build systém)
```

---

## Project structure

```
src/
├── main.rs    ← App setup, UI/menu, HUD, camera, render sync
├── solver.rs  ← physics core: SoftBody (SoA), node-beam solver, tire model
├── vehicle.rs ← the car: suspension + builders, drivetrain, controls
├── world.rs   ← terrain as a trait (FlatWorld / Hills / later OSM heightfield)
├── physics.rs ← static world (ground) only, as impl Bundle for rapier
└── veh.rs     ← loading the custom .veh (serde)
```

---

## What's done

### converter — [`jbeam2veh`](https://github.com/alex-stepa/jbeam2veh) (separate repo)
- [x] `.jbeam` parsing (custom preprocess — jbeam has missing commas, not clean json5)
- [x] Conversion of nodes, beams, **wheels**, triangles to `.veh`
- [x] CLI: `jbeam-rs input.jbeam -o output.veh`
- [x] Tested on a real `.jbeam` car (500 nodes, 5871 beams, 4 wheels, 791 triangles)

### `magic-formula`
- [x] Bevy setup, window, camera, light, flat ground
- [x] `.veh` loader (deserialization + soft-body construction)
- [x] **Custom node-beam solver** (replaced rapier soft-body and the rapier rigid car)
- [x] Adaptive substepping from CFL (stability at real stiffness values, no scaling hack)
- [x] Tire model: Pacejka longitudinal and lateral, friction circle, spin dynamics
- [x] Drive / brake / smooth steering (input writes intent, the solver resolves forces)
- [x] Chase + FPV camera (toggle with C)
- [x] Spawn menu UI ("Test car" + any `.veh`, Shift+C = clear)
- [x] Drivable car on the solver: wheels attached to hub nodes + `ChassisFrame` from
  node positions → **step A**
- [x] Drivetrain: engine (torque curve, RPM) + gearbox + diff/AWD → **step C**
- [x] Suspension with vertical travel (`SuspensionKind::Coil`) on the test car →
  **step B, part 1**: 2 stiff ~horizontal arms (locate the wheel) + a softer ~vertical
  spring with a damper — a geometry trick, solver untouched. + anti-roll bar (V-node
  above the axle) + launch traction control
- [x] Build menu (first part): `CarBuild` resource, suspension choice Rigid/Coil/McPherson
  in the menu with live highlighting; switching rebuilds a standing car at runtime
- [x] McPherson strut: a tower (node rigidly tied to the frame) above the hub,
  spring+damper ~vertical → full stiffness carries the weight; more realistic geometry
  than Coil into a frame corner
- [x] Double wishbone: wheel = hub + top node (knuckle), SLA arms (short upper, long
  lower) → real camber through travel; the disk tilts along the hub→top vector
- [x] Camber thrust in the tire model: a tilted wheel (with a knuckle) pulls toward the
  lean (`CAMBER_THRUST·Fz·sin γ`, before the friction circle) — SLA helps cornering
- [x] Clutch (left Shift) + engine as state: RPM is integrated (inertia), clutch =
  viscous coupling with capacity; only what the clutch transmits reaches the wheels.
  Clear → Backspace
- [x] Terrain (`World` trait): solver queries `height(x,z)`/`normal(x,z)` — Fz along the
  surface normal, slip directions projected onto the tangent plane. `Hills` = procedural
  rolling hills (flat zone around spawn), render mesh sampled from the terrain → **step D**

### Phase 2.5 — UI/UX (design v `Game magic formula UI.zip`)
- [x] Title screen: režimy + map picker + klávesy
- [x] Garáž: auto + díly (zavěšení/pohon/diff) + spec tabulka, Vyjet/Zpět
- [x] Herní HUD: rally dash — RPM bar + shift LED, rychlost, kvalt, stavový řádek
- [x] Telemetrie overlay (T): per-kolo tabulka Fz/skluz/áz/F_long/F_lat + souhrn
- [x] Pauza (Esc): Pokračovat / Garáž / Hlavní menu

### NOT yet
- ❌ Suspension for `.veh` cars (still `Rigid` + placeholder ride height, **parked**)
- ❌ Beam precompression gradual application (ponytail: deferred, complex + low ROI)
- ❌ Per-axle tire balance tuning (both axles same Pacejka now)
- ❌ Tire temps, wear, temperature dynamics
- ❌ Real OSM location, rally checkpoints, multiplayer

---

## Roadmap

### Phase 2 — Physics ✅ DONE
- **A) Get the `.veh` car driving** — ✅ done.
- **B) Real suspension** — ✅ Coil + McPherson + double wishbone with camber;
  bump stops + asymmetric damping (rebound ~2× bump); per-axle brakes.
- **C) Drivetrain** — ✅ engine + gearbox + FWD/RWD/AWD + clutch.
- **D) Terrain** — ✅ `World` trait, procedural hills, SRTM heightfield support.
- **E) Telemetry + Grip tuning** — ✅ per-wheel contact CSV log (T key),
  Pacejka lateral `b` tuned from real test data (8→18), vehicle feel tested on
  skidpad + test track (rovinka). Auto is drivable and repeatable.
- **Tire model** — Pacejka procedural, good enough. Future: tire temps (teplota → grip),
  per-axle balance, wear simulation.
- **Parked (return after physics stable):** `.veh` car suspension + beam precompression
  deferral.

### Phase 2.5 — UI/UX (design hotový — `Game magic formula UI.zip`, klikací prototyp)
Struktura obrazovek podle návrhu:
- **Title screen**: logo + režimy — Volná jízda / Garáž / Rally (brzy) / Nastavení
  + map picker (Testovací plocha · Kopce · Reálný terén SRTM) + přehled kláves.
- **Garáž**: výběr auta (Testovací vůz / Fabia `.veh`) + osazení dílů — zavěšení
  (Rigid/Coil/McPherson/Double wishbone), pohon (FWD/RWD/AWD), zadní diff
  (otevřený/zamčený) + živá tabulka specifikací. Tlačítka Zpět / Vyjet.
  Výměna dílu přestaví stojící auto za běhu (`CarBuild` pattern).
- **Herní HUD**: rally dash (styl Sinco Tech, viz referenční foto v zipu) — RPM
  oblouk se segmenty + shift LED, rychlost km/h, kvalt, ot/min; boční štítky:
  pohon, diff, spojka, Fz předek, slip angle, ● REC indikátor.
- **Telemetrie overlay** (T): tabulka per kolo — Fz, skluz, slip angle, F_long, F_lat.
- **Pauza** (Esc): Pokračovat / Garáž / Hlavní menu + nápověda (Shift+R restart,
  Backspace clear).

### Phase 3 — World
- OSM data loader, SRTM integration, road mesh from OSM ways, one real location.

### Phase 3 — World
- OSM data loader, SRTM elevation → terrain mesh, road mesh from OSM ways, one real location

### Phase 4 — Build system
- Part inventory, assembly UI (My Summer Car style), compatibility, tuning (Automation style)
- The first piece is already alive: `CarBuild` resource + suspension choice in the menu
  (pattern: part = data, resource holds the choice, builder reads — same as `Drivetrain`)

### Phase 5 — Game modes
- Freeroam, rally checkpoints + timer, ghost replay
- **SS7 — narrative archive mode**: standalone investigation mode, fully isolated
  from the node-beam solver / drivetrain / tire model (shares render + optionally
  `world.rs` only). See [`docs/narrative/SS7_pribeh.md`](docs/narrative/SS7_pribeh.md)
  (story) and [`docs/narrative/SS7_technicka.md`](docs/narrative/SS7_technicka.md)
  (technical plan).

### Phase 6 — Steam
- Steam Direct, achievements, leaderboards, Workshop for custom `.veh`

### Possible future scope (non-binding)
- **Visual deformation / damage** — plastic beam deformation on impact + render mesh
  recomputation (permanent dents, not just elastic bounce).
- **Alternative rapier rigid-body vehicle mode** — rigid bodies instead of node-beam as
  an optional variant (faster, no deformation) ALONGSIDE the existing solver.
  The node-beam solver stays the default; rapier would be a second path only.

---

## Learning in parallel

| When | What |
|---|---|
| During Bevy | ECS pattern, Bevy docs |
| Physics | Pacejka papers, iforce2d.net vehicle physics, explicit integrators + CFL stability |
| Before phase 3 | GeoJSON spec, OSM data format, SRTM format |
| Converter | serde, error handling, clap, json5 |

---

## Resources

- **Bevy**: bevyengine.org/learn
- **bevy_rapier3d**: rapier.rs/docs
- **Vehicle physics**: iforce2d.net/b2dtut/vehicle-physics (2D, but the principles hold)
- **Pacejka**: search "Magic Formula tire model explained"
- **OSM data**: download.geofabrik.de (free OSM dumps)
- **SRTM elevation**: earthexplorer.usgs.gov
