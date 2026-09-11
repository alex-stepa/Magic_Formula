## Purpose

The car runs on an own node-beam solver: one Structure-of-Arrays state (`SoftBody`), a dumb
semi-implicit Euler integrator with adaptive substepping, a Pacejka tire model, suspension
built as beams by per-kind builders, and a drivetrain where the engine is integrated state
behind a clutch. Sources: `src/solver.rs`, `src/vehicle.rs`.

## Requirements

### Requirement: All vehicle physics state lives in one SoA resource

The whole car's physics state MUST live in the single `SoftBody` resource as parallel arrays
(`pos`, `vel`, `inv_mass`, `force`, `beams`, `wheels`, `entities`, `fuel_nodes`). Beams and
wheels MUST reference nodes **by index**, never by `Entity`. `inv_mass = 0` MUST mean an
anchored node. Vehicles MUST NEVER be simulated through rapier.

#### Scenario: New element added to the car

- **WHEN** a builder adds a node, beam or wheel
- **THEN** it goes through `SoftBody::add_node` / `add_beam` / `add_beam_damped` so damping
  and substep bookkeeping stay in one place

#### Scenario: Rendering the car

- **WHEN** a frame is drawn
- **THEN** each node's and wheel's `Transform` is overwritten from solver positions —
  render entities are output, never the source of truth

### Requirement: The solver is a dumb integrator

`softbody_solve` MUST know nothing about suspension kinds, parts or layouts; it MUST only
iterate indices and read data. All variability MUST be poured in at spawn time by the
builders (`spawn_solver_car`, `spawn_softbody`, `build_suspension`). Adding a suspension kind
or a part MUST NOT change the solver loop.

#### Scenario: A new suspension kind is added

- **WHEN** a new `SuspensionKind` is introduced
- **THEN** only `build_suspension` (and the spawn code that picks anchors) changes
- **AND** `softbody_solve` is untouched

#### Scenario: Car direction is needed

- **WHEN** the solver needs the car's forward/right direction for the tire model
- **THEN** it derives them from `ChassisFrame` node indices (forward = mean(front) −
  mean(rear), right = mean(right) − mean(left), horizontal only), staying data-driven
- **AND** with no chassis frame the directions are zero and the wheels produce no drive force

### Requirement: Real stiffness values, stability from substepping

Stiffness MUST be taken from the data as-is — no scaling, no caps (the old ×0.01 hack is
gone). Stability MUST come from adaptive substepping per frame, computed from a CFL bound
(Gershgorin: `ω²ₘₐₓ ≤ 2·maxᵢ(Σkᵢ/mᵢ)`) with `CFL_SAFETY` margin and clamped to
`MAX_SUBSTEPS`, plus damping scaled with stiffness `d = 2·ζ·√(k·μ)` where `μ` is the beam's
reduced mass. Bump-stop stiffness MUST be included in the substep estimate, since the
effective stiffness on the stop is `k + stop_k`.

#### Scenario: Stiff .veh loaded

- **WHEN** a large `.veh` with real stiffness values spawns
- **THEN** `compute_substeps` raises the substep count (hundreds are expected) instead of
  softening the data
- **AND** the count is clamped at `MAX_SUBSTEPS` so a pathological file cannot freeze the game

#### Scenario: Beam added without explicit damping

- **WHEN** `add_beam` is used
- **THEN** damping is derived as `2·DAMPING_RATIO·√(k·μ)`, so a softer spring gets
  proportionally less damping

#### Scenario: Physics is being verified

- **WHEN** a physics change is tested
- **THEN** it is run with `cargo run --release` — hundreds of substeps per frame are too slow
  in a debug build

### Requirement: Frame-rate-independent ground friction

Ground contact MUST push nodes above the terrain, kill the inward normal velocity
(inelastic ground, no bounce), and damp the tangential component. The tangential factor MUST
be recomputed per substep (`0.7^(1/substeps)`) so the **per-frame** result is the same
regardless of substep count — a fixed per-substep factor would glue a 400-substep `.veh` to
the ground while barely affecting a 4-substep test car.

#### Scenario: Same contact, different substep counts

- **WHEN** a car with 4 substeps and one with 400 substeps touch the ground identically
- **THEN** both lose the same fraction of tangential velocity over the frame

### Requirement: Beams are springs with asymmetric damping and continuous bump stops

Beam force MUST be Hooke along the axis plus damping, using `d_rebound` while extending and
`d` while compressing (real dampers are roughly twice as stiff in rebound). Bump stops MUST
be an **additional** spring `stop_k` outside `[min, max]`, producing a continuous force with
no jump; `stop_k = 0` MUST mean no stops. Degenerate beams (length < 1e-6) MUST be skipped
rather than producing NaN.

#### Scenario: Travel exceeded

- **WHEN** a beam is compressed below `min` or extended past `max`
- **THEN** the stop spring adds force from the threshold on, continuously
- **AND** this is pinned by `solver::tests::bump_stop_engages_outside_travel`

#### Scenario: Suspension spring finished by the builder

- **WHEN** a suspension spring beam is completed (`finish_spring`)
- **THEN** the builder applies the damper multiplier, rebound asymmetry and travel stops —
  the solver stays untouched

### Requirement: Pacejka tire model with a friction circle

Tire forces MUST come from the Magic Formula
`F = D·sin(C·atan(B·x − E·(B·x − atan(B·x))))` with `D = friction·Fz`, evaluated separately
for slip ratio `κ = (roll − v_long)/v_ref` and slip angle `α = atan2(v_lat, v_ref)`, where
`v_ref` is floored at `V_LOW` to avoid division by zero at standstill. Directions MUST be
projected into the terrain's tangent plane, so forces follow slopes. Combined slip MUST be
capped by the friction circle `|(F_long, F_lat)| ≤ friction·Fz`. Vertical contact MUST be a
spring/damper on penetration, and penetration MUST be hard-capped at `TIRE_MAX_SQUASH`
("hitting the rim"), where the hub is projected out and the inward velocity killed. Base
tire curves MUST NOT be edited to express thermal, pressure or wear effects — those are
multiplicative layers on `friction`.

#### Scenario: Launch from standstill

- **WHEN** the car starts from rest
- **THEN** `V_LOW` keeps `κ` finite and the car does not explode

#### Scenario: Wheel in the air

- **WHEN** there is no penetration
- **THEN** `Fz` is zero, no tire force is applied, and telemetry stays zeroed

#### Scenario: Grip layers applied

- **WHEN** temperature, compound, pressure, wear or burst factors are active
- **THEN** they multiply `friction`, which feeds both the curve peak and the friction circle,
  keeping them consistent

#### Scenario: Camber thrust with a knuckle

- **WHEN** a wheel has a `top` node (DoubleWishbone) tilted by γ
- **THEN** `CAMBER_THRUST·Fz·sin(γ)` is added to lateral force **before** the friction circle
  clamp, so camber cannot exceed `μ·Fz`

### Requirement: Wheel spin is its own state

Each wheel MUST integrate its own spin from drive torque, brake torque (always opposing
rotation) and the reaction of the longitudinal tire force, using a disc inertia
`I = ½·m·r²`. Wheel spin MUST NOT be derived from car speed.

#### Scenario: Wheel locks under braking

- **WHEN** brake torque exceeds what the tire can react
- **THEN** spin goes to zero and the tire enters full longitudinal slip

### Requirement: Differential couples wheel speeds by lock factor

For each axle with `lock > 0` the solver MUST lerp both wheels' spin velocities towards
their mean by `lock`: 0 = open (free difference, one wheel spins up), 1 = locked (equal
speed, traction but scrubbing in corners), in between = LSD.

#### Scenario: Open differential

- **WHEN** `lock` is 0
- **THEN** wheel speeds are left untouched

#### Scenario: Locked differential in a corner

- **WHEN** `lock` is 1 while cornering
- **THEN** both wheels are forced to the same speed

### Requirement: Suspension is beams built per kind

`build_suspension` MUST implement each kind as a beam topology between the hub and body
anchors chosen by the spawn code: `Rigid` (stiff beams to nearby body nodes, no travel);
`Coil`/`McPherson`/`TorsionBeam`/`LeafSpring` (two nearly horizontal stiff arms locating the
wheel plus one softer near-vertical spring carrying the weight — vertical travel comes from
geometry, arms resist it only by `Δ²/2L`); `DoubleWishbone` (two arm planes plus a knuckle
`hub→top` giving real camber). Rest length MUST be the spawn distance, so the car settles by
its own sag. Kinds MUST be selectable **per axle**.

#### Scenario: Mixed axles

- **WHEN** the front is McPherson and the rear is TorsionBeam (Fabia MK1 layout)
- **THEN** each wheel is built from its own axle's kind

#### Scenario: DoubleWishbone without a knuckle

- **WHEN** `build_suspension` gets `DoubleWishbone` with `top = None`
- **THEN** it panics loudly rather than silently building a wheel with no camber plane

#### Scenario: Solid versus semi-independent rear

- **WHEN** `TorsionBeam` or `LeafSpring` is built
- **THEN** a cross beam between the rear hubs couples the wheels — a softer twist beam
  (semi-independent, acts as the rear anti-roll bar) or a stiff solid-axle beam respectively

#### Scenario: Anti-roll bar

- **WHEN** `build_antiroll` adds a bar
- **THEN** it is a light apex node above the axle with stiff arms to both hubs, so pure
  bounce moves it freely while roll tensions the arms

### Requirement: Engine is integrated state behind a viscous clutch

Engine speed MUST be integrated from throttle torque, clutch reaction, internal friction and
a deliberately weak idle governor, over `ENGINE_INERTIA` — never derived from wheel speed.
Wheel torque MUST be the **clutch** torque through the gearing, not raw engine torque: what
the clutch cannot transmit does not exist. Clutch torque MUST be viscous in the speed
difference, scaled by engagement and capped by `CLUTCH_CAP`. Below `STALL_RPM` the engine
MUST stall; it MUST restart on the Start action or by being push-started above idle through
an engaged clutch. `engine_torque` MUST interpolate the curve linearly, hold the end values
outside it, and return 0 at or above the limiter.

#### Scenario: Dumping the clutch at idle

- **WHEN** the clutch is engaged against stationary wheels without throttle
- **THEN** the weak idle governor is overpowered and the engine stalls — launching needs
  throttle control, as in a real car

#### Scenario: Neutral or clutch fully out

- **WHEN** gear is neutral or engagement is zero
- **THEN** clutch torque is zero and the engine revs freely

#### Scenario: Shifting

- **WHEN** a shift is requested
- **THEN** it only happens with the clutch disengaged (< 0.5), up to the fitted gearbox's
  gear count and down to reverse

#### Scenario: Stalled engine winds down

- **WHEN** the engine is stalled
- **THEN** it produces no torque and is dragged to zero by friction plus a constant
  compression drag, instead of freezing on the HUD

### Requirement: Drive torque split, brakes and driver aids

Engine torque MUST be split by `DriveLayout::axle_fraction` and then evenly between that
axle's wheels. Brake torque MUST come per axle from the fitted `Brakes` part, with the
handbrake adding to the rear only, and MUST be multiplied by thermal fade regardless of
source. Traction control MUST cut drive torque proportionally above κ ≈ 0.3, and ABS MUST
cut brake torque to `ABS_CUT` when a wheel's slip ratio goes below `−ABS_SLIP`, pulsing at
update rate.

#### Scenario: FWD selected

- **WHEN** the layout is FWD
- **THEN** the front axle gets all torque and the rear gets none

#### Scenario: Wheelspin on launch

- **WHEN** TC is on and a driven wheel exceeds κ ≈ 0.3
- **THEN** its torque is scaled by `0.3 / slip`, settling just past the Pacejka peak instead
  of revving into the limiter and losing lateral grip

#### Scenario: Aid disabled

- **WHEN** ABS or TC is switched off
- **THEN** the corresponding cut is not applied and the wheel is free to lock or spin

### Requirement: Fuel mass is physics, not a gauge

Fuel MUST burn proportionally to engine load and its mass MUST be distributed over the
`fuel_nodes` by rewriting their `inv_mass`, so a lighter car accelerates better but has less
`Fz` for grip. An empty tank MUST stall the engine. Substeps MUST NOT be recomputed per
frame for fuel loss — the ~25 % mass change stays inside the CFL margin.

#### Scenario: Tank runs dry

- **WHEN** fuel reaches zero
- **THEN** the engine stalls and restarting immediately stalls again

#### Scenario: Half a tank burned

- **WHEN** fuel mass drops
- **THEN** the fuel nodes' masses drop with it and the car's behaviour changes through the
  normal physics, with no separate "lightness" factor

### Requirement: Obstacles are analytic colliders on the solver

World obstacles MUST be analytic shapes (`Box`, vertical `Cylinder`) resolved the same way
as the ground: push the node out along the cheapest escape and kill the inward velocity
(inelastic). The strongest inward velocity killed per frame MUST be reported as a single
`CrashImpact` event above `CRASH_MIN`, so scraping a wall does not fire bangs.

#### Scenario: Node inside a box

- **WHEN** a node is inside a box and moving further in
- **THEN** it is pushed to the nearest face and the inward velocity component is removed
- **AND** this is pinned by `solver::tests::box_pushes_node_out_and_kills_inward_vel`

#### Scenario: Node beside a cylinder

- **WHEN** a node is inside the cylinder radius but outside its height range
- **THEN** nothing is changed
- **AND** this is pinned by `solver::tests::cyl_pushes_node_out_horizontally`

#### Scenario: Light scrape along a wall

- **WHEN** the killed inward velocity stays below `CRASH_MIN`
- **THEN** no crash event is emitted

### Requirement: Tuning values are named constants in one place

Tire, suspension and drivetrain calibration values MUST be named `pub const`s
(`TIRE_LONG`, `TIRE_LAT`, `TIRE_DEFAULT`, `SUSP_*`, `STOP_*`, `REBOUND_MULT`, `ARB_K`,
`KNUCKLE_*`, `CLUTCH_*`, `IDLE_GAIN`, `STALL_RPM`, …) so the car can be retuned without
touching solver logic. The Pacejka base curves are tuned and MUST NOT be edited to model a
different part, compound or temperature — add a layer instead.

#### Scenario: A part needs more grip

- **WHEN** a sportier compound should grip better
- **THEN** it raises the `friction` ceiling / grip layer rather than reshaping `TIRE_LAT`
