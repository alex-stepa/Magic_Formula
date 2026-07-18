// Herní HUD podle návrhu: rally dash (styl Sinco Tech — RPM bar se segmenty,
// shift LED, velká rychlost, kvalt) + telemetrie overlay na T + pauza na Esc.

use bevy::prelude::*;

use crate::solver::SoftBody;
use crate::ui::*;
use crate::vehicle::Drivetrain;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::Game), spawn_hud)
        .add_systems(OnExit(AppState::Game), |mut p: ResMut<Paused>| p.0 = false)
        .add_systems(
            Update,
            (esc_pause, toggle_telemetry_panel, sync_pause_overlay, pause_buttons, update_dash, update_telemetry)
                .run_if(in_state(AppState::Game)),
        );
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
#[derive(Component, Clone, Copy)]
enum PauseAction {
    Resume,
    Garage,
    Title,
}

const SEGMENTS: usize = 32; // RPM bar
const LEDS: usize = 8; // shift LED (posledních ~2 = „červená" zóna)
const UNLIT: Color = Color::srgba(0.949, 0.949, 0.953, 0.14);
const LCD_BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.35);
const PANEL_BG: Color = Color::srgba(0.114, 0.176, 0.239, 0.92); // accent-900 ~92 %

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
            for chip in ["T — TELEMETRIE", "ESC — PAUZA"] {
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

fn esc_pause(keys: Res<ButtonInput<KeyCode>>, mut paused: ResMut<Paused>) {
    if keys.just_pressed(KeyCode::Escape) {
        paused.0 = !paused.0;
    }
}

// T přepíná viditelnost overlay (CSV záznam přepíná telemetry_log na stejnou klávesu;
// pod pauzou log neběží, tak panel netoggluj, ať se stavy nerozjedou).
fn toggle_telemetry_panel(
    keys: Res<ButtonInput<KeyCode>>,
    paused: Res<Paused>,
    mut panel: Query<&mut Node, With<TelemetryPanel>>,
) {
    if paused.0 || !keys.just_pressed(KeyCode::KeyT) {
        return;
    }
    for mut node in &mut panel {
        node.display = if node.display == Display::None { Display::Flex } else { Display::None };
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
    mut next: ResMut<NextState<AppState>>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            PauseAction::Resume => paused.0 = false,
            PauseAction::Garage => next.set(AppState::Garage),
            PauseAction::Title => next.set(AppState::Title),
        }
    }
}

// Dash: RPM bar + LED + čísla. Prahy jako v návrhu: posledních ~12 % baru
// a poslední 2 LED = „červená" zóna (accent-200), RPM číslo zčervená u limitu.
fn update_dash(
    dt: Res<Drivetrain>,
    mut segs: Query<(&RpmSeg, &mut BackgroundColor), (Without<ShiftLed>,)>,
    mut leds: Query<(&ShiftLed, &mut BackgroundColor), (With<ShiftLed>, Without<RpmSeg>)>,
    mut texts: ParamSet<(
        Query<(&mut Text, &mut TextColor), With<RpmText>>,
        Query<&mut Text, With<SpeedText>>,
        Query<&mut Text, With<GearText>>,
        Query<&mut Text, With<StatusText>>,
    )>,
) {
    let max = dt.engine.max_rpm.max(1.0);
    let frac = (dt.rpm / max).clamp(0.0, 1.0);

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
        text.0 = format!("{:.0}", dt.rpm);
        *color = TextColor(if frac > 0.9 { ACCENT_200 } else { TEXT });
    }
    if let Ok(mut text) = texts.p1().get_single_mut() {
        text.0 = format!("{:.0}", dt.speed_kmh.abs());
    }
    if let Ok(mut text) = texts.p2().get_single_mut() {
        text.0 = match dt.gear {
            -1 => "R".into(),
            0 => "N".into(),
            g => g.to_string(),
        };
    }
    if let Ok(mut text) = texts.p3().get_single_mut() {
        text.0 = if dt.stalled {
            "CHCÍPLO — nastartuj R".into()
        } else if dt.clutch < 0.95 {
            "SPOJKA".into()
        } else {
            String::new()
        };
    }
}

// Telemetrie overlay: per-kolo řádky (jméno FL/FR/RL/RR z rámu) + souhrnné buňky.
fn update_telemetry(
    sb: Res<SoftBody>,
    dt: Res<Drivetrain>,
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
    let clutch = if dt.stalled {
        "CHCÍPLO"
    } else if dt.clutch < 0.95 {
        "ROZP"
    } else {
        "—"
    };
    let recording = tel.writer.is_some();
    let stat_vals = [
        fz,
        sa,
        dt.layout.label().to_string(),
        if dt.rear_diff.lock > 0.5 { "ZAMČ".into() } else { "OTEV".into() },
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
