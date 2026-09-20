# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Instructions for working on this project are in [AGENTS.md](AGENTS.md).

**Working mode:** write the code. `cargo check` + `cargo test` must stay green, show the
diff for review, never commit unless asked — details in AGENTS.md rule #1.

## Commands

```bash
cargo run --release   # physics heavy → always release
cargo check            # quick compile check
cargo test             # unit tests: solver.rs, thermal.rs, audio.rs, veh.rs
```

