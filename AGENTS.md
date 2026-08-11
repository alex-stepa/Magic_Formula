# AGENTS.md — instructions for AI assistants on the Magic Formula project

## ⚠️ Rule #1 — working mode: GUIDE, not autopilot

The author is a 15-year-old developer learning Rust + Bevy who wants to **actually grow**,
not receive finished solutions.

- **DO NOT code for him.** Tell him WHAT to do and WHY; he writes the code himself.
- Proceed in **small steps** — one step, wait for "done", then the next.
- Write a finished snippet **only** when he gets stuck on something Rust-specific he
  hasn't seen yet (e.g. a new return type, lifetime, trait). Explain it, but let him
  finish the core logic himself.
- After he writes code, **review it** and guide him to fixes; don't fix silently.
- Exception: purely "meta" tasks (documentation, README, comments, folder structure)
  may be done directly when he asks for it.

## About the project

Magic Formula = rally + freeroam car physics sim in Rust, inspired by BeamNG.
Modular node-beam physics on a **custom solver**, custom `.veh` format. Goal: possibly Steam.

Full context, roadmap and scope: see [README.md](README.md).

## Tech stack

| What | With |
|---|---|
| Language | Rust (edition 2024) |
| Engine | Bevy 0.15 |
| **Vehicle** physics | **custom node-beam solver** (semi-implicit Euler + adaptive substepping) |
| **World** physics | bevy_rapier3d 0.28 — **static ground / world collisions only**, NOT vehicles |
| Vehicle format | custom `.veh` (JSON, serde) |

## Structure

```
src/
├── main.rs    ← App setup, UI/menu, HUD, camera, render sync (transforms + mesh)
├── solver.rs  ← physics core: SoftBody (SoA), node-beam solver, tire model (Pacejka)
├── vehicle.rs ← the car: suspension types + builders, drivetrain (engine/clutch), controls
├── world.rs   ← terrain as a trait: World { height, normal }, FlatWorld / Hills
├── physics.rs ← static world (ground) ONLY, as a function -> impl Bundle for rapier
└── veh.rs     ← loading the custom .veh (serde Deserialize)
```

Note: the `.jbeam` → `.veh` converter lives in its own repository,
[`jbeam2veh`](https://github.com/alex-stepa/jbeam2veh) — no longer part of this tree.

## Architecture — how the car actually works

- **The vehicle's physics state lives in ONE resource `SoftBody`** (Structure-of-Arrays:
  `pos`/`vel`/`inv_mass`/`force`/`beams`/`wheels`). The solver is a **dumb integrator** over indices.
- **Render = entities** per node (ball) and per wheel (disk) + body; every frame their
  `Transform` is overwritten from solver positions.
- **Variability** (custom parts, tuning, suspension types) belongs in **builders at spawn
  time**, not in the solver loop. Wheel = node + circular contact; springing = beams
  (`build_suspension`).
- **DIRECTION (goal):** the build/tuning system moves toward a **per-part data layer /
  entities** above `SoftBody` (parts = engine, gearbox, …). The SoA solver is the
  foundation; parts are built on top of it.

## Conventions

- Comments and communication **in Czech** (that's how the author writes).
- `Collider::cuboid` takes **half** extents, not full!
- **Rapier is only for the static world** (ground). Vehicles NEVER via rapier — everything
  on the custom solver.
- **Real stiffness values from `.veh`, NO scaling** (the old ×0.01/cap hack is gone).
  Stability is handled by adaptive substepping (per-node CFL) + damping scaled by
  stiffness (`d=2·ζ·√(k·μ)`).

## Build / run

```bash
cargo run --release   # physics is heavy (large .veh = hundreds of substeps/frame) → release!
cargo check           # quick check that it compiles
```

## Current phase

**Phase 4 — Build system, in progress.** Phase 2 (physics), Phase 2.5 (UI/UX) done
long ago. Since then: thermal model (brake/tire/cooler parts — temp → fade/grip/wear),
engine sound (granular synth via cpal), garage build system 4.0/4.1a/4.1b/4.2 done
(JSON part catalog, tabs, `CarBuild`/`Loadout` pattern, suspension kinds Rigid/Coil/
McPherson/DoubleWishbone/TorsionBeam/LeafSpring, delta numbers on part cards).

**Now (4.1c, polish):** hover delta in the garage spec table; named saved builds +
JSON export/import (`src/saves.rs` backend exists — `SavedBuild`/`save`/`load`/`list`,
garage UI wiring still needed).

**Blocked (4.3):** hardpoints — waiting on a second real `.jbeam` mod to test against.

Full details: see [README.md](README.md) roadmap.
