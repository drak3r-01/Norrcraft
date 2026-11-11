//! Carte du monde vue de dessus et statistiques de répartition.
//!
//! **Vanilla** (biomes + relief ombré, PPM ou PNG selon l'extension) :
//! `cargo run --release -p nb-world --example worldmap -- <graine> <type> <centre_x> <centre_z> <taille> <pas> <fichier.ppm|png>`
//! (`type` : normal | flat | amplified | island | archipelago | highlands | classic ; `taille` en pixels, `pas` en blocs par pixel)
//!
//! **Norrcraft** (relief et champs macroscopiques, PNG) :
//! `cargo run --release -p nb-world --example worldmap -- <graine> <préréglage> <centre_x> <centre_z> <taille> <pas> <fichier.png> [mode]`
//! (`préréglage` : norrland | skargard | hogfjall ; `mode` : voir `norr::MODES`, `overview` par défaut)

mod image_out;
mod norr;

use std::sync::Arc;

use nb_world::{Biome, BiomeSystem, GeneratorKind, Noise, WorldConfig, WorldType};

fn color(b: Biome) -> [u8; 3] {
    match b {
        Biome::Plains => [141, 179, 96],
        Biome::Forest => [5, 102, 33],
        Biome::Desert => [250, 148, 24],
        Biome::Taiga => [11, 102, 89],
        Biome::Jungle => [83, 123, 9],
        Biome::Swamp => [7, 249, 178],
        Biome::Savanna => [189, 178, 95],
        Biome::Mountains => [96, 96, 96],
        Biome::Ocean => [0, 0, 112],
        Biome::DeepOcean => [0, 0, 48],
        Biome::WarmOcean => [0, 0, 172],
        Biome::FrozenOcean => [144, 144, 160],
        Biome::River => [0, 0, 255],
        Biome::Beach => [250, 222, 85],
        Biome::StonyShore => [162, 162, 132],
        Biome::BirchForest => [48, 116, 68],
        Biome::DarkForest => [64, 81, 26],
        Biome::FlowerMeadow => [255, 140, 200],
        Biome::CherryGrove => [255, 170, 210],
        Biome::AutumnForest => [210, 110, 20],
        Biome::SnowyPlains => [255, 255, 255],
        Biome::SnowyTaiga => [49, 85, 74],
        Biome::IceSpikes => [180, 220, 220],
        Biome::Heath => [140, 100, 180],
        Biome::MegaTaiga => [89, 102, 81],
        Biome::Badlands => [217, 69, 21],
        Biome::Hills => [90, 150, 110],
        Biome::SnowyPeaks => [220, 220, 240],
        Biome::Volcano => [255, 0, 0],
        Biome::MushroomFields => [255, 0, 255],
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let seed: i64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(12345);
    let kind = a.get(2).map(String::as_str).unwrap_or("normal");
    if nb_world::norrcraft::NorrPreset::parse(kind).is_some() || kind == "norrcraft" {
        norr::run(&a);
        return;
    }
    let cx: i32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let cz: i32 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let size: i32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(512);
    let step: i32 = a.get(6).and_then(|s| s.parse().ok()).unwrap_or(8);
    let out = a.get(7).cloned().unwrap_or_else(|| "map.ppm".into());

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
    let reg = nb_data::BlockRegistry::builtin();
    let bs = BiomeSystem::new(Arc::new(Noise::new(seed)), Arc::new(cfg.clone()), reg.ids());
    let t = bs.terrain().clone();

    let mut hist = [0u64; nb_world::BIOME_COUNT];
    let mut dec_t = [0u64; 10];
    let mut dec_h = [0u64; 10];
    let mut dec_w = [0u64; 10];
    let mut heights: Vec<i32> = Vec::new();
    let mut sample: Vec<Option<(i32, i32)>> = vec![None; nb_world::BIOME_COUNT];
    let mut px = vec![0u8; (size * size * 3) as usize];
    let x0 = cx - size * step / 2;
    let z0 = cz - size * step / 2;
    for j in 0..size {
        for i in 0..size {
            let (wx, wz) = (x0 + i * step, z0 + j * step);
            let col = t.column(wx, wz);
            let b = if cfg.generator == GeneratorKind::Norrblock {
                col.biome
            } else {
                bs.biome_at(wx, wz)
            };
            hist[b.ordinal()] += 1;
            // Un point représentatif : le plus proche du centre de la carte parmi les premiers.
            if sample[b.ordinal()].is_none() && hist[b.ordinal()] % 40 == 20 {
                sample[b.ordinal()] = Some((wx, wz));
            }
            dec_t[((col.temp * 9.999) as usize).min(9)] += 1;
            dec_h[((col.humid * 9.999) as usize).min(9)] += 1;
            dec_w[((col.weird * 9.999) as usize).min(9)] += 1;
            heights.push(col.height);
            let hx = t.column(wx + step, wz).height;
            let shade = 1.0 + f64::from(col.height - hx) * 0.06;
            let mut c = color(b);
            if a.get(8).is_some() {
                let g = (f64::from(col.height) * 2.0).min(255.0) as u8;
                c = if col.height < cfg.sea_level {
                    [0, 40, g.saturating_add(60)]
                } else {
                    [g, g, g]
                };
            }
            let o = ((j * size + i) * 3) as usize;
            for k in 0..3 {
                px[o + k] = (f64::from(c[k]) * shade.clamp(0.55, 1.35)).min(255.0) as u8;
            }
        }
    }
    let total = f64::from(size * size);
    println!("-- biomes (% de la carte)");
    let mut rows: Vec<(f64, Biome)> = Biome::ALL
        .iter()
        .map(|&b| (hist[b.ordinal()] as f64 * 100.0 / total, b))
        .collect();
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    for (p, b) in rows {
        println!("{:>16} {:5.1}", b.name(), p);
    }
    let pr = |n: &str, d: &[u64; 10]| {
        println!(
            "{n}: {}",
            d.iter()
                .map(|v| format!("{:4.1}", *v as f64 * 100.0 / total))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    pr("temp  déciles", &dec_t);
    pr("humid déciles", &dec_h);
    pr("weird déciles", &dec_w);
    heights.sort_unstable();
    let q = |f: f64| heights[((heights.len() - 1) as f64 * f) as usize];
    println!(
        "hauteur min/10/50/90/max: {} {} {} {} {}",
        q(0.0),
        q(0.1),
        q(0.5),
        q(0.9),
        q(1.0)
    );
    for b in Biome::ALL {
        if let Some((x, z)) = sample[b.ordinal()] {
            println!("SAMPLE {} {x} {z}", b.name());
        }
    }
    image_out::write_image(&out, size as u32, size as u32, &px).unwrap();
}
