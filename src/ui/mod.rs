// ui — obrazovky hry podle návrhu (Game magic formula UI.zip).
// Stavový automat: Title → Garage → Game (+ Paused jako resource, ne stav).
// Fyzika/solver se tu NEřeší — UI jen čte/píše resources (CarBuild, Drivetrain,
// SelectedMap) a přepíná stavy.

use bevy::prelude::*;

use crate::world::MapKind;

pub mod title;
pub mod garage;
pub mod hud;

// ── Stavy ────────────────────────────────────────────────────────────────────

#[derive(States, Default, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum AppState {
    #[default]
    Title,
    Garage,
    Game,
}

// Pauza jen uvnitř Game (Esc). Resource místo stavu: overlay leží nad HUDem
// a gameplay systémy se jen vypnou run_if podmínkou.
#[derive(Resource, Default)]
pub struct Paused(pub bool);

// Vybraná mapa (title picker). Aplikuje se hned při kliknutí (rebuild terénu).
#[derive(Resource)]
pub struct SelectedMap(pub MapKind);

impl Default for SelectedMap {
    fn default() -> Self {
        Self(MapKind::TestPad)
    }
}

// Run condition: jede se (Game a není pauza) — gate pro gameplay + solver.
pub fn in_game_unpaused(state: Res<State<AppState>>, paused: Res<Paused>) -> bool {
    *state.get() == AppState::Game && !paused.0
}

// ── Design tokens (z prototypu) ──────────────────────────────────────────────

const fn hex(r: u8, g: u8, b: u8) -> Color {
    Color::srgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

pub const BG: Color = hex(0x1d, 0x2d, 0x3d);
pub const TEXT: Color = hex(0xf2, 0xf2, 0xf3);
pub const ACCENT: Color = hex(0x59, 0x80, 0xa6);
pub const ACCENT_300: Color = hex(0xb5, 0xd9, 0xfd);
pub const ACCENT_200: Color = hex(0xd6, 0xeb, 0xff); // "červená" zóna dashů (světlá)
pub const DIVIDER: Color = Color::srgba(0.949, 0.949, 0.953, 0.22);
pub const TEXT_MUTED: Color = Color::srgba(0.949, 0.949, 0.953, 0.55);

// Fonty návrhu (assets/fonts, OFL).
#[derive(Resource)]
pub struct UiFonts {
    pub body: Handle<Font>,    // Barlow
    pub heading: Handle<Font>, // Barlow Condensed SemiBold
    pub mono: Handle<Font>,    // Share Tech Mono (dash čísla)
}


// ── Pomocníci ────────────────────────────────────────────────────────────────

// Text bundle zkratky (Bevy 0.15: Text + TextFont + TextColor).
pub fn txt(s: impl Into<String>, font: &Handle<Font>, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(s),
        TextFont { font: font.clone(), font_size: size, ..default() },
        TextColor(color),
    )
}

// Smaže stojící auto a spawne nové podle CarBuild (Volná jízda / Vyjet z garáže).
#[allow(clippy::too_many_arguments)] // parametry = to, co potřebuje spawn_solver_car
pub fn respawn_car(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    softbody: &mut crate::solver::SoftBody,
    drivetrain: &mut crate::vehicle::Drivetrain,
    build: &crate::vehicle::CarBuild,
    terrain: &crate::world::Terrain,
    spawned: &Query<Entity, With<crate::vehicle::Spawned>>,
) {
    crate::clear_spawned(commands, spawned, softbody);
    let ground_y = terrain.0.height(0.0, 0.0);
    match build.car {
        crate::vehicle::CarChoice::Veh(i) if crate::available_vehs().get(i).is_some() => {
            let path = crate::available_vehs()[i].to_string_lossy().into_owned();
            crate::vehicle::spawn_softbody(commands, meshes, materials, softbody, drivetrain, build, &path, ground_y);
        }
        _ => crate::vehicle::spawn_solver_car(commands, meshes, materials, softbody, drivetrain, build, ground_y),
    }
}

// ── Plugin ───────────────────────────────────────────────────────────────────

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // Fonty rovnou tady — OnEnter(Title) běží dřív než PreStartup commands,
        // resource musí existovat před prvním state transition.
        let assets = app.world().resource::<AssetServer>();
        let fonts = UiFonts {
            body: assets.load("fonts/Barlow-Regular.ttf"),
            heading: assets.load("fonts/BarlowCondensed-SemiBold.ttf"),
            mono: assets.load("fonts/ShareTechMono-Regular.ttf"),
        };
        app.insert_resource(fonts)
            .init_state::<AppState>()
            .enable_state_scoped_entities::<AppState>()
            .init_resource::<Paused>()
            .init_resource::<SelectedMap>()
            .add_plugins((title::plugin, garage::plugin, hud::plugin));
    }
}
