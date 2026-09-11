## Purpose

All game sound comes out of one self-written low-latency audio path (`src/audio.rs`): the
engine is **granular synthesis** — a single synthesized combustion grain retriggered at the
firing frequency with overlapping voices — mixed with tire screech, shift blip, limiter
bounce, crash, wind, brake squeal and surface rumble.

## Requirements

### Requirement: Direct cpal output, not Bevy audio

Sound MUST be produced by an own `cpal` output stream with a small buffer
(`BufferSize::Fixed(512)` ≈ 12 ms, falling back to the device default when Fixed is
unsupported), because Bevy/rodio's ~100–250 ms buffer is both too laggy and too coarse for
engine sound. The stream MUST be held in a `NonSend` resource (`cpal::Stream` is `!Send`) or
it would drop and go silent. Failure to open a device or an unexpected sample format MUST
warn and continue **silently**, never panic.

#### Scenario: No audio device available

- **WHEN** `build_cpal_stream` cannot get a device or gets a non-F32 format
- **THEN** a warning is logged and the game runs without sound

#### Scenario: Device rejects the fixed buffer size

- **WHEN** building the stream with `Fixed(512)` fails
- **THEN** it is rebuilt with the device default buffer size

### Requirement: Game thread and audio thread talk through atomics only

The game system (`drive_engine_sound`) MUST only **write** state into shared atomics
(`f32` in bits), and the cpal callback MUST only **read** them — no locks, no allocation in
the callback. Discrete events (shift, crash) MUST be passed as monotonically increasing
sequence counters, so the audio side detects "counter changed" and fires its envelope; it
must be impossible to miss or double-fire by sampling a boolean.

#### Scenario: Shift happens between audio callbacks

- **WHEN** the game bumps `shift_seq`
- **THEN** the audio side sees a new value once and triggers exactly one blip

#### Scenario: Several crashes in one frame

- **WHEN** multiple `CrashImpact` events arrive in a frame
- **THEN** only the strongest is used, and a 0.25 s cooldown prevents bangs from merging

### Requirement: Pitch is retrigger timing, never a playback-rate parameter

Engine pitch MUST come from the phase accumulator that decides **when** the next grain
fires, advanced in audio time from the firing frequency `rpm / 120 · cylinders`. It MUST NOT
be implemented as a pitch/speed parameter applied to a buffer: the audio thread renders a
whole buffer from one atomic snapshot, so a rate parameter freezes and then jumps (the bug
of the earlier loop/synth versions). With retrigger timing, a stale rpm is only inaudible
jitter.

#### Scenario: Rapid rev change

- **WHEN** rpm changes quickly between callbacks
- **THEN** the pitch follows smoothly with no audible steps or stalls

#### Scenario: Sound derived from engine data

- **WHEN** the fitted engine's cylinder count, displacement or configuration changes
- **THEN** firing frequency, cylinder size (depth) and firing evenness change accordingly —
  Inline fires evenly, V and Boxer get uneven spacing (burble/offbeat rumble)

### Requirement: Overlapping voices stay level and bounded

Voices MUST be capped (48 concurrent) and the mix MUST be normalized by `1/√(voices)` with
smoothing, so idle and redline hold a comparable loudness and a jump in voice count cannot
click. Individual voices MUST get slightly randomized speed and amplitude — a perfectly
periodic engine sounds like a musical instrument.

#### Scenario: Redline

- **WHEN** rpm rises so grains overlap heavily
- **THEN** voice count stays bounded and output level stays comparable to idle

### Requirement: Sound is silent unless actually driving

The engine gain MUST be zero unless the app is in `AppState::Game`, a car is spawned, and
the engine is not stalled. All driving-derived inputs (skid, brake squeal, surface, wind,
redline) MUST be zeroed when not playing.

#### Scenario: In the menu

- **WHEN** the player is in a menu or no car is spawned
- **THEN** the mix is silent

#### Scenario: Engine stalled

- **WHEN** the drivetrain reports `stalled`
- **THEN** the engine voice goes quiet while the stream stays alive

#### Scenario: Re-entering the game

- **WHEN** the player re-enters `AppState::Game`
- **THEN** no spurious shift blip fires on the first frame (previous gear is unknown, so it
  is only recorded)

### Requirement: Driving sounds are derived from physics and thermal state

Each auxiliary sound MUST be computed from real simulation state, per wheel, taking the
loudest wheel:

- Tire screech from combined slip past onset (κ/0.22, α/0.20), scaled by
  `thermal::tire_squeal_gain` — cold or overheated rubber protests more.
- Brake squeal only while pads actually slip on a **turning** disc (`brake > 1.0`,
  `fz > 0`, from `spin_vel`) and louder when pads are cold (silent by ~200 °C); a locked
  wheel MUST be silent there and hand over to tire screech.
- Surface rumble from map roughness × speed, only with a wheel on the ground; asphalt
  (`TestPad`) MUST be silent.
- Shift blip only with a turbo fitted (`boost > 0`) above ~60 % of redline — it is a
  BOV/flutter, so an atmospheric engine MUST shift silently.
- Limiter bounce as a fast stutter gate at redline.
- Crash magnitude scaled with impact speed (~12 m/s = full).

#### Scenario: Locked wheel under braking

- **WHEN** a wheel locks (`spin_vel → 0`) with the brake on
- **THEN** brake squeal stops and tire screech takes over

#### Scenario: Shifting an atmospheric engine

- **WHEN** the fitted engine has `boost = 0` and the player shifts
- **THEN** no blip is produced

#### Scenario: Driving on the test pad

- **WHEN** the map is `TestPad`
- **THEN** surface rumble is zero at any speed

### Requirement: Volume settings apply live

`AudioSettings` (master, engine, screech, crash, wind) MUST be pushed to the audio side
every frame and applied inside the callback — master scaling the whole mix, the rest their
own voices. The final mix MUST be clamped to [-1, 1].

#### Scenario: Slider moved during play

- **WHEN** the player changes a volume in the pause menu
- **THEN** the change is audible immediately, without rebuilding the stream

### Requirement: Sound is verified by listening, not by assertions

Audio quality cannot be asserted, so the module's tests MUST be `#[ignore]`d **render
tools** that write WAV/CSV to `/tmp` for the author to listen to (`render_grain`,
`render_grain_sweep`, `render_config`, `render_extras`), driven where useful by a real rpm
trace captured with `MF_RPM_TRACE=1`. A change to the sound MUST name a listening test as
its verification; `cargo test` alone is not evidence.

#### Scenario: Engine sound is retuned

- **WHEN** grain synthesis or a mix constant changes
- **THEN** the relevant `--ignored` render test is run and the WAV is listened to before the
  change is called done

#### Scenario: Default test run

- **WHEN** `cargo test` runs normally
- **THEN** the render tools are skipped and nothing writes to `/tmp`
