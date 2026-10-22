# AGENTS.md — instructions for AI assistants on the Magic Formula project

## Rule #1 — working mode: write the code

Claude implements. No step-by-step walkthroughs, no waiting for the author to type it out.

- **Write the code**, then show what changed and why in a few lines.
- `cargo check` + `cargo test` must stay green (0 warnings) before anything is called done.
- Physics changes are verified by a real drive test (`cargo run --release`), not by
  reasoning — same for sound (listen to the render tests) and visuals.
- The author reviews the diff. **Never commit or push unless asked.**
- Explain the non-obvious parts (a new trait, a lifetime, a solver invariant) briefly in the
  summary — enough to review the diff, not a tutorial.
- Say when something is uncertain or untested rather than presenting it as verified.

## About the project

Magic Formula = rally + freeroam car physics sim in Rust, inspired by BeamNG.
Modular node-beam physics on a **custom solver**, custom `.veh` format. Open source (public repo); Steam is a possible later goal.

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
├── main.rs     ← App setup, UI/menu, HUD, camera, render sync (transforms + mesh)
├── solver.rs   ← physics core: SoftBody (SoA), node-beam solver, tire model (Pacejka)
├── vehicle.rs  ← the car: suspension types + builders, drivetrain (engine/clutch), controls
├── world.rs    ← terrain as a trait: World { height, normal }, FlatWorld / Hills
├── physics.rs  ← static world (ground) ONLY, as a function -> impl Bundle for rapier
├── veh.rs      ← loading the custom .veh (serde Deserialize)
├── thermal.rs  ← thermal model: brake/tire/cooler parts → temp → fade/grip/wear
├── audio.rs    ← engine sound: granular synth (combustion grains) via direct cpal, tire/gear/crash/wind
├── effects.rs  ← tire visual effects: skid marks + smoke (untested headless, needs drive test)
├── keybinds.rs ← rebindable controls: gameplay action → key, defaults = old hardcoded keys
├── parts.rs    ← JSON part catalog (assets/parts/*.json) → PartOption cards → Loadout
├── saves.rs    ← named build save/export/import (player/builds/*.json)
├── stats.rs    ← derived build KPIs for garage (power, weight, distribution, 0–100)
└── ui/         ← garage.rs, garage_preview.rs, dyno.rs, hud.rs, settings.rs, title.rs
```

Note: the Rust `.jbeam` → `.veh` converter lives in its own repository,
[`jbeam2veh`](https://github.com/alex-stepa/jbeam2veh). But `tools/jbeam_to_veh.py` **stays
in this tree** — the Python script is still ahead of the Rust crate (slot selection, tire
sizes) and is the reference implementation until those are ported. See README "converter".

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
cargo test            # unit tests (solver, thermal, audio, veh)
```

## Spec-driven workflow (OpenSpec)

Behaviour is specified, not just coded. Source of truth for *what the game does* =
`openspec/specs/<capability>/spec.md`; README stays roadmap + context, AGENTS.md stays rules.

```bash
/opsx:propose <name>   # new change: proposal.md + delta specs + tasks.md
/opsx:apply <name>     # implement the tasks (rule #1: Claude writes the code)
/opsx:archive <name>   # done → fold deltas into main specs, move change to archive
openspec validate --specs   # specs must stay valid
openspec list --specs       # what capabilities exist
```

Project context and architecture invariants for these artifacts live in
`openspec/config.yaml`. A change that touches an invariant has to say so in its proposal.

## Current phase

**Phase 4 — Build system, in progress.** Phase 2 (physics), Phase 2.5 (UI/UX) done
long ago. Since then: thermal model (brake/tire/cooler parts — temp → fade/grip/wear),
engine sound (granular synth via cpal), garage build system 4.0/4.1a/4.1b/4.2 done
(JSON part catalog, tabs, `CarBuild`/`Loadout` pattern, suspension kinds Rigid/Coil/
McPherson/DoubleWishbone/TorsionBeam/LeafSpring, delta numbers on part cards).

**Done since:** 4.1c (hover delta in the spec table, named saved builds + JSON
export/import wired into the garage) and rebindable controls (`keybinds.rs` + rebind UI).

**Now:** transition to spec-driven work — baseline specs for existing capabilities in
`openspec/specs/` so future changes are deltas against the real behaviour.

**Parked:** visual polish (fog, HDR tonemapping, bloom, SSAO+TAA, PBR materials) — one
commit on `origin/worktree-visual-polish`, local branch and worktree deleted; never merged,
never drive-tested. `effects.rs` (skid marks + smoke) still needs a drive test too.

**Next:** Phase 3 — world (OSM roads, one real location).

**Blocked (4.3):** hardpoints — waiting on a second real `.jbeam` mod to test against.

Full details: see [README.md](README.md) roadmap.
