// keybinds.rs — přemapovatelné ovládání auta. Gameplay akce → klávesa, defaulty = dnešní
// natvrdo zadané klávesy (viz vehicle.rs/main.rs/hud.rs před tímhle modulem).
// Esc (pauza/zavření menu) a dev klávesy (Backspace smaž auta, 1–5 spawn buildů) zůstávají
// natvrdo — nejsou to gameplay akce, jsou zapletené do stavového automatu / testování.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// Bevy KeyCode nemá (bez extra feature) serde — vlastní malý enum pro klávesy, které
// dává smysl přemapovat (písmena + pár pojmenovaných). Kdo stiskne cokoliv mimo tenhle
// výčet při rebindu, ignorujeme (viz Key::from_keycode).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Space, ShiftLeft, ShiftRight, ControlLeft, AltLeft, Tab, Enter,
}

impl Key {
    pub fn to_keycode(self) -> KeyCode {
        use Key::*;
        match self {
            A => KeyCode::KeyA, B => KeyCode::KeyB, C => KeyCode::KeyC, D => KeyCode::KeyD,
            E => KeyCode::KeyE, F => KeyCode::KeyF, G => KeyCode::KeyG, H => KeyCode::KeyH,
            I => KeyCode::KeyI, J => KeyCode::KeyJ, K => KeyCode::KeyK, L => KeyCode::KeyL,
            M => KeyCode::KeyM, N => KeyCode::KeyN, O => KeyCode::KeyO, P => KeyCode::KeyP,
            Q => KeyCode::KeyQ, R => KeyCode::KeyR, S => KeyCode::KeyS, T => KeyCode::KeyT,
            U => KeyCode::KeyU, V => KeyCode::KeyV, W => KeyCode::KeyW, X => KeyCode::KeyX,
            Y => KeyCode::KeyY, Z => KeyCode::KeyZ,
            Space => KeyCode::Space,
            ShiftLeft => KeyCode::ShiftLeft,
            ShiftRight => KeyCode::ShiftRight,
            ControlLeft => KeyCode::ControlLeft,
            AltLeft => KeyCode::AltLeft,
            Tab => KeyCode::Tab,
            Enter => KeyCode::Enter,
        }
    }

    pub fn from_keycode(k: KeyCode) -> Option<Self> {
        use Key::*;
        Some(match k {
            KeyCode::KeyA => A, KeyCode::KeyB => B, KeyCode::KeyC => C, KeyCode::KeyD => D,
            KeyCode::KeyE => E, KeyCode::KeyF => F, KeyCode::KeyG => G, KeyCode::KeyH => H,
            KeyCode::KeyI => I, KeyCode::KeyJ => J, KeyCode::KeyK => K, KeyCode::KeyL => L,
            KeyCode::KeyM => M, KeyCode::KeyN => N, KeyCode::KeyO => O, KeyCode::KeyP => P,
            KeyCode::KeyQ => Q, KeyCode::KeyR => R, KeyCode::KeyS => S, KeyCode::KeyT => T,
            KeyCode::KeyU => U, KeyCode::KeyV => V, KeyCode::KeyW => W, KeyCode::KeyX => X,
            KeyCode::KeyY => Y, KeyCode::KeyZ => Z,
            KeyCode::Space => Space,
            KeyCode::ShiftLeft => ShiftLeft,
            KeyCode::ShiftRight => ShiftRight,
            KeyCode::ControlLeft => ControlLeft,
            KeyCode::AltLeft => AltLeft,
            KeyCode::Tab => Tab,
            KeyCode::Enter => Enter,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        use Key::*;
        match self {
            A => "A", B => "B", C => "C", D => "D", E => "E", F => "F", G => "G", H => "H",
            I => "I", J => "J", K => "K", L => "L", M => "M", N => "N", O => "O", P => "P",
            Q => "Q", R => "R", S => "S", T => "T", U => "U", V => "V", W => "W", X => "X",
            Y => "Y", Z => "Z",
            Space => "Space",
            ShiftLeft => "Shift", ShiftRight => "R Shift",
            ControlLeft => "Ctrl", AltLeft => "Alt", Tab => "Tab", Enter => "Enter",
        }
    }
}

// Přemapovatelné gameplay akce. Pořadí = pořadí v UI (settings Main tab).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Throttle, Brake, SteerLeft, SteerRight, Clutch, ShiftUp, ShiftDown, Start,
    Handbrake, DiffLock, DriveLayout, Camera, Telemetry, Abs, Tc,
}

impl Action {
    pub const ALL: [Action; 15] = [
        Action::Throttle, Action::Brake, Action::SteerLeft, Action::SteerRight, Action::Clutch,
        Action::ShiftUp, Action::ShiftDown, Action::Start, Action::Handbrake, Action::DiffLock,
        Action::DriveLayout, Action::Camera, Action::Telemetry, Action::Abs, Action::Tc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Throttle => "Plyn",
            Action::Brake => "Brzda / zpátečka",
            Action::SteerLeft => "Řízení vlevo",
            Action::SteerRight => "Řízení vpravo",
            Action::Clutch => "Spojka",
            Action::ShiftUp => "Řadit nahoru",
            Action::ShiftDown => "Řadit dolů",
            Action::Start => "Nastartovat",
            Action::Handbrake => "Ruční brzda",
            Action::DiffLock => "Uzamknout diff",
            Action::DriveLayout => "Rozvod pohonu",
            Action::Camera => "Kamera",
            Action::Telemetry => "Telemetrie",
            Action::Abs => "ABS",
            Action::Tc => "Kontrola trakce",
        }
    }
}

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct KeyBindings(HashMap<Action, Key>);

impl Default for KeyBindings {
    fn default() -> Self {
        use Action::*;
        use Key::*;
        Self(HashMap::from([
            (Throttle, W), (Brake, S), (SteerLeft, A), (SteerRight, D), (Clutch, ShiftLeft),
            (ShiftUp, E), (ShiftDown, Q), (Start, R), (Handbrake, Space), (DiffLock, L),
            (DriveLayout, K), (Camera, C), (Telemetry, T), (Abs, B), (Tc, N),
        ]))
    }
}

impl KeyBindings {
    pub fn key(&self, a: Action) -> Key {
        self.0[&a]
    }
    pub fn pressed(&self, keys: &ButtonInput<KeyCode>, a: Action) -> bool {
        keys.pressed(self.key(a).to_keycode())
    }
    pub fn just_pressed(&self, keys: &ButtonInput<KeyCode>, a: Action) -> bool {
        keys.just_pressed(self.key(a).to_keycode())
    }
    pub fn rebind(&mut self, a: Action, k: Key) {
        self.0.insert(a, k);
    }
    // Serializovatelný snapshot pro player/settings.json (settings.rs).
    pub fn as_map(&self) -> &HashMap<Action, Key> {
        &self.0
    }
    pub fn from_map(m: HashMap<Action, Key>) -> Self {
        let mut s = Self::default();
        for (a, k) in m {
            s.0.insert(a, k);
        }
        s
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<KeyBindings>();
}
