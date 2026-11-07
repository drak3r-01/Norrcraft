//! Vue de dessus des blocs réellement générés (surface visible) et coupe verticale.
//!
//! Usage : cargo run --release -p nb-world --example blockmap -- <graine> <type> <x0> <z0> <taille> <fichier.ppm> [coupe]
//! Sans `coupe` : vue de dessus de `taille × taille` blocs depuis (x0, z0). Avec `coupe` : coupe
//! verticale le long de x (z = z0 fixe), `taille` blocs de large sur 128 de haut.

use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use nb_core::ChunkPos;
use nb_data::BlockRegistry;
use nb_world::{Chunk, GeneratorKind, WorldConfig, WorldGenerator, WorldType};

fn color(name: &str) -> [u8; 3] {
    match name {
        "air" => [160, 200, 255],
        "grass" => [90, 160, 50],
        "dirt" => [120, 85, 55],
        "stone" => [125, 125, 125],
        "water" => [40, 80, 200],
        "sand" => [225, 210, 150],
        "bedrock" => [20, 20, 20],
        "clay" => [160, 165, 180],
        "snow" => [250, 250, 255],
        "wood" => [100, 75, 40],
        "leaves" => [50, 130, 40],
        "birch_wood" => [220, 220, 200],
        "birch_leaves" => [110, 170, 60],
        "spruce_wood" => [60, 42, 25],
        "spruce_leaves" => [30, 90, 60],
        "jungle_wood" => [110, 80, 40],
        "jungle_leaves" => [30, 150, 20],
        "acacia_wood" => [150, 100, 70],
        "acacia_leaves" => [130, 150, 40],
        "tall_grass" => [100, 180, 60],
        "fern" => [60, 130, 60],
        "dead_bush" => [140, 100, 40],
        "cactus" => [20, 120, 30],
        "lava" => [255, 100, 0],
        "sandstone" => [215, 195, 140],
        "gravel" => [140, 135, 130],
        "red_sand" => [190, 100, 44],
        "terracotta" => [152, 94, 67],
        "terracotta_red" => [143, 61, 46],
        "terracotta_orange" => [200, 100, 40],
        "terracotta_yellow" => [220, 170, 50],
        "terracotta_white" => [220, 195, 180],
        "terracotta_brown" => [77, 51, 36],
        "ice" => [150, 200, 255],
        "packed_ice" => [120, 165, 238],
        "podzol" => [90, 62, 25],
        "mycelium" => [112, 96, 112],
        "basalt" => [60, 62, 68],
        "marble" => [230, 230, 235],
        "slate" => [78, 83, 94],
        "ash" => [122, 120, 124],
        "magma" => [220, 90, 20],
        "cherry_wood" => [140, 80, 90],
        "dark_oak_wood" => [50, 36, 22],
        "mushroom_stem" => [218, 208, 192],
        "mushroom_cap_red" => [200, 40, 36],
        "mushroom_cap_brown" => [150, 110, 75],
        "coral_pink" => [226, 104, 164],
        "coral_purple" => [150, 64, 200],
        "coral_yellow" => [232, 204, 52],
        "coral_blue" => [54, 96, 222],
        "flower_blue" | "flower_pink" | "flower_white" | "flower_orange" | "flower_red"
        | "flower_yellow" | "lavender" => [255, 90, 200],
        "reeds" => [116, 168, 58],
        "moss" => [84, 112, 42],
        "lily_pad" => [20, 140, 40],
        "mushroom_red" | "mushroom_brown" => [200, 60, 60],
        n if n.ends_with("_ore") => [200, 170, 60],
        _ => [255, 0, 255],
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let seed: i64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(12345);
    let kind = a.get(2).map(String::as_str).unwrap_or("normal");
    let x0: i32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let z0: i32 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let size: i32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(256);
    let out = a.get(6).cloned().unwrap_or_else(|| "blocks.ppm".into());
    let section = a.get(7).is_some();

    let mut cfg = match kind {
        "flat" => WorldConfig::preset(WorldType::Flat),
        "amplified" => WorldConfig::preset(WorldType::Amplified),
        "island" => WorldConfig::preset(WorldType::Island),
        "archipelago" => WorldConfig::preset(WorldType::Archipelago),
        "highlands" => WorldConfig::preset(WorldType::Highlands),
        _ => WorldConfig::preset(WorldType::Normal),
    };
    if kind == "classic" {
        cfg.generator = GeneratorKind::Classic;
    }
    let reg = BlockRegistry::builtin();
    let gen = WorldGenerator::new(seed, Arc::new(cfg), reg.ids());
    let ids = reg.ids();
    let palette: Vec<[u8; 3]> = (0..=255u8).map(|i| color(&reg.from_id(i).name)).collect();

    let (cx0, cx1) = (x0.div_euclid(16), (x0 + size - 1).div_euclid(16));
    let (cz0, cz1) = if section {
        (z0.div_euclid(16), z0.div_euclid(16))
    } else {
        (z0.div_euclid(16), (z0 + size - 1).div_euclid(16))
    };
    let t = Instant::now();
    let mut chunks = std::collections::HashMap::new();
    for cz in cz0..=cz1 {
        for cx in cx0..=cx1 {
            let mut c = Chunk::new(ChunkPos::new(cx, cz));
            gen.generate_chunk(&mut c);
            chunks.insert((cx, cz), c);
        }
    }
    let n = chunks.len();
    eprintln!(
        "{} chunks en {:?} ({:?}/chunk)",
        n,
        t.elapsed(),
        t.elapsed() / n as u32
    );
    let block = |x: i32, y: i32, z: i32| -> u8 {
        chunks[&(x.div_euclid(16), z.div_euclid(16))].block(x.rem_euclid(16), y, z.rem_euclid(16))
    };
    let mut f = std::fs::File::create(&out).unwrap();
    let mut buf = Vec::new();
    let mut counts = std::collections::BTreeMap::new();
    if section {
        write!(f, "P6\n{size} 128\n255\n").unwrap();
        for y in (0..128).rev() {
            for i in 0..size {
                let b = block(x0 + i, y, z0);
                buf.extend_from_slice(&palette[usize::from(b)]);
            }
        }
    } else {
        write!(f, "P6\n{size} {size}\n255\n").unwrap();
        for j in 0..size {
            for i in 0..size {
                let (x, z) = (x0 + i, z0 + j);
                let mut y = 127;
                while y > 0 && block(x, y, z) == ids.air {
                    y -= 1;
                }
                let b = block(x, y, z);
                *counts.entry(reg.from_id(b).name.clone()).or_insert(0u32) += 1;
                let mut c = palette[usize::from(b)];
                // Ombrage par la hauteur voisine (relief).
                let mut yn = 127;
                let xn = (x + 1).min(x0 + size - 1);
                while yn > 0 && block(xn, yn, z) == ids.air {
                    yn -= 1;
                }
                let d = f64::from(y - yn);
                let sh = (1.0 + d * 0.05).clamp(0.6, 1.3);
                for k in &mut c {
                    *k = (f64::from(*k) * sh).min(255.0) as u8;
                }
                buf.extend_from_slice(&c);
            }
        }
        let mut v: Vec<_> = counts.into_iter().collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.1));
        for (k, c) in v.iter().take(14) {
            eprintln!("{k:>20} {c}");
        }
    }
    f.write_all(&buf).unwrap();
}
