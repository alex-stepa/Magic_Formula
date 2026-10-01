# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Instructions for working on this project are in [AGENTS.md](AGENTS.md).

**Working mode:** write the code. `cargo check` + `cargo test` must stay green, show the
diff for review, never commit unless asked — details in AGENTS.md rule #1.

## Commands

```bash
cargo run --release       # physics heavy → always release
cargo check                # quick compile check
cargo test                 # unit tests: solver.rs, thermal.rs, audio.rs, veh.rs, stats.rs
cargo test <name> -- --nocapture   # single test, e.g. `cargo test tire_grip_falls_with_temp`
```

## Architecture (one resource, dumb integrator)

Vehicle physics state lives in **one `SoftBody` resource** (SoA: `pos`/`vel`/`inv_mass`/
`force`/`beams`/`wheels`), solved by a custom node-beam solver (semi-implicit Euler,
adaptive per-node substepping from CFL) — **never bevy_rapier3d**, which is wired only for
the static ground. Render entities (node balls, wheel disks, body) get their `Transform`
overwritten from solver positions every frame. Part/tuning variability belongs in
**builders at spawn time** (e.g. `build_suspension`), not in the solver loop.

```
src/
├── main.rs      ← app setup, UI/menu, HUD, camera, render sync
├── solver.rs    ← SoftBody (SoA), node-beam solver, Pacejka tire model
├── vehicle.rs   ← suspension builders, drivetrain (engine/clutch), controls
├── world.rs     ← terrain trait World { height, normal } — FlatWorld / Hills
├── physics.rs   ← static ground ONLY, as a rapier Bundle
├── veh.rs       ← custom .veh format loader (serde)
├── thermal.rs   ← brake/tire/cooler parts → temp → fade/grip/wear
├── audio.rs     ← granular engine synth via direct cpal (not Bevy audio)
├── effects.rs   ← tire skid marks + smoke
├── keybinds.rs  ← rebindable controls (action → key)
├── parts.rs     ← JSON part catalog (assets/parts/*.json) → Loadout
├── saves.rs     ← named build save/export/import (player/builds/*.json)
├── stats.rs     ← derived build KPIs (power, weight, distribution, 0–100)
└── ui/          ← garage.rs, garage_preview.rs, dyno.rs, hud.rs, settings.rs, title.rs
```

Behaviour is spec-driven: `openspec/specs/<capability>/spec.md` is the source of truth for
what the game does; `openspec/changes/<name>/` holds work in flight. See AGENTS.md's
"Spec-driven workflow" section for the `/opsx:*` commands and current phase/roadmap state.

