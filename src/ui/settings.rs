// Sdílené nastavení — overlay s taby (MAIN / SOUND / VIDEO). Otevírá se z title menu
// (NASTAVENÍ) i z pauzy (SETTINGS). Není state-scoped: spawne se jednou v Startup a leží
// skrytý nad vším (z-index 100), přepíná se přes SettingsUi resource. SOUND tab drží
// hlasitosti (AudioSettings). MAIN/VIDEO zatím placeholdery.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode};
use serde::{Deserialize, Serialize};

use crate::audio::AudioSettings;
use crate::ui::*;

const SETTINGS_PATH: &str = "player/settings.json";

// Vše persistované v jednom souboru (audio + video + fps). Ne přes serde na resources —
// jen ruční kopie polí, ať audio.rs nemusí znát serde.
#[derive(Serialize, Deserialize)]
struct PersistedSettings {
    master: f32,
    engine: f32,
    screech: f32,
    #[serde(default = "default_vol")]
    crash: f32,
    #[serde(default = "default_vol")]
    wind: f32,
    vsync: bool,
    fullscreen: bool,
    msaa: u8,
    fps_limit: u32,
}

// Default pro crash u starších settings.json bez tohoto pole.
fn default_vol() -> f32 {
    0.8
}

pub fn plugin(app: &mut App) {
    app.init_resource::<SettingsUi>()
        .init_resource::<VideoSettings>()
        .init_resource::<FpsCap>()
        .add_systems(Startup, (load_settings, spawn_settings))
        .add_systems(
            Update,
            (
                save_settings,
                sync_settings_visibility,
                settings_tab_buttons,
                sync_settings_tabs,
                settings_close_button,
                settings_esc,
                vol_buttons,
                update_vol_labels,
                video_buttons,
                update_video_labels,
                apply_video,
                fps_slider_drag,
                update_fps_widgets,
                limit_fps,
            ),
        );
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Main,
    Sound,
    Video,
}

#[derive(Resource)]
pub struct SettingsUi {
    pub open: bool,
    pub tab: SettingsTab,
}

impl Default for SettingsUi {
    fn default() -> Self {
        Self { open: false, tab: SettingsTab::Main }
    }
}

#[derive(Component)]
struct SettingsOverlay;
#[derive(Component, Clone, Copy)]
struct SettingsTabBtn(SettingsTab);
#[derive(Component, Clone, Copy)]
struct SettingsPanel(SettingsTab);
#[derive(Component)]
struct SettingsClose;

// Hlasitost: +/− tlačítko a % popisek pro daný cíl.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VolTarget {
    Master,
    Engine,
    Screech,
    Crash,
    Wind,
}
#[derive(Component, Clone, Copy)]
struct VolAdjust {
    target: VolTarget,
    delta: f32,
}
#[derive(Component, Clone, Copy)]
struct VolLabel {
    target: VolTarget,
}

// Grafika — cyklovací tlačítka. VideoSettings drží stav, apply_video ho zrcadlí do
// Window (present_mode, mode) a Msaa komponenty na kameře.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VideoSetting {
    Vsync,
    Fullscreen,
    Msaa,
}
#[derive(Component, Clone, Copy)]
struct VideoBtn(VideoSetting);
#[derive(Component, Clone, Copy)]
struct VideoLabel(VideoSetting);

#[derive(Resource)]
struct VideoSettings {
    vsync: bool,
    fullscreen: bool,
    msaa: u8, // 0=vyp, 2, 4, 8
}

impl Default for VideoSettings {
    fn default() -> Self {
        // Odpovídá výchozímu oknu (vsync zap, okno) a Bevy default MSAA 4×.
        Self { vsync: true, fullscreen: false, msaa: 4 }
    }
}

// FPS limit — táhlo. Track pevné šířky, drag mapuje kurzor na hodnotu.
const SLIDER_W: f32 = 220.0;
const FPS_MIN: u32 = 30;
const FPS_MAX: u32 = 300; // nad max (frac=1) → bez limitu (0)
const FPS_STEP: u32 = 10;

#[derive(Component)]
struct FpsSlider;
#[derive(Component)]
struct FpsFill;
#[derive(Component)]
struct FpsValLabel;

// 0 = bez limitu. Výchozí bez limitu (vsync stejně stropuje na obnovovací frekvenci).
#[derive(Resource, Default)]
struct FpsCap {
    limit: u32,
}

fn spawn_settings(mut commands: Commands, fonts: Res<UiFonts>) {
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
            BackgroundColor(Color::srgba(0.07, 0.07, 0.08, 0.75)),
            SettingsOverlay,
            GlobalZIndex(100), // nad title BG i pauzou
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: Val::Px(560.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(28.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(BG),
                    BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.25)),
                ))
                .with_children(|card| {
                    // ── hlavička: nadpis + zavřít ──
                    card.spawn(Node {
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        margin: UiRect::bottom(Val::Px(16.0)),
                        ..default()
                    })
                    .with_children(|h| {
                        h.spawn(txt("NASTAVENÍ / SETTINGS", &fonts.heading, 22.0, TEXT));
                        h.spawn((
                            Button,
                            SettingsClose,
                            Node {
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.3)),
                        ))
                        .with_children(|t| {
                            t.spawn(txt("ZAVŘÍT — ESC", &fonts.body, 12.0, TEXT));
                        });
                    });

                    // ── taby ──
                    card.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(4.0),
                        border: UiRect::bottom(Val::Px(1.0)),
                        margin: UiRect::bottom(Val::Px(18.0)),
                        ..default()
                    })
                    .with_children(|tabs| {
                        for (tab, name) in [
                            (SettingsTab::Main, "OBECNÉ — MAIN"),
                            (SettingsTab::Sound, "ZVUK — SOUND"),
                            (SettingsTab::Video, "OBRAZ — VIDEO"),
                        ] {
                            tabs.spawn((
                                Button,
                                SettingsTabBtn(tab),
                                Node {
                                    padding: UiRect::axes(Val::Px(16.0), Val::Px(9.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::NONE),
                            ))
                            .with_children(|t| {
                                t.spawn(txt(name, &fonts.heading, 14.0, TEXT));
                            });
                        }
                    });

                    // ── obsah tabů (jen aktivní má Display::Flex) ──
                    // OBECNÉ — přehled ovládání (dva sloupce). Klávesy dle kódu (vehicle/main).
                    card.spawn((
                        Node {
                            min_height: Val::Px(120.0),
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(32.0),
                            display: Display::None,
                            ..default()
                        },
                        SettingsPanel(SettingsTab::Main),
                    ))
                    .with_children(|panel| {
                        let cols: [&[(&str, &str)]; 2] = [
                            &[
                                ("W", "Plyn"),
                                ("S", "Brzda / zpátečka"),
                                ("A / D", "Řízení"),
                                ("Shift", "Spojka"),
                                ("E", "Řadit nahoru"),
                                ("Q", "Řadit dolů"),
                                ("R", "Nastartovat"),
                            ],
                            &[
                                ("L", "Uzamknout diff"),
                                ("K", "Rozvod pohonu"),
                                ("C", "Kamera"),
                                ("T", "Telemetrie"),
                                ("Esc", "Pauza"),
                                ("Backspace", "Smazat auta"),
                                ("1–5", "Spawn buildů"),
                            ],
                        ];
                        for rows in cols {
                            panel
                                .spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px(7.0),
                                    ..default()
                                })
                                .with_children(|col| {
                                    for (key, desc) in rows {
                                        col.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(10.0), ..default() })
                                            .with_children(|row| {
                                                row.spawn((
                                                    Node {
                                                        width: Val::Px(84.0),
                                                        padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                                                        justify_content: JustifyContent::Center,
                                                        border: UiRect::all(Val::Px(1.0)),
                                                        ..default()
                                                    },
                                                    BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.25)),
                                                    BackgroundColor(Color::srgba(0.949, 0.949, 0.953, 0.05)),
                                                ))
                                                .with_children(|k| {
                                                    k.spawn(txt(*key, &fonts.mono, 13.0, ACCENT_300));
                                                });
                                                row.spawn(txt(*desc, &fonts.body, 13.0, TEXT));
                                            });
                                    }
                                });
                        }
                    });

                    // OBRAZ — runtime grafická nastavení (klik = cyklus hodnoty)
                    card.spawn((
                        Node {
                            min_height: Val::Px(120.0),
                            flex_direction: FlexDirection::Column,
                            display: Display::None,
                            ..default()
                        },
                        SettingsPanel(SettingsTab::Video),
                    ))
                    .with_children(|panel| {
                        for (setting, name) in [
                            (VideoSetting::Vsync, "VSYNC"),
                            (VideoSetting::Fullscreen, "CELÁ OBRAZOVKA — FULLSCREEN"),
                            (VideoSetting::Msaa, "VYHLAZENÍ — MSAA"),
                        ] {
                            panel
                                .spawn(Node {
                                    flex_direction: FlexDirection::Row,
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::SpaceBetween,
                                    margin: UiRect::vertical(Val::Px(6.0)),
                                    ..default()
                                })
                                .with_children(|row| {
                                    row.spawn(txt(name, &fonts.body, 14.0, TEXT));
                                    row.spawn((
                                        Button,
                                        VideoBtn(setting),
                                        Node {
                                            width: Val::Px(96.0),
                                            padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                            justify_content: JustifyContent::Center,
                                            border: UiRect::all(Val::Px(1.0)),
                                            ..default()
                                        },
                                        BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.3)),
                                    ))
                                    .with_children(|b| {
                                        b.spawn((txt("...", &fonts.mono, 13.0, ACCENT_300), VideoLabel(setting)));
                                    });
                                });
                        }

                        // FPS limit — táhlo (drag) jako v Minecraftu. Track pevné šířky, fill
                        // roste s hodnotou, popisek vpravo. Nad max = bez limitu.
                        panel
                            .spawn(Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::SpaceBetween,
                                margin: UiRect::vertical(Val::Px(6.0)),
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn(txt("FPS LIMIT", &fonts.body, 14.0, TEXT));
                                row.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(12.0), ..default() })
                                    .with_children(|ctrl| {
                                        // track (tažitelné, pevná šířka SLIDER_W). RelativeCursorPosition
                                        // dá normalizovanou polohu kurzoru v nodu (řeší scale factor).
                                        ctrl.spawn((
                                            FpsSlider,
                                            RelativeCursorPosition::default(),
                                            Node {
                                                width: Val::Px(SLIDER_W),
                                                height: Val::Px(14.0),
                                                border: UiRect::all(Val::Px(1.0)),
                                                ..default()
                                            },
                                            BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.3)),
                                            BackgroundColor(Color::srgba(0.949, 0.949, 0.953, 0.06)),
                                        ))
                                        .with_children(|track| {
                                            track.spawn((
                                                Node { width: Val::Percent(50.0), height: Val::Percent(100.0), ..default() },
                                                BackgroundColor(ACCENT),
                                                FpsFill,
                                            ));
                                        });
                                        ctrl.spawn((txt("...", &fonts.mono, 13.0, ACCENT_300), FpsValLabel, Node { width: Val::Px(56.0), ..default() }));
                                    });
                            });
                    });

                    // ZVUK — hlasitosti
                    card.spawn((
                        Node {
                            min_height: Val::Px(120.0),
                            flex_direction: FlexDirection::Column,
                            display: Display::None,
                            ..default()
                        },
                        SettingsPanel(SettingsTab::Sound),
                    ))
                    .with_children(|panel| {
                        for (target, name) in [
                            (VolTarget::Master, "MASTER"),
                            (VolTarget::Engine, "MOTOR — ENGINE"),
                            (VolTarget::Screech, "SKŘÍPOT — TIRES"),
                            (VolTarget::Crash, "NÁRAZ — CRASH"),
                            (VolTarget::Wind, "VÍTR — WIND"),
                        ] {
                            panel
                                .spawn(Node {
                                    flex_direction: FlexDirection::Row,
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::SpaceBetween,
                                    margin: UiRect::vertical(Val::Px(6.0)),
                                    ..default()
                                })
                                .with_children(|row| {
                                    row.spawn(txt(name, &fonts.body, 14.0, TEXT));
                                    row.spawn(Node {
                                        flex_direction: FlexDirection::Row,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    })
                                    .with_children(|ctrl| {
                                        for (delta, sym) in [(-0.1f32, "−"), (0.1, "+")] {
                                            // value patří mezi − a +
                                            if delta > 0.0 {
                                                ctrl.spawn((
                                                    txt("80%", &fonts.mono, 14.0, TEXT),
                                                    VolLabel { target },
                                                    Node { width: Val::Px(52.0), margin: UiRect::horizontal(Val::Px(10.0)), ..default() },
                                                ));
                                            }
                                            ctrl.spawn((
                                                Button,
                                                VolAdjust { target, delta },
                                                Node {
                                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                                    border: UiRect::all(Val::Px(1.0)),
                                                    ..default()
                                                },
                                                BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.3)),
                                            ))
                                            .with_children(|t| {
                                                t.spawn(txt(sym, &fonts.heading, 18.0, TEXT));
                                            });
                                        }
                                    });
                                });
                        }
                    });
                });
        });
}

// ── systémy ──

fn sync_settings_visibility(ui: Res<SettingsUi>, mut overlay: Query<&mut Node, With<SettingsOverlay>>) {
    for mut node in &mut overlay {
        node.display = if ui.open { Display::Flex } else { Display::None };
    }
}

fn settings_tab_buttons(
    interactions: Query<(&Interaction, &SettingsTabBtn), Changed<Interaction>>,
    mut ui: ResMut<SettingsUi>,
) {
    for (interaction, btn) in &interactions {
        if *interaction == Interaction::Pressed {
            ui.tab = btn.0;
        }
    }
}

// Zvýrazni aktivní tab + přepni viditelnost panelů. Levné, běží každý snímek.
fn sync_settings_tabs(
    ui: Res<SettingsUi>,
    mut tabs: Query<(&SettingsTabBtn, &mut BackgroundColor)>,
    mut panels: Query<(&SettingsPanel, &mut Node)>,
) {
    for (btn, mut bg) in &mut tabs {
        *bg = BackgroundColor(if btn.0 == ui.tab { ACCENT } else { Color::NONE });
    }
    for (panel, mut node) in &mut panels {
        node.display = if panel.0 == ui.tab { Display::Flex } else { Display::None };
    }
}

fn settings_close_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<SettingsClose>)>,
    mut ui: ResMut<SettingsUi>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            ui.open = false;
        }
    }
}

// Esc zavře nastavení (má přednost před pauzou — esc_pause se při open vypne).
fn settings_esc(keys: Res<ButtonInput<KeyCode>>, mut ui: ResMut<SettingsUi>) {
    if ui.open && keys.just_pressed(KeyCode::Escape) {
        ui.open = false;
    }
}

// +/− tlačítka hlasitosti → uprav AudioSettings (0..1, krok 0.1).
fn vol_buttons(interactions: Query<(&Interaction, &VolAdjust), Changed<Interaction>>, mut settings: ResMut<AudioSettings>) {
    for (interaction, adj) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let v = match adj.target {
            VolTarget::Master => &mut settings.master,
            VolTarget::Engine => &mut settings.engine,
            VolTarget::Screech => &mut settings.screech,
            VolTarget::Crash => &mut settings.crash,
            VolTarget::Wind => &mut settings.wind,
        };
        *v = (*v + adj.delta).clamp(0.0, 1.0);
    }
}

fn update_vol_labels(settings: Res<AudioSettings>, mut labels: Query<(&mut Text, &VolLabel)>) {
    for (mut text, lbl) in &mut labels {
        let v = match lbl.target {
            VolTarget::Master => settings.master,
            VolTarget::Engine => settings.engine,
            VolTarget::Screech => settings.screech,
            VolTarget::Crash => settings.crash,
            VolTarget::Wind => settings.wind,
        };
        text.0 = format!("{}%", (v * 100.0).round() as i32);
    }
}

// Klik na grafické tlačítko → cyklus hodnoty ve VideoSettings.
fn video_buttons(interactions: Query<(&Interaction, &VideoBtn), Changed<Interaction>>, mut vs: ResMut<VideoSettings>) {
    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match btn.0 {
            VideoSetting::Vsync => vs.vsync = !vs.vsync,
            VideoSetting::Fullscreen => vs.fullscreen = !vs.fullscreen,
            VideoSetting::Msaa => vs.msaa = match vs.msaa { 0 => 2, 2 => 4, 4 => 8, _ => 0 },
        }
    }
}

fn update_video_labels(vs: Res<VideoSettings>, mut labels: Query<(&mut Text, &VideoLabel)>) {
    for (mut text, lbl) in &mut labels {
        text.0 = match lbl.0 {
            VideoSetting::Vsync => if vs.vsync { "ZAP" } else { "VYP" }.to_string(),
            VideoSetting::Fullscreen => if vs.fullscreen { "ZAP" } else { "VYP" }.to_string(),
            VideoSetting::Msaa => if vs.msaa == 0 { "VYP".to_string() } else { format!("{}×", vs.msaa) },
        };
    }
}

// Zrcadli VideoSettings do skutečného okna a MSAA (jen při změně).
fn apply_video(
    vs: Res<VideoSettings>,
    mut win: Query<&mut Window, With<PrimaryWindow>>,
    mut msaa: Query<&mut Msaa>,
) {
    if !vs.is_changed() {
        return;
    }
    if let Ok(mut w) = win.get_single_mut() {
        w.present_mode = if vs.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
        w.mode = if vs.fullscreen { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed };
    }
    for mut m in &mut msaa {
        *m = match vs.msaa {
            0 => Msaa::Off,
            2 => Msaa::Sample2,
            8 => Msaa::Sample8,
            _ => Msaa::Sample4,
        };
    }
}

// FPS táhlo: drag přes RelativeCursorPosition (normalizovaná 0..1 poloha kurzoru v tracku,
// řeší scale factor). Drag začne stiskem nad trackem, pokračuje dokud držíš. Nad max = bez
// limitu. frac<0 (kurzor vlevo od tracku) při dragu = nejnižší FPS.
fn fps_slider_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    slider: Query<&RelativeCursorPosition, With<FpsSlider>>,
    mut cap: ResMut<FpsCap>,
    mut dragging: Local<bool>,
) {
    if mouse.just_released(MouseButton::Left) {
        *dragging = false;
    }
    let Ok(rel) = slider.get_single() else { return };
    if mouse.just_pressed(MouseButton::Left) && rel.mouse_over() {
        *dragging = true;
    }
    if !*dragging {
        return;
    }
    let Some(pos) = rel.normalized else { return };
    let frac = pos.x.clamp(0.0, 1.0);
    let raw = FPS_MIN as f32 + frac * (FPS_MAX - FPS_MIN) as f32;
    let stepped = ((raw / FPS_STEP as f32).round() as u32) * FPS_STEP;
    cap.limit = if stepped >= FPS_MAX { 0 } else { stepped.max(FPS_MIN) };
}

fn update_fps_widgets(
    cap: Res<FpsCap>,
    mut fill: Query<&mut Node, With<FpsFill>>,
    mut label: Query<&mut Text, With<FpsValLabel>>,
) {
    let frac = if cap.limit == 0 {
        1.0
    } else {
        ((cap.limit - FPS_MIN) as f32 / (FPS_MAX - FPS_MIN) as f32).clamp(0.0, 1.0)
    };
    for mut n in &mut fill {
        n.width = Val::Percent(frac * 100.0);
    }
    for mut t in &mut label {
        t.0 = if cap.limit == 0 { "MAX".to_string() } else { format!("{}", cap.limit) };
    }
}

// Prostý limiter: uspí zbytek do cílového frametime. Ne dokonale přesné (bez extra
// dependency), ale Minecraft-level stačí. limit=0 → bez uspání. ponytail: bevy_framepace
// kdyby chtěl přesnější/nižší jitter.
fn limit_fps(cap: Res<FpsCap>, mut last: Local<Option<Instant>>) {
    if cap.limit > 0 {
        let target = Duration::from_secs_f64(1.0 / cap.limit as f64);
        if let Some(prev) = *last {
            let elapsed = prev.elapsed();
            if elapsed < target {
                std::thread::sleep(target - elapsed);
            }
        }
    }
    *last = Some(Instant::now());
}

// Načti settings z disku do resources (Startup). Chybějící/vadný soubor = necháme defaulty.
fn load_settings(mut audio: ResMut<AudioSettings>, mut video: ResMut<VideoSettings>, mut fps: ResMut<FpsCap>) {
    let Ok(s) = std::fs::read_to_string(SETTINGS_PATH) else { return };
    let Ok(p) = serde_json::from_str::<PersistedSettings>(&s) else { return };
    audio.master = p.master;
    audio.engine = p.engine;
    audio.screech = p.screech;
    audio.crash = p.crash;
    audio.wind = p.wind;
    video.vsync = p.vsync;
    video.fullscreen = p.fullscreen;
    video.msaa = p.msaa;
    fps.limit = p.fps_limit;
}

// Zapiš settings při každé změně (is_changed je true jen ve snímku změny — žádné trvalé psaní).
// První snímek se zapíšou (loaded/default) → vznikne soubor. Levné.
fn save_settings(audio: Res<AudioSettings>, video: Res<VideoSettings>, fps: Res<FpsCap>) {
    if !(audio.is_changed() || video.is_changed() || fps.is_changed()) {
        return;
    }
    let p = PersistedSettings {
        master: audio.master,
        engine: audio.engine,
        screech: audio.screech,
        crash: audio.crash,
        wind: audio.wind,
        vsync: video.vsync,
        fullscreen: video.fullscreen,
        msaa: video.msaa,
        fps_limit: fps.limit,
    };
    let _ = std::fs::create_dir_all("player");
    if let Ok(json) = serde_json::to_string_pretty(&p) {
        let _ = std::fs::write(SETTINGS_PATH, json);
    }
}
