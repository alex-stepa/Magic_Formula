// Title screen podle návrhu: logo + režimy (Volná jízda / Garáž / Rally / Nastavení)
// + map picker vpravo + přehled kláves v patičce.

use bevy::prelude::*;

use crate::ui::*;
use crate::world::{MapKind, Terrain};

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::Title), spawn_title).add_systems(
        Update,
        (title_buttons, map_cards, sync_map_selection).run_if(in_state(AppState::Title)),
    );
}

#[derive(Component, Clone, Copy)]
enum TitleAction {
    Drive,
    Garage,
    Settings,
}

// Karta mapy v pickeru; tag „vybráno" má vlastní marker (toggluje se Display).
#[derive(Component, Clone, Copy)]
struct MapCard(MapKind);
#[derive(Component, Clone, Copy)]
struct MapSelectedTag(MapKind);

fn spawn_title(mut commands: Commands, fonts: Res<UiFonts>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(BG),
            StateScoped(AppState::Title),
        ))
        .with_children(|page| {
            // ── hlavička ──
            page.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(40.0), Val::Px(22.0)),
                    column_gap: Val::Px(14.0),
                    align_items: AlignItems::Baseline,
                    border: UiRect::bottom(Val::Px(1.0)),
                    ..default()
                },
                BorderColor(DIVIDER),
            ))
            .with_children(|h| {
                h.spawn(txt("MAGIC FORMULA", &fonts.heading, 20.0, TEXT));
                h.spawn((
                    Node {
                        border: UiRect::all(Val::Px(1.0)),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                        ..default()
                    },
                    BorderColor(ACCENT_300),
                ))
                .with_children(|t| {
                    t.spawn(txt("v0.1 — dev build", &fonts.body, 12.0, ACCENT_300));
                });
                h.spawn((
                    txt("NODE-BEAM RALLY SIMULATOR", &fonts.body, 12.0, TEXT_MUTED),
                    Node { margin: UiRect::left(Val::Auto), ..default() },
                ));
            });

            // ── střed: nadpis + režimy | map picker ──
            page.spawn(Node {
                flex_grow: 1.0,
                padding: UiRect::axes(Val::Px(96.0), Val::Px(0.0)),
                column_gap: Val::Px(56.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|mid| {
                mid.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|l| {
                    l.spawn(txt("RALLY · FREEROAM · NODE-BEAM", &fonts.body, 12.0, ACCENT_300));
                    l.spawn((
                        txt("MAGIC\nFORMULA", &fonts.heading, 84.0, TEXT),
                        Node { margin: UiRect::bottom(Val::Px(32.0)), ..default() },
                    ));

                    // režimy
                    l.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(10.0),
                        width: Val::Px(360.0),
                        ..default()
                    })
                    .with_children(|m| {
                        // Volná jízda — primární (accent)
                        m.spawn((
                            Button,
                            TitleAction::Drive,
                            Node {
                                padding: UiRect::axes(Val::Px(18.0), Val::Px(14.0)),
                                column_gap: Val::Px(10.0),
                                align_items: AlignItems::Baseline,
                                ..default()
                            },
                            BackgroundColor(ACCENT),
                        ))
                        .with_children(|b| {
                            b.spawn(txt("VOLNÁ JÍZDA", &fonts.heading, 22.0, BG));
                            b.spawn(txt("Free roam", &fonts.body, 12.0, BG));
                        });
                        // Garáž
                        m.spawn((
                            Button,
                            TitleAction::Garage,
                            Node {
                                padding: UiRect::axes(Val::Px(18.0), Val::Px(12.0)),
                                column_gap: Val::Px(10.0),
                                align_items: AlignItems::Baseline,
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor(DIVIDER),
                        ))
                        .with_children(|b| {
                            b.spawn(txt("GARÁŽ", &fonts.heading, 18.0, TEXT));
                            b.spawn(txt("Garage · build", &fonts.body, 12.0, TEXT_MUTED));
                        });
                        // Rally — zatím neaktivní (ztlumené, bez Button)
                        m.spawn((
                            Node {
                                padding: UiRect::axes(Val::Px(18.0), Val::Px(12.0)),
                                column_gap: Val::Px(10.0),
                                align_items: AlignItems::Baseline,
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.10)),
                        ))
                        .with_children(|b| {
                            b.spawn(txt("RALLY", &fonts.heading, 18.0, Color::srgba(0.949, 0.949, 0.953, 0.45)));
                            b.spawn(txt("checkpoints · brzy / soon", &fonts.body, 12.0, Color::srgba(0.949, 0.949, 0.953, 0.35)));
                        });
                        // Nastavení — otevře sdílený settings overlay
                        m.spawn((
                            Button,
                            TitleAction::Settings,
                            Node {
                                padding: UiRect::axes(Val::Px(18.0), Val::Px(12.0)),
                                column_gap: Val::Px(10.0),
                                align_items: AlignItems::Baseline,
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor(DIVIDER),
                        ))
                        .with_children(|b| {
                            b.spawn(txt("NASTAVENÍ", &fonts.heading, 18.0, TEXT));
                            b.spawn(txt("Settings", &fonts.body, 12.0, TEXT_MUTED));
                        });
                    });
                });

                // map picker
                mid.spawn(Node { flex_direction: FlexDirection::Column, width: Val::Px(380.0), ..default() })
                    .with_children(|r| {
                        r.spawn((
                            txt("MAPA / MAP", &fonts.heading, 14.0, ACCENT_300),
                            Node { margin: UiRect::bottom(Val::Px(12.0)), ..default() },
                        ));
                        for (kind, cz, en, note) in [
                            (MapKind::TestPad, "TESTOVACÍ PLOCHA", "Test pad", "rovina + skidpad, ladění dat"),
                            (MapKind::Hills, "KOPCE", "Hills", "procedurální zvlněný terén"),
                            (MapKind::Real, "REÁLNÝ TERÉN", "Real terrain (SRTM)", "N50E014 — výšková data"),
                        ] {
                            r.spawn((
                                Button,
                                MapCard(kind),
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    padding: UiRect::axes(Val::Px(14.0), Val::Px(12.0)),
                                    margin: UiRect::bottom(Val::Px(10.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BorderColor(DIVIDER),
                            ))
                            .with_children(|card| {
                                card.spawn(Node {
                                    column_gap: Val::Px(8.0),
                                    align_items: AlignItems::Baseline,
                                    ..default()
                                })
                                .with_children(|row| {
                                    row.spawn(txt(cz, &fonts.heading, 16.0, TEXT));
                                    row.spawn(txt(en, &fonts.body, 11.0, TEXT_MUTED));
                                    row.spawn((
                                        Node {
                                            margin: UiRect::left(Val::Auto),
                                            padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
                                            ..default()
                                        },
                                        BackgroundColor(ACCENT),
                                        MapSelectedTag(kind),
                                    ))
                                    .with_children(|t| {
                                        t.spawn(txt("vybráno", &fonts.body, 11.0, BG));
                                    });
                                });
                                card.spawn(txt(note, &fonts.body, 12.0, TEXT_MUTED));
                            });
                        }
                    });
            });

            // ── patička: klávesy ──
            page.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(40.0), Val::Px(16.0)),
                    column_gap: Val::Px(24.0),
                    border: UiRect::top(Val::Px(1.0)),
                    ..default()
                },
                BorderColor(DIVIDER),
            ))
            .with_children(|f| {
                f.spawn(txt("W/S plyn · A/D řízení · Shift spojka · Space brzda", &fonts.body, 12.0, TEXT_MUTED));
                f.spawn(txt("C kamera · T telemetrie · Esc pauza", &fonts.body, 12.0, TEXT_MUTED));
                f.spawn((
                    txt("custom node-beam solver · Pacejka Magic Formula", &fonts.body, 12.0, TEXT_MUTED),
                    Node { margin: UiRect::left(Val::Auto), ..default() },
                ));
            });
        });
}

// Volná jízda → spawn auta podle Loadout + přepnout do hry. Garáž → Garage.
fn title_buttons(
    interactions: Query<(&Interaction, &TitleAction), Changed<Interaction>>,
    spawned: Query<Entity, With<crate::vehicle::Spawned>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut softbody: ResMut<crate::solver::SoftBody>,
    mut loadout: ResMut<crate::vehicle::Loadout>,
    mut state: ResMut<crate::vehicle::DrivetrainState>,
    chassis: Res<crate::vehicle::ChassisParams>,
    mut test: ResMut<crate::vehicle::TestDrive>,
    terrain: Res<Terrain>,
    mut next: ResMut<NextState<AppState>>,
    mut settings_ui: ResMut<crate::ui::settings::SettingsUi>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            TitleAction::Drive => {
                test.0 = false; // volná jízda = žádný auto-plyn (kdyby zbyl z testu)
                respawn_car(&mut commands, &mut meshes, &mut materials, &mut softbody, &mut loadout, &mut state, &chassis, &terrain, &spawned);
                next.set(AppState::Game);
            }
            TitleAction::Garage => next.set(AppState::Garage),
            TitleAction::Settings => settings_ui.open = true,
        }
    }
}

// Klik na kartu mapy: přepne SelectedMap a hned přestaví terén.
fn map_cards(
    interactions: Query<(&Interaction, &MapCard), Changed<Interaction>>,
    ground: Query<Entity, With<crate::GroundMesh>>,
    track: Query<Entity, With<crate::TrackProp>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut terrain: ResMut<Terrain>,
    mut colliders: ResMut<crate::solver::WorldColliders>,
    mut selected: ResMut<SelectedMap>,
) {
    for (interaction, MapCard(kind)) in &interactions {
        if *interaction != Interaction::Pressed || selected.0 == *kind {
            continue;
        }
        selected.0 = *kind;
        crate::rebuild_map(
            *kind,
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut terrain,
            &mut colliders,
            ground.iter().collect::<Vec<_>>(),
            track.iter().collect::<Vec<_>>(),
        );
    }
}

// Zvýraznění vybrané mapy (rámeček + tag „vybráno"). Levné — běží každý snímek.
fn sync_map_selection(
    selected: Res<SelectedMap>,
    mut cards: Query<(&MapCard, &mut BorderColor, &mut BackgroundColor)>,
    mut tags: Query<(&MapSelectedTag, &mut Node)>,
) {
    for (MapCard(kind), mut border, mut bg) in &mut cards {
        let on = *kind == selected.0;
        *border = BorderColor(if on { ACCENT } else { DIVIDER });
        *bg = BackgroundColor(if on { Color::srgba(0.349, 0.502, 0.651, 0.12) } else { Color::NONE });
    }
    for (MapSelectedTag(kind), mut node) in &mut tags {
        node.display = if *kind == selected.0 { Display::Flex } else { Display::None };
    }
}
