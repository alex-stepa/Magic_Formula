## Why

ABS and TC are gameplay inputs that escaped the action layer: `vehicle_drive` reads
`KeyCode::KeyB` and `KeyCode::KeyN` literally, so they are the only driving controls a
player cannot remap. The `controls` baseline spec records this as a known gap. Closing it
now is cheap (the whole rebind machinery already exists) and removes the one exception from
the rule "gameplay input goes through `Action`", before more aids are added and copy the
same shortcut.

## What Changes

- Add `Action::Abs` and `Action::Tc` to `Action::ALL` with Czech labels, defaults `B` and `N`
  (the current hardcoded keys) → they appear in the Settings rebind list automatically.
- Replace the two `keys.just_pressed(KeyCode::KeyB / KeyN)` reads in `vehicle_drive` with
  `bindings.just_pressed(&keys, Action::Abs / Action::Tc)`.
- Not a breaking change for saved settings: `KeyBindings::from_map` starts from the defaults,
  so an existing `player/settings.json` without the new actions keeps its bindings and the
  two new ones get `B`/`N`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `controls`: `Gameplay input goes through the action layer` grows to 15 actions and loses
  its "only exceptions are the driver aid toggles" escape clause; `Default bindings match
  the pre-rebind keys` gains B = ABS, N = TC; `Driver aid toggles are not rebindable yet`
  is removed — the gap it documents is closed.

## Impact

- `src/keybinds.rs`: two enum variants, two labels, two default entries, `ALL` array length
  13 → 15.
- `src/vehicle.rs` (`vehicle_drive`): two input reads switch to the bindings resource; the
  system already takes `Res<KeyBindings>`, so no signature change.
- `src/ui/settings.rs`: no change — the rebind UI is generated from `Action::ALL`.
- `player/settings.json`: forward compatible, no migration.
- No physics, no solver, no architecture invariant touched: ABS/TC logic itself
  (`ABS_SLIP`, `ABS_CUT`, the κ > 0.3 TC cut) stays exactly as it is.

## Non-goals

- Not touching how ABS or TC actually work — no retuning of thresholds, no new aids.
- Not adding duplicate-bind detection (a separate known gap in the same spec).
- Not making Esc or the dev keys (Backspace, 1–5) rebindable — they stay hardcoded by design.
- No gamepad or wheel input; steering feel work waits for real hardware.

## Architecture invariants touched

None. This is input plumbing above the solver; `SoftBody`, the builders and the tire model
are untouched.

## Implementation mode: guide

Per AGENTS.md rule #1b this is close to boilerplate/wiring, but it edits `vehicle_drive`
(a gameplay system) and adding enum variants plus `HashMap` defaults is exactly the kind of
small Rust exercise the author should write himself. Claude explains each step and reviews.
