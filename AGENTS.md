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
Modulární node-beam fyzika na **vlastním solveru**, vlastní `.veh` formát. Cíl: případně Steam.

Plný kontext, roadmapa a scope: viz [README.md](README.md).

## Tech stack

| Co | Čím |
|---|---|
| Jazyk | Rust (edition 2024) |
| Engine | Bevy 0.15 |
| Fyzika **vozidel** | **vlastní node-beam solver** (semi-implicitní Euler + adaptivní substepping) |
| Fyzika **světa** | bevy_rapier3d 0.28 — **jen statická zem / kolize světa**, NE vozidla |
| Formát vozidla | vlastní `.veh` (JSON, serde) |

## Struktura

```
src/
├── main.rs    ← App setup, solver, tire model, builder aut, UI, kamera (zatím VŠE tady)
├── physics.rs ← JEN statický svět (zem) jako funkce -> impl Bundle pro rapier
└── veh.rs     ← načtení vlastního .veh (serde Deserialize)
jbeam-rs/      ← SAMOSTATNÝ konvertor .jbeam → .veh (vlastní Cargo projekt, není součástí hry)
```

Pozn.: dvě `src/` složky jsou záměr — `./src` je hra, `jbeam-rs/src` je konvertor.
**PLÁN:** `main.rs` je obtloustlý — solver/tire/vozidlo se mají vyčlenit do vlastního
modulu (`solver.rs` / `vehicle.rs`). Při větší úpravě fyziky to zvaž.

## Architektura — jak auto reálně funguje

- **Fyzikální stav vozidla žije v JEDNOM resource `SoftBody`** (Structure-of-Arrays:
  `pos`/`vel`/`inv_mass`/`force`/`beams`/`wheels`). Solver je **hloupý integrátor** nad indexy.
- **Render = entity** per node (kulička) a per kolo (disk) + trup; každý snímek se jim
  přepíše `Transform` z pozic v solveru.
- **Variabilita** (custom díly, tuning, typy zavěšení) patří do **builderů při spawnu**,
  ne do smyčky solveru. Kolo = node + kruhový kontakt; pérování = beamy (`build_suspension`).
- **SMĚR (cíl):** build/tuning systém půjde k **per-part datové vrstvě / entitám** nad
  `SoftBody` (parts = engine, gearbox, …). SoA solver je základ, parts se staví nad ním.

## Konvence

- Komentáře a komunikace **v češtině** (autor tak píše).
- `Collider::cuboid` bere **poloviční** rozměry (half-extents), ne celé!
- **Rapier je jen pro statický svět** (zem). Vozidla NIKDY přes rapier — vše na vlastním solveru.
- **Reálné tuhosti z `.veh`, ŽÁDNÉ škálování** (starý ×0.01/cap hack je pryč). Stabilitu řeší
  adaptivní substepping (CFL na úrovni uzlu) + tlumení škálované tuhostí (`d=2·ζ·√(k·μ)`).

## Build / běh

```bash
cargo run --release   # fyzika je náročná (velké .veh = stovky substepů/snímek) → release!
cargo check           # rychlá kontrola, že se to přeloží
```

## Aktuální fáze

**Fáze 2 — Fyzika.** Hotovo: vlastní node-beam solver (nahradil rapier soft-body),
jednotný model aut na solveru, tire model (Pacejka podélně i bočně, friction circle,
dynamika spinu), chase/FPV kamera, **A)** `.veh` auto jezdí, **C)** drivetrain
(motor + převodovka + diff/AWD), **B část 1)** zavěšení `Coil` se zdvihem u test auta
(2 tuhá ~vodorovná ramena + ~svislá pružina s tlumičem — trik geometrie, solver netknutý),
stabilizátor jako V-nod, trakční kontrola rozjezdu, build menu (`CarBuild`: výběr Rigid/Coil).
**SE LADÍ:** hodnoty tire modelu (Pacejka `b/c/e`, `MU`, grip, `V_LOW`).

Další (pořadí): **B pokračování)** zavěšení pro `.veh` auta (zatím `Rigid`
+ placeholder světlost), McPherson/wishbone geometrie → **C')** skutečná spojka místo
TC hacku, `.veh parts` (radius/hmota kol z dat) → **D)** terén. Ladění tire modelu průběžně.
