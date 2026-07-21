// Živý 3D náhled auta v garáži přes render-to-texture: dedikovaná kamera renderuje
// preview model (RenderLayer 1) do Image, ta se ukáže jako ImageNode ve středu garáže.
// Oddělené od herní kamery/terénu — čistý „otočný stůl". Model reflektuje výběr auta:
// testovací = kvádr + 4 kola, .veh = vlastní trojúhelníkový mesh (staticky, vycentrovaný).

use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::RenderLayers;

use crate::ui::{AppState, BG};
use crate::vehicle::{veh_pos, CarChoice, Loadout};

// Render vrstva náhledu — jen preview kamera + preview model, herní kamera je nevidí.
const PREVIEW_LAYER: usize = 1;

#[derive(Resource)]
pub struct PreviewImage(pub Handle<Image>);

// Kořen otočného modelu — rotuje se každý snímek; přestavuje se při změně auta.
#[derive(Component)]
struct PreviewCar;

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, init_preview_image)
        .add_systems(OnEnter(AppState::Garage), spawn_preview_rig)
        .add_systems(Update, (refresh_model, rotate_preview).run_if(in_state(AppState::Garage)));
}

// Render-target textura jednou při startu (handle přežije přepínání stavů). Vyšší
// rozlišení = ostřejší náhled po roztažení do panelu.
fn init_preview_image(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = Extent3d { width: 1600, height: 1000, depth_or_array_layers: 1 };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    commands.insert_resource(PreviewImage(images.add(image)));
}

// Kamera + světlo náhledu (persistentní po dobu garáže). Model spawnne refresh_model.
fn spawn_preview_rig(mut commands: Commands, preview: Res<PreviewImage>) {
    let layer = RenderLayers::layer(PREVIEW_LAYER);
    commands.spawn((
        Camera3d::default(),
        Camera {
            target: RenderTarget::Image(preview.0.clone()),
            clear_color: ClearColorConfig::Custom(BG),
            order: -1, // renderuj před herní kamerou
            ..default()
        },
        Msaa::Sample4, // vyhlazení hran náhledu
        Transform::from_xyz(3.4, 1.9, 4.8).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
        layer.clone(),
        StateScoped(AppState::Garage),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 7000.0, shadows_enabled: false, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, -0.6, 0.0)),
        layer,
        StateScoped(AppState::Garage),
    ));
}

// Přestaví model při změně vybraného auta (Local drží poslední). Reflektuje reálný build.
fn refresh_model(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    loadout: Res<Loadout>,
    old: Query<Entity, With<PreviewCar>>,
    mut last: Local<Option<CarChoice>>,
) {
    if *last == Some(loadout.car) && !old.is_empty() {
        return; // beze změny a model existuje
    }
    *last = Some(loadout.car);
    for e in &old {
        commands.entity(e).despawn_recursive();
    }
    spawn_model(&mut commands, &mut meshes, &mut materials, loadout.car);
}

fn spawn_model(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, car: CarChoice) {
    let layer = RenderLayers::layer(PREVIEW_LAYER);
    let root = commands
        .spawn((Transform::default(), Visibility::default(), PreviewCar, layer.clone(), StateScoped(AppState::Garage)))
        .id();

    let wheel_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.06, 0.06, 0.07), ..default() });

    // .veh: vlastní mesh karoserie z trojúhelníků + kola z bounding boxu (axle nody mají
    // lokální souřadnice, tak polohu kol odhadneme z rohů karoserie). Fallback = kvádr.
    let veh = match car {
        CarChoice::Veh(i) => crate::available_vehs()
            .get(i)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<crate::veh::Vehicle>(&s).ok())
            .and_then(|v| build_veh_model(&v)),
        CarChoice::TestCar => None,
    };

    if let Some((mesh, wheels)) = veh {
        let mat = materials.add(StandardMaterial { base_color: Color::srgb(0.72, 0.74, 0.78), perceptual_roughness: 0.5, ..default() });
        commands.entity(root).with_children(|c| {
            c.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat), Transform::default(), layer.clone()));
            for (pos, radius) in wheels {
                c.spawn((
                    Mesh3d(meshes.add(Cylinder::new(radius, 0.18))),
                    MeshMaterial3d(wheel_mat.clone()),
                    Transform::from_translation(pos).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                    layer.clone(),
                ));
            }
        });
    } else {
        // kvádr karoserie + 4 kola (rozměry ze spawn_solver_car)
        let body_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.35, 0.5, 0.65), perceptual_roughness: 0.5, ..default() });
        let body_mesh = meshes.add(Cuboid::new(1.4, 0.5, 3.6));
        let wheel_mesh = meshes.add(Cylinder::new(0.34, 0.22));
        commands.entity(root).with_children(|c| {
            c.spawn((Mesh3d(body_mesh), MeshMaterial3d(body_mat), Transform::from_xyz(0.0, 0.15, 0.0), layer.clone()));
            for (sx, sz) in [(-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                c.spawn((
                    Mesh3d(wheel_mesh.clone()),
                    MeshMaterial3d(wheel_mat.clone()),
                    Transform::from_xyz(sx * 0.8, -0.25, sz * 1.4).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                    layer.clone(),
                ));
            }
        });
    }
}

// Statický model z .veh: mesh karoserie (vycentrovaný) + kola na SKUTEČNÝCH pozicích
// z jbeam dat (stejná logika jako spawn_softbody: střed kola z suspension anchors +
// wheel.center/nodeOffset). Vrací (mesh, kola[(pozice, poloměr)]).
type VehModel = (Mesh, Vec<(Vec3, f32)>);
fn build_veh_model(vehicle: &crate::veh::Vehicle) -> Option<VehModel> {
    let axle: std::collections::HashSet<&str> =
        vehicle.wheels.iter().flat_map(|w| [w.node1.as_str(), w.node2.as_str()]).collect();

    // pozice nodů karoserie (game space) — vrcholy meshe i úchyty zavěšení
    let node_pos: std::collections::HashMap<&str, Vec3> = vehicle
        .nodes
        .iter()
        .filter(|n| !axle.contains(n.id.as_str()))
        .map(|n| (n.id.as_str(), veh_pos(n.pos)))
        .collect();

    let mut vert_of: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    let mut pos: Vec<Vec3> = Vec::new();
    for n in &vehicle.nodes {
        if let Some(&p) = node_pos.get(n.id.as_str()) {
            vert_of.insert(n.id.as_str(), pos.len() as u32);
            pos.push(p);
        }
    }
    let mut indices: Vec<u32> = Vec::new();
    for t in &vehicle.triangles {
        if let (Some(&a), Some(&b), Some(&c)) = (vert_of.get(t.a.as_str()), vert_of.get(t.b.as_str()), vert_of.get(t.c.as_str())) {
            indices.extend([a, b, c]);
        }
    }
    if indices.is_empty() {
        return None;
    }
    let center = pos.iter().copied().sum::<Vec3>() / pos.len() as f32;
    let verts: Vec<[f32; 3]> = pos.iter().map(|p| (*p - center).to_array()).collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, verts);
    mesh.insert_indices(Indices::U32(indices));
    mesh.compute_normals();

    // sousedství přes beamy (podle id) — axle nody v meshi nejsou, jejich beamy vedou
    // k suspension nodům (úchytům), co globální pozici mají
    let mut neighbors: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for b in &vehicle.beams {
        neighbors.entry(b.a.as_str()).or_default().push(b.b.as_str());
        neighbors.entry(b.b.as_str()).or_default().push(b.a.as_str());
    }

    // pro každé kolo: úchyty = existující sousedi axle nodů → jejich průměr = hrubý hub;
    // wheel.center dá přesné y/z + půlrozchod (|x|), znaménko strany z úchytů.
    let mut wheels: Vec<(Vec3, f32)> = Vec::new();
    for w in &vehicle.wheels {
        let mut anchors: Vec<Vec3> = Vec::new();
        for ax in [w.node1.as_str(), w.node2.as_str()] {
            for &nb in neighbors.get(ax).map(|v| v.as_slice()).unwrap_or(&[]) {
                if let Some(&p) = node_pos.get(nb) {
                    if !anchors.contains(&p) {
                        anchors.push(p);
                    }
                }
            }
        }
        if anchors.len() < 2 {
            continue;
        }
        let mut hub = anchors.iter().copied().sum::<Vec3>() / anchors.len() as f32;
        let mut half_track = 0.0;
        if let Some(c) = w.center {
            let p = veh_pos(c);
            hub.y = p.y;
            hub.z = p.z;
            half_track = c[0].abs();
        }
        hub.x = hub.x.signum() * if half_track > 0.0 { half_track } else { 0.80 };
        let radius = if w.radius > 0.0 { w.radius } else { w.center.map(|c| veh_pos(c).y).unwrap_or(0.34) };
        wheels.push((hub - center, radius));
    }

    Some((mesh, wheels))
}

fn rotate_preview(time: Res<Time>, mut q: Query<&mut Transform, With<PreviewCar>>) {
    for mut t in &mut q {
        t.rotate_y(0.5 * time.delta_secs());
    }
}
