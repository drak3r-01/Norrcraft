//! Aperçus visuels (hors écran, lavapipe) d'un paysage plus réaliste que le monde de démonstration
//! de `render.rs` : collines, lac, bassin de lave, arbres, herbes hautes, mobs. Ignoré par
//! défaut ; écrit `renders/preview_*.png` pour inspection :
//! `NB_GFX_REQUIRE_GPU=1 crates/nb-gfx/scripts/vk-docker.sh test -p nb-gfx --test preview -- --ignored`

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use glam::Vec3;
use nb_data::BlockRegistry;
use nb_gfx::{
    Camera, ChunkPos, EyeFluid, FrameInput, GfxSettings, GpuContext, MobInstance, MobModelDesc,
    OffscreenRenderer, SceneAssets, SkyClock,
};
use nb_mesh::{mesh_chunk, BlockTable, ChunkVoxels, Neighborhood, UniformBiome};

const W: u32 = 960;
const H: u32 = 540;
const RADIUS: i32 = 13;
const LAVA_SITE: (i32, i32) = (-26, 34);

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn hash(x: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (z as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h
}

fn height_at(x: i32, z: i32) -> i32 {
    let (fx, fz) = (x as f32, z as f32);
    let mut h = 65.0
        + 4.0 * (fx * 0.03).sin() * (fz * 0.025).cos()
        + 2.5 * ((fx + fz) * 0.06).sin()
        + 1.2 * (fx * 0.2).sin() * (fz * 0.17).cos();
    // Massif lointain au nord.
    let (mx, mz) = (fx + 10.0, fz + 150.0);
    h += 38.0 * (-(mx * mx / 5200.0 + mz * mz / 1800.0)).exp();
    h += 14.0 * (-((fx - 90.0).powi(2) / 3000.0 + (fz + 95.0).powi(2) / 2500.0)).exp();
    // Lac.
    let (dx, dz) = (fx - 45.0, fz + 40.0);
    h -= 14.0 * (-(dx * dx + dz * dz) / 700.0).exp();
    h as i32
}

fn lava_level() -> i32 {
    height_at(LAVA_SITE.0, LAVA_SITE.1) - 1
}

fn build_world(r: &mut OffscreenRenderer) {
    let reg = BlockRegistry::builtin();
    let ids = *reg.ids();
    let atlas = r.scene.block_atlas().clone();
    let table = BlockTable::from_registry(&reg, |n| atlas.layer(n));
    let lava_y = lava_level();
    let block = |x: i32, y: i32, z: i32| -> u8 {
        let in_lava = {
            let (dx, dz) = (x - LAVA_SITE.0, z - LAVA_SITE.1);
            dx * dx + dz * dz <= 20
        };
        if in_lava {
            return if y <= lava_y - 1 {
                ids.stone
            } else if y == lava_y {
                ids.lava
            } else {
                0
            };
        }
        let h = height_at(x, z);
        if y < h - 3 {
            ids.stone
        } else if y < h {
            ids.dirt
        } else if y == h {
            if h <= 62 {
                ids.sand
            } else {
                ids.grass
            }
        } else if y <= 62 {
            ids.water
        } else if y == h + 1 && h > 62 {
            match hash(x, z) % 100 {
                0..=45 => ids.tall_grass,
                46..=48 => ids.flower_red,
                49..=51 => ids.flower_yellow,
                52..=53 => ids.flower_white,
                _ => 0,
            }
        } else {
            // Arbres : un par maille de 9×9.
            let (cx, cz) = (x.div_euclid(9), z.div_euclid(9));
            let (tx, tz) = (cx * 9 + 2 + (hash(cx, cz) % 5) as i32, cz * 9 + 2 + (hash(cz, cx) % 5) as i32);
            let th = height_at(tx, tz);
            let lava_near = {
                let (dx, dz) = (tx - LAVA_SITE.0, tz - LAVA_SITE.1);
                dx * dx + dz * dz < 64
            };
            let near_cam = [(20, -5), (88, -55), (-26, 42), (0, 40), (4, 14)]
                .iter()
                .any(|&(cx, cz)| (tx - cx) * (tx - cx) + (tz - cz) * (tz - cz) < 100);
            if th <= 63 || lava_near || near_cam || hash(cx + 7, cz + 3) % 3 == 0 {
                return 0;
            }
            let (dx, dz) = (x - tx, z - tz);
            if dx == 0 && dz == 0 && y > th && y <= th + 5 {
                ids.wood
            } else if (th + 4..th + 8).contains(&y) && dx.abs() <= 2 && dz.abs() <= 2 {
                ids.leaves
            } else {
                0
            }
        }
    };
    let mut voxels = HashMap::new();
    for cz in -RADIUS - 1..=RADIUS + 1 {
        for cx in -RADIUS - 1..=RADIUS + 1 {
            let mut v = ChunkVoxels::filled(0);
            for lx in 0..16usize {
                for lz in 0..16usize {
                    for y in 0..110usize {
                        let id = block(cx * 16 + lx as i32, y as i32, cz * 16 + lz as i32);
                        if id != 0 {
                            v.set(lx, y, lz, id);
                        }
                    }
                }
            }
            voxels.insert((cx, cz), v);
        }
    }
    for cz in -RADIUS..=RADIUS {
        for cx in -RADIUS..=RADIUS {
            let n: [Option<&ChunkVoxels>; 9] = std::array::from_fn(|i| {
                voxels.get(&(cx + (i % 3) as i32 - 1, cz + (i / 3) as i32 - 1))
            });
            let mesh = mesh_chunk(
                ChunkPos::new(cx, cz),
                &Neighborhood::from_voxels(n),
                &table,
                &UniformBiome(0),
            );
            r.scene
                .upload_chunk_mesh(ChunkPos::new(cx, cz), &mesh.vertices)
                .unwrap();
        }
    }
}

struct Shot {
    name: &'static str,
    pos: (f32, f32),
    eye: f32,
    pitch: f32,
    yaw: f32,
    time: f32,
    mobs: bool,
}

#[test]
#[ignore]
fn preview_scenes() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => Arc::new(c),
        Err(e) => panic!("Vulkan indisponible : {e}"),
    };
    let reg = nb_data::MobRegistry::builtin();
    let descs = MobModelDesc::from_registry(&reg);
    let (assets, _) = SceneAssets::load(&crate_dir().join("../../resource_packs"), "Default", &descs);
    let mut r = OffscreenRenderer::new(ctx, assets, W, H, &GfxSettings::default()).unwrap();
    build_world(&mut r);

    let only = std::env::var("NB_PREVIEW_ONLY").ok();
    let lava_y = lava_level() as f32;
    let shots = [
        Shot { name: "hills_noon", pos: (0.0, 40.0), eye: 3.0, pitch: 3.0, yaw: 15.0, time: 900.0, mobs: false },
        Shot { name: "hills_morning", pos: (0.0, 40.0), eye: 3.0, pitch: 2.0, yaw: -35.0, time: 700.0, mobs: false },
        Shot { name: "lake_noon", pos: (20.0, -5.0), eye: 5.0, pitch: 14.0, yaw: 30.0, time: 850.0, mobs: false },
        Shot { name: "lake_sun", pos: (88.0, -55.0), eye: 4.0, pitch: 6.0, yaw: -88.0, time: 1050.0, mobs: false },
        Shot { name: "lava_dusk", pos: (-26.0, 42.0), eye: 3.0, pitch: 28.0, yaw: 0.0, time: 1140.0, mobs: false },
        Shot { name: "lava_night", pos: (-26.0, 42.0), eye: 3.0, pitch: 25.0, yaw: 0.0, time: 300.0, mobs: true },
        Shot { name: "mobs_noon", pos: (4.0, 14.0), eye: 2.0, pitch: 4.0, yaw: 0.0, time: 900.0, mobs: true },
        Shot { name: "mobs_night", pos: (4.0, 14.0), eye: 2.0, pitch: 6.0, yaw: 0.0, time: 300.0, mobs: true },
    ];
    let mob_list: Vec<(&'static str, f32, f32, f32)> = vec![
        ("elk", 1.0, 6.0, 0.6),
        ("ostrich", 5.0, 8.0, -0.4),
        ("camel", -3.0, 3.0, 1.0),
        ("giraffe", 9.0, 2.0, 2.4),
        ("wild_boar", 3.0, 11.0, 0.2),
        ("zebra", -26.5 + 7.0, 33.5, 0.0),
    ];
    for s in shots {
        if only.as_deref().is_some_and(|o| o != s.name) {
            continue;
        }
        let ground = height_at(s.pos.0 as i32, s.pos.1 as i32).max(62) as f32;
        let cam = Camera {
            position: Vec3::new(s.pos.0, ground + 1.0 + s.eye, s.pos.1),
            pitch_deg: s.pitch,
            yaw_deg: s.yaw,
            ..Camera::default()
        };
        let mut clock = SkyClock::new();
        clock.set_time(s.time);
        let mobs: Vec<MobInstance<'_>> = if s.mobs {
            mob_list
                .iter()
                .map(|&(id, x, z, yaw)| {
                    let c = reg.get(id).unwrap();
                    let (wx, wz) = if id == "zebra" { (x, z) } else { (s.pos.0 - 3.0 + x, s.pos.1 - 12.0 + z) };
                    let y = (height_at(wx as i32, wz as i32).max(62) + 1) as f32;
                    let y = if id == "zebra" { lava_y + 1.0 } else { y };
                    MobInstance {
                        model_id: id,
                        position: Vec3::new(wx, y, wz),
                        yaw_rad: yaw,
                        pitch_rad: 0.0,
                        missing_scale: Vec3::new(c.width_x, c.height, c.width_z),
                        light: 1.0,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let input = FrameInput {
            camera: &cam,
            time_seconds: 3.0,
            sky_clock: &clock,
            eye_fluid: EyeFluid::None,
            heat_intensity: 0.0,
            mobs: &mobs,
            particles: &[],
            highlight_block: None,
        };
        for _ in 0..3 {
            r.render(&input).unwrap();
        }
        let img = r.render(&input).unwrap();
        let dir = crate_dir().join("renders");
        std::fs::create_dir_all(&dir).unwrap();
        img.save(dir.join(format!("preview_{}.png", s.name))).unwrap();
    }
}

#[test]
#[ignore]
fn preview_map() {
    for z in (-110..60).step_by(6) {
        let row: String = (-60..130)
            .step_by(3)
            .map(|x| {
                let h = height_at(x, z);
                if h <= 62 {
                    '~'
                } else if h < 66 {
                    '.'
                } else if h < 72 {
                    '+'
                } else {
                    '#'
                }
            })
            .collect();
        eprintln!("{z:5} {row}");
    }
}
