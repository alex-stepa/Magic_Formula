// Garáž podle návrhu: výběr auta + osazení dílů (zavěšení / pohon / zadní diff)
// + živá spec tabulka. Díl = data — garáž jen píše do Loadout,
// auto se postaví při Vyjet (respawn_car).

use bevy::prelude::*;

use crate::parts::{PartCatalog, PartCategory, PartData};
use crate::ui::*;
use crate::vehicle::{CarChoice, DriveLayout, DrivetrainState, EngineConfig, Loadout};
use crate::world::Terrain;

pub fn plugin(app: &mut App) {
    app.init_resource::<ActiveTab>()
        .init_resource::<BuildMass>()
        .init_resource::<SavedBuilds>()
        .init_resource::<BuildsDirty>()
        .add_systems(OnEnter(AppState::Garage), spawn_garage)
        .add_systems(
            Update,
            (garage_clicks, sync_selection, sync_tabs, drag_sliders, sync_sliders, refresh_build_list, (update_specs, update_part_deltas).chain(), garage_scroll)
                .run_if(in_state(AppState::Garage)),
        );
}

// Tab v garáži. Pohon sdružuje motor+převodovku (katalog) + layout/diff (přepínače).
#[derive(Clone, Copy, PartialEq)]
enum GarageTab {
    Suspension,
    Drivetrain,
    Brakes,
}

impl GarageTab {
    fn label(self) -> &'static str {
        match self {
            GarageTab::Suspension => "ZAVĚŠENÍ",
            GarageTab::Drivetrain => "POHON",
            GarageTab::Brakes => "BRZDY / PNEU",
        }
    }
}

#[derive(Resource)]
struct ActiveTab(GarageTab);
impl Default for ActiveTab {
    fn default() -> Self {
        Self(GarageTab::Suspension)
    }
}

#[derive(Component, Clone, Copy)]
enum GarageAction {
    Back,
    Drive,
    Test, // vyjet s auto-plynem (měření 0–100)
    Tab(GarageTab),
    Part(usize), // index do PartCatalog (celoautové kategorie)
    SuspPart { idx: usize, front: bool }, // zavěšení na konkrétní nápravu
    Layout(DriveLayout),
    DiffLock(bool),
    Car(CarChoice),
    EngineCfg(EngineConfig), // I / V / Boxer
    SaveBuild,
    LoadBuild(usize),   // index do SavedBuilds
    DeleteBuild(usize),
}

// Uložené buildy hráče (načtené z player/builds) + příznak přenačtení.
#[derive(Resource, Default)]
struct SavedBuilds(Vec<crate::saves::SavedBuild>);
#[derive(Resource, Default)]
struct BuildsDirty(bool);

// Kontejner seznamu uložených buildů (děti se přestavují při změně).
#[derive(Component)]
struct BuildList;

// Obal sekce jednoho tabu (toggluje se Display podle ActiveTab).
#[derive(Component, Clone, Copy)]
struct TabContent(GarageTab);

// Tag „osazeno" u karty dílu (Display podle is_selected daného indexu katalogu).
#[derive(Component, Clone, Copy)]
struct PartTag(usize);

// Tag „osazeno" u karty zavěšení — highlight dle nápravy (idx katalogu, front).
#[derive(Component, Clone, Copy)]
struct SuspTag(usize, bool);

// Hodnota v spec tabulce (index řádku) + caption pod obrázkem auta.
#[derive(Component, Clone, Copy)]
struct SpecValue(usize);
#[derive(Component)]
struct CarCaption;

// KPI buňka pod autem: 0 = 0–100, 1 = hmotnost, 2 = výkon, 3 = rozložení př/z.
#[derive(Component, Clone, Copy)]
struct KpiValue(usize);

// Sloupec, který jde scrollovat kolečkem (obsah může být vyšší než okno).
#[derive(Component)]
struct Scrollable;

// Delta řádek na kartě dílu: co díl změní proti aktuálnímu buildu (index do katalogu).
#[derive(Component, Clone, Copy)]
struct PartDelta(usize);

// Hmotnost/rozložení aktuálního auta (počítá update_specs, čte delta karet i KPI).
#[derive(Resource, Default)]
struct BuildMass {
    mass: f32,
    front: f32,
}

// Parametr laděný sliderem — motor i zavěšení. Generický: get/set operují nad Loadout.
#[derive(Clone, Copy, PartialEq)]
enum SliderKind {
    Displacement,
    Redline,
    Boost,
    Cylinders,
    SuspSpring(bool), // bool = přední náprava
    SuspArm(bool),
    WheelOffset(bool), // offset ráfku (m, + ven) — bool = přední náprava
    BrakeBias,  // podíl brzdné síly na přední osu (0..1)
    BrakePower, // celková brzdná síla (front+rear, Nm)
}

impl SliderKind {
    fn range(self) -> (f32, f32) {
        match self {
            SliderKind::Displacement => (0.8, 4.0),
            SliderKind::Redline => (5000.0, 9000.0),
            SliderKind::Boost => (0.0, 1.5),
            SliderKind::Cylinders => (3.0, 12.0),
            SliderKind::SuspSpring(_) => (40_000.0, 250_000.0),
            SliderKind::SuspArm(_) => (80_000.0, 300_000.0),
            SliderKind::WheelOffset(_) => (-0.04, 0.08), // −40 mm (užší) až +80 mm (širší)
            SliderKind::BrakeBias => (0.4, 0.8),
            SliderKind::BrakePower => (800.0, 3000.0),
        }
    }
    fn is_engine(self) -> bool {
        matches!(self, SliderKind::Displacement | SliderKind::Redline | SliderKind::Boost | SliderKind::Cylinders)
    }
    fn get(self, l: &Loadout) -> f32 {
        let total = l.brakes.front + l.brakes.rear;
        match self {
            SliderKind::Displacement => l.engine_spec.displacement,
            SliderKind::Redline => l.engine_spec.redline,
            SliderKind::Boost => l.engine_spec.boost,
            SliderKind::Cylinders => l.engine_spec.cylinders as f32,
            SliderKind::SuspSpring(f) => l.susp(f).spring(),
            SliderKind::SuspArm(f) => l.susp(f).arm(),
            SliderKind::WheelOffset(f) => l.offset(f),
            SliderKind::BrakeBias => if total > 0.0 { l.brakes.front / total } else { 0.6 },
            SliderKind::BrakePower => total,
        }
    }
    fn set(self, l: &mut Loadout, v: f32) {
        match self {
            SliderKind::Displacement => l.engine_spec.displacement = v,
            SliderKind::Redline => l.engine_spec.redline = (v / 100.0).round() * 100.0,
            SliderKind::Boost => l.engine_spec.boost = v,
            SliderKind::Cylinders => l.engine_spec.cylinders = v.round() as u8,
            SliderKind::SuspSpring(f) => { let k = l.susp(f).with_spring(v); l.set_susp(f, k); }
            SliderKind::SuspArm(f) => { let k = l.susp(f).with_arm(v); l.set_susp(f, k); }
            SliderKind::WheelOffset(f) => l.set_offset(f, v),
            // bias/power přerozdělí front/rear (parking beze změny)
            SliderKind::BrakeBias => {
                let total = l.brakes.front + l.brakes.rear;
                l.brakes.front = total * v;
                l.brakes.rear = total * (1.0 - v);
            }
            SliderKind::BrakePower => {
                let total = l.brakes.front + l.brakes.rear;
                let bias = if total > 0.0 { l.brakes.front / total } else { 0.6 };
                l.brakes.front = v * bias;
                l.brakes.rear = v * (1.0 - bias);
            }
        }
    }
    fn fmt(self, v: f32) -> String {
        match self {
            SliderKind::Displacement => format!("{v:.1} L"),
            SliderKind::Redline => format!("{v:.0} ot/min"),
            SliderKind::Boost => if v < 0.05 { "atmosf.".into() } else { format!("{v:.2} bar") },
            SliderKind::Cylinders => format!("{v:.0} válců"),
            SliderKind::SuspSpring(_) | SliderKind::SuspArm(_) => format!("{:.0} kN/m", v / 1000.0),
            SliderKind::WheelOffset(_) => format!("{:+.0} mm", v * 1000.0),
            SliderKind::BrakeBias => format!("{:.0}/{:.0}", v * 100.0, (1.0 - v) * 100.0),
            SliderKind::BrakePower => format!("{v:.0} Nm"),
        }
    }
}

// Track slideru (drag mění parametr), výplň (šířka % = hodnota), hodnota (text).
#[derive(Component, Clone, Copy)]
struct SliderTrack(SliderKind);
#[derive(Component, Clone, Copy)]
struct SliderFill(SliderKind);
#[derive(Component, Clone, Copy)]
struct SliderLabel(SliderKind);

const SPEC_ROWS: [&str; 7] = ["Nody / nodes", "Beamy / beams", "Trojúhelníky", "Kola / wheels", "Zavěšení", "Pohon", "Diferenciál"];

// Karty dílů jedné kategorie z katalogu (generický PartPicker — nový díl = JSON entry).
// axle=None → celoautová kategorie (Part); Some(front) → zavěšení na danou nápravu (SuspPart).
fn spawn_cards(l: &mut ChildBuilder, catalog: &PartCatalog, cat: PartCategory, fonts: &UiFonts, axle: Option<bool>) {
    for (i, opt) in catalog.0.iter().enumerate().filter(|(_, o)| o.data.category() == cat) {
        let action = match axle {
            Some(front) => GarageAction::SuspPart { idx: i, front },
            None => GarageAction::Part(i),
        };
        l.spawn((
            Button,
            action,
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
                    row.spawn(txt(&opt.name, &fonts.heading, 15.0, TEXT));
                    let mut badge = row.spawn((
                        Node {
                            margin: UiRect::left(Val::Auto),
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(ACCENT),
                    ));
                    match axle {
                        Some(front) => { badge.insert(SuspTag(i, front)); }
                        None => { badge.insert(PartTag(i)); }
                    }
                    badge.with_children(|t| {
                        t.spawn(txt("osazeno", &fonts.body, 11.0, BG));
                    });
                });
            card.spawn(txt(&opt.note, &fonts.body, 12.0, TEXT_MUTED));
            // delta proti aktuálu (jen převodovka — vyplní update_part_deltas)
            card.spawn((txt("", &fonts.mono, 11.0, TEXT_MUTED), PartDelta(i)));
        });
    }
}

// Slidery (motor i zavěšení). Track = Button + RelativeCursorPosition → drag_sliders
// mapuje kurzor na hodnotu; dyno/KPI se aktualizují živě. Znovupoužitelné pro budoucí
// parametry (brzdy/pneu) — přidat variantu SliderKind + řádek sem.
fn spawn_sliders(l: &mut ChildBuilder, fonts: &UiFonts, rows: &[(SliderKind, &str)]) {
    for &(kind, label) in rows {
        l.spawn(Node { flex_direction: FlexDirection::Column, margin: UiRect::bottom(Val::Px(12.0)), row_gap: Val::Px(4.0), ..default() })
            .with_children(|s| {
                s.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() })
                    .with_children(|row| {
                        row.spawn(txt(label, &fonts.body, 13.0, TEXT));
                        row.spawn((txt("—", &fonts.mono, 13.0, ACCENT_300), SliderLabel(kind)));
                    });
                s.spawn((
                    Button,
                    SliderTrack(kind),
                    bevy::ui::RelativeCursorPosition::default(),
                    Node { height: Val::Px(10.0), border: UiRect::all(Val::Px(1.0)), padding: UiRect::all(Val::Px(1.0)), ..default() },
                    BorderColor(DIVIDER),
                    BackgroundColor(Color::srgba(0.949, 0.949, 0.953, 0.06)),
                ))
                .with_children(|track| {
                    track.spawn((
                        Node { width: Val::Percent(50.0), height: Val::Percent(100.0), ..default() },
                        BackgroundColor(ACCENT),
                        SliderFill(kind),
                    ));
                });
            });
    }
}

fn spawn_garage(mut commands: Commands, fonts: Res<UiFonts>, catalog: Res<PartCatalog>, preview: Res<crate::ui::garage_preview::PreviewImage>, dyno: Res<crate::ui::dyno::DynoImage>, mut dirty: ResMut<BuildsDirty>, mut test: ResMut<crate::vehicle::TestDrive>) {
    dirty.0 = true; // přenačti seznam uložených buildů při vstupu do garáže
    test.0 = false; // zruš zbytkový auto-plyn (kdyby test nedojel na 100)
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
                            GarageAction::Test,
                            Node { padding: UiRect::axes(Val::Px(18.0), Val::Px(8.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
                            BorderColor(ACCENT),
                        ))
                        .with_children(|b| {
                            b.spawn(txt("TEST 0–100", &fonts.heading, 14.0, ACCENT_300));
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
            page.spawn(Node {
                flex_grow: 1.0,
                width: Val::Percent(100.0),
                min_height: Val::Px(0.0),
                overflow: Overflow::clip(),
                ..default()
            })
            .with_children(|cols| {
                // vlevo: díly (scrollovatelné — obsah tabu Pohon může přesáhnout okno)
                cols.spawn((
                    Node {
                        width: Val::Px(320.0),
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(24.0)),
                        border: UiRect::right(Val::Px(1.0)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BorderColor(DIVIDER),
                    Scrollable,
                ))
                .with_children(|l| {
                    // ── tab bar: Zavěšení / Pohon / Brzdy ──
                    l.spawn(Node {
                        margin: UiRect::bottom(Val::Px(14.0)),
                        border: UiRect::bottom(Val::Px(1.0)),
                        column_gap: Val::Px(4.0),
                        ..default()
                    })
                    .insert(BorderColor(DIVIDER))
                    .with_children(|bar| {
                        for tab in [GarageTab::Suspension, GarageTab::Drivetrain, GarageTab::Brakes] {
                            bar.spawn((
                                Button,
                                GarageAction::Tab(tab),
                                Node {
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                                    border: UiRect::bottom(Val::Px(2.0)),
                                    ..default()
                                },
                                BorderColor(Color::NONE),
                            ))
                            .with_children(|b| {
                                b.spawn(txt(tab.label(), &fonts.heading, 14.0, TEXT));
                            });
                        }
                    });

                    // ── ZAVĚŠENÍ: dvě sekce (přední / zadní), každá karty + slidery té nápravy ──
                    l.spawn((Node { flex_direction: FlexDirection::Column, ..default() }, TabContent(GarageTab::Suspension)))
                        .with_children(|s| {
                            for (front, heading) in [(true, "PŘEDNÍ ZAVĚŠENÍ"), (false, "ZADNÍ ZAVĚŠENÍ")] {
                                s.spawn((txt(heading, &fonts.heading, 13.0, ACCENT_300), Node { margin: UiRect::vertical(Val::Px(8.0)), ..default() }));
                                spawn_cards(s, &catalog, PartCategory::Suspension, &fonts, Some(front));
                                spawn_sliders(s, &fonts, &[
                                    (SliderKind::SuspSpring(front), "Tuhost pružiny"),
                                    (SliderKind::SuspArm(front), "Tuhost ramen"),
                                    (SliderKind::WheelOffset(front), "Offset ráfku"),
                                ]);
                            }
                        });

                    // ── POHON: motor + převodovka (katalog) + layout/diff (přepínače) ──
                    l.spawn((Node { flex_direction: FlexDirection::Column, ..default() }, TabContent(GarageTab::Drivetrain)))
                        .with_children(|d| {
                            d.spawn((txt("MOTOR / ENGINE", &fonts.heading, 13.0, ACCENT_300), Node { margin: UiRect::bottom(Val::Px(8.0)), ..default() }));
                            spawn_sliders(d, &fonts, &[
                                (SliderKind::Displacement, "Objem"),
                                (SliderKind::Cylinders, "Válce"),
                                (SliderKind::Redline, "Otáčky"),
                                (SliderKind::Boost, "Přeplňování"),
                            ]);
                            // konfigurace válců (I / V / Boxer) — segment
                            d.spawn((Node { border: UiRect::all(Val::Px(1.0)), margin: UiRect::bottom(Val::Px(4.0)), ..default() }, BorderColor(DIVIDER)))
                                .with_children(|seg| {
                                    for cfg in [EngineConfig::Inline, EngineConfig::V, EngineConfig::Boxer] {
                                        seg.spawn((
                                            Button,
                                            GarageAction::EngineCfg(cfg),
                                            Node { flex_grow: 1.0, justify_content: JustifyContent::Center, padding: UiRect::axes(Val::Px(0.0), Val::Px(6.0)), ..default() },
                                        ))
                                        .with_children(|b| {
                                            b.spawn(txt(cfg.label(), &fonts.body, 13.0, TEXT));
                                        });
                                    }
                                });
                            d.spawn((txt("PŘEVODOVKA / GEARBOX", &fonts.heading, 13.0, ACCENT_300), Node { margin: UiRect::vertical(Val::Px(8.0)), ..default() }));
                            spawn_cards(d, &catalog, PartCategory::Gearbox, &fonts, None);

                            d.spawn((txt("ROZVOD / DRIVE LAYOUT", &fonts.heading, 13.0, ACCENT_300), Node { margin: UiRect::vertical(Val::Px(8.0)), ..default() }));
                            d.spawn((Node { border: UiRect::all(Val::Px(1.0)), ..default() }, BorderColor(DIVIDER)))
                                .with_children(|seg| {
                                    for layout in [DriveLayout::Fwd, DriveLayout::Rwd, DriveLayout::Awd { front: 0.5 }] {
                                        seg.spawn((
                                            Button,
                                            GarageAction::Layout(layout),
                                            Node { flex_grow: 1.0, justify_content: JustifyContent::Center, padding: UiRect::axes(Val::Px(0.0), Val::Px(7.0)), ..default() },
                                        ))
                                        .with_children(|b| {
                                            b.spawn(txt(layout.label(), &fonts.body, 14.0, TEXT));
                                        });
                                    }
                                });

                            d.spawn((txt("ZADNÍ DIFERENCIÁL / REAR DIFF", &fonts.heading, 13.0, ACCENT_300), Node { margin: UiRect::vertical(Val::Px(8.0)), ..default() }));
                            d.spawn((Node { border: UiRect::all(Val::Px(1.0)), ..default() }, BorderColor(DIVIDER)))
                                .with_children(|seg| {
                                    for (locked, label) in [(false, "Otevřený / open"), (true, "Zamčený / locked")] {
                                        seg.spawn((
                                            Button,
                                            GarageAction::DiffLock(locked),
                                            Node { flex_grow: 1.0, justify_content: JustifyContent::Center, padding: UiRect::axes(Val::Px(0.0), Val::Px(7.0)), ..default() },
                                        ))
                                        .with_children(|b| {
                                            b.spawn(txt(label, &fonts.body, 14.0, TEXT));
                                        });
                                    }
                                });

                            d.spawn((txt("CHLAZENÍ / COOLING", &fonts.heading, 13.0, ACCENT_300), Node { margin: UiRect::vertical(Val::Px(8.0)), ..default() }));
                            spawn_cards(d, &catalog, PartCategory::Cooler, &fonts, None);
                        });

                    // ── BRZDY / PNEU ──
                    l.spawn((Node { flex_direction: FlexDirection::Column, ..default() }, TabContent(GarageTab::Brakes)))
                        .with_children(|s| {
                            s.spawn((txt("BRZDY / BRAKES", &fonts.heading, 13.0, ACCENT_300), Node { margin: UiRect::bottom(Val::Px(8.0)), ..default() }));
                            spawn_sliders(s, &fonts, &[
                                (SliderKind::BrakeBias, "Rozložení (př/z)"),
                                (SliderKind::BrakePower, "Síla brzd"),
                            ]);
                            s.spawn((txt("PNEU / TIRES", &fonts.heading, 13.0, ACCENT_300), Node { margin: UiRect::vertical(Val::Px(8.0)), ..default() }));
                            spawn_cards(s, &catalog, PartCategory::Tire, &fonts, None);
                        });
                });

                // střed: výběr auta + náhled. min_width/height 0 = smí se zmenšit,
                // ať netlačí pravý sloupec z okna a nepřetéká dolů.
                cols.spawn(Node {
                    flex_grow: 1.0,
                    min_width: Val::Px(0.0),
                    min_height: Val::Px(0.0),
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
                    // Živý 3D náhled auta (render-to-texture, viz garage_preview).
                    c.spawn((
                        Node {
                            flex_grow: 1.0,
                            min_height: Val::Px(0.0), // ustup KPI/caption pod sebou
                            border: UiRect::all(Val::Px(1.0)),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        BorderColor(DIVIDER),
                    ))
                    .with_children(|fig| {
                        fig.spawn((
                            ImageNode::new(preview.0.clone()),
                            Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                        ));
                    });
                    // KPI pruh — živě z reálného buildu (stats.rs). Čísla napřed.
                    c.spawn((
                        Node {
                            margin: UiRect::top(Val::Px(10.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor(DIVIDER),
                    ))
                    .with_children(|row| {
                        for (idx, label) in ["0–100 km/h", "Hmotnost", "Výkon", "Rozložení př/z"].into_iter().enumerate() {
                            row.spawn(Node {
                                flex_grow: 1.0,
                                flex_direction: FlexDirection::Column,
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                                border: if idx > 0 { UiRect::left(Val::Px(1.0)) } else { UiRect::default() },
                                row_gap: Val::Px(2.0),
                                ..default()
                            })
                            .insert(BorderColor(DIVIDER))
                            .with_children(|cell| {
                                cell.spawn(txt(label, &fonts.body, 11.0, TEXT_MUTED));
                                cell.spawn((txt("—", &fonts.mono, 20.0, ACCENT_300), KpiValue(idx)));
                            });
                        }
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

                // vpravo: spec tabulka (scrollovatelná pro jistotu)
                cols.spawn((
                    Node {
                        width: Val::Px(300.0),
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(24.0)),
                        border: UiRect::left(Val::Px(1.0)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BorderColor(DIVIDER),
                    Scrollable,
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
                    // ── DYNO: momentová (accent) + výkonová (světlá) křivka motoru ──
                    r.spawn((
                        txt("DYNO / MOTOR", &fonts.heading, 14.0, ACCENT_300),
                        Node { margin: UiRect::vertical(Val::Px(10.0)).with_top(Val::Px(20.0)), ..default() },
                    ));
                    r.spawn((
                        ImageNode::new(dyno.0.clone()),
                        Node { width: Val::Percent(100.0), aspect_ratio: Some(480.0 / 220.0), ..default() },
                        BorderColor(DIVIDER),
                    ));
                    r.spawn(Node { column_gap: Val::Px(14.0), margin: UiRect::top(Val::Px(6.0)), ..default() })
                        .with_children(|leg| {
                            for (col, label) in [(ACCENT, "moment (Nm)"), (ACCENT_300, "výkon (kW)")] {
                                leg.spawn(Node { column_gap: Val::Px(5.0), align_items: AlignItems::Center, ..default() })
                                    .with_children(|it| {
                                        it.spawn((Node { width: Val::Px(14.0), height: Val::Px(2.0), ..default() }, BackgroundColor(col)));
                                        it.spawn(txt(label, &fonts.body, 11.0, TEXT_MUTED));
                                    });
                            }
                        });

                    // ── ULOŽENÉ BUILDY (player data) ──
                    r.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, margin: UiRect::vertical(Val::Px(10.0)).with_top(Val::Px(24.0)), ..default() })
                        .with_children(|h| {
                            h.spawn(txt("ULOŽENÉ BUILDY", &fonts.heading, 14.0, ACCENT_300));
                            h.spawn((
                                Button,
                                GarageAction::SaveBuild,
                                Node { padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
                                BorderColor(ACCENT),
                            ))
                            .with_children(|b| {
                                b.spawn(txt("+ ULOŽIT", &fonts.body, 12.0, ACCENT_300));
                            });
                        });
                    r.spawn((Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() }, BuildList));
                });
            });
        });
}

// Kliky: díly píšou do Loadout, Zpět/Vyjet přepínají stav.
fn garage_clicks(
    interactions: Query<(&Interaction, &GarageAction), Changed<Interaction>>,
    spawned: Query<Entity, With<crate::vehicle::Spawned>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut softbody: ResMut<crate::solver::SoftBody>,
    mut loadout: ResMut<Loadout>,
    mut state: ResMut<DrivetrainState>,
    chassis: Res<crate::vehicle::ChassisParams>,
    catalog: Res<PartCatalog>,
    mut active: ResMut<ActiveTab>,
    builds: Res<SavedBuilds>,
    mut dirty: ResMut<BuildsDirty>,
    mut test: ResMut<crate::vehicle::TestDrive>,
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
                test.0 = false;
                respawn_car(&mut commands, &mut meshes, &mut materials, &mut softbody, &mut loadout, &mut state, &chassis, &terrain, &spawned);
                next.set(AppState::Game);
            }
            GarageAction::Test => {
                respawn_car(&mut commands, &mut meshes, &mut materials, &mut softbody, &mut loadout, &mut state, &chassis, &terrain, &spawned);
                test.0 = true; // auto-plyn → AccelTimer změří 0–100
                next.set(AppState::Game);
            }
            GarageAction::Tab(tab) => active.0 = tab,
            GarageAction::Part(i) => catalog.0[i].data.apply(&mut loadout),
            GarageAction::SuspPart { idx, front } => {
                if let PartData::Suspension(k) = catalog.0[idx].data {
                    loadout.set_susp(front, k);
                }
            }
            GarageAction::Layout(layout) => loadout.layout = layout,
            GarageAction::DiffLock(locked) => loadout.rear_diff.lock = if locked { 1.0 } else { 0.0 },
            GarageAction::Car(choice) => loadout.car = choice,
            GarageAction::EngineCfg(cfg) => {
                loadout.engine_spec.config = cfg;
                loadout.engine = loadout.engine_spec.build();
            }
            GarageAction::SaveBuild => {
                let b = crate::saves::SavedBuild::from_loadout(&loadout, crate::saves::SavedBuild::auto_name(&loadout));
                if let Err(e) = crate::saves::save(&b) {
                    warn!("uložení buildu selhalo: {e}");
                }
                dirty.0 = true;
            }
            GarageAction::LoadBuild(i) => {
                if let Some(b) = builds.0.get(i) {
                    b.apply(&mut loadout);
                }
            }
            GarageAction::DeleteBuild(i) => {
                if let Some(b) = builds.0.get(i) {
                    crate::saves::delete(&b.name);
                    dirty.0 = true;
                }
            }
        }
    }
}

// Přenačte seznam uložených buildů z disku a přestaví UI (při dirty). despawn+respawn
// dětí BuildListu — jednoduchý dynamický seznam.
fn refresh_build_list(
    mut dirty: ResMut<BuildsDirty>,
    mut builds: ResMut<SavedBuilds>,
    fonts: Res<UiFonts>,
    list: Query<Entity, With<BuildList>>,
    mut commands: Commands,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    builds.0 = crate::saves::list();
    let Ok(list) = list.get_single() else { return };
    commands.entity(list).despawn_descendants();
    commands.entity(list).with_children(|l| {
        if builds.0.is_empty() {
            l.spawn(txt("žádné — ulož aktuální build tlačítkem výše", &fonts.body, 12.0, TEXT_MUTED));
        }
        for (i, b) in builds.0.iter().enumerate() {
            l.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, column_gap: Val::Px(8.0), ..default() })
                .with_children(|row| {
                    row.spawn((
                        Button,
                        GarageAction::LoadBuild(i),
                        Node { flex_grow: 1.0, padding: UiRect::axes(Val::Px(10.0), Val::Px(7.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
                        BorderColor(DIVIDER),
                    ))
                    .with_children(|b2| {
                        b2.spawn(txt(&b.name, &fonts.body, 13.0, TEXT));
                    });
                    row.spawn((
                        Button,
                        GarageAction::DeleteBuild(i),
                        Node { padding: UiRect::axes(Val::Px(9.0), Val::Px(7.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
                        BorderColor(DIVIDER),
                    ))
                    .with_children(|x| {
                        x.spawn(txt("×", &fonts.body, 16.0, TEXT_MUTED));
                    });
                });
        }
    });
}

// Zvýraznění vybraných dílů/auta/tabu. Běží každý snímek (pár entit).
fn sync_selection(
    loadout: Res<Loadout>,
    active: Res<ActiveTab>,
    catalog: Res<PartCatalog>,
    mut nodes: Query<(&GarageAction, Option<&mut BorderColor>, &mut BackgroundColor), With<Button>>,
    mut tags: Query<(&PartTag, &mut Node), Without<SuspTag>>,
    mut susp_tags: Query<(&SuspTag, &mut Node), Without<PartTag>>,
) {
    for (action, border, mut bg) in &mut nodes {
        let (selectable, on) = match *action {
            GarageAction::Part(i) => (true, catalog.0[i].data.is_selected(&loadout)),
            GarageAction::SuspPart { idx, front } => (
                true,
                matches!(&catalog.0[idx].data, PartData::Suspension(k) if loadout.susp(front).same_variant(*k)),
            ),
            GarageAction::Tab(tab) => (true, tab == active.0),
            GarageAction::Layout(layout) => (true, layout.label() == loadout.layout.label()),
            GarageAction::EngineCfg(cfg) => (true, cfg == loadout.engine_spec.config),
            GarageAction::DiffLock(locked) => (true, locked == (loadout.rear_diff.lock > 0.5)),
            GarageAction::Car(choice) => (true, choice == loadout.car),
            _ => (false, false),
        };
        if !selectable {
            continue;
        }
        match *action {
            // tab: spodní čára accent u aktivního
            GarageAction::Tab(_) => {
                if let Some(mut border) = border {
                    *border = BorderColor(if on { ACCENT } else { Color::NONE });
                }
            }
            // segmenty (pohon/diff): výplň accent, bez rámečku
            GarageAction::Layout(_) | GarageAction::DiffLock(_) | GarageAction::EngineCfg(_) => {
                *bg = BackgroundColor(if on { ACCENT } else { Color::NONE });
            }
            // karty (díly/auto): rámeček accent + slabá výplň
            _ => {
                if let Some(mut border) = border {
                    *border = BorderColor(if on { ACCENT } else { DIVIDER });
                }
                *bg = BackgroundColor(if on { Color::srgba(0.349, 0.502, 0.651, 0.12) } else { Color::NONE });
            }
        }
    }
    for (PartTag(i), mut node) in &mut tags {
        node.display = if catalog.0[*i].data.is_selected(&loadout) { Display::Flex } else { Display::None };
    }
    for (SuspTag(i, front), mut node) in &mut susp_tags {
        let on = matches!(&catalog.0[*i].data, PartData::Suspension(k) if loadout.susp(*front).same_variant(*k));
        node.display = if on { Display::Flex } else { Display::None };
    }
}

// Scroll kolečkem — obsah sloupce (hlavně tab Pohon) může být vyšší než okno.
fn garage_scroll(
    mut wheel: EventReader<bevy::input::mouse::MouseWheel>,
    mut q: Query<&mut ScrollPosition, With<Scrollable>>,
) {
    use bevy::input::mouse::MouseScrollUnit;
    let dy: f32 = wheel
        .read()
        .map(|e| match e.unit {
            MouseScrollUnit::Line => e.y * 24.0,
            MouseScrollUnit::Pixel => e.y,
        })
        .sum();
    if dy == 0.0 {
        return;
    }
    for mut sp in &mut q {
        sp.offset_y = (sp.offset_y - dy).max(0.0);
    }
}

// Přepínání viditelnosti sekcí podle aktivního tabu (Display-toggle vzor).
fn sync_tabs(active: Res<ActiveTab>, mut sections: Query<(&TabContent, &mut Node)>) {
    for (TabContent(tab), mut node) in &mut sections {
        node.display = if *tab == active.0 { Display::Flex } else { Display::None };
    }
}

// Spec tabulka + caption + KPI. Struktura + hmotnost/rozložení .veh se parsují jednou
// a cachují (Local); test car je deterministický z ChassisParams.
#[allow(clippy::type_complexity)]
fn update_specs(
    loadout: Res<Loadout>,
    chassis: Res<crate::vehicle::ChassisParams>,
    mut build_mass: ResMut<BuildMass>,
    mut cache: Local<Option<(usize, [String; 4], String, f32, f32)>>, // (i, counts, cap, mass, front)
    mut values: Query<(&SpecValue, &mut Text), (Without<CarCaption>, Without<KpiValue>, Without<PartDelta>)>,
    mut kpis: Query<(&KpiValue, &mut Text), (Without<CarCaption>, Without<SpecValue>, Without<PartDelta>)>,
    mut caption: Query<&mut Text, With<CarCaption>>,
) {
    // Struktura + hmotnost/rozložení podle výběru, s cache pro .veh.
    let (counts, cap, mass, front) = match loadout.car {
        CarChoice::TestCar => (
            ["18".into(), "64".into(), "—".into(), "4".into()],
            "Testovací vůz — kvádr + 4 kola, vestavěný".to_string(),
            crate::stats::test_car_mass(&chassis),
            0.5, // symetrický kvádr
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
                        let (mass, front) = crate::stats::veh_mass_and_front(&v, chassis.hub_mass);
                        (i, [v.nodes.len().to_string(), v.beams.len().to_string(), v.triangles.len().to_string(), v.wheels.len().to_string()], cap, mass, front)
                    }
                    None => (i, ["?".into(), "?".into(), "?".into(), "?".into()], "soubor nejde načíst".into(), 0.0, 0.5),
                });
            }
            let (_, counts, cap, mass, front) = cache.as_ref().unwrap();
            (counts.clone(), cap.clone(), *mass, *front)
        }
    };
    build_mass.mass = mass;
    build_mass.front = front;
    let diff = if loadout.rear_diff.lock > 0.5 { "zamčený" } else { "otevřený" };
    // Zavěšení: stejné vpředu/vzadu → jeden label, jinak "předek / zadek".
    let susp_label = if loadout.front_susp.same_variant(loadout.rear_susp) {
        loadout.front_susp.label().to_string()
    } else {
        format!("{} / {}", loadout.front_susp.label(), loadout.rear_susp.label())
    };
    let rows: [&str; 7] = [
        &counts[0], &counts[1], &counts[2], &counts[3],
        &susp_label, loadout.layout.label(), diff,
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

    // ── KPI z reálného buildu (stats.rs) ──
    let power = crate::stats::peak_power_kw(&loadout.engine);
    let t_0_100 = crate::stats::estimate_0_100_s(
        &loadout.engine, &loadout.gearbox, loadout.layout, mass, 0.34, 1.0, front,
    );
    let kpi_str: [String; 4] = [
        t_0_100.map_or("—".into(), |t| format!("{t:.1} s")),
        format!("{mass:.0} kg"),
        format!("{power:.0} kW"),
        format!("{:.0}/{:.0}", front * 100.0, (1.0 - front) * 100.0),
    ];
    for (KpiValue(i), mut text) in &mut kpis {
        if text.0 != kpi_str[*i] {
            text.0 = kpi_str[*i].clone();
        }
    }
}

// Delta na kartách: co díl změní proti aktuálnímu buildu. Motor/převodovka = měřené KPI
// (výkon + 0–100). Pneu/chladič = PŘÍMÁ charakteristika dílu (grip/opotřebení/chlazení) proti
// osazenému. Zavěšení mění handling, který zatím neměříme → bez čísel (až test-loop 4.2).
// Zelená = lepší, amber = horší/kompromis, prázdné = beze změny (nebo osazený díl).
fn update_part_deltas(
    loadout: Res<Loadout>,
    bm: Res<BuildMass>,
    catalog: Res<PartCatalog>,
    mut q: Query<(&PartDelta, &mut Text, &mut TextColor)>,
) {
    use crate::stats::{estimate_0_100_s, peak_power_kw};
    let cur_p = peak_power_kw(&loadout.engine);
    let cur_t = estimate_0_100_s(&loadout.engine, &loadout.gearbox, loadout.layout, bm.mass, 0.34, 1.0, bm.front);
    for (PartDelta(i), mut text, mut color) in &mut q {
        let (s, c) = match &catalog.0[*i].data {
            crate::parts::PartData::Gearbox(g) => {
                let t = estimate_0_100_s(&loadout.engine, g, loadout.layout, bm.mass, 0.34, 1.0, bm.front);
                delta_line(cur_t, t, cur_p, cur_p)
            }
            crate::parts::PartData::Tire(t) => tire_delta(&loadout.compound, t),
            crate::parts::PartData::Cooler(cl) => cooler_delta(&loadout.cooler, cl),
            _ => (String::new(), TEXT_MUTED),
        };
        if text.0 != s {
            text.0 = s;
        }
        if color.0 != c {
            color.0 = c;
        }
    }
}

// Barvy delta (sdílené s delta_line): zelená = lepší, amber = horší/kompromis.
const DELTA_GREEN: Color = Color::srgb(0.475, 0.788, 0.627);
const DELTA_AMBER: Color = Color::srgb(0.878, 0.659, 0.4);

// Pneu kompound: grip a opotřebení proti osazenému (přímo z dat, ne měřeno). Grip řídí barvu
// (víc gripu = zelená), opotřebení je jen info (měkká drží líp, ale sjede dřív = kompromis).
fn tire_delta(cur: &crate::vehicle::Compound, new: &crate::vehicle::Compound) -> (String, Color) {
    let mut parts: Vec<String> = Vec::new();
    let dg = (new.grip / cur.grip - 1.0) * 100.0;
    if dg.abs() >= 0.5 {
        parts.push(format!("grip {}{:.0} %", if dg > 0.0 { "+" } else { "−" }, dg.abs()));
    }
    let dw = (new.wear_rate / cur.wear_rate - 1.0) * 100.0;
    if dw.abs() >= 1.0 {
        parts.push(format!("opotř. {}{:.0} %", if dw > 0.0 { "+" } else { "−" }, dw.abs()));
    }
    if parts.is_empty() {
        return (String::new(), TEXT_MUTED);
    }
    let color = if dg > 0.5 { DELTA_GREEN } else { DELTA_AMBER };
    (parts.join(" · "), color)
}

// Chladič: kapacita odvodu tepla (h0 + h1·v při ~20 m/s) proti osazenému. Víc = zelená.
fn cooler_delta(cur: &crate::vehicle::Cooler, new: &crate::vehicle::Cooler) -> (String, Color) {
    let cap = |c: &crate::vehicle::Cooler| c.h0 + c.h1 * 20.0;
    let dc = (cap(new) / cap(cur) - 1.0) * 100.0;
    if dc.abs() < 1.0 {
        return (String::new(), TEXT_MUTED);
    }
    let color = if dc > 0.0 { DELTA_GREEN } else { DELTA_AMBER };
    (format!("chlazení {}{:.0} %", if dc > 0.0 { "+" } else { "−" }, dc.abs()), color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle::{Compound, Cooler};

    #[test]
    fn tire_cooler_deltas() {
        // stejný díl = prázdné (osazený)
        assert!(tire_delta(&Compound::default(), &Compound::default()).0.is_empty());
        assert!(cooler_delta(&Cooler::default(), &Cooler::default()).0.is_empty());
        // měkčí pneu: víc grip (zelená) + rychlejší opotřebení
        let soft = Compound { grip: 1.05, wear_rate: 3.0e-7, ..Compound::default() };
        let (s, c) = tire_delta(&Compound::default(), &soft);
        assert!(s.contains("grip +5") && s.contains("opotř. +50"), "{s}");
        assert_eq!(c, DELTA_GREEN);
        // tvrdší pneu: míň grip = amber
        let hard = Compound { grip: 0.97, ..Compound::default() };
        assert_eq!(tire_delta(&Compound::default(), &hard).1, DELTA_AMBER);
        // lepší chladič = zelená, kladné %
        let race = Cooler { h0: 110.0, h1: 50.0 };
        let (s, c) = cooler_delta(&Cooler::default(), &race);
        assert!(s.starts_with("chlazení +"), "{s}");
        assert_eq!(c, DELTA_GREEN);
    }
}

// Sestaví delta řetězec „0–100 −0.6 s · +90 kW" + barvu (zelená rychlejší / amber pomalejší
// / muted beze změny). Prázdné = aktuálně osazený díl (nebo bez měřitelného dopadu).
fn delta_line(cur_t: Option<f32>, new_t: Option<f32>, cur_p: f32, new_p: f32) -> (String, Color) {
    let mut parts: Vec<String> = Vec::new();
    let mut faster = 0.0_f32;
    if let (Some(a), Some(b)) = (cur_t, new_t) {
        let d = b - a;
        if d.abs() >= 0.05 {
            parts.push(format!("0–100 {}{:.1} s", if d < 0.0 { "−" } else { "+" }, d.abs()));
            faster = -d;
        }
    }
    let dp = new_p - cur_p;
    if dp.abs() >= 1.0 {
        parts.push(format!("{}{:.0} kW", if dp > 0.0 { "+" } else { "−" }, dp.abs()));
        if faster == 0.0 {
            faster = dp;
        }
    }
    if parts.is_empty() {
        return (String::new(), TEXT_MUTED);
    }
    let color = if faster > 0.0 { DELTA_GREEN } else { DELTA_AMBER };
    (parts.join(" · "), color)
}

// Tažení sliderů motoru: track je Button + RelativeCursorPosition. Při držení mapuje
// vodorovnou pozici kurzoru (0..1) na hodnotu parametru a přegeneruje engine z EngineSpec.
fn drag_sliders(
    tracks: Query<(&SliderTrack, &Interaction, &bevy::ui::RelativeCursorPosition)>,
    mut loadout: ResMut<Loadout>,
) {
    let mut engine_changed = false;
    for (SliderTrack(kind), interaction, rel) in &tracks {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(pos) = rel.normalized else { continue };
        let (min, max) = kind.range();
        kind.set(&mut loadout, min + pos.x.clamp(0.0, 1.0) * (max - min));
        engine_changed |= kind.is_engine();
    }
    if engine_changed {
        loadout.engine = loadout.engine_spec.build(); // dyno/KPI se přepočtou
    }
}

// Vizuál sliderů z aktuálního buildu: šířka výplně + textová hodnota.
fn sync_sliders(
    loadout: Res<Loadout>,
    mut fills: Query<(&SliderFill, &mut Node)>,
    mut labels: Query<(&SliderLabel, &mut Text)>,
) {
    for (SliderFill(k), mut node) in &mut fills {
        let (min, max) = k.range();
        node.width = Val::Percent((((k.get(&loadout) - min) / (max - min)) * 100.0).clamp(0.0, 100.0));
    }
    for (SliderLabel(k), mut text) in &mut labels {
        let s = k.fmt(k.get(&loadout));
        if text.0 != s {
            text.0 = s;
        }
    }
}
