## MODIFIED Requirements

### Requirement: Gameplay input goes through the action layer

Every rebindable gameplay input MUST be read as an `Action` via `KeyBindings`
(`pressed` / `just_pressed`), never as a literal `KeyCode` in a gameplay system. The 15
actions are Throttle, Brake, SteerLeft, SteerRight, Clutch, ShiftUp, ShiftDown, Start,
Handbrake, DiffLock, DriveLayout, Camera, Telemetry, Abs, Tc. There are no exceptions:
a gameplay input that is not in `Action` is a defect.

#### Scenario: A system reads driver input

- **WHEN** a gameplay system needs throttle
- **THEN** it asks `bindings.pressed(&keys, Action::Throttle)`
- **AND** it does not reference `KeyCode::KeyW` directly

#### Scenario: A new gameplay input is added

- **WHEN** a change introduces a new driving input
- **THEN** it MUST be added to `Action::ALL` with a Czech UI label, so it appears in the
  rebind list automatically

#### Scenario: Driver aids toggled

- **WHEN** the player toggles ABS or TC
- **THEN** the toggle is read through `Action::Abs` / `Action::Tc` and therefore honours
  whatever keys the player bound

### Requirement: Default bindings match the pre-rebind keys

`KeyBindings::default()` MUST reproduce the keys the game used before the rebind feature
existed: W throttle, S brake/reverse, A/D steering, Left Shift clutch, E/Q shift up/down,
R start, Space handbrake, L diff lock, K drive layout, C camera, T telemetry, B ABS,
N traction control.

#### Scenario: Fresh install

- **WHEN** no `player/settings.json` exists
- **THEN** the defaults above are active

#### Scenario: Aids on their historical keys

- **WHEN** a player who never rebinds anything presses B or N
- **THEN** ABS and TC toggle exactly as they did before this change

## REMOVED Requirements

### Requirement: Driver aid toggles are not rebindable yet

**Reason**: The gap is closed — ABS and TC now go through `Action::Abs` / `Action::Tc` like
every other gameplay input, so the requirement documenting the exception no longer describes
the system.

**Migration**: None for players: the new actions default to the previously hardcoded `B` and
`N`, and a saved `player/settings.json` without them keeps its existing bindings because
`from_map` starts from the defaults. Any code still reading `KeyCode::KeyB` / `KeyCode::KeyN`
for aids MUST be switched to the bindings resource.
