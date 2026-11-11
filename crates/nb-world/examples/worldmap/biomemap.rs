//! Carte des biomes Norrcraft (session 2.1, mode `biomes`) : un pixel par colonne échantillonnée
//! (sélection exacte de la colonne, frontières tramées comprises), répartition comparée aux cibles
//! du doc 02 §7.5 et distance du plus proche représentant de chaque biome à l'origine.

use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::biomes::DISTRIBUTION_TARGETS;
use nb_world::norrcraft::{NorrBiome, NorrBlocks, NorrConfig, NorrPreset, NorrcraftTerrain};

use crate::image_out::write_image;

/// Couleur de carte d'un biome.
pub fn biome_color(b: NorrBiome) -> [u8; 3] {
    use NorrBiome::*;
    match b {
        Vasterhav => [30, 70, 140],
        Djuphav => [15, 30, 90],
        Bottenhav => [60, 110, 130],
        Packis => [200, 225, 240],
        Fjord => [70, 40, 140],
        Skargard => [120, 150, 190],
        Klintkust => [235, 225, 170],
        Hogkust => [190, 90, 80],
        Strandang => [150, 210, 150],
        Bokskog => [60, 140, 40],
        Hagmark => [160, 205, 80],
        Alvar => [215, 215, 140],
        Sjoland => [70, 175, 150],
        Urskog => [20, 85, 40],
        Tallhed => [205, 170, 90],
        Myr => [130, 100, 50],
        Alv => [60, 170, 240],
        Brannfalt => [90, 55, 45],
        Bergslag => [175, 80, 40],
        Fjallbjorkskog => [130, 175, 95],
        Fjallhed => [165, 150, 115],
        Kalfjall => [125, 125, 125],
        Glaciar => [245, 250, 255],
        Palsmyr => [160, 110, 170],
        Lavafalt => [50, 45, 50],
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(seed: i64, preset: NorrPreset, cx: i32, cz: i32, size: i32, step: i32, out: &str) {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let terrain = NorrcraftTerrain::new(
        seed,
        NorrConfig::with_preset(preset),
        NorrBlocks::from_registry(&reg),
    );
    let x0 = cx - size * step / 2;
    let z0 = cz - size * step / 2;
    let mut px = vec![0u8; (size * size * 3) as usize];
    let mut count = [0u64; NorrBiome::COUNT];
    let mut nearest = [f64::INFINITY; NorrBiome::COUNT];
    let (mut snow, mut fresh) = (0u64, 0u64);
    for j in 0..size {
        for i in 0..size {
            let (wx, wz) = (x0 + i * step, z0 + j * step);
            let s = terrain.column_biome(wx, wz);
            let k = s.biome as usize;
            count[k] += 1;
            snow += u64::from(s.snow);
            fresh += u64::from(s.fresh_water);
            let d = (f64::from(wx) * f64::from(wx) + f64::from(wz) * f64::from(wz)).sqrt();
            nearest[k] = nearest[k].min(d);
            let mut c = biome_color(s.biome);
            if s.fresh_water && s.biome != NorrBiome::Alv {
                c = [c[0] / 2, c[1] / 2 + 40, c[2] / 2 + 90];
            } else if s.snow {
                c = [235, 235, 240];
            }
            let o = ((j * size + i) * 3) as usize;
            px[o..o + 3].copy_from_slice(&c);
        }
    }
    let total = (size * size) as f64;
    let pct = |v: u64| v as f64 * 100.0 / total;
    println!(
        "-- biomes Norrcraft « {} », graine {seed}, {size} px × {step} blocs ({} km), centre ({cx}, {cz})",
        preset.label(),
        f64::from(size * step) / 1000.0
    );
    println!(
        "{:>16} {:>7} {:>7} {:>6}",
        "catégorie", "part %", "cible", "écart"
    );
    for (name, biomes, target) in DISTRIBUTION_TARGETS {
        let v: u64 = biomes.iter().map(|b| count[*b as usize]).sum();
        let p = pct(v);
        let flag = if (p - target).abs() <= 0.4 * target {
            ""
        } else {
            "  hors ±40 %"
        };
        println!(
            "{name:>16} {p:7.2} {target:7.1} {:+6.0}%{flag}",
            (p / target - 1.0) * 100.0
        );
    }
    println!("-- par biome (part %, plus proche de l'origine en km)");
    for b in NorrBiome::ALL {
        println!(
            "{:>16} {:6.2} {:>7}",
            b.name(),
            pct(count[b as usize]),
            if nearest[b as usize].is_finite() {
                format!("{:.1}", nearest[b as usize] / 1000.0)
            } else {
                "absent".to_owned()
            }
        );
    }
    println!(
        "neige éternelle : {:.2} %   eau douce : {:.2} %",
        pct(snow),
        pct(fresh)
    );
    write_image(out, size as u32, size as u32, &px).expect("écriture de l'image");
    println!("écrit : {out}");
}
