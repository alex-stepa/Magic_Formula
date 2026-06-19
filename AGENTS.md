# AGENTS.md — pokyny pro AI asistenty na projektu Magic Formula

## ⚠️ Pravidlo č. 1 — režim práce: GUIDE, ne autopilot

Autor je 15letý vývojář, který se učí Rust + Bevy a chce **opravdu růst**, ne dostávat
hotová řešení.

- **NEPROGRAMUJ za něj.** Říkej mu CO a PROČ udělat, kód píše on sám.
- Postupuj **po malých krocích** — jeden krok, počkej na „hotovo", pak další.
- Hotový snippet napiš **jen** když se zasekne na něčem Rust-specifickém, co ještě
  neviděl (např. nový typ návratové hodnoty, lifetime, trait). Vysvětli ho, ale
  základní logiku ať dopíše on.
- Po napsání kód **zkontroluj** a naveď na opravy, neopravuj potichu.
- Výjimka: čistě „meta" úkoly (dokumentace, README, komentáře, struktura složek)
  smíš dělat přímo, když o to požádá.

## O projektu

Magic Formula = rally + freeroam car physics sim v Rustu, inspirovaný BeamNG.
Modulární car fyzika (node-beam), vlastní `.veh` formát. Cíl: případně Steam.

Plný kontext, roadmapa a scope: viz [README.md](../README.md).

## Tech stack

| Co | Čím |
|---|---|
| Jazyk | Rust (edition 2024) |
| Engine | Bevy 0.15 |
| Fyzika | bevy_rapier3d 0.28 |
| Formát vozidla | vlastní `.veh` (JSON, serde) |

## Struktura

```
src/
├── main.rs     ← App setup, pluginy, systémy (setup, camera_mode)
└── physics.rs  ← fyzikální komponenty (RigidBody + Collider) jako funkce -> impl Bundle
jbeam-rs/       ← SAMOSTATNÝ konvertor .jbeam → .veh (vlastní Cargo projekt, není součástí hry)
```

Pozn.: dvě `src/` složky jsou záměr — `./src` je hra, `jbeam-rs/src` je konvertor.

## Konvence

- Komentáře a komunikace **v češtině** (autor tak píše).
- Fyzika patří do `physics.rs`, ne přímo do `main.rs` setup.
- `Collider::cuboid` bere **poloviční** rozměry (half-extents), ne celé!
- Architektura: auto NENÍ jedna entita — každý part (chassis, kola, engine) je
  vlastní entita. Drž se toho, ať jde později vyměnit rigid body za softbody.

## Build / běh

```bash
cargo run       # spustí hru
cargo check     # rychlá kontrola, že se to přeloží
```

## Aktuální fáze

Základ hry (léto): okno, kamera, světlo, zem, auto placeholder — hotovo.
Právě se dělá: **Collider na zem + RigidBody na auto** v `physics.rs`.
Další: `.veh` loader, kola s joints, input, kamery. Softbody až ve Fázi 2.
