//! Phase 9 : structures, flore et arbres supplémentaires du générateur « Norrblock ».

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use nb_core::ChunkPos;
use nb_data::{BlockId, BlockRegistry};
use nb_world::{
    Biome, BiomeSystem, Chunk, Noise, StructureKind, WorldConfig, WorldGenerator, WorldType,
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

/// Compte, dans les chunks couvrant `[x0, x1] × [z0, z1]`, les blocs de la liste.
fn count_blocks(
    g: &WorldGenerator,
    (x0, z0, x1, z1): (i32, i32, i32, i32),
    y_min: i32,
    set: &[BlockId],
) -> usize {
    let mut n = 0;
    for cz in z0.div_euclid(16)..=z1.div_euclid(16) {
        for cx in x0.div_euclid(16)..=x1.div_euclid(16) {
            let c = chunk(g, cx, cz);
            for z in 0..16 {
                for x in 0..16 {
                    let (wx, wz) = (cx * 16 + x, cz * 16 + z);
                    if wx < x0 || wx > x1 || wz < z0 || wz > z1 {
                        continue;
                    }
                    for y in y_min.max(0)..128 {
                        if set.contains(&c.block(x, y, z)) {
                            n += 1;
                        }
                    }
                }
            }
        }
    }
    n
}

fn all_sites(
    seeds: &[i64],
) -> BTreeMap<&'static str, Vec<(i64, WorldType, nb_world::StructureSite)>> {
    let mut by: BTreeMap<&'static str, Vec<_>> = BTreeMap::new();
    for &seed in seeds {
        for ty in [
            WorldType::Normal,
            WorldType::Archipelago,
            WorldType::Highlands,
        ] {
            let (g, _) = gen(seed, ty);
            for s in g.structure_sites(-5000, -5000, 5000, 5000) {
                by.entry(s.kind.name()).or_default().push((seed, ty, s));
            }
        }
    }
    by
}

#[test]
fn every_structure_kind_is_generated_somewhere() {
    let by = all_sites(&[12345, 7, 2024]);
    let missing: Vec<_> = StructureKind::ALL
        .iter()
        .filter(|k| !by.contains_key(k.name()))
        .map(|k| k.name())
        .collect();
    assert!(missing.is_empty(), "types jamais produits : {missing:?}");
}

#[test]
fn structures_are_numerous_and_do_not_overlap() {
    let (g, _) = gen(12345, WorldType::Normal);
    let sites = g.structure_sites(-2000, -2000, 2000, 2000);
    // 16 km² : au moins une structure tous les ~200 × 200 blocs en moyenne.
    assert!(sites.len() > 400, "seulement {} structures", sites.len());
    for (i, a) in sites.iter().enumerate() {
        for b in &sites[i + 1..] {
            let d = f64::from((a.x - b.x).pow(2) + (a.z - b.z).pow(2)).sqrt();
            assert!(
                d >= f64::from(a.kind.radius() + b.kind.radius()),
                "{} et {} se chevauchent ({d:.0} blocs)",
                a.kind.name(),
                b.kind.name()
            );
        }
    }
}

#[test]
fn structure_density_zero_disables_structures() {
    let reg = BlockRegistry::builtin();
    let mut cfg = WorldConfig::preset(WorldType::Normal);
    cfg.structure_density = 0.0;
    let g = WorldGenerator::new(12345, Arc::new(cfg), reg.ids());
    assert!(g.structure_sites(-2000, -2000, 2000, 2000).is_empty());
}

#[test]
fn sites_sit_on_dry_land_or_on_the_seabed() {
    let sea = WorldConfig::default().sea_level;
    let by = all_sites(&[12345]);
    for sites in by.values() {
        for (_, _, s) in sites {
            if matches!(s.kind, StructureKind::Shipwreck | StructureKind::OceanRuin) {
                assert!(s.base_y < sea - 2, "{} hors de l'eau", s.kind.name());
            } else {
                assert!(s.base_y > sea, "{} dans l'eau", s.kind.name());
            }
        }
    }
}

/// Blocs que seules les structures posent (planches, briques, verre, laine, os…).
fn structure_palette(reg: &BlockRegistry) -> Vec<BlockId> {
    [
        "planks_oak",
        "planks_spruce",
        "planks_birch",
        "planks_acacia",
        "planks_dark_oak",
        "planks_cherry",
        "cobblestone",
        "mossy_cobblestone",
        "stone_bricks",
        "mossy_stone_bricks",
        "cut_sandstone",
        "chiseled_sandstone",
        "bricks",
        "hay_block",
        "bookshelf",
        "glass",
        "lantern",
        "sea_lantern",
        "wool_white",
        "wool_red",
        "wool_blue",
        "wool_yellow",
        "bone_block",
        "obsidian",
        "basalt_bricks",
        "gold_block",
        "campfire",
        "dirt_path",
        "farmland",
        "prismarine_bricks",
        "torch",
    ]
    .iter()
    .map(|n| reg.get(n).id)
    .collect()
}

#[test]
fn every_structure_kind_leaves_blocks_across_chunks() {
    let by = all_sites(&[12345, 7, 2024]);
    let reg = BlockRegistry::builtin();
    let mut palette = structure_palette(&reg);
    // Le cercle de pierres et l'igloo sont surtout en pierre / marbre / neige.
    palette.extend([
        reg.get("marble").id,
        reg.get("packed_ice").id,
        reg.get("snow").id,
    ]);
    for kind in StructureKind::ALL {
        let (seed, ty, s) = by[kind.name()][0];
        let (g, _) = gen(seed, ty);
        let r = kind.radius() + 2;
        let n = count_blocks(
            &g,
            (s.x - r, s.z - r, s.x + r, s.z + r),
            s.base_y - 1,
            &palette,
        );
        let min = if kind == StructureKind::StoneCircle {
            9
        } else {
            25
        };
        assert!(
            n >= min,
            "{} : seulement {n} blocs de construction",
            kind.name()
        );
    }
}

#[test]
fn structure_generation_is_independent_of_chunk_order() {
    let by = all_sites(&[12345]);
    let (seed, ty, s) = by["hameau"][0];
    let (g1, _) = gen(seed, ty);
    let (g2, _) = gen(seed, ty);
    let (c0x, c0z) = (s.x.div_euclid(16) - 2, s.z.div_euclid(16) - 2);
    let coords: Vec<(i32, i32)> = (0..5)
        .flat_map(|i| (0..5).map(move |j| (c0x + i, c0z + j)))
        .collect();
    let a: Vec<u64> = coords
        .iter()
        .map(|&(x, z)| chunk(&g1, x, z).fingerprint())
        .collect();
    let mut b: Vec<u64> = coords
        .iter()
        .rev()
        .map(|&(x, z)| chunk(&g2, x, z).fingerprint())
        .collect();
    b.reverse();
    assert_eq!(a, b);
}

/// Premiers chunks dont le centre est dans le biome demandé.
fn chunks_in_biome(seed: i64, biome: Biome, n: usize) -> Vec<(i32, i32)> {
    let reg = BlockRegistry::builtin();
    let bs = BiomeSystem::new(
        Arc::new(Noise::new(seed)),
        Arc::new(WorldConfig::default()),
        reg.ids(),
    );
    let mut out = Vec::new();
    for cz in (-300..300).step_by(3) {
        for cx in (-300..300).step_by(3) {
            if bs.biome_at(cx * 16 + 8, cz * 16 + 8) == biome {
                out.push((cx, cz));
                if out.len() >= n {
                    return out;
                }
            }
        }
    }
    out
}

fn found_in_biome(biome: Biome, names: &[&str], seeds: &[i64]) -> HashSet<String> {
    let reg = BlockRegistry::builtin();
    let ids: Vec<(BlockId, String)> = names
        .iter()
        .map(|n| (reg.get(n).id, (*n).to_string()))
        .collect();
    let mut seen = HashSet::new();
    for &seed in seeds {
        let g = WorldGenerator::new(seed, Arc::new(WorldConfig::default()), reg.ids());
        for (cx, cz) in chunks_in_biome(seed, biome, 40) {
            let c = chunk(&g, cx, cz);
            for i in 0..16 * 128 * 16usize {
                let (x, y, z) = (
                    (i % 16) as i32,
                    ((i / 16) % 128) as i32,
                    (i / (16 * 128)) as i32,
                );
                let b = c.block(x, y, z);
                if let Some((_, n)) = ids.iter().find(|(id, _)| *id == b) {
                    seen.insert(n.clone());
                }
            }
        }
    }
    seen
}

#[test]
fn new_flora_and_trees_appear_in_their_biomes() {
    let cases: &[(Biome, &[&str])] = &[
        (Biome::Plains, &["sunflower", "apple_leaves"]),
        (Biome::Forest, &["berry_bush", "lily_of_the_valley"]),
        (Biome::Swamp, &["cattail", "willow_leaves"]),
        (Biome::Savanna, &["pampas_grass", "baobab_wood"]),
        (Biome::MegaTaiga, &["redwood_wood"]),
        (Biome::Jungle, &["bamboo", "hibiscus"]),
        (Biome::SnowyPlains, &["cotton_grass"]),
        (Biome::Desert, &["desert_sage", "cactus"]),
        (Biome::Volcano, &["ember_bloom"]),
        (Biome::BirchForest, &["lily_of_the_valley"]),
    ];
    for (biome, names) in cases {
        let seen = found_in_biome(*biome, names, &[12345, 99, 2024]);
        for n in *names {
            assert!(seen.contains(*n), "{n} introuvable en {biome:?}");
        }
    }
}
