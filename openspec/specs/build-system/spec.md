## Purpose

The garage is where a car is assembled and tuned: a JSON part catalog
(`assets/parts/*.json`), parametric sliders, honest derived numbers (power, mass,
distribution, 0–100), a live 3D preview and a dyno graph, plus named builds saved to
`player/builds/*.json` with export/import. Sources: `src/parts.rs`, `src/saves.rs`,
`src/stats.rs`, `src/ui/garage.rs`, `src/ui/garage_preview.rs`, `src/ui/dyno.rs`.

## Requirements

### Requirement: A part is data, the build is a resource, the builder reads it

A part MUST be plain data (`PartData` wrapping an existing game struct such as
`SuspensionKind`, `Gearbox`, `Compound`, `Cooler`). Clicking a card MUST only write into the
`Loadout` resource; the actual car MUST be constructed from the loadout at spawn time, never
by mutating a running simulation. Adding a **part** MUST therefore be a JSON-only change;
adding a **category** MUST cost one match arm each in `category`, `apply` and `is_selected`.

#### Scenario: New part shipped

- **WHEN** a new entry is added to `assets/parts/*.json` in an existing category
- **THEN** it appears in the garage and works with no Rust changes

#### Scenario: Part fitted

- **WHEN** the player clicks a part card
- **THEN** the matching `Loadout` field is set and the card is marked as fitted
- **AND** the change takes effect on the next spawn, not mid-drive

#### Scenario: Suspension selection compares topology only

- **WHEN** checking whether a suspension card is fitted
- **THEN** only the variant is compared (`same_variant`), not the exact stiffness — stiffness
  is what the sliders tune

### Requirement: Catalog loading tolerates bad data

`load_part_catalog` MUST read every `.json` in `assets/parts` in sorted order using
`std::fs` + `serde_json` (no Bevy asset loader), and a missing directory or an unparsable
file MUST produce a warning and be skipped, leaving the rest of the catalog usable.

#### Scenario: One catalog file is malformed

- **WHEN** one file fails to parse
- **THEN** it is logged and skipped, and the parts from the other files still load

#### Scenario: Serde tags stay intact

- **WHEN** part JSON is written
- **THEN** struct variants are externally tagged (`{"suspension":{"coil":{…}}}`) and unit
  variants are bare strings (`{"suspension":"rigid"}`)
- **AND** this contract is pinned by `parts::tests::parses_each_category`

### Requirement: Parametric parts stay sliders, not catalog entries

Engine and brakes MUST stay parametric (sliders over `Loadout`: displacement, redline,
boost, cylinders, spring/arm stiffness per axle, wheel offset per axle), not discrete catalog
cards. Suspension MUST be both: a topology card plus sliders. Suspension cards MUST be
fittable **per axle** (`SuspPart { idx, front }`).

#### Scenario: Different suspension front and rear

- **WHEN** the player fits McPherson to the front and TorsionBeam to the rear
- **THEN** both axles keep their own kind and the build name shows `McP/TB`

### Requirement: Garage numbers are computed from real loadout data

Every number shown MUST be derived from the actual loadout, never faked. Peak power MUST be
integrated from the torque curve (`P = M·ω`, sampled every 50 rpm up to the limiter). Mass
and front distribution MUST come from real node masses for a `.veh` (wheel axle nodes
excluded, replaced by 4 × `hub_mass`) or from the chassis parameters for the test car.
Where a number is a simplification, it MUST be documented as such.

#### Scenario: Stock engine

- **WHEN** the default 1.4 L atmospheric engine is fitted
- **THEN** peak power lands in a physically plausible range (~65 kW)
- **AND** this is pinned by `stats::tests::stock_engine_power_plausible`

#### Scenario: 0–100 estimate

- **WHEN** `estimate_0_100_s` runs
- **THEN** it deterministically integrates wheel force (engine torque through the gearing,
  capped by traction `μ·m·g·driven_fraction`, 0.9 drivetrain efficiency, shifting at the
  limiter) and returns `None` if the car cannot accelerate or needs over 60 s
- **AND** it explicitly ignores aero, clutch slip and shift time

#### Scenario: Test car mass

- **WHEN** the built-in test car is selected
- **THEN** its mass is 8 × body node mass + 4 × hub mass (1100 kg by default)

### Requirement: Delta numbers before fitting

A part card MUST show what fitting it would change relative to the current build, and
hovering a suspension card MUST preview the affected row in the spec table — so the player
can compare without committing to a swap.

#### Scenario: Hovering a suspension card

- **WHEN** the pointer is over a suspension card for a given axle
- **THEN** `HoveredSusp` records `(catalog index, front)` and the spec table shows that
  card's value as a preview
- **AND** moving off restores the current build's value

### Requirement: Preview and dyno add no physics

The 3D preview MUST be render-to-texture on a dedicated layer and camera (`PREVIEW_LAYER`),
isolated from the game camera and terrain, and MUST reflect the selected car (test car =
box + 4 wheels, `.veh` = its own triangle mesh, centred). The dyno MUST rasterize
`engine_torque` into an image with fixed scales so engines stay comparable, and MUST
redraw on `Changed<Loadout>`. Neither MUST introduce new physics or simulate anything.

#### Scenario: Engine tuned in the garage

- **WHEN** an engine slider changes
- **THEN** the dyno curve is redrawn from the same `engine_torque` the game drives with

#### Scenario: Car selection changed

- **WHEN** the player picks another car
- **THEN** the preview mesh and caption follow the selection

### Requirement: Named builds persist and round-trip

A build MUST be saveable by name into `player/builds/<safe-name>.json` holding the tuned
loadout (car, engine spec, both suspensions, gearbox, brakes, layout, diff locks, wheel
offsets). Loading MUST restore it exactly, regenerating the engine from its spec. File names
MUST be sanitized to alphanumerics and `_`. Listing MUST sort by name and skip unreadable or
invalid files. A missing name MUST be fillable by `auto_name` (e.g. `1.4L NA · Coil · FWD`).

#### Scenario: Save then load

- **WHEN** a build is saved and loaded back
- **THEN** the loadout matches the saved one field by field
- **AND** this is pinned by `saves::tests::roundtrip_build`

#### Scenario: Older save file

- **WHEN** a save predates the per-axle suspension fields
- **THEN** `front_susp`/`rear_susp` default to `SuspensionKind::COIL` instead of failing to load

#### Scenario: Corrupt save file in the folder

- **WHEN** one file in `player/builds` is invalid
- **THEN** it is skipped and the remaining builds are still listed

### Requirement: Builds can be shared as files

Export MUST write a build to `export/<name>.json` outside player data so it can be shared by
hand, and import MUST read every `export/*.json` into `player/builds`, returning how many
were imported. Invalid files MUST be skipped silently rather than aborting the import.

#### Scenario: Import with a bad file present

- **WHEN** `export/` holds two valid builds and one invalid file
- **THEN** import returns 2 and the invalid file is ignored

### Requirement: Garage layout is tabs plus a spec table

The garage MUST group parts into tabs (`ZAVĚŠENÍ`, `POHON`, `BRZDY / PNEU`) with one tab
active at a time, and MUST show the KPI cells (0–100, mass, power, front/rear distribution)
plus the spec table next to the preview. It MUST offer leaving to the title screen, driving
the build, and a test run with automatic throttle for 0–100 measurement.

#### Scenario: Tab switched

- **WHEN** the player clicks a tab
- **THEN** only that tab's content is displayed and the rest stays loaded

#### Scenario: Test run started

- **WHEN** the player starts the test run
- **THEN** the car is spawned with automatic throttle so the measured 0–100 can be compared
  against the estimate
