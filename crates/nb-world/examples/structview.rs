//! Rendu isométrique (PPM) d'une structure générée, pour la mise au point.
//!
//! Usage : cargo run --release -p nb-world --example structview -- <graine> <type> <nom> <n> <palette.txt> <sortie.ppm> [rot]
//! `nom` : nom français du type (ex. « hameau ») ; `n` : n-ième occurrence dans ±4000 blocs.
//! `palette.txt` : lignes `id r g b` (couleur moyenne des textures). Les blocs d'eau sont ignorés.
use std::io::Write;
use std::sync::Arc;

use nb_core::ChunkPos;
use nb_data::BlockRegistry;
use nb_world::{Chunk, WorldConfig, WorldGenerator, WorldType};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let seed: i64 = a[1].parse().unwrap();
    let ty = match a[2].as_str() {
        "archipelago" => WorldType::Archipelago,
        "highlands" => WorldType::Highlands,
        _ => WorldType::Normal,
    };
    let name = &a[3];
    let n: usize = a[4].parse().unwrap();
    let pal_txt = std::fs::read_to_string(&a[5]).unwrap();
    let out = &a[6];
    let rot: i32 = a.get(7).map_or(0, |s| s.parse().unwrap());
    let reg = BlockRegistry::builtin();
    let gen = WorldGenerator::new(seed, Arc::new(WorldConfig::preset(ty)), reg.ids());
    let mut pal = vec![[255u8, 0, 255]; 256];
    for l in pal_txt.lines() {
        let v: Vec<u32> = l
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if v.len() == 4 {
            pal[v[0] as usize] = [v[1] as u8, v[2] as u8, v[3] as u8];
        }
    }
    let sites: Vec<_> = gen
        .structure_sites(-4000, -4000, 4000, 4000)
        .into_iter()
        .filter(|s| s.kind.name() == name)
        .collect();
    let Some(site) = sites.get(n) else {
        eprintln!("seulement {} « {name} »", sites.len());
        return;
    };
    eprintln!(
        "{} en ({}, {}) y={} variante {}",
        name, site.x, site.z, site.base_y, site.variant
    );
    let r = site.kind.radius() + 5;
    let (x0, z0) = (site.x - r, site.z - r);
    let size = 2 * r + 1;
    let mut chunks = std::collections::HashMap::new();
    for cz in z0.div_euclid(16)..=(z0 + size).div_euclid(16) {
        for cx in x0.div_euclid(16)..=(x0 + size).div_euclid(16) {
            let mut c = Chunk::new(ChunkPos::new(cx, cz));
            gen.generate_chunk(&mut c);
            chunks.insert((cx, cz), c);
        }
    }
    let air = reg.ids().air;
    let water = reg.ids().water;
    let block = |x: i32, y: i32, z: i32| -> u8 {
        if !(0..128).contains(&y) {
            return air;
        }
        let b = chunks[&(x.div_euclid(16), z.div_euclid(16))].block(
            x.rem_euclid(16),
            y,
            z.rem_euclid(16),
        );
        if b == water {
            air
        } else {
            b
        }
    };
    let (ylo, yhi) = (site.base_y - 10, site.base_y + 34);
    let (w, h) = (1500usize, 1000usize);
    let mut img = vec![[200u8, 225, 255]; w * h];
    let s = 10.0f32;
    let (cxs, cys) = (w as f32 / 2.0, h as f32 - 330.0);
    let proj = |x: i32, y: i32, z: i32| -> (f32, f32) {
        let (mut dx, mut dz) = ((x - site.x) as f32, (z - site.z) as f32);
        for _ in 0..rot {
            let t = dx;
            dx = -dz;
            dz = t;
        }
        (
            cxs + (dx - dz) * s,
            cys + (dx + dz) * s * 0.5 - (y - site.base_y) as f32 * s,
        )
    };
    let mut poly = |pts: [(f32, f32); 4], c: [u8; 3]| {
        let (miny, maxy) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p.1), m.1.max(p.1)));
        for y in miny.floor() as i32..=maxy.ceil() as i32 {
            let fy = y as f32 + 0.5;
            let mut xs = Vec::new();
            for i in 0..4 {
                let (p, q) = (pts[i], pts[(i + 1) % 4]);
                if (p.1 <= fy && q.1 > fy) || (q.1 <= fy && p.1 > fy) {
                    xs.push(p.0 + (fy - p.1) / (q.1 - p.1) * (q.0 - p.0));
                }
            }
            if xs.len() >= 2 {
                xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                for x in xs[0].round() as i32..xs[xs.len() - 1].round() as i32 {
                    if x >= 0 && (x as usize) < w && y >= 0 && (y as usize) < h {
                        img[y as usize * w + x as usize] = c;
                    }
                }
            }
        }
    };
    // Ordre : du plus loin au plus proche selon la rotation.
    let mut cells = Vec::new();
    for y in ylo..yhi {
        for z in z0..z0 + size {
            for x in x0..x0 + size {
                cells.push((x, y, z));
            }
        }
    }
    cells.sort_by_key(|&(x, y, z)| {
        let (mut dx, mut dz) = (x - site.x, z - site.z);
        for _ in 0..rot {
            let t = dx;
            dx = -dz;
            dz = t;
        }
        (dx + dz, y)
    });
    for (x, y, z) in cells {
        let b = block(x, y, z);
        if b == air {
            continue;
        }
        // Face visible ? (voisins +x, +z, +y après rotation approximée : on garde tout si un voisin air)
        let vis = [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0)]
            .iter()
            .any(|&(dx, dy, dz)| block(x + dx, y + dy, z + dz) == air);
        if !vis {
            continue;
        }
        let c = pal[b as usize];
        let sh = |f: f32| {
            [
                (c[0] as f32 * f) as u8,
                (c[1] as f32 * f) as u8,
                (c[2] as f32 * f) as u8,
            ]
        };
        let p100 = proj(x + 1, y, z);
        let p001 = proj(x, y, z + 1);
        let p101 = proj(x + 1, y, z + 1);
        let p010 = proj(x, y + 1, z);
        let p110 = proj(x + 1, y + 1, z);
        let p011 = proj(x, y + 1, z + 1);
        let p111 = proj(x + 1, y + 1, z + 1);
        poly([p010, p110, p111, p011], sh(1.0));
        poly([p011, p111, p101, p001], sh(0.8));
        poly([p110, p111, p101, p100], sh(0.62));
    }
    let mut f = std::fs::File::create(out).unwrap();
    write!(f, "P6\n{w} {h}\n255\n").unwrap();
    for p in img {
        f.write_all(&p).unwrap();
    }
}
