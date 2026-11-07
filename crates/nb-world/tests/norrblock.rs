//! Propriétés du générateur « Norrblock » (le Java est couvert par `reference.rs`).

use std::collections::HashSet;
use std::sync::Arc;

use nb_core::ChunkPos;
use nb_data::BlockRegistry;
use nb_world::{
    Biome, BiomeSystem, Chunk, GeneratorKind, Noise, WorldConfig, WorldGenerator, WorldType,
    BIOME_COUNT, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z,
};

fn gen(seed: i64, t: WorldType) -> (WorldGenerator, BlockRegistry) {
    let reg = BlockRegistry::builtin();
    let g = WorldGenerator::new(seed, Arc::new(WorldConfig::preset(t)), reg.ids());
    (g, reg)
}

fn chunk(g: &WorldGenerator, cx: i32, cz: i32) -> Chunk {
    let mut c = Chunk::new(ChunkPos::new(cx, cz));
    g.generate_chunk(&mut c);
    c
}

#[test]
fn default_world_uses_norrblock_but_old_world_json_stays_classic() {
    assert_eq!(WorldConfig::default().generator, GeneratorKind::Norrblock);
    // Un world.json écrit avant l'arrivée du générateur n'a pas de champ `generator`.
    let old: WorldConfig = serde_json::from_str(r#"{"seaLevel": 34}"#).unwrap();
    assert_eq!(old.generator, GeneratorKind::Classic);
    let back: WorldConfig =
        serde_json::from_str(&serde_json::to_string(&WorldConfig::default()).unwrap()).unwrap();
    assert_eq!(back.generator, GeneratorKind::Norrblock);
}

#[test]
fn generation_is_deterministic_and_independent_of_order() {
    let (g, _) = gen(4242, WorldType::Normal);
    let a: Vec<u64> = (-3..3).map(|i| chunk(&g, i, 2 * i).fingerprint()).collect();
    // Autre ordre, autre instance : mêmes chunks.
    let (g2, _) = gen(4242, WorldType::Normal);
    let b: Vec<u64> = (-3..3)
        .rev()
        .map(|i| chunk(&g2, i, 2 * i).fingerprint())
        .collect();
    assert_eq!(a, b.into_iter().rev().collect::<Vec<_>>());
    let (g3, _) = gen(4243, WorldType::Normal);
    assert_ne!(a[0], chunk(&g3, -3, -6).fingerprint());
}

#[test]
fn every_biome_appears_and_none_dominates() {
    let reg = BlockRegistry::builtin();
    let mut seen = HashSet::new();
    let mut hist = [0u32; BIOME_COUNT];
    let mut total = 0u32;
    for seed in [12345i64, 7, 99, 2024] {
        let bs = BiomeSystem::new(
            Arc::new(Noise::new(seed)),
            Arc::new(WorldConfig::default()),
            reg.ids(),
        );
        for z in (-6000..6000).step_by(40) {
            for x in (-6000..6000).step_by(40) {
                let b = bs.biome_at(x, z);
                seen.insert(b);
                hist[b.ordinal()] += 1;
                total += 1;
            }
        }
    }
    let missing: Vec<_> = Biome::ALL.iter().filter(|b| !seen.contains(b)).collect();
    assert!(missing.is_empty(), "biomes jamais produits : {missing:?}");
    let max = *hist.iter().max().unwrap();
    assert!(
        f64::from(max) / f64::from(total) < 0.2,
        "un biome occupe plus de 20 % du monde : {hist:?}"
    );
}

#[test]
fn mesher_and_generator_agree_on_biome_and_height() {
    let (g, reg) = gen(31337, WorldType::Normal);
    let bs = g.biome_system();
    let ids = reg.ids();
    let (mut holes, mut total) = (0, 0);
    for cz in -2..2 {
        for cx in -2..2 {
            let c = chunk(&g, cx, cz);
            for lz in (0..16).step_by(5) {
                for lx in (0..16).step_by(5) {
                    let (wx, wz) = (cx * 16 + lx, cz * 16 + lz);
                    let col = bs.terrain().column(wx, wz);
                    assert_eq!(bs.biome_at(wx, wz), col.biome);
                    assert_eq!(g.compute_surface_height(wx, wz), col.height);
                    // Le sol est plein juste sous `height`, sauf grotte débouchante.
                    total += 1;
                    if c.block(lx, col.height - 1, lz) == ids.air {
                        holes += 1;
                    }
                }
            }
        }
    }
    assert!(holes * 10 < total, "{holes} trous sur {total} colonnes");
}

#[test]
fn chunks_only_contain_known_blocks_and_sane_fluids() {
    let (g, reg) = gen(555, WorldType::Normal);
    let ids = reg.ids();
    let sea = g.config().sea_level;
    let mut ores = 0;
    for cz in -3..3 {
        for cx in -3..3 {
            let c = chunk(&g, cx, cz);
            for lz in 0..CHUNK_SIZE_Z as i32 {
                for lx in 0..CHUNK_SIZE_X as i32 {
                    for y in 0..CHUNK_SIZE_Y as i32 {
                        let b = c.block(lx, y, lz);
                        assert!(reg.from_id(b).id == b, "bloc inconnu {b}");
                        if b == ids.water {
                            assert!(y < sea, "eau au-dessus du niveau de la mer : y={y}");
                        }
                        if b == ids.coal_ore || b == ids.iron_ore || b == ids.diamond_ore {
                            ores += 1;
                        }
                    }
                    assert_eq!(c.block(lx, 0, lz), ids.bedrock);
                }
            }
        }
    }
    assert!(ores > 100, "minerais : {ores}");
}

#[test]
fn flat_world_stays_flat_with_biomes() {
    let (g, _) = gen(8, WorldType::Flat);
    for z in (-500..500).step_by(31) {
        for x in (-500..500).step_by(31) {
            let h = g.compute_surface_height(x, z);
            assert!((5..=6).contains(&h), "relief dans un monde plat : {h}");
        }
    }
}

#[test]
fn terrain_is_continuous_between_neighbours() {
    // Le Java avait des falaises de 30+ blocs à la frontière des biomes. Ici, hors rivière,
    // océan et cratère, la hauteur de deux colonnes voisines ne saute que rarement de plus de 4.
    let (g, _) = gen(2718, WorldType::Normal);
    let mut steep = 0;
    let mut n = 0;
    for z in (-1500..1500).step_by(7) {
        for x in (-1500..1500).step_by(7) {
            let a = g.compute_surface_height(x, z);
            let b = g.compute_surface_height(x + 1, z);
            n += 1;
            if (a - b).abs() > 4 {
                steep += 1;
            }
        }
    }
    assert!(
        f64::from(steep) / f64::from(n) < 0.01,
        "{steep} falaises sur {n} paires"
    );
}

#[test]
fn volcanoes_have_a_crater_with_lava() {
    // Cherche un volcan et vérifie que le cratère contient de la lave visible depuis le ciel.
    let (g, reg) = gen(12345, WorldType::Normal);
    let ids = reg.ids();
    let bs = g.biome_system();
    let mut found = None;
    'search: for z in (-3000..3000).step_by(16) {
        for x in (-3000..3000).step_by(16) {
            let col = bs.terrain().column(x, z);
            if col.lava_y > col.height {
                found = Some((x, z, col.lava_y));
                break 'search;
            }
        }
    }
    let (x, z, lava_y) = found.expect("aucun volcan dans 6000×6000 blocs");
    let c = chunk(&g, x.div_euclid(16), z.div_euclid(16));
    let (lx, lz) = (x.rem_euclid(16), z.rem_euclid(16));
    assert_eq!(c.block(lx, lava_y - 1, lz), ids.lava);
    assert_eq!(c.block(lx, lava_y, lz), ids.air);
}

#[test]
fn new_world_types_generate() {
    for t in WorldType::ALL {
        let (g, reg) = gen(1, t);
        let c = chunk(&g, 0, 0);
        assert!(c.voxels().iter().any(|&b| b != reg.ids().air), "{t:?}");
    }
}

#[test]
fn caves_exist_but_rarely_open_the_surface() {
    let (g, reg) = gen(5150, WorldType::Normal);
    let ids = reg.ids();
    let (mut air, mut cells, mut holes, mut cols) = (0u32, 0u32, 0u32, 0u32);
    for cz in -4..4 {
        for cx in -4..4 {
            let c = chunk(&g, cx, cz);
            for lz in 0..16 {
                for lx in 0..16 {
                    let h = g.compute_surface_height(cx * 16 + lx, cz * 16 + lz);
                    if h > g.config().sea_level + 2 {
                        cols += 1;
                        if c.block(lx, h - 1, lz) == ids.air {
                            holes += 1;
                        }
                    }
                    for y in 3..(h - 6).min(60) {
                        cells += 1;
                        if c.block(lx, y, lz) == ids.air || c.block(lx, y, lz) == ids.lava {
                            air += 1;
                        }
                    }
                }
            }
        }
    }
    let frac = f64::from(air) / f64::from(cells);
    assert!((0.01..0.25).contains(&frac), "part de grottes : {frac}");
    assert!(
        f64::from(holes) / f64::from(cols) < 0.06,
        "{holes} puits sur {cols} colonnes émergées"
    );
}
