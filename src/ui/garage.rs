// Garáž podle návrhu: výběr auta + osazení dílů (zavěšení / pohon / zadní diff)
// + živá spec tabulka. Díl = data — garáž jen píše do CarBuild/Drivetrain,
// auto se postaví při Vyjet (respawn_car).

use bevy::prelude::*;

use crate::ui::*;
use crate::vehicle::{CarBuild, CarChoice, DriveLayout, Drivetrain, SuspensionKind};
use crate::world::Terrain;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::Garage), spawn_garage).add_systems(
        Update,
        (garage_clicks, sync_selection, update_specs).run_if(in_state(AppState::Garage)),
    );
}

#[derive(Component, Clone, Copy)]
enum GarageAction {
    Back,
    Drive,
    Susp(SuspensionKind),
    Layout(DriveLayout),
    DiffLock(bool),
    Car(CarChoice),
}

// Tag „osazeno" u karty zavěšení (toggluje se Display podle výběru).
#[derive(Component, Clone, Copy)]
struct SuspTag(SuspensionKind);

// Hodnota v spec tabulce (index řádku) + caption pod obrázkem auta.
#[derive(Component, Clone, Copy)]
struct SpecValue(usize);
#[derive(Component)]
struct CarCaption;

const SUSPENSIONS: [(SuspensionKind, &str, &str); 4] = [
    (SuspensionKind::RIGID, "RIGID", "tuhé, bez zdvihu"),
    (SuspensionKind::COIL, "COIL", "vinutá pružina, zdvih + stabilizátor"),
    (SuspensionKind::MCPHERSON, "MCPHERSON", "vzpěra do věže, realistická geometrie"),
    (SuspensionKind::DOUBLE_WISHBONE, "DOUBLE WISHBONE", "SLA ramena — camber v propružení"),
];

const SPEC_ROWS: [&str; 7] = ["Nody / nodes", "Beamy / beams", "Trojúhelníky", "Kola / wheels", "Zavěšení", "Pohon", "Diferenciál"];

fn spawn_garage(mut commands: Commands, fonts: Res<UiFonts>) {
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
            StateScoped(AppState::Garage),
        ))
        .with_children(|page| {
            // ── hlavička: logo | Garáž / Garage | Zpět · Vyjet ──
            page.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(32.0), Val::Px(14.0)),
                    column_gap: Val::Px(16.0),
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(Val::Px(1.0)),
                    ..default()
                },
                BorderColor(DIVIDER),
            ))
            .with_children(|h| {
                h.spawn(txt("MAGIC FORMULA", &fonts.heading, 18.0, TEXT));
                h.spawn((Node { width: Val::Px(1.0), height: Val::Px(16.0), ..default() }, BackgroundColor(DIVIDER)));
                h.spawn(txt("GARÁŽ / GARAGE", &fonts.body, 12.0, ACCENT_300));
                h.spawn(Node { margin: UiRect::left(Val::Auto), column_gap: Val::Px(8.0), ..default() })
                    .with_children(|btns| {
                        btns.spawn((
                            Button,
                            GarageAction::Back,
                            Node {
                                padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor(DIVIDER),
                        ))
                        .with_children(|b| {
                            b.spawn(txt("Zpět — Back", &fonts.body, 14.0, TEXT));
                        });
                        btns.spawn((
                            Button,
                            GarageAction::Drive,
                            Node { padding: UiRect::axes(Val::Px(22.0), Val::Px(8.0)), ..default() },
                            BackgroundColor(ACCENT),
                        ))
                        .with_children(|b| {
                            b.spawn(txt("VYJET — DRIVE", &fonts.heading, 15.0, BG));
                        });
                    });
            });

            // ── tři sloupce ──
            page.spawn(Node { flex_grow: 1.0, ..default() }).with_children(|cols| {
                // vlevo: díly
                cols.spawn((
                    Node {
                        width: Val::Px(320.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(24.0)),
                        border: UiRect::right(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor(DIVIDER),
                ))
                .with_children(|l| {
                    l.spawn((
                        txt("ZAVĚŠENÍ / SUSPENSION", &fonts.heading, 14.0, ACCENT_300),
                        Node { margin: UiRect::bottom(Val::Px(10.0)), ..default() },
                    ));
                    for (kind, name, note) in SUSPENSIONS {
                        l.spawn((
                            Button,
                            GarageAction::Susp(kind),
                            Node {
                                flex_direction: FlexDirection::Column,
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(10.0)),
                                margin: UiRect::bottom(Val::Px(8.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor(DIVIDER),
                        ))
                        .with_children(|card| {
                            card.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Baseline, ..default() })
                                .with_children(|row| {
                                    row.spawn(txt(name, &fonts.heading, 15.0, TEXT));
                                    row.spawn((
                                        Node {
                                            margin: UiRect::left(Val::Auto),
                                            padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
                                            ..default()
                                        },
                                        BackgroundColor(ACCENT),
                                        SuspTag(kind),
                                    ))
                                    .with_children(|t| {
                                        t.spawn(txt("osazeno", &fonts.body, 11.0, BG));
                                    });
                                });
                            card.spawn(txt(note, &fonts.body, 12.0, TEXT_MUTED));
                        });
                    }

                    l.spawn((
                        txt("POHON / DRIVE LAYOUT", &fonts.heading, 14.0, ACCENT_300),
                        Node { margin: UiRect::vertical(Val::Px(10.0)).with_top(Val::Px(20.0)), ..default() },
                    ));
                    l.spawn((Node { border: UiRect::all(Val::Px(1.0)), ..default() }, BorderColor(DIVIDER)))
                        .with_children(|seg| {
                            for layout in [DriveLayout::Fwd, DriveLayout::Rwd, DriveLayout::Awd { front: 0.5 }] {
                                seg.spawn((
                                    Button,
                                    GarageAction::Layout(layout),
                                    Node {
                                        flex_grow: 1.0,
                                        justify_content: JustifyContent::Center,
                                        padding: UiRect::axes(Val::Px(0.0), Val::Px(7.0)),
                                        ..default()
                                    },
                                ))
                                .with_children(|b| {
                                    b.spawn(txt(layout.label(), &fonts.body, 14.0, TEXT));
                                });
                            }
                        });

                    l.spawn((
                        txt("ZADNÍ DIFERENCIÁL / REAR DIFF", &fonts.heading, 14.0, ACCENT_300),
                        Node { margin: UiRect::vertical(Val::Px(10.0)).with_top(Val::Px(20.0)), ..default() },
                    ));
                    l.spawn((Node { border: UiRect::all(Val::Px(1.0)), ..default() }, BorderColor(DIVIDER)))
                        .with_children(|seg| {
                            for (locked, label) in [(false, "Otevřený / open"), (true, "Zamčený / locked")] {
                                seg.spawn((
                                    Button,
                                    GarageAction::DiffLock(locked),
                                    Node {
                                        flex_grow: 1.0,
                                        justify_content: JustifyContent::Center,
                                        padding: UiRect::axes(Val::Px(0.0), Val::Px(7.0)),
                                        ..default()
                                    },
                                ))
                                .with_children(|b| {
                                    b.spawn(txt(label, &fonts.body, 14.0, TEXT));
                                });
                            }
                        });
                });

                // střed: výběr auta + „obrázek"
                cols.spawn(Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(40.0), Val::Px(32.0)),
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(Node { column_gap: Val::Px(8.0), margin: UiRect::bottom(Val::Px(16.0)), ..default() })
                        .with_children(|tabs| {
                            let mut cars: Vec<(CarChoice, String, String)> =
                                vec![(CarChoice::TestCar, "TESTOVACÍ VŮZ".into(), "built-in".into())];
                            for (i, path) in crate::available_vehs().iter().enumerate() {
                                let file = path.file_name().unwrap().to_string_lossy().into_owned();
                                // Jediný .veh je dnes fabia — jméno srozumitelně natvrdo, jinak soubor.
                                let name = if file == "fabia.veh" { "ŠKODA FABIA MK1".into() } else { file.to_uppercase() };
                                cars.push((CarChoice::Veh(i), name, file));
                            }
                            for (choice, name, file) in cars {
                                tabs.spawn((
                                    Button,
                                    GarageAction::Car(choice),
                                    Node {
                                        padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                                        column_gap: Val::Px(8.0),
                                        align_items: AlignItems::Baseline,
                                        border: UiRect::all(Val::Px(1.0)),
                                        ..default()
                                    },
                                    BorderColor(DIVIDER),
                                ))
                                .with_children(|b| {
                                    b.spawn(txt(name, &fonts.heading, 15.0, TEXT));
                                    b.spawn(txt(file, &fonts.body, 11.0, TEXT_MUTED));
                                });
                            }
                        });
                    c.spawn((
                        Node {
                            flex_grow: 1.0,
                            border: UiRect::all(Val::Px(1.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BorderColor(DIVIDER),
                    ))
                    .with_children(|fig| {
                        fig.spawn(txt("render auta — brzy / soon", &fonts.body, 13.0, TEXT_MUTED));
                    });
                    c.spawn(Node { margin: UiRect::top(Val::Px(8.0)), column_gap: Val::Px(16.0), ..default() })
                        .with_children(|cap| {
                            cap.spawn((txt("", &fonts.body, 12.0, TEXT_MUTED), CarCaption));
                            cap.spawn((
                                txt(".veh — vlastní JSON formát", &fonts.body, 12.0, TEXT_MUTED),
                                Node { margin: UiRect::left(Val::Auto), ..default() },
                            ));
                        });
                });

                // vpravo: spec tabulka
                cols.spawn((
                    Node {
                        width: Val::Px(300.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(24.0)),
                        border: UiRect::left(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor(DIVIDER),
                ))
                .with_children(|r| {
                    r.spawn((
                        txt("SPECIFIKACE / SPEC", &fonts.heading, 14.0, ACCENT_300),
                        Node { margin: UiRect::bottom(Val::Px(10.0)), ..default() },
                    ));
                    for (i, label) in SPEC_ROWS.iter().enumerate() {
                        r.spawn((
                            Node {
                                justify_content: JustifyContent::SpaceBetween,
                                padding: UiRect::axes(Val::Px(0.0), Val::Px(5.0)),
                                border: UiRect::bottom(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor(Color::srgba(0.949, 0.949, 0.953, 0.10)),
                        ))
                        .with_children(|row| {
                            row.spawn(txt(*label, &fonts.body, 13.0, TEXT_MUTED));
                            row.spawn((txt("—", &fonts.body, 13.0, TEXT), SpecValue(i)));
                        });
                    }
                    r.spawn((
                        txt(
                            "Výměna dílu přestaví stojící auto za běhu — solver zůstává beze změny, díl je jen data (CarBuild).",
                            &fonts.body,
                            12.0,
                            TEXT_MUTED,
                        ),
                        Node { margin: UiRect::top(Val::Px(16.0)), ..default() },
                    ));
                });
            });
        });
}

// Kliky: díly píšou do CarBuild/Drivetrain, Zpět/Vyjet přepínají stav.
fn garage_clicks(
    interactions: Query<(&Interaction, &GarageAction), Changed<Interaction>>,
    spawned: Query<Entity, With<crate::vehicle::Spawned>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut softbody: ResMut<crate::solver::SoftBody>,
    mut drivetrain: ResMut<Drivetrain>,
    mut build: ResMut<CarBuild>,
    terrain: Res<Terrain>,
    mut next: ResMut<NextState<AppState>>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *action {
            GarageAction::Back => next.set(AppState::Title),
            GarageAction::Drive => {
                respawn_car(&mut commands, &mut meshes, &mut materials, &mut softbody, &mut drivetrain, &build, &terrain, &spawned);
                next.set(AppState::Game);
            }
            GarageAction::Susp(kind) => build.suspension = kind,
            GarageAction::Layout(layout) => drivetrain.layout = layout,
            GarageAction::DiffLock(locked) => drivetrain.rear_diff.lock = if locked { 1.0 } else { 0.0 },
            GarageAction::Car(choice) => build.car = choice,
        }
    }
}

// Zvýraznění vybraných dílů/auta. Běží každý snímek (pár entit).
fn sync_selection(
    build: Res<CarBuild>,
    dt: Res<Drivetrain>,
    mut nodes: Query<(&GarageAction, Option<&mut BorderColor>, &mut BackgroundColor), With<Button>>,
    mut tags: Query<(&SuspTag, &mut Node)>,
) {
    for (action, border, mut bg) in &mut nodes {
        let (selectable, on) = match *action {
            GarageAction::Susp(kind) => (true, kind == build.suspension),
            GarageAction::Layout(layout) => (true, layout.label() == dt.layout.label()),
            GarageAction::DiffLock(locked) => (true, locked == (dt.rear_diff.lock > 0.5)),
            GarageAction::Car(choice) => (true, choice == build.car),
            _ => (false, false),
        };
        if !selectable {
            continue;
        }
        match *action {
            // segmenty (pohon/diff): výplň accent, bez rámečku
            GarageAction::Layout(_) | GarageAction::DiffLock(_) => {
                *bg = BackgroundColor(if on { ACCENT } else { Color::NONE });
            }
            // karty (zavěšení/auto): rámeček accent + slabá výplň
            _ => {
                if let Some(mut border) = border {
                    *border = BorderColor(if on { ACCENT } else { DIVIDER });
                }
                *bg = BackgroundColor(if on { Color::srgba(0.349, 0.502, 0.651, 0.12) } else { Color::NONE });
            }
        }
    }
    for (SuspTag(kind), mut node) in &mut tags {
        node.display = if *kind == build.suspension { Display::Flex } else { Display::None };
    }
}

// Spec tabulka + caption. Počty nodů/beamů pro .veh se parsují jednou a cachují
// (Local); test car má konstanty (16 nodů / 32 beamů viz spawn_solver_car).
fn update_specs(
    build: Res<CarBuild>,
    dt: Res<Drivetrain>,
    mut cache: Local<Option<(usize, [String; 4], String)>>,
    mut values: Query<(&SpecValue, &mut Text), Without<CarCaption>>,
    mut caption: Query<&mut Text, With<CarCaption>>,
) {
    // Struktura auta (nody/beamy/trojúhelníky/kola) podle výběru, s cache pro .veh.
    let (counts, cap) = match build.car {
        CarChoice::TestCar => (
            ["16".into(), "32".into(), "—".into(), "4".into()],
            "Testovací vůz — kvádr + 4 kola, vestavěný".to_string(),
        ),
        CarChoice::Veh(i) => {
            if cache.as_ref().map(|(ci, ..)| *ci) != Some(i) {
                let parsed = crate::available_vehs()
                    .get(i)
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .and_then(|s| serde_json::from_str::<crate::veh::Vehicle>(&s).ok());
                *cache = Some(match parsed {
                    Some(v) => {
                        let cap = format!(
                            "{} — {} nodů · {} beamů · {} trojúhelníků · {} kola",
                            v.name, v.nodes.len(), v.beams.len(), v.triangles.len(), v.wheels.len()
                        );
                        (i, [v.nodes.len().to_string(), v.beams.len().to_string(), v.triangles.len().to_string(), v.wheels.len().to_string()], cap)
                    }
                    None => (i, ["?".into(), "?".into(), "?".into(), "?".into()], "soubor nejde načíst".into()),
                });
            }
            let (_, counts, cap) = cache.as_ref().unwrap();
            (counts.clone(), cap.clone())
        }
    };
    let diff = if dt.rear_diff.lock > 0.5 { "zamčený" } else { "otevřený" };
    let rows: [&str; 7] = [
        &counts[0], &counts[1], &counts[2], &counts[3],
        build.suspension.label(), dt.layout.label(), diff,
    ];
    for (SpecValue(i), mut text) in &mut values {
        if text.0 != rows[*i] {
            text.0 = rows[*i].to_string();
        }
    }
    if let Ok(mut text) = caption.get_single_mut() {
        if text.0 != cap {
            text.0 = cap;
        }
    }
}
