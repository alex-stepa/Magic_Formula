## 1. Akce v keybinds.rs

- [ ] 1.1 Přidat varianty `Abs` a `Tc` do `enum Action`, doplnit je do `Action::ALL`
  (délka 13 → 15) a do `label()` dát české popisky ("ABS", "Kontrola trakce").
  Ověření: `cargo check` projde (chybějící arm v `label()` je compile error, `ALL` má
  pevnou délku v typu → překladač obojí vynutí).
- [ ] 1.2 Doplnit defaulty `(Abs, B)` a `(Tc, N)` do `KeyBindings::default()`.
  Ověření: `cargo test` zelené; `bindings.key(Action::Abs)` nesmí panikovat (lookup
  indexuje mapu přímo — chybějící default = panika za běhu, ne compile error).

## 2. Čtení v vehicle_drive

- [ ] 2.1 Nahradit `keys.just_pressed(KeyCode::KeyB)` a `KeyCode::KeyN` ve `vehicle_drive`
  za `bindings.just_pressed(&keys, Action::Abs)` / `Action::Tc`. Logiku ABS/TC (`ABS_SLIP`,
  `ABS_CUT`, kráceni momentu nad κ 0.3) nechat bez jediné změny.
  Ověření: `cargo check`; `grep -n "KeyCode::Key[BN]" src/vehicle.rs` nesmí nic najít.

## 3. Ověření

- [ ] 3.1 Jízdní test: `cargo run --release` → v Settings přemapovat ABS na jinou klávesu,
  vyjet, ověřit že nová klávesa ABS přepíná (info log "ABS: ON/OFF") a stará už ne.
  Ověření: viditelný log + chování brzdy při zámku kola.
- [ ] 3.2 Zpětná kompatibilita settings: mít `player/settings.json` z předchozí verze
  (bez `Abs`/`Tc`), spustit hru, ověřit že stará přemapování drží a aidy sedí na B/N.
  Ověření: přemapování zůstala + oba aidy fungují na defaultech.
