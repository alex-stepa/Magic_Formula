# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Instructions for working on this project are in [AGENTS.md](AGENTS.md).

**Most important:** the working mode is **GUIDE, not autopilot** by default — don't code for
the author; walk him through step by step and he writes the code himself. Exception:
OpenSpec changes declared `Implementation mode: autopilot` (refactors, boilerplate, tests,
docs only) — see AGENTS.md rules #1 and #1b.

## Commands

```bash
cargo run --release   # physics heavy → always release
cargo check            # quick compile check
cargo test             # unit tests: solver.rs, thermal.rs, audio.rs, veh.rs
```

