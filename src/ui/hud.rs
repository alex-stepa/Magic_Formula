// Herní HUD podle návrhu: rally dash (styl Sinco Tech — RPM bar se segmenty,
// shift LED, velká rychlost, kvalt) + telemetrie overlay na T + pauza na Esc.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::solver::SoftBody;
use crate::ui::settings::SettingsUi;
use crate::ui::*;
use crate::vehicle::{DrivetrainState, Loadout};

pub fn plugin(app: &mut App) {
    app.init_resource::<AccelTimer>()
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_systems(OnEnter(AppState::Game), (spawn_hud, spawn_temps))
        .add_systems(OnExit(AppState::Game), |mut p: ResMut<Paused>| p.0 = false)
        .add_systems(
            Update,
            (esc_pause, toggle_telemetry_panel, toggle_temps_panel, sync_node_viz, sync_pause_overlay, pause_buttons, update_dash, update_telemetry, update_temps, update_fps)
                .run_if(in_state(AppState::Game)),
        )
        .add_systems(Update, accel_timer.run_if(in_game_unpaused));
}

// Měřené 0–100 km/h: nasčítá čas od rozjezdu z místa do 100. Latch po dosažení,
// rearm při zastavení. Garáž dává odhad, tohle je MĚŘENÍ z reálné jízdy.
#[derive(Resource, Default)]
struct AccelTimer {
    launch: Option<f32>, // čas startu (od rozjezdu), None = nejede / zlatchováno
    best: Option<f32>,   // poslední změřený čas 0–100
    latched: bool,       // dosáhlo 100 → neměř znovu dokud nezastaví
}

fn accel_timer(
    time: Res<Time>,
    state: Res<DrivetrainState>,
    mut t: ResMut<AccelTimer>,
    mut test: ResMut<crate::vehicle::TestDrive>,
) {
    t.tick(state.speed_kmh.abs(), time.elapsed_secs());
    if t.latched && test.0 {
        test.0 = false; // dosáhlo 100 → vypni auto-plyn (test dojel)
    }
}

impl AccelTimer {
    // Stavový automat měření: rearm pod 1 km/h, start rozjezdu od 2, latch při 100.
    fn tick(&mut self, v: f32, now: f32) {
        if v < 1.0 {
            self.launch = None;
            self.latched = false;
        } else if !self.latched {
            if self.launch.is_none() && v >= 2.0 {
                self.launch = Some(now);
            }
            if let Some(t0) = self.launch {
                if v >= 100.0 {
                    self.best = Some(now - t0);
                    self.launch = None;
                    self.latched = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod accel_tests {
    use super::AccelTimer;
    #[test]
    fn measures_launch_to_100() {
        let mut t = AccelTimer::default();
        t.tick(0.0, 0.0); // stojí
        t.tick(3.0, 1.0); // rozjezd → launch = 1.0
        t.tick(50.0, 5.0); // jede
        t.tick(101.0, 8.5); // překročil 100 → best = 7.5
        assert_eq!(t.best, Some(7.5));
        assert!(t.latched);
        // dokud nezastaví, neměří znovu
        t.tick(120.0, 12.0);
        assert_eq!(t.best, Some(7.5));
        // zastavení rearmuje
        t.tick(0.0, 20.0);
        assert!(!t.latched && t.launch.is_none());
    }
}

// ── markery ──────────────────────────────────────────────────────────────────

#[derive(Component)]
struct RpmSeg(usize); // segment RPM baru (0..32)
#[derive(Component)]
struct ShiftLed(usize); // LED na rámečku (0..8, 4 vlevo + 4 vpravo)
#[derive(Component)]
struct RpmText;
#[derive(Component)]
struct SpeedText;
#[derive(Component)]
struct GearText;
#[derive(Component)]
struct StatusText; // SPOJKA / CHCÍPLO — malý řádek v LCD
#[derive(Component)]
struct AccelText; // měřené 0–100 km/h

#[derive(Component)]
struct TelemetryPanel;
#[derive(Component)]
struct TelemName(usize); // jméno kola v řádku tabulky
#[derive(Component)]
struct TelemCell(usize, usize); // (řádek=kolo, sloupec: 0 Fz, 1 skluz, 2 áz, 3 F_long, 4 F_lat)
#[derive(Component)]
struct TelemStat(usize); // spodní grid: 0 Fz předek, 1 áz, 2 pohon, 3 diff, 4 spojka, 5 záznam
#[derive(Component)]
struct RecText; // „● REC → CSV" v hlavičce overlay

#[derive(Component)]
struct PauseOverlay;
#[derive(Component)]
struct FpsText;
#[derive(Component, Clone, Copy)]
enum PauseAction {
    Resume,
    Settings,
    Garage,
    Title,
}

const SEGMENTS: usize = 32; // RPM bar
const LEDS: usize = 8; // shift LED (posledních ~2 = „červená" zóna)
const UNLIT: Color = Color::srgba(0.949, 0.949, 0.953, 0.14);
const LCD_BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.35);
const PANEL_BG: Color = Color::srgba(0.114, 0.176, 0.239, 0.92); // accent-900 ~92 %
// Panel TEPLOTY (toggle G, vlevo dole): bary brzda/pneu/opotřebení per kolo + motor.
// Fill node = šířka % (podíl v rozsahu) + barva z gradientu; label = číslo (°C / %).
#[derive(Component)]
struct TempsPanel;
#[derive(Clone, Copy, PartialEq)]
enum TempKind {
    Brake,
    Tire,
    Wear,
    Pressure,
    Engine,
}
#[derive(Component)]
struct TempFill(TempKind, usize); // (druh, index kola; Engine kolo ignoruje)
#[derive(Component)]
struct TempLabel(TempKind, usize); // číslo pod barem
// Aktivní roh car-ikony řádku (Fáze 3) — obarvuje se gradientem dle teploty pneu daného kola.
#[derive(Component)]
struct IconDot(usize);
// Palivový bar (Fáze 2b→3): fill (šířka % nádrže) + text (L + %). Krmí FuelState.
#[derive(Component)]
struct FuelFill;
#[derive(Component)]
struct FuelLabel;

// Gradient barva dle podílu: modrá (studená 0.0) → zelená (optimum 0.55) → červená (horká 1.0).
// Stejná paleta jako blueprint redesign (accent-blue / green / red).
fn grad_color(f: f32) -> Color {
    let f = f.clamp(0.0, 1.0);
    let (ap, a, bp, b) = if f <= 0.55 {
        (0.0f32, [89.0, 128.0, 166.0], 0.55f32, [92.0, 168.0, 110.0])
    } else {
        (0.55f32, [92.0, 168.0, 110.0], 1.0f32, [199.0, 84.0, 66.0])
    };
    let lf = (f - ap) / (bp - ap);
    let ch = |i: usize| (a[i] + (b[i] - a[i]) * lf) / 255.0;
    Color::srgb(ch(0), ch(1), ch(2))
}

// Podíl v rozsahu (0..1) — rozsahy z redesignu (display, ne fyzikální stropy).
fn temp_frac(kind: TempKind, v: f32) -> f32 {
    let (lo, hi) = match kind {
        TempKind::Brake => (80.0, 650.0),
        TempKind::Tire => (40.0, 115.0),
        TempKind::Wear => (0.0, 100.0),
        TempKind::Pressure => (1.8, 2.8), // bar — modrá podhuštěná, zelená ideál, červená přehuštěná
        TempKind::Engine => (60.0, 115.0),
    };
    ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
}

// Hodnota (pro frac) + text (pro label) daného druhu/kola.
fn temp_read(sb: &SoftBody, engine_c: f32, loadout: &Loadout, ambient: f32, kind: TempKind, wheel: usize) -> (f32, String) {
    match kind {
        TempKind::Engine => (engine_c, format!("{engine_c:.0}°C")),
        _ => match sb.wheels.get(wheel) {
            Some(w) => match kind {
                TempKind::Brake => (w.thermal.brake_c, format!("{:.0}°C", w.thermal.brake_c)),
                TempKind::Tire => (w.thermal.tire_surf_c, format!("{:.0}°C", w.thermal.tire_surf_c)),
                TempKind::Pressure => {
                    let p = crate::thermal::tire_pressure(w.thermal.tire_core_c, loadout.compound.pressure, ambient);
                    (p, format!("{p:.1}"))
                }
                _ => {
                    let pct = w.thermal.wear * 100.0;
                    (pct, if w.thermal.burst { "PRASK".into() } else { format!("{pct:.0}%") })
                }
            },
            None => (0.0, "—".into()),
        },
    }
}

fn spawn_hud(mut commands: Commands, fonts: Res<UiFonts>, map: Res<SelectedMap>) {
    // ── top-right: režim + mapa ──
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(20.0),
                right: Val::Px(24.0),
                padding: UiRect::axes(Val::Px(16.0), Val::Px(7.0)),
                column_gap: Val::Px(10.0),
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor(DIVIDER),
            StateScoped(AppState::Game),
        ))
        .with_children(|p| {
            p.spawn(txt("VOLNÁ JÍZDA", &fonts.body, 11.0, ACCENT_300));
            p.spawn((Node { width: Val::Px(1.0), height: Val::Px(12.0), ..default() }, BackgroundColor(DIVIDER)));
            p.spawn(txt(map.0.label().to_uppercase(), &fonts.heading, 15.0, TEXT));
        });

    // ── top-left: hint chipy ──
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(20.0),
                left: Val::Px(24.0),
                column_gap: Val::Px(8.0),
                ..default()
            },
            StateScoped(AppState::Game),
        ))
        .with_children(|p| {
            for chip in ["G — TEPLOTY", "T — TELEMETRIE", "ESC — PAUZA"] {
                p.spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                    BorderColor(DIVIDER),
                ))
                .with_children(|c| {
                    c.spawn(txt(chip, &fonts.body, 11.0, Color::srgba(0.949, 0.949, 0.953, 0.8)));
                });
            }
        });

    // ── FPS counter (top-left, pod chipy) — stejný chip styl jako hinty ──
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(52.0),
                left: Val::Px(24.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor(DIVIDER),
            StateScoped(AppState::Game),
        ))
        .with_children(|p| {
            p.spawn((txt("— FPS", &fonts.mono, 11.0, Color::srgba(0.949, 0.949, 0.953, 0.8)), FpsText));
        });

    // ── dash panel (pravý dolní roh) ──
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(32.0),
                right: Val::Px(28.0),
                width: Val::Px(640.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::new(Val::Px(14.0), Val::Px(14.0), Val::Px(10.0), Val::Px(12.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor(DIVIDER),
            StateScoped(AppState::Game),
        ))
        .with_children(|dash| {
            // shift LED na „rámečku": 4 vlevo, 4 vpravo
            dash.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::axes(Val::Px(22.0), Val::Px(0.0)),
                margin: UiRect::bottom(Val::Px(8.0)),
                ..default()
            })
            .with_children(|row| {
                for half in 0..2 {
                    row.spawn(Node { column_gap: Val::Px(10.0), ..default() }).with_children(|side| {
                        for i in 0..LEDS / 2 {
                            side.spawn((
                                Node { width: Val::Px(10.0), height: Val::Px(10.0), ..default() },
                                BackgroundColor(UNLIT),
                                BorderRadius::MAX,
                                ShiftLed(half * LEDS / 2 + i),
                            ));
                        }
                    });
                }
            });

            // LCD blok
            dash.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::new(Val::Px(14.0), Val::Px(14.0), Val::Px(14.0), Val::Px(10.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(LCD_BG),
                BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.15)),
            ))
            .with_children(|lcd| {
                // RPM bar — 32 segmentů
                lcd.spawn(Node {
                    height: Val::Px(12.0),
                    column_gap: Val::Px(3.0),
                    margin: UiRect::bottom(Val::Px(10.0)),
                    ..default()
                })
                .with_children(|bar| {
                    for i in 0..SEGMENTS {
                        bar.spawn((Node { flex_grow: 1.0, ..default() }, BackgroundColor(UNLIT), RpmSeg(i)));
                    }
                });

                // střed: RPM + rychlost | kvalt
                lcd.spawn(Node { justify_content: JustifyContent::Center, column_gap: Val::Px(24.0), ..default() })
                    .with_children(|mid| {
                        mid.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|c| {
                            c.spawn((txt("0", &fonts.mono, 34.0, TEXT), RpmText));
                            c.spawn(txt("ot/min", &fonts.body, 10.0, Color::srgba(0.949, 0.949, 0.953, 0.45)));
                            c.spawn((
                                txt("0", &fonts.mono, 76.0, TEXT),
                                Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                                SpeedText,
                            ));
                            c.spawn(txt("km/h", &fonts.body, 13.0, Color::srgba(0.949, 0.949, 0.953, 0.6)));
                            c.spawn((
                                txt("0–100: —", &fonts.mono, 12.0, ACCENT_300),
                                Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                                AccelText,
                            ));
                        });
                        mid.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        })
                        .with_children(|g| {
                            g.spawn(txt("KVALT", &fonts.body, 10.0, Color::srgba(0.949, 0.949, 0.953, 0.5)));
                            g.spawn((txt("N", &fonts.mono, 46.0, TEXT), GearText));
                        });
                    });

                // stavový řádek (SPOJKA / CHCÍPLO) — prázdný, když se nic neděje
                lcd.spawn(Node { justify_content: JustifyContent::Center, ..default() }).with_children(|s| {
                    s.spawn((txt("", &fonts.body, 11.0, ACCENT_300), StatusText));
                });
            });
        });

    // ── telemetrie overlay (levý dolní roh, toggle T) ──
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(32.0),
                left: Val::Px(28.0),
                width: Val::Px(372.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(16.0), Val::Px(14.0)),
                border: UiRect::all(Val::Px(1.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor(DIVIDER),
            TelemetryPanel,
            StateScoped(AppState::Game),
        ))
        .with_children(|panel| {
            // hlavička
            panel
                .spawn(Node {
                    align_items: AlignItems::Baseline,
                    column_gap: Val::Px(8.0),
                    margin: UiRect::bottom(Val::Px(10.0)),
                    ..default()
                })
                .with_children(|h| {
                    h.spawn(txt("TELEMETRIE / TELEMETRY", &fonts.heading, 13.0, ACCENT_300));
                    h.spawn((
                        txt("", &fonts.body, 11.0, ACCENT_300),
                        Node { margin: UiRect::left(Val::Auto), ..default() },
                        RecText,
                    ));
                });

            // tabulka: hlavička + 4 řádky kol
            let cols = ["Fz N", "skluz", "áz °", "F_long", "F_lat"];
            panel
                .spawn((
                    Node {
                        padding: UiRect::bottom(Val::Px(4.0)),
                        border: UiRect::bottom(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.2)),
                ))
                .with_children(|head| {
                    head.spawn((Node { width: Val::Px(40.0), ..default() },)).with_children(|c| {
                        c.spawn(txt("KOLO", &fonts.body, 10.0, Color::srgba(0.949, 0.949, 0.953, 0.5)));
                    });
                    for label in cols {
                        head.spawn(Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), justify_content: JustifyContent::FlexEnd, ..default() })
                            .with_children(|c| {
                                c.spawn(txt(label, &fonts.body, 10.0, Color::srgba(0.949, 0.949, 0.953, 0.5)));
                            });
                    }
                });
            for row in 0..4 {
                panel
                    .spawn(Node { padding: UiRect::axes(Val::Px(0.0), Val::Px(2.5)), ..default() })
                    .with_children(|r| {
                        r.spawn(Node { width: Val::Px(40.0), ..default() }).with_children(|c| {
                            c.spawn((txt("—", &fonts.heading, 12.0, TEXT), TelemName(row)));
                        });
                        for col in 0..cols.len() {
                            r.spawn(Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), justify_content: JustifyContent::FlexEnd, ..default() })
                                .with_children(|c| {
                                    c.spawn((txt("—", &fonts.mono, 12.0, TEXT), TelemCell(row, col)));
                                });
                        }
                    });
            }
            panel.spawn((
                txt("áz = slip angle předních kol · Pacejka b/c/e", &fonts.body, 11.0, Color::srgba(0.949, 0.949, 0.953, 0.5)),
                Node { margin: UiRect::axes(Val::Px(0.0), Val::Px(8.0)), ..default() },
            ));

            // spodní grid 3×2: Fz předek / áz / pohon / diff / spojka / záznam
            panel
                .spawn((
                    Node {
                        flex_wrap: FlexWrap::Wrap,
                        padding: UiRect::top(Val::Px(10.0)),
                        border: UiRect::top(Val::Px(1.0)),
                        row_gap: Val::Px(8.0),
                        ..default()
                    },
                    BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.2)),
                ))
                .with_children(|grid| {
                    let labels = ["FZ PŘEDEK", "ÁZ °", "POHON", "DIFF", "SPOJKA", "ZÁZNAM"];
                    for (i, label) in labels.iter().enumerate() {
                        grid.spawn(Node {
                            width: Val::Percent(33.3),
                            flex_direction: FlexDirection::Column,
                            ..default()
                        })
                        .with_children(|cell| {
                            cell.spawn(txt(*label, &fonts.body, 8.5, Color::srgba(0.949, 0.949, 0.953, 0.45)));
                            cell.spawn((txt("—", &fonts.mono, 15.0, TEXT), TelemStat(i)));
                        });
                    }
                });
        });

    // ── pauza overlay (Esc) ──
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.11, 0.11, 0.12, 0.62)),
            PauseOverlay,
            StateScoped(AppState::Game),
            // musí ležet nad HUDem
            GlobalZIndex(10),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: Val::Px(400.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::new(Val::Px(28.0), Val::Px(28.0), Val::Px(28.0), Val::Px(24.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(BG),
                    BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.25)),
                ))
                .with_children(|card| {
                    card.spawn(txt("HRA POZASTAVENA / PAUSED", &fonts.body, 11.0, ACCENT_300));
                    card.spawn((
                        txt("PAUZA", &fonts.heading, 28.0, TEXT),
                        Node { margin: UiRect::new(Val::Px(0.0), Val::Px(0.0), Val::Px(4.0), Val::Px(20.0)), ..default() },
                    ));
                    for (action, label, primary) in [
                        (PauseAction::Resume, "POKRAČOVAT — RESUME", true),
                        (PauseAction::Settings, "NASTAVENÍ — SETTINGS", false),
                        (PauseAction::Garage, "GARÁŽ — GARAGE", false),
                        (PauseAction::Title, "HLAVNÍ MENU — MAIN MENU", false),
                    ] {
                        let mut b = card.spawn((
                            Button,
                            action,
                            Node {
                                padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                                margin: UiRect::bottom(Val::Px(8.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                        ));
                        if primary {
                            b.insert((BackgroundColor(ACCENT), BorderColor(ACCENT)));
                        } else {
                            b.insert(BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.3)));
                        }
                        b.with_children(|t| {
                            t.spawn(txt(label, &fonts.heading, 16.0, if primary { BG } else { TEXT }));
                        });
                    }

                    card.spawn((
                        Node {
                            margin: UiRect::top(Val::Px(8.0)),
                            padding: UiRect::top(Val::Px(10.0)),
                            border: UiRect::top(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.2)),
                    ))
                    .with_children(|hint| {
                        hint.spawn(txt(
                            "Shift+R — restart auta · Backspace — clear",
                            &fonts.body,
                            12.0,
                            Color::srgba(0.949, 0.949, 0.953, 0.55),
                        ));
                    });
                });
        });
}

// ── systémy ──────────────────────────────────────────────────────────────────

fn esc_pause(keys: Res<ButtonInput<KeyCode>>, mut paused: ResMut<Paused>, settings_ui: Res<SettingsUi>) {
    // Když je otevřené nastavení, Esc ho zavře (settings_esc) — pauzu netoggluj.
    if settings_ui.open {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        paused.0 = !paused.0;
    }
}

// T přepíná viditelnost overlay (CSV záznam přepíná telemetry_log na stejnou klávesu;
// pod pauzou log neběží, tak panel netoggluj, ať se stavy nerozjedou).
fn toggle_telemetry_panel(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<crate::keybinds::KeyBindings>,
    paused: Res<Paused>,
    mut panel: Query<&mut Node, With<TelemetryPanel>>,
) {
    if paused.0 || !bindings.just_pressed(&keys, crate::keybinds::Action::Telemetry) {
        return;
    }
    for mut node in &mut panel {
        node.display = if node.display == Display::None { Display::Flex } else { Display::None };
    }
}

// Node kuličky vidět jen když je telemetry panel zobrazený. Sync každý frame (ne flip na T),
// ať to sedí i po respawnu auta, kdy nová kulička vznikne Visibility::Inherited (= vidět).
fn sync_node_viz(
    panel: Query<&Node, With<TelemetryPanel>>,
    mut nodes: Query<&mut Visibility, With<crate::vehicle::NodeViz>>,
) {
    let Ok(panel) = panel.get_single() else { return };
    let want = if panel.display == Display::None { Visibility::Hidden } else { Visibility::Visible };
    for mut v in &mut nodes {
        if *v != want {
            *v = want;
        }
    }
}

fn sync_pause_overlay(paused: Res<Paused>, mut overlay: Query<&mut Node, With<PauseOverlay>>) {
    for mut node in &mut overlay {
        node.display = if paused.0 { Display::Flex } else { Display::None };
    }
}

fn pause_buttons(
    interactions: Query<(&Interaction, &PauseAction), Changed<Interaction>>,
    mut paused: ResMut<Paused>,
    mut settings_ui: ResMut<SettingsUi>,
    mut next: ResMut<NextState<AppState>>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            PauseAction::Resume => paused.0 = false,
            PauseAction::Settings => settings_ui.open = true,
            PauseAction::Garage => next.set(AppState::Garage),
            PauseAction::Title => next.set(AppState::Title),
        }
    }
}

// FPS counter — vyhlazená hodnota z FrameTimeDiagnosticsPlugin.
fn update_fps(diag: Res<DiagnosticsStore>, mut q: Query<&mut Text, With<FpsText>>) {
    let Some(fps) = diag.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()) else { return };
    for mut t in &mut q {
        t.0 = format!("{fps:.0} FPS");
    }
}

// Dash: RPM bar + LED + čísla. Prahy jako v návrhu: posledních ~12 % baru
// a poslední 2 LED = „červená" zóna (accent-200), RPM číslo zčervená u limitu.
fn update_dash(
    loadout: Res<Loadout>,
    state: Res<DrivetrainState>,
    accel: Res<AccelTimer>,
    sb: Res<SoftBody>,
    mut segs: Query<(&RpmSeg, &mut BackgroundColor), (Without<ShiftLed>,)>,
    mut leds: Query<(&ShiftLed, &mut BackgroundColor), (With<ShiftLed>, Without<RpmSeg>)>,
    mut texts: ParamSet<(
        Query<(&mut Text, &mut TextColor), With<RpmText>>,
        Query<&mut Text, With<SpeedText>>,
        Query<&mut Text, With<GearText>>,
        Query<&mut Text, With<StatusText>>,
        Query<&mut Text, With<AccelText>>,
    )>,
) {
    let max = loadout.engine.max_rpm.max(1.0);
    let frac = (state.rpm / max).clamp(0.0, 1.0);

    let lit_segs = (frac * SEGMENTS as f32).round() as usize;
    for (RpmSeg(i), mut bg) in &mut segs {
        let red = *i >= SEGMENTS - 4;
        *bg = BackgroundColor(if *i < lit_segs { if red { ACCENT_200 } else { TEXT } } else { UNLIT });
    }
    for (ShiftLed(i), mut bg) in &mut leds {
        let lit = frac > (*i as f32 + 1.0) / (LEDS as f32 + 1.0);
        let red = *i >= LEDS - 2;
        *bg = BackgroundColor(if lit { if red { ACCENT_200 } else { ACCENT } } else { UNLIT });
    }

    if let Ok((mut text, mut color)) = texts.p0().get_single_mut() {
        text.0 = format!("{:.0}", state.rpm);
        *color = TextColor(if frac > 0.9 { ACCENT_200 } else { TEXT });
    }
    if let Ok(mut text) = texts.p1().get_single_mut() {
        text.0 = format!("{:.0}", state.speed_kmh.abs());
    }
    if let Ok(mut text) = texts.p2().get_single_mut() {
        text.0 = match state.gear {
            -1 => "R".into(),
            0 => "N".into(),
            g => g.to_string(),
        };
    }
    if let Ok(mut text) = texts.p3().get_single_mut() {
        let hot_brake = sb.wheels.iter().map(|w| w.thermal.brake_c).fold(0.0_f32, f32::max);
        let hot_tire = sb.wheels.iter().map(|w| w.thermal.tire_surf_c).fold(0.0_f32, f32::max);
        let cold_tire = sb.wheels.iter().map(|w| w.thermal.tire_surf_c).fold(f32::MAX, f32::min);
        text.0 = if state.stalled {
            "CHCÍPLO — nastartuj R".into()
        } else if sb.wheels.iter().any(|w| w.thermal.burst) {
            "PNEU PRASKLÁ".into()
        } else if state.clutch < 0.95 {
            "SPOJKA".into()
        } else if hot_brake > crate::thermal::FADE_START {
            "BRZDY HORKÉ".into()
        } else if state.engine_c > crate::thermal::ENGINE_OVERHEAT {
            "MOTOR HORKÝ".into()
        } else if hot_tire > crate::thermal::TIRE_HOT {
            "PNEU PŘEHŘÁTÉ".into()
        } else if cold_tire < crate::thermal::TIRE_COLD {
            "PNEU STUDENÉ".into()
        } else {
            String::new()
        };
    }
    if let Ok(mut text) = texts.p4().get_single_mut() {
        text.0 = match (accel.best, accel.launch) {
            (Some(b), _) => format!("0–100: {b:.1} s"),
            (None, Some(_)) => "0–100: …".into(),
            _ => "0–100: —".into(),
        };
    }
}

// Panel TEPLOTY (toggle G, vlevo dole, default open) dle redesignu: hlavička + sloupcové
// popisky, 4 řádky kol (label + 3 bary brzda/pneu/opot, každý s číslem pod barem) + motor
// (coolant) dole. Bar = kontejner + fill child (šířka % + gradient barva). Plní update_temps.
fn spawn_temps(mut commands: Commands, fonts: Res<UiFonts>) {
    // Car-ikona řádku (Fáze 3): top-down auto z bevy_ui nodů (bevy_ui nemá SVG). 2×2 rohy =
    // kola FL/FR/RL/RR; roh tohoto řádku (active) svítí gradientem dle teploty pneu, ostatní
    // ztlumené → řádek říká „které kolo" polohou i barvou místo textu FL/FR/RL/RR.
    let car_icon = |r: &mut ChildBuilder, active: usize| {
        r.spawn(Node { width: Val::Px(34.0), justify_content: JustifyContent::Center, ..default() })
            .with_children(|c| {
                c.spawn((
                    Node {
                        width: Val::Px(18.0),
                        height: Val::Px(24.0),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::SpaceBetween,
                        padding: UiRect::all(Val::Px(3.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor(DIVIDER),
                    BackgroundColor(LCD_BG),
                    BorderRadius::all(Val::Px(3.0)),
                ))
                .with_children(|body| {
                    for row in 0..2 {
                        body.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() })
                            .with_children(|rw| {
                                for col in 0..2 {
                                    let j = row * 2 + col;
                                    let dot = Node { width: Val::Px(5.0), height: Val::Px(5.0), ..default() };
                                    if j == active {
                                        rw.spawn((dot, BackgroundColor(grad_color(0.0)), BorderRadius::MAX, IconDot(active)));
                                    } else {
                                        rw.spawn((dot, BackgroundColor(Color::srgba(0.6, 0.6, 0.6, 0.25)), BorderRadius::MAX));
                                    }
                                }
                            });
                    }
                });
            });
    };
    // jedna buňka baru: sloupec [bar][číslo pod ním], zabírá stejný podíl šířky
    let bar_cell = |r: &mut ChildBuilder, fonts: &UiFonts, kind: TempKind, wheel: usize, align: JustifyContent| {
        r.spawn(Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() })
            .with_children(|col| {
                col.spawn((Node { height: Val::Px(9.0), border: UiRect::all(Val::Px(1.0)), ..default() }, BackgroundColor(LCD_BG), BorderColor(DIVIDER)))
                    .with_children(|b| {
                        b.spawn((Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(grad_color(0.0)), TempFill(kind, wheel)));
                    });
                col.spawn(Node { justify_content: align, ..default() }).with_children(|t| {
                    t.spawn((txt("—", &fonts.mono, 10.0, TEXT_MUTED), TempLabel(kind, wheel)));
                });
            });
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(24.0),
                bottom: Val::Px(32.0),
                width: Val::Px(460.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(16.0), Val::Px(14.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor(DIVIDER),
            TempsPanel,
            StateScoped(AppState::Game),
        ))
        .with_children(|p| {
            // hlavička: název + zarovnaná legenda
            p.spawn(Node { align_items: AlignItems::Baseline, column_gap: Val::Px(8.0), margin: UiRect::bottom(Val::Px(12.0)), ..default() })
                .with_children(|h| {
                    h.spawn(txt("TEPLOTY / TEMPS", &fonts.heading, 13.0, ACCENT_300));
                    h.spawn((txt("brzda · pneu · opotřebení", &fonts.body, 10.0, TEXT_MUTED), Node { margin: UiRect::left(Val::Auto), ..default() }));
                });
            // sloupcové popisky
            p.spawn(Node { column_gap: Val::Px(10.0), margin: UiRect::bottom(Val::Px(6.0)), ..default() })
                .with_children(|hr| {
                    hr.spawn((Node { width: Val::Px(34.0), ..default() },));
                    for lbl in ["BRZDA", "PNEU", "OPOT.", "TLAK"] {
                        hr.spawn(Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), justify_content: JustifyContent::Center, ..default() })
                            .with_children(|c| {
                                c.spawn(txt(lbl, &fonts.body, 9.0, TEXT_MUTED));
                            });
                    }
                });
            // řádky kol
            for w in 0..4 {
                p.spawn(Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, margin: UiRect::bottom(Val::Px(8.0)), ..default() })
                    .with_children(|r| {
                        car_icon(r, w);
                        bar_cell(r, &fonts, TempKind::Brake, w, JustifyContent::Center);
                        bar_cell(r, &fonts, TempKind::Tire, w, JustifyContent::Center);
                        bar_cell(r, &fonts, TempKind::Wear, w, JustifyContent::Center);
                        bar_cell(r, &fonts, TempKind::Pressure, w, JustifyContent::Center);
                    });
            }
            // motor / coolant (oddělovač nahoře)
            p.spawn((
                Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, border: UiRect::top(Val::Px(1.0)), padding: UiRect::top(Val::Px(10.0)), margin: UiRect::top(Val::Px(2.0)), ..default() },
                BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.2)),
            ))
            .with_children(|r| {
                r.spawn((Node { width: Val::Px(34.0), ..default() },)).with_children(|c| {
                    c.spawn(txt("MTR", &fonts.mono, 11.0, TEXT));
                });
                bar_cell(r, &fonts, TempKind::Engine, 0, JustifyContent::FlexEnd);
            });
            // PALIVO: bar (šířka = % nádrže, barva varuje při docházení) + text (L + %). Fáze 2b.
            p.spawn(Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, margin: UiRect::top(Val::Px(6.0)), ..default() })
                .with_children(|r| {
                    r.spawn((Node { width: Val::Px(34.0), ..default() },)).with_children(|c| {
                        c.spawn(txt("PAL", &fonts.mono, 11.0, TEXT));
                    });
                    r.spawn(Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() })
                        .with_children(|col| {
                            col.spawn((Node { height: Val::Px(9.0), border: UiRect::all(Val::Px(1.0)), ..default() }, BackgroundColor(LCD_BG), BorderColor(DIVIDER)))
                                .with_children(|b| {
                                    b.spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(grad_color(0.4)), FuelFill));
                                });
                            col.spawn(Node { justify_content: JustifyContent::FlexEnd, ..default() }).with_children(|t| {
                                t.spawn((txt("—", &fonts.mono, 10.0, TEXT_MUTED), FuelLabel));
                            });
                        });
                });
        });
}

// Toggle panelu TEPLOTY klávesou G (default open).
fn toggle_temps_panel(keys: Res<ButtonInput<KeyCode>>, mut panel: Query<&mut Node, With<TempsPanel>>) {
    if keys.just_pressed(KeyCode::KeyG) {
        for mut n in &mut panel {
            n.display = if n.display == Display::None { Display::Flex } else { Display::None };
        }
    }
}

// Naplní/obarví bary + čísla teplot každý snímek ze stavu kol + motoru.
fn update_temps(
    sb: Res<SoftBody>,
    state: Res<DrivetrainState>,
    loadout: Res<Loadout>,
    ambient: Res<crate::thermal::Ambient>,
    fuel: Res<crate::vehicle::FuelState>,
    mut fills: Query<(&TempFill, &mut Node, &mut BackgroundColor), (Without<IconDot>, Without<FuelFill>)>,
    mut dots: Query<(&IconDot, &mut BackgroundColor), (Without<TempFill>, Without<FuelFill>)>,
    mut labels: Query<(&TempLabel, &mut Text), Without<FuelLabel>>,
    mut fuel_fill: Query<(&mut Node, &mut BackgroundColor), (With<FuelFill>, Without<TempFill>, Without<IconDot>)>,
    mut fuel_label: Query<&mut Text, (With<FuelLabel>, Without<TempLabel>)>,
) {
    for (TempFill(kind, wheel), mut node, mut bg) in &mut fills {
        let (v, _) = temp_read(&sb, state.engine_c, &loadout, ambient.0, *kind, *wheel);
        let f = temp_frac(*kind, v);
        let pct = Val::Percent((f * 100.0).max(4.0)); // min 4 % ať je i studený bar vidět
        if node.width != pct {
            node.width = pct;
        }
        let color = grad_color(f);
        if bg.0 != color {
            *bg = BackgroundColor(color);
        }
    }
    for (TempLabel(kind, wheel), mut text) in &mut labels {
        let (_, s) = temp_read(&sb, state.engine_c, &loadout, ambient.0, *kind, *wheel);
        if text.0 != s {
            text.0 = s;
        }
    }
    // Car-ikona (Fáze 3): aktivní roh obarvi gradientem dle teploty pneu daného kola.
    for (IconDot(wheel), mut bg) in &mut dots {
        let f = match sb.wheels.get(*wheel) {
            Some(w) => temp_frac(TempKind::Tire, w.thermal.tire_surf_c),
            None => 0.0,
        };
        let color = grad_color(f);
        if bg.0 != color {
            *bg = BackgroundColor(color);
        }
    }
    // Palivo (Fáze 2b): bar = % nádrže, žlutá/zelená plná → červená při docházení.
    let frac = if fuel.capacity_kg > 0.0 { (fuel.kg / fuel.capacity_kg).clamp(0.0, 1.0) } else { 0.0 };
    let fuel_color = if frac < 0.15 { grad_color(1.0) } else { grad_color(0.35) };
    for (mut node, mut bg) in &mut fuel_fill {
        let pct = Val::Percent((frac * 100.0).max(2.0));
        if node.width != pct {
            node.width = pct;
        }
        if bg.0 != fuel_color {
            *bg = BackgroundColor(fuel_color);
        }
    }
    for mut text in &mut fuel_label {
        let s = format!("{:.1} L  ({:.0} %)", fuel.liters(), frac * 100.0);
        if text.0 != s {
            text.0 = s;
        }
    }
}

// Telemetrie overlay: per-kolo řádky (jméno FL/FR/RL/RR z rámu) + souhrnné buňky.
fn update_telemetry(
    sb: Res<SoftBody>,
    loadout: Res<Loadout>,
    state: Res<DrivetrainState>,
    tel: Res<crate::Telemetry>,
    panel: Query<&Node, With<TelemetryPanel>>,
    mut names: Query<(&TelemName, &mut Text), (Without<TelemCell>, Without<TelemStat>, Without<RecText>)>,
    mut cells: Query<(&TelemCell, &mut Text), (Without<TelemName>, Without<TelemStat>, Without<RecText>)>,
    mut stats: Query<(&TelemStat, &mut Text), (Without<TelemName>, Without<TelemCell>, Without<RecText>)>,
    mut rec: Query<&mut Text, (With<RecText>, Without<TelemName>, Without<TelemCell>, Without<TelemStat>)>,
) {
    // Skrytý panel nepřepočítávej.
    if panel.iter().all(|n| n.display == Display::None) {
        return;
    }

    // Jméno kola: přední/zadní z flagu, levá/pravá podle pozice vůči rámu.
    let side = |hub: usize| -> Option<bool> {
        let cf = sb.chassis.as_ref()?;
        let mean = |idx: &[usize]| idx.iter().map(|&i| sb.pos[i]).sum::<Vec3>() / idx.len() as f32;
        let right = mean(&cf.right) - mean(&cf.left);
        let center = (mean(&cf.right) + mean(&cf.left)) * 0.5;
        Some((sb.pos[hub] - center).dot(right) > 0.0)
    };
    let mut labels = [""; 4];
    for (i, w) in sb.wheels.iter().take(4).enumerate() {
        labels[i] = match (w.front, side(w.hub)) {
            (true, Some(false)) => "FL",
            (true, Some(true)) => "FR",
            (false, Some(false)) => "RL",
            (false, Some(true)) => "RR",
            (true, None) => "F?",
            (false, None) => "R?",
        };
    }
    for (TelemName(row), mut text) in &mut names {
        let s = if *row < sb.wheels.len() { labels[*row] } else { "—" };
        if text.0 != s {
            text.0 = s.to_string();
        }
    }

    // Teploty/opotřebení jsou teď ve vždy-viditelném panelu TEPLOTY (bary) — telemetrie
    // drží jen síly/skluz.
    for (TelemCell(row, col), mut text) in &mut cells {
        let s = match sb.wheels.get(*row) {
            Some(w) => match col {
                0 => format!("{:.0}", w.telem.fz),
                1 => format!("{:.3}", w.telem.slip_ratio),
                2 => format!("{:.1}", w.telem.slip_angle.to_degrees()),
                3 => format!("{:.0}", w.telem.f_long),
                _ => format!("{:.0}", w.telem.f_lat),
            },
            None => "—".into(),
        };
        if text.0 != s {
            text.0 = s;
        }
    }

    let front: Vec<_> = sb.wheels.iter().filter(|w| w.front).collect();
    let (fz, sa) = if front.is_empty() {
        ("—".to_string(), "—".to_string())
    } else {
        (
            format!("{:.0} N", front.iter().map(|w| w.telem.fz).sum::<f32>()),
            format!("{:.1}", front.iter().map(|w| w.telem.slip_angle.to_degrees().abs()).sum::<f32>() / front.len() as f32),
        )
    };
    let clutch = if state.stalled {
        "CHCÍPLO"
    } else if state.clutch < 0.95 {
        "ROZP"
    } else {
        "—"
    };
    let recording = tel.writer.is_some();
    let stat_vals = [
        fz,
        sa,
        loadout.layout.label().to_string(),
        if loadout.rear_diff.lock > 0.5 { "ZAMČ".into() } else { "OTEV".into() },
        clutch.to_string(),
        if recording { "●REC".into() } else { "—".into() },
    ];
    for (TelemStat(i), mut text) in &mut stats {
        if text.0 != stat_vals[*i] {
            text.0 = stat_vals[*i].clone();
        }
    }
    if let Ok(mut text) = rec.get_single_mut() {
        let s = if recording { "● REC → CSV" } else { "T spouští i CSV záznam" };
        if text.0 != s {
            text.0 = s.to_string();
        }
    }
}
