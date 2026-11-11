//! Tests d'acceptation de la strate basse Norrcraft (session 2.4 ; GDD doc 02 §9.3 et §13).

use std::collections::BTreeMap;

use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::flora::Understory;
use nb_world::norrcraft::{NorrBiome, NorrBlocks, NorrConfig, NorrcraftTerrain};
use nb_world::{Chunk, TerrainGenerator};

const SEEDS: [i64; 3] = [12345, -98765, 20261111];

fn terrain(seed: i64) -> NorrcraftTerrain {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    NorrcraftTerrain::new(seed, NorrConfig::default(), NorrBlocks::from_registry(&reg))
}

fn gen(t: &NorrcraftTerrain, cx: i32, cz: i32) -> Chunk {
    let mut c = Chunk::new(ChunkPos::new(cx, cz));
    t.generate_chunk(&mut c);
    c
}

/// Plantes par (bloc, biome) sur une grille de chunks.
fn survey(
    t: &NorrcraftTerrain,
    reg: &BlockRegistry,
    half: i32,
    step: usize,
) -> BTreeMap<(String, NorrBiome), u32> {
    let mut m = BTreeMap::new();
    let ids: BTreeMap<u8, String> = Understory::block_names()
        .map(|n| (reg.get(n).id, n.to_string()))
        .collect();
    for cz in (-half..half).step_by(step) {
        for cx in (-half..half).step_by(step) {
            let c = gen(t, cx, cz);
            for z in 0..16 {
                for x in 0..16 {
                    for y in 1..127 {
                        if let Some(n) = ids.get(&c.block(x, y, z)) {
                            let b = NorrBiome::ALL[t.biome_at(cx * 16 + x, cz * 16 + z) as usize];
                            *m.entry((n.clone(), b)).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }
    m
}

#[test]
#[ignore]
fn print_survey() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    for seed in SEEDS {
        let t = terrain(seed);
        let m = survey(&t, &reg, 100, 3);
        let mut by_plant: BTreeMap<&str, (u32, Vec<String>)> = BTreeMap::new();
        for ((n, b), k) in &m {
            let e = by_plant.entry(n.as_str()).or_default();
            e.0 += k;
            e.1.push(format!("{}:{}", b.name(), k));
        }
        println!("== graine {seed}");
        for n in Understory::block_names() {
            let e = by_plant.get(n);
            println!("{n:20} {:?}", e);
        }
    }
}

/// Toutes les plantes d'un chunk avec le bloc posé dessous : `(nom, bloc dessous, biome)`.
fn plants_in(
    t: &NorrcraftTerrain,
    reg: &BlockRegistry,
    c: &Chunk,
    cx: i32,
    cz: i32,
) -> Vec<(String, String, NorrBiome)> {
    let ids: BTreeMap<u8, String> = Understory::block_names()
        .map(|n| (reg.get(n).id, n.to_string()))
        .collect();
    let mut v = Vec::new();
    for z in 0..16 {
        for x in 0..16 {
            for y in 1..127 {
                if let Some(n) = ids.get(&c.block(x, y, z)) {
                    let below = reg.from_id(c.block(x, y - 1, z)).name.clone();
                    let b = NorrBiome::ALL[t.biome_at(cx * 16 + x, cz * 16 + z) as usize];
                    v.push((n.clone(), below, b));
                }
            }
        }
    }
    v
}

#[test]
fn understory_is_deterministic_and_independent_of_order() {
    for seed in SEEDS {
        let t = terrain(seed);
        let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
        let mut with_plants = 0;
        for (cx, cz) in [(0, 0), (-3, 5), (40, -17), (-60, 90), (7, 7)] {
            let a = gen(&t, cx, cz);
            // Autre ordre : voisins d'abord, puis le chunk par une copie du générateur.
            let t2 = terrain(seed);
            for (dx, dz) in [(1, 0), (0, 1), (-1, -1), (1, 1)] {
                gen(&t2, cx + dx, cz + dz);
            }
            let b = gen(&t2, cx, cz);
            assert_eq!(a.fingerprint(), b.fingerprint(), "graine {seed} chunk {cx},{cz}");
            let f = t.fork();
            let mut c = Chunk::new(ChunkPos::new(cx, cz));
            f.generate_chunk(&mut c);
            assert_eq!(a.fingerprint(), c.fingerprint());
            with_plants += plants_in(&t, &reg, &a, cx, cz).len();
        }
        assert!(with_plants > 0, "graine {seed} : aucune plante près de l'origine");
    }
}

#[test]
fn plants_stand_on_ground_never_on_snow_or_ice_or_in_the_air() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let forbidden = ["sno", "blais", "havsis", "air", "bedrock"];
    for seed in SEEDS {
        let t = terrain(seed);
        let mut total = 0;
        for cz in (-90..90).step_by(6) {
            for cx in (-90..90).step_by(6) {
                let c = gen(&t, cx, cz);
                for (n, below, _) in plants_in(&t, &reg, &c, cx, cz) {
                    total += 1;
                    assert!(
                        !forbidden.contains(&below.as_str()),
                        "{n} sur {below} (chunk {cx},{cz})"
                    );
                    if n != "nackros" {
                        assert!(
                            below != "water",
                            "{n} flotte dans l'eau (seul nackros le peut)"
                        );
                    }
                }
            }
        }
        assert!(total > 1000, "graine {seed} : {total} plantes seulement");
    }
}

#[test]
fn most_plants_grow_in_their_declared_biomes() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let mut seen: BTreeMap<String, u32> = BTreeMap::new();
    for seed in SEEDS {
        let t = terrain(seed);
        for ((n, b), k) in survey(&t, &reg, 100, 4) {
            // Les plantes spécialisées ne sortent pas de leurs biomes.
            match n.as_str() {
                "hjortron" | "angsull" => assert!(
                    matches!(b, NorrBiome::Myr | NorrBiome::Palsmyr | NorrBiome::Fjallhed),
                    "{n} en {}",
                    b.name()
                ),
                "skagglav" => assert_eq!(b, NorrBiome::Urskog),
                "rallarros" => assert!(matches!(
                    b,
                    NorrBiome::Brannfalt | NorrBiome::Bergslag | NorrBiome::Kalfjall
                )),
                _ => {}
            }
            *seen.entry(n).or_insert(0) += k;
        }
    }
    let missing: Vec<&str> = Understory::block_names()
        .filter(|n| !seen.contains_key(*n))
        .collect();
    // Les biomes de côte et de calcaire sont rares près de l'origine : quelques plantes manquent.
    assert!(missing.len() <= 7, "plantes jamais vues : {missing:?}");
    for n in ["blabar", "hogt_gras", "ormbunke", "hjortron", "kantarell", "vass", "nackros"] {
        assert!(seen.contains_key(n), "{n} absente");
    }
}

/// Mesure de temps (ignorée par défaut) : `-- --ignored --nocapture`.
#[test]
#[ignore]
fn time_per_chunk() {
    let t = terrain(12345);
    let start = std::time::Instant::now();
    let mut n = 0;
    for cz in -20..20 {
        for cx in -20..20 {
            gen(&t, cx, cz);
            n += 1;
        }
    }
    println!("{:.2} ms/chunk", start.elapsed().as_secs_f64() * 1000.0 / f64::from(n));
}
