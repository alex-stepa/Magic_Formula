## Purpose

Driving input is data, not hardcoded keys: gameplay actions map to keys through the
`KeyBindings` resource (`src/keybinds.rs`), the player rebinds them in Settings
(`src/ui/settings.rs`), and the choice persists in `player/settings.json`.

## Requirements

### Requirement: Gameplay input goes through the action layer

Every rebindable gameplay input MUST be read as an `Action` via `KeyBindings`
(`pressed` / `just_pressed`), never as a literal `KeyCode` in a gameplay system. The 13
actions are Throttle, Brake, SteerLeft, SteerRight, Clutch, ShiftUp, ShiftDown, Start,
Handbrake, DiffLock, DriveLayout, Camera, Telemetry. The only exceptions today are the
driver aid toggles (see below).

#### Scenario: A system reads driver input

- **WHEN** a gameplay system needs throttle
- **THEN** it asks `bindings.pressed(&keys, Action::Throttle)`
- **AND** it does not reference `KeyCode::KeyW` directly

#### Scenario: A new gameplay input is added

- **WHEN** a change introduces a new driving input
- **THEN** it MUST be added to `Action::ALL` with a Czech UI label, so it appears in the
  rebind list automatically

### Requirement: Driver aid toggles are not rebindable yet

The ABS and TC toggles are read as literal `KeyCode::KeyB` / `KeyCode::KeyN` inside
`vehicle_drive`, so they are gameplay inputs that escaped the action layer. This is a known
gap: a change that makes them rebindable MUST add them to `Action::ALL` with labels and
switch `vehicle_drive` to `bindings.just_pressed`.

#### Scenario: Player rebinds driving keys

- **WHEN** the player remaps controls in Settings
- **THEN** ABS (B) and TC (N) do not appear in the list and stay on their hardcoded keys

### Requirement: System and dev keys stay hardcoded

Esc (pause / close menu) and the dev keys (Backspace = delete cars, 1–5 = spawn builds)
MUST stay hardcoded and MUST NOT be part of `Action` — they are wired into the state
machine and testing, not driving. `Key` therefore deliberately has no Esc variant.

#### Scenario: Rebinding Esc is attempted

- **WHEN** Esc is pressed while a rebind box is waiting for a key
- **THEN** the capture is cancelled and no binding changes

### Requirement: Default bindings match the pre-rebind keys

`KeyBindings::default()` MUST reproduce the keys the game used before the rebind feature
existed: W throttle, S brake/reverse, A/D steering, Left Shift clutch, E/Q shift up/down,
R start, Space handbrake, L diff lock, K drive layout, C camera, T telemetry.

#### Scenario: Fresh install

- **WHEN** no `player/settings.json` exists
- **THEN** the defaults above are active

### Requirement: Bindings map is always complete

`KeyBindings` MUST hold a key for every `Action` — lookup (`key`) indexes the map directly
and would panic on a missing action. `from_map` MUST therefore start from the defaults and
only overwrite the entries present in the loaded data.

#### Scenario: Partial data on disk

- **WHEN** `player/settings.json` contains only some actions (older version of the file)
- **THEN** the missing actions keep their default keys and lookup never panics

#### Scenario: A new action ships in an update

- **WHEN** a player's saved settings predate a newly added action
- **THEN** the new action gets its default key and the rest of the player's bindings survive

### Requirement: Rebind flow in Settings

Clicking a key box MUST arm capture for that action; the first accepted key press
(`Key::from_keycode` returns `Some`) MUST rebind it and disarm capture. Keys outside the
`Key` enum MUST be ignored, the armed box MUST show `...` with a highlighted border, and
clicking a different box MUST move the capture without committing the previous one.

#### Scenario: Successful rebind

- **WHEN** the player clicks the Throttle box and presses `Up`… an unsupported key, then `I`
- **THEN** the unsupported key is ignored, `I` becomes the Throttle key and the box shows `I`

#### Scenario: Capture moved to another action

- **WHEN** a box is armed and the player clicks a different box
- **THEN** capture switches to the new action and the first box keeps its old key

### Requirement: Settings persist on change

Settings (audio volumes, video, FPS cap, bindings) MUST be written to
`player/settings.json` in the frame a change is detected — not every frame — and MUST be
loaded at startup. A missing or corrupt file MUST leave the defaults in place instead of
failing.

#### Scenario: Corrupt settings file

- **WHEN** `player/settings.json` cannot be read or parsed
- **THEN** the game starts on defaults and does not crash

#### Scenario: Value changed in the UI

- **WHEN** a settings resource changes
- **THEN** the full settings snapshot is written once, creating `player/` if needed

### Requirement: Duplicate bindings are currently allowed

Two actions MAY end up on the same key — nothing rejects or swaps a conflicting bind today.
This is a known gap, not a decision: a change that adds conflict handling MUST also define
what happens to the action that previously owned the key.

#### Scenario: Player binds an already-used key

- **WHEN** the player binds `W` to Brake while Throttle is also `W`
- **THEN** both actions currently fire on `W`
