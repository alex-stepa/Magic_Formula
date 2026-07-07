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
jbeam-rs/      ← SEPARATE .jbeam → .veh converter (own Cargo project, not part of the game)
```

Note: two `src/` folders are intentional — `./src` is the game, `jbeam-rs/src` is the converter.

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

**Phase 2 — Physics.** Done: custom node-beam solver (replaced rapier soft-body),
unified car model on the solver, tire model (Pacejka longitudinal and lateral, friction
circle, spin dynamics), chase/FPV camera, **A)** the `.veh` car drives, **C)** drivetrain
(engine + gearbox + diff/AWD), **B part 1)** `Coil` suspension with travel on the test car
(2 stiff ~horizontal arms + a ~vertical spring with a damper — geometry trick, solver
untouched), anti-roll bar as a V-node, launch traction control, build menu (`CarBuild`:
Rigid/Coil/McPherson/DoubleWishbone — McPherson = strut onto a tower above the hub;
DW = knuckle (top node) + SLA arms → real camber, the disk tilts along hub→top),
clutch (left Shift; engine = integrated state, viscous coupling with capacity,
Clear on Backspace), **D)** terrain (`World` trait — solver queries height/normal,
Fz along the surface normal, slip in the tangent plane; `Hills` procedural, flat
zone around spawn).
**BEING TUNED:** tire model values (Pacejka `b/c/e`, `MU`, grip, `V_LOW`).

Next (in order): **B continuation)** suspension for `.veh` cars (still `Rigid`
+ placeholder ride height) → **C')** `.veh parts`
(wheel radius/mass from data). Tire model tuning ongoing.
