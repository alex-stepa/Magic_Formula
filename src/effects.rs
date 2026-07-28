// Vizuální efekty pneu: skid marks (tmavé stopy na zemi při skluzu) + kouř (přehřátá
// pneu + skluz). POZOR: headless jsem NEOVĚŘIL — vzhled/tuning (velikosti, barvy, rate,
// billboard) na jízdním testu. Cíl je robustní placeholder, ne finální look.

use bevy::prelude::*;

use crate::solver::SoftBody;
use crate::ui::AppState;
use crate::world::Terrain;

pub struct TireFxPlugin;

impl Plugin for TireFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MarkPool>()
            .add_systems(Startup, setup_fx)
            .add_systems(OnEnter(AppState::Game), reset_marks) // stopy jsou StateScoped(Game) → pool resetuj
            .add_systems(
                Update,
                (tire_marks, smoke_spawn, smoke_update).run_if(crate::ui::in_game_unpaused),
            );
    }
}

// Stopy se při odchodu ze hry despawnou (StateScoped) — pool drží mrtvé entity, vyčisti.
fn reset_marks(mut pool: ResMut<MarkPool>) {
    *pool = MarkPool::default();
}

// Sdílené assety (mesh + materiál stopy; kouř má materiál per-puff kvůli fade).
#[derive(Resource)]
struct FxAssets {
    mark_mesh: Handle<Mesh>,
    mark_mat: Handle<StandardMaterial>,
    smoke_mesh: Handle<Mesh>,
}

// Ring-buffer stop: staré se recyklují (přesune se Transform), ať nerostou do nekonečna.
#[derive(Resource, Default)]
struct MarkPool {
    marks: Vec<Entity>,
    next: usize,
    last_pos: [Vec3; 8], // poslední stopa per kolo — rozestup, ať netvoříme 60 stop/s
}

#[derive(Component)]
struct TireMark;

#[derive(Component)]
struct Smoke {
    age: f32,
    life: f32,
    rise: f32,
}

const MARK_CAP: usize = 500;
const MARK_SPACING: f32 = 0.10; // m mezi stopami (hustší = výraznější souvislá stopa)
const SKID_MARK: f32 = 1.35; // combined slip (κ/α onset) nad kterým kreslíme stopu
// Kouř: PŘÍSNÝ — jen výrazně přehřátá pneu (nad TIRE_HOT) při silném skluzu. Vzácný, ale
// dramatický. ponytail: hodnoty blind-tuned (headless nerozjedu), doladí autor po cargo run.
const SMOKE_TEMP: f32 = 165.0; // °C práh kouře (TIRE_HOT je 140 — tohle je „hodně za")
const SMOKE_SLIP: f32 = 1.6; // combined slip práh kouře

fn setup_fx(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mark_mesh = meshes.add(Cuboid::new(0.22, 0.02, 0.4)); // plochý slab na zem
    let mark_mat = mats.add(StandardMaterial {
        base_color: Color::srgba(0.03, 0.03, 0.04, 0.88), // tmavší + míň průhledné = výraznější
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let smoke_mesh = meshes.add(Sphere::new(0.25));
    commands.insert_resource(FxAssets { mark_mesh, mark_mat, smoke_mesh });
}

// Combined skluz kola (podélný κ + boční α vůči onset prahům) — >1 = za onsetem.
fn wheel_slip(t: &crate::solver::WheelTelem) -> f32 {
    ((t.slip_ratio / 0.22).powi(2) + (t.slip_angle / 0.20).powi(2)).sqrt()
}

fn tire_marks(
    mut commands: Commands,
    sb: Res<SoftBody>,
    terrain: Res<Terrain>,
    fx: Res<FxAssets>,
    mut pool: ResMut<MarkPool>,
    mut tf: Query<&mut Transform, With<TireMark>>,
) {
    for (i, w) in sb.wheels.iter().enumerate().take(8) {
        if w.telem.fz <= 0.0 || wheel_slip(&w.telem) < SKID_MARK {
            continue;
        }
        let p = sb.pos[w.hub];
        let pos = Vec3::new(p.x, terrain.0.height(p.x, p.z) + 0.02, p.z);
        if pos.distance(pool.last_pos[i]) < MARK_SPACING {
            continue; // moc blízko poslední stopy tohoto kola
        }
        pool.last_pos[i] = pos;
        let t = Transform::from_translation(pos);
        if pool.marks.len() < MARK_CAP {
            let e = commands
                .spawn((Mesh3d(fx.mark_mesh.clone()), MeshMaterial3d(fx.mark_mat.clone()), t, TireMark, StateScoped(AppState::Game)))
                .id();
            pool.marks.push(e);
        } else {
            let idx = pool.next % MARK_CAP;
            pool.next = pool.next.wrapping_add(1);
            if let Ok(mut tr) = tf.get_mut(pool.marks[idx]) {
                *tr = t; // recykluj nejstarší stopu na nové místo
            }
        }
    }
}

fn smoke_spawn(
    mut commands: Commands,
    sb: Res<SoftBody>,
    terrain: Res<Terrain>,
    fx: Res<FxAssets>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    mut acc: Local<f32>,
) {
    // Rate cap — bez něj by to sypalo puff každý snímek (churn materiálů + perf).
    *acc += time.delta_secs();
    if *acc < 0.06 {
        return;
    }
    *acc = 0.0;
    for w in sb.wheels.iter() {
        // Kouř jen z HODNĚ přehřáté pneu, která zároveň silně klouže (extrémní burnout/drift).
        if w.telem.fz <= 0.0 || w.thermal.tire_surf_c < SMOKE_TEMP || wheel_slip(&w.telem) < SMOKE_SLIP {
            continue;
        }
        let p = sb.pos[w.hub];
        let pos = Vec3::new(p.x, terrain.0.height(p.x, p.z) + 0.1, p.z);
        let mat = mats.add(StandardMaterial {
            base_color: Color::srgba(0.72, 0.72, 0.74, 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        commands.spawn((
            Mesh3d(fx.smoke_mesh.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos).with_scale(Vec3::splat(0.5)),
            Smoke { age: 0.0, life: 2.0, rise: 1.4 }, // větší + déle žije (vzácný = ať je vidět)
            StateScoped(AppState::Game),
        ));
    }
}

fn smoke_update(
    mut commands: Commands,
    time: Res<Time>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut Transform, &mut Smoke, &MeshMaterial3d<StandardMaterial>)>,
) {
    let dt = time.delta_secs();
    for (e, mut tr, mut s, math) in &mut q {
        s.age += dt;
        if s.age >= s.life {
            commands.entity(e).despawn();
            continue;
        }
        let f = s.age / s.life; // 0..1
        tr.translation.y += s.rise * dt; // stoupá
        tr.scale = Vec3::splat(0.5 + f * 1.8); // roste do velkého oblaku
        if let Some(m) = mats.get_mut(&math.0) {
            m.base_color = m.base_color.with_alpha(0.55 * (1.0 - f)); // slábne
        }
    }
}
