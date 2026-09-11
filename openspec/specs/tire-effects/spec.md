## Purpose

Visual feedback for what the tires are doing: dark skid marks on the ground while sliding
and smoke from badly overheated sliding tires (`src/effects.rs`). Both read solver wheel
telemetry and thermal state; neither feeds back into physics.

## Requirements

### Requirement: Effects are driven by solver telemetry, never the other way round

Skid marks and smoke MUST be derived from `WheelTelem` (`slip_ratio`, `slip_angle`, `fz`)
and `thermal.tire_surf_c`. They MUST NOT write to `SoftBody` or influence grip — turning the
effects off MUST NOT change how the car drives.

#### Scenario: Wheel is airborne

- **WHEN** a wheel has `telem.fz <= 0.0` (no load, no contact)
- **THEN** it produces neither marks nor smoke, whatever its slip says

#### Scenario: Combined slip threshold

- **WHEN** combined slip `sqrt((slip_ratio/0.22)² + (slip_angle/0.20)²)` — 1.0 being the
  onset of the tire curves — exceeds `SKID_MARK` (1.35)
- **THEN** the wheel lays down marks

### Requirement: Skid marks are a bounded recycling pool

Marks MUST be capped at `MARK_CAP` (500) entities. Once the cap is reached the oldest mark
MUST be moved to the new position (ring buffer over `MarkPool.next`) instead of spawning
more. Consecutive marks of one wheel MUST be at least `MARK_SPACING` (0.10 m) apart so a
standing wheel cannot emit one mark per frame.

#### Scenario: Long drift

- **WHEN** a car drifts long enough to exceed 500 marks
- **THEN** entity count stays at 500 and the oldest marks visually disappear as they are reused

#### Scenario: Slipping in place

- **WHEN** a wheel spins with almost no movement
- **THEN** no new mark is created until it has travelled `MARK_SPACING`

#### Scenario: Leaving the game state

- **WHEN** the player leaves `AppState::Game`
- **THEN** marks despawn via `StateScoped` and `MarkPool` MUST be reset on re-entry, so the
  pool never holds dead entities

### Requirement: Marks and smoke sit on the queried terrain height

Effect positions MUST be taken from the wheel hub's X/Z with Y sampled from
`Terrain::height` (marks +0.02 m, smoke +0.1 m), so effects follow hills and the SRTM
heightfield instead of assuming flat ground.

#### Scenario: Skidding on a slope

- **WHEN** a wheel skids on a hill
- **THEN** the mark is placed just above the terrain surface at that point

### Requirement: Smoke is rare, rate-capped and short-lived

Smoke MUST require both a badly overheated tire (`tire_surf_c` above `SMOKE_TEMP` 165 °C,
well past the 140 °C `TIRE_HOT`) and strong slip (above `SMOKE_SLIP` 1.6). Spawning MUST be
rate-capped (one pass per 0.06 s), and each puff MUST rise, grow and fade out, then despawn
at the end of its life so nothing accumulates.

#### Scenario: Hot tire, no slip

- **WHEN** tires are above 165 °C but the car drives straight without sliding
- **THEN** no smoke is produced

#### Scenario: Burnout

- **WHEN** a tire is above 165 °C and combined slip exceeds 1.6
- **THEN** puffs spawn at most every 0.06 s, each rising, growing and fading over 2 s
  before despawning

### Requirement: Effects only run in active gameplay

Effect systems MUST run only while in game and unpaused (`in_game_unpaused`), so a paused
or menu frame produces no marks, no puffs and no material churn.

#### Scenario: Game paused mid-drift

- **WHEN** the player pauses while sliding
- **THEN** no new marks or puffs appear and existing smoke stops animating

### Requirement: Look and tuning are an unverified placeholder

The current sizes, colours, thresholds and rates were tuned blind (the author cannot run the
game headless) and are a deliberate placeholder. A change that touches the look MUST be
verified by a real drive test (`cargo run --release`), not by reasoning alone, and this
requirement MUST be dropped once the look is confirmed.

#### Scenario: Someone proposes new effect constants

- **WHEN** a change adjusts thresholds, colours or sizes here
- **THEN** its task list names a drive test as the verification step
