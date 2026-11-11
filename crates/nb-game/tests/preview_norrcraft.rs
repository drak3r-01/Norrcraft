//! Aperçu 3D d'un monde Norrcraft (session 2.1 : sols et teintes des biomes), rendu hors écran
//! par lavapipe. Ignoré par défaut ; écrit `crates/nb-game/renders/norrcraft_*.png` :
//! `NB_GFX_REQUIRE_GPU=1 NB_SH=1 crates/nb-gfx/scripts/vk-docker.sh \
//!   'cargo test -p nb-game --test preview_norrcraft -- --ignored'`
//! (`NB_NORR_SEED`, `NB_NORR_AT=x,z` pour choisir le lieu ; par défaut le point de départ ;
//! `NB_NORR_SEASON=0..7` pour le pas de saison, session 3.2 : 0 janvier, 1 mars, 2 mai, 3 juin,
//! 4 août, 5 septembre, 6 octobre, 7 décembre ; absent : état de fin d'été).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use glam::Vec3;
use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_game::WorldBiomes;
use nb_gfx::{
    Camera, EyeFluid, FrameInput, GfxSettings, GpuContext, OffscreenRenderer, SceneAssets, SkyClock,
};
use nb_mesh::{mesh_chunk, BlockTable, ChunkVoxels, Neighborhood};
use nb_world::norrcraft::NorrConfig;
use nb_world::{Chunk, WorldProfile};

const W: u32 = 960;
const H: u32 = 540;
const RADIUS: i32 = 12;

#[test]
#[ignore]
fn preview_norrcraft() {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => Arc::new(c),
        Err(e) => panic!("Vulkan indisponible : {e}"),
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mobs = nb_data::MobRegistry::builtin();
    let descs = nb_gfx::MobModelDesc::from_registry(&mobs);
    let (assets, _) = SceneAssets::load(&root.join("../../resource_packs"), "Default", &descs);
    let mut r = OffscreenRenderer::new(ctx, assets, W, H, &GfxSettings::default()).unwrap();

    let seed: i64 = std::env::var("NB_NORR_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12345);
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let profile = Arc::new(WorldProfile::norrcraft(seed, NorrConfig::default(), &reg));
    let gen = profile.new_generator();
    let season: Option<u8> = std::env::var("NB_NORR_SEASON")
        .ok()
        .and_then(|s| s.parse().ok());
    gen.set_season(season);
    let (ox, oz) = match std::env::var("NB_NORR_AT").ok() {
        Some(s) => {
            let v: Vec<i32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            (v[0], v[1])
        }
        None => {
            let s = gen.find_spawn(40);
            (s[0] as i32, s[2] as i32)
        }
    };
    let (ccx, ccz) = (ox.div_euclid(16), oz.div_euclid(16));

    let atlas = r.scene.block_atlas().clone();
    let table = BlockTable::from_registry(&reg, |n| atlas.layer(n));
    let mut voxels = HashMap::new();
    for cz in ccz - RADIUS - 1..=ccz + RADIUS + 1 {
        for cx in ccx - RADIUS - 1..=ccx + RADIUS + 1 {
            let mut chunk = Chunk::new(ChunkPos::new(cx, cz));
            gen.generate_chunk(&mut chunk);
            let mut v = ChunkVoxels::filled(0);
            for x in 0..16 {
                for z in 0..16 {
                    for y in 0..128 {
                        let id = chunk.block(x, y, z);
                        if id != 0 {
                            v.set(x as usize, y as usize, z as usize, id);
                        }
                    }
                }
            }
            voxels.insert((cx, cz), v);
        }
    }
    let biomes = WorldBiomes::new(profile.clone());
    for cz in ccz - RADIUS..=ccz + RADIUS {
        for cx in ccx - RADIUS..=ccx + RADIUS {
            let n: [Option<&ChunkVoxels>; 9] = std::array::from_fn(|i| {
                voxels.get(&(cx + (i % 3) as i32 - 1, cz + (i / 3) as i32 - 1))
            });
            let mesh = mesh_chunk(
                ChunkPos::new(cx, cz),
                &Neighborhood::from_voxels(n),
                &table,
                &biomes,
            );
            r.scene
                .upload_chunk_mesh(ChunkPos::new(cx, cz), &mesh.vertices)
                .unwrap();
        }
    }

    let ground = gen.surface_height(ox, oz).max(40) as f32;
    let name = |b: u8| profile.biome(b).name.clone();
    println!(
        "Norrcraft graine {seed} en ({ox}, {oz}) : biome {}",
        name(gen.biome_at(ox, oz))
    );
    let dir = root.join("renders");
    std::fs::create_dir_all(&dir).unwrap();
    for (label, yaw, pitch, eye) in [
        ("n", 0.0, 8.0, 6.0),
        ("e", 90.0, 8.0, 6.0),
        ("s", 180.0, 8.0, 6.0),
        ("w", 270.0, 8.0, 6.0),
        ("high", 45.0, 30.0, 40.0),
    ] {
        let cam = Camera {
            position: Vec3::new(ox as f32 + 0.5, ground + eye, oz as f32 + 0.5),
            pitch_deg: pitch,
            yaw_deg: yaw,
            ..Camera::default()
        };
        let mut clock = SkyClock::new();
        clock.set_time(900.0);
        let input = FrameInput {
            camera: &cam,
            time_seconds: 3.0,
            sky_clock: &clock,
            eye_fluid: EyeFluid::None,
            heat_intensity: 0.0,
            mobs: &[],
            particles: &[],
            highlight_block: None,
        };
        for _ in 0..3 {
            r.render(&input).unwrap();
        }
        let img = r.render(&input).unwrap();
        let tag = season.map_or(String::new(), |s| format!("_s{s}"));
        img.save(dir.join(format!("norrcraft_{seed}_{ox}_{oz}{tag}_{label}.png")))
            .unwrap();
    }
}
