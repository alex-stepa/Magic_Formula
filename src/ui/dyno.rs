// Dyno graf: křivka momentu (Nm) a výkonu (kW) vybraného motoru přes otáčky.
// Nula nové fyziky — rasterizuje čistou engine_torque do obrázku, ten se ukáže
// v garáži jako ImageNode. Překreslí se při změně motoru (Changed<Loadout>).

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::vehicle::{engine_torque, Engine, Loadout};

const W: usize = 480;
const H: usize = 220;
const PAD: usize = 8;
// Pevné škály, ať jsou motory porovnatelné mezi sebou.
const RPM_MAX: f32 = 7500.0;
const NM_MAX: f32 = 350.0;
const KW_MAX: f32 = 350.0;

// Barvy (RGBA) z palety hry.
const BG: [u8; 4] = [29, 45, 61, 255];
const GRID: [u8; 4] = [45, 62, 78, 255];
const TORQUE: [u8; 4] = [89, 128, 166, 255]; // ACCENT
const POWER: [u8; 4] = [181, 217, 253, 255]; // ACCENT_300

#[derive(Resource)]
pub struct DynoImage(pub Handle<Image>);

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, init_dyno)
        .add_systems(Update, redraw_dyno.run_if(resource_changed::<Loadout>));
}

fn init_dyno(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let img = Image::new(
        Extent3d { width: W as u32, height: H as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; W * H * 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    commands.insert_resource(DynoImage(images.add(img)));
}

// Překreslí dyno při změně buildu (motor). resource_changed fírne i při jiné změně
// Loadoutu — překreslení je levné (480×220), nevadí.
fn redraw_dyno(dyno: Res<DynoImage>, loadout: Res<Loadout>, mut images: ResMut<Assets<Image>>) {
    if let Some(img) = images.get_mut(&dyno.0) {
        img.data = render(&loadout.engine);
    }
}

fn render(engine: &Engine) -> Vec<u8> {
    let mut buf = vec![0u8; W * H * 4];
    let put = |buf: &mut [u8], x: usize, y: usize, c: [u8; 4]| {
        if x < W && y < H {
            let i = (y * W + x) * 4;
            buf[i..i + 4].copy_from_slice(&c);
        }
    };
    // pozadí
    for p in buf.chunks_exact_mut(4) {
        p.copy_from_slice(&BG);
    }
    // mřížka: 4 svislé + 4 vodorovné
    for k in 1..4 {
        let gx = PAD + k * (W - 2 * PAD) / 4;
        for y in PAD..H - PAD {
            put(&mut buf, gx, y, GRID);
        }
        let gy = PAD + k * (H - 2 * PAD) / 4;
        for x in PAD..W - PAD {
            put(&mut buf, x, gy, GRID);
        }
    }

    // vzorkuj rpm, převeď na body (moment, výkon)
    let px = |rpm: f32| PAD + ((rpm / RPM_MAX).clamp(0.0, 1.0) * (W - 2 * PAD) as f32) as usize;
    let py = |val: f32, max: f32| {
        let f = (val / max).clamp(0.0, 1.0);
        H - PAD - (f * (H - 2 * PAD) as f32) as usize
    };
    let mut last: Option<(usize, usize, usize, usize)> = None; // (tx,ty,px,py)
    let mut rpm = 0.0;
    while rpm <= RPM_MAX {
        let t = engine_torque(rpm, engine);
        let p = t * rpm * std::f32::consts::TAU / 60.0 / 1000.0; // kW
        let (tx, ty) = (px(rpm), py(t, NM_MAX));
        let (wx, wy) = (px(rpm), py(p, KW_MAX));
        if let Some((ltx, lty, lwx, lwy)) = last {
            line(&mut buf, ltx, lty, tx, ty, TORQUE, put);
            line(&mut buf, lwx, lwy, wx, wy, POWER, put);
        }
        last = Some((tx, ty, wx, wy));
        rpm += 50.0;
    }
    buf
}

// DDA čára tloušťky 2 px (svisle) — dost pro křivku v malém grafu.
fn line(buf: &mut [u8], x0: usize, y0: usize, x1: usize, y1: usize, c: [u8; 4], put: impl Fn(&mut [u8], usize, usize, [u8; 4]) + Copy) {
    let (x0, y0, x1, y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
    for s in 0..=steps {
        let t = s as f32 / steps as f32;
        let x = (x0 as f32 + (x1 - x0) as f32 * t) as usize;
        let y = (y0 as f32 + (y1 - y0) as f32 * t) as usize;
        put(buf, x, y, c);
        put(buf, x, y + 1, c);
    }
}
