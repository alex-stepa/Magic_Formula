## Purpose

Brakes, tires and the engine have temperature, and temperature has consequences: brake fade,
grip loss outside the tire's operating window, tire pressure rise, wear, burst, and engine
power loss when overheating (`src/thermal.rs`). Everything is a **neutral layer above** the
base physics — stock parts at working temperature must feel exactly like no thermal model.

## Requirements

### Requirement: Lumped-capacity model, one step per frame

Temperatures MUST be integrated as lumped capacities, `T_dot = (heat_in − cooling) / C`,
**once per frame** in `wheel_thermal` (Update), never per solver substep — thermal events
take seconds while substeps are sub-millisecond. Cooling MUST be `(h0 + h1·|v|)·(T − ambient)`:
passive plus speed-proportional convection. No temperature may fall below ambient.

#### Scenario: Frame with zero delta time

- **WHEN** `dt <= 0.0`
- **THEN** the thermal step returns without touching any temperature

#### Scenario: Coasting after abuse

- **WHEN** the car rolls without braking or slipping
- **THEN** brake and tire surface temperatures both fall towards ambient
- **AND** this is pinned by `thermal::tests::integrate_heats_and_cools`

#### Scenario: Ambient changed at runtime

- **WHEN** the player moves the ambient slider (`Ambient` resource)
- **THEN** parts cool towards the new ambient and spawn temperatures follow it

### Requirement: Constants are calibration knobs in one place

Every thermal constant (capacities, `h0`/`h1`, thresholds, floors, rates) MUST live as a
named `pub const` in `thermal.rs`, not inline in a formula. The physical world needs tuning
the model cannot see, so these values MUST stay adjustable without touching logic.

#### Scenario: Retuning a threshold

- **WHEN** fade onset or the grip window needs adjusting
- **THEN** it is a constant edit (or a part value), not a code change in the integrator

### Requirement: Brake heat comes from real brake work

Brake heating MUST be `brake_nm · |spin_vel|`, computed from the **already faded** wheel
brake torque (`wheel_thermal` runs after `vehicle_drive`), which makes the model
self-limiting: less torque means less heat. A locked wheel (`spin_vel ≈ 0`) MUST therefore
generate no brake heat — that friction work goes into tire slip instead. Front and rear
discs MUST use their own capacity from the fitted `Brakes` part.

#### Scenario: Locked wheel under full brake pressure

- **WHEN** a wheel is locked (`spin_vel ≈ 0`) with brake applied
- **THEN** its brake temperature does not rise
- **AND** its tire surface temperature does, from slip

#### Scenario: Heavy braking from speed

- **WHEN** the car brakes hard for ~10 s at ~28 m/s
- **THEN** brake temperature rises by tens of degrees and settles around the fade band
  rather than climbing without limit

### Requirement: Brake fade

`brake_fade` MUST return 1.0 up to the fitted part's `fade_start` (default `FADE_START`
450 °C), then fall linearly to `FADE_FLOOR` (0.4) at `FADE_FULL` (700 °C) and stay there —
overheated pads lose bite but never all of it. The result MUST multiply wheel brake torque
in `vehicle_drive`.

#### Scenario: Cold brakes

- **WHEN** brake temperature is at or below `fade_start`
- **THEN** the factor is exactly 1.0 (no behaviour change from the model)

#### Scenario: Softer pad compound

- **WHEN** a part with a lower `fade_start` is fitted
- **THEN** it fades earlier than stock at the same temperature
- **AND** this is pinned by `thermal::tests::brake_fade_curve`

### Requirement: Two-layer tire temperature

A tire MUST have a fast surface (contact patch, small capacity) and a slow carcass core,
coupled by conduction `TIRE_COND`, each also cooling to air. The surface feeds grip; the
core carries heat between corners and drives pressure. Heat input MUST be computed from the
**wheel's** slip speed (circumferential minus longitudinal car speed), not the car's speed,
and MUST be capped by `SLIP_SPEED_CAP` to stop runaway. Rolling resistance (`ROLL_HEAT`)
MUST heat a loaded tire even with zero slip.

#### Scenario: Burnout from standstill

- **WHEN** the car stands still (`v ≈ 0`) and a driven wheel spins
- **THEN** tire surface temperature rises sharply
- **AND** this is pinned by the burnout case in `integrate_heats_and_cools`

#### Scenario: Cold start on a gentle drive

- **WHEN** the car is driven for a minute without sliding
- **THEN** rolling resistance alone warms the tire towards the operating window

#### Scenario: Surface versus core

- **WHEN** tires are worked hard
- **THEN** surface temperature is above core temperature (core lags)

#### Scenario: Weight transfer in a corner

- **WHEN** the outer wheel carries more `fz` (and thus more lateral force) than the inner
- **THEN** the outer tire ends up hotter, purely from per-wheel telemetry
- **AND** this is pinned by `thermal::tests::asymmetric_heating_from_fz`

### Requirement: Temperature grip layer is neutral inside the window

`grip_mult` MUST read **surface** temperature and return exactly 1.0 inside the fitted
compound's window `[win_lo, win_hi]` — inside the window the Pacejka curves MUST be
untouched. Below it, grip ramps down to `GRIP_COLD` at `TIRE_COLD`; above it, down to
`GRIP_HOT` at `TIRE_HOT`. The single returned factor MUST scale solver friction so peak and
friction circle stay consistent. The base tire curves MUST NOT be edited to express thermal
effects.

#### Scenario: Tire in its window

- **WHEN** surface temperature is between `win_lo` and `win_hi`
- **THEN** the multiplier is exactly 1.0

#### Scenario: Wider-window compound

- **WHEN** a compound with a higher `win_hi` is fitted
- **THEN** at 108 °C it is still on the plateau while the stock window is already fading
- **AND** both are pinned by `thermal::tests::grip_mult_window`

### Requirement: Pressure and wear are neutral on stock parts

Tire pressure MUST rise linearly with **core** temperature above ambient
(`PRESS_TEMP_COEF`). `pressure_grip` MUST be 1.0 at the pressure the tire reaches in the
middle of its window, losing grip with deviation proportional to the compound's `press_sens`
and never below `PRESS_GRIP_FLOOR`. With the stock compound (`press_sens = 0`) it MUST be
1.0 always. Wear MUST accumulate from slip work, accelerate above the window, reduce grip
towards `WEAR_GRIP_FLOOR`, and at `BURST_WEAR` set `burst`.

#### Scenario: Stock compound, any temperature

- **WHEN** the default compound is fitted
- **THEN** the pressure grip factor is 1.0 at every temperature (layer parked)

#### Scenario: Softer compound wears faster

- **WHEN** two compounds differ only in `wear_rate` and take the same slip
- **THEN** the higher-rate compound wears more
- **AND** this is pinned by `thermal::tests::tire_wear_and_burst`

#### Scenario: Gentle driving

- **WHEN** the car is driven for a minute without slip
- **THEN** wear stays negligible (< 0.05)

#### Scenario: Tire bursts

- **WHEN** `wear` reaches `BURST_WEAR` during a long burnout
- **THEN** `burst` is set, grip collapses to `BURST_GRIP`, and a `CrashImpact` event fires
  once for the bang sound — reusing the crash voice rather than adding one

### Requirement: Engine cooling circuit

Engine temperature MUST use `engine_step`: idle baseline plus load-proportional heat
(`load = rpm_frac × throttle`), cooled by `(h0 + h1·|v|)` taken from the fitted `Cooler`
part. A stalled engine MUST produce no heat and cool to ambient. `engine_power_factor` MUST
be 1.0 below `ENGINE_OVERHEAT` (115 °C) and fall linearly to `ENGINE_POWER_FLOOR` (0.6) at
`ENGINE_COOK` (135 °C).

#### Scenario: Idling in place

- **WHEN** the engine idles at standstill for minutes
- **THEN** temperature settles around ~90 °C (tuned) and power stays at 1.0

#### Scenario: Sustained load without airflow

- **WHEN** the engine runs at full load with `v = 0` for two minutes
- **THEN** it exceeds `ENGINE_OVERHEAT` and loses power

#### Scenario: Better cooler fitted

- **WHEN** a part with higher `h0`/`h1` is fitted at the same load
- **THEN** it settles at a lower temperature than stock
- **AND** all three are pinned by `thermal::tests::engine_thermal`

### Requirement: Stock parts reproduce the constants exactly

Default part values (`Compound`, `Cooler`, `Brakes` via `Loadout::default()`) MUST equal the
module constants, so fitting a stock part changes nothing about how the car feels. This is a
no-change guarantee from moving thermals onto parts and MUST stay enforced by a test.

#### Scenario: Default loadout

- **WHEN** the default loadout is used
- **THEN** compound window/wear rate, cooler `h0`/`h1`, brake capacities and `fade_start`
  equal the constants
- **AND** this is pinned by `thermal::tests::default_parts_equal_constants`
