//! Eau vivante côté générateur : table d'eau et courant des rivières (doc 07 §2.3, §8.1, §15).

use std::collections::HashMap;

use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::fluid::{RestKind, WaterTable};
use nb_world::norrcraft::{NorrConfig, NorrPreset};
use nb_world::{Chunk, TerrainGenerator, WorldProfile};

fn profile(seed: i64) -> WorldProfile {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    WorldProfile::norrcraft(seed, NorrConfig::with_preset(NorrPreset::default()), &reg)
}

const SEEDS: [i64; 3] = [7, 42, 2026];

#[test]
fn only_norrcraft_has_a_water_table() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    assert!(profile(1).systems.living_water);
    assert!(profile(1).generator().water_table(0, 0).is_some());
    assert!(nb_world::FlatGraniteTerrain::from_registry(&reg)
        .water_table(0, 0)
        .is_none());
    assert!(nb_world::EmptyTerrain.water_table(0, 0).is_none());
    let vanilla = WorldProfile::vanilla(
        1,
        std::sync::Arc::new(nb_world::WorldConfig::default()),
        BlockRegistry::builtin().ids(),
    );
    assert!(!vanilla.systems.living_water);
}

/// Table égale aux colonnes générées : toute cellule `top..level` d'une colonne mouillée est de
/// l'eau, la cellule `level` n'en est pas.
#[test]
fn table_matches_generated_columns() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let water = reg.get("water").id;
    let fb = nb_world::fluid::FluidBlocks::from_registry(&reg);
    let wet_cell = |b: u8| b == water || fb.class_of(b) == nb_world::fluid::FluidClass::Aquatic;
    let mut wet_total = 0;
    for seed in SEEDS {
        let gen = profile(seed).new_generator();
        let mut wet = 0;
        for cz in -6..6 {
            for cx in -6..6 {
                let table = gen.water_table(cx, cz).expect("table");
                let mut chunk = Chunk::new(ChunkPos::new(cx, cz));
                gen.generate_chunk(&mut chunk);
                for lz in 0..16 {
                    for lx in 0..16 {
                        let r = table.0[lz * 16 + lx];
                        if r.kind == RestKind::Dry {
                            continue;
                        }
                        wet += 1;
                        for y in i32::from(r.top)..i32::from(r.level) {
                            assert!(
                                wet_cell(chunk.block(lx as i32, y, lz as i32)),
                                "seed {seed} chunk ({cx}, {cz}) colonne ({lx}, {lz}) y {y}"
                            );
                        }
                        assert_ne!(
                            chunk.block(lx as i32, i32::from(r.level), lz as i32),
                            water,
                            "seed {seed} : eau au-dessus du niveau déclaré"
                        );
                        if r.kind != RestKind::River {
                            assert_eq!(r.flow, [0, 0], "seule une rivière a un courant");
                        }
                    }
                }
            }
        }
        wet_total += wet;
    }
    assert!(wet_total > 1000, "trop peu d'eau explorée : {wet_total}");
}

/// Le courant d'une rivière est orienté vers l'aval : 8 blocs plus loin dans le sens du courant,
/// la nappe n'est jamais plus haute (elle descend par marches), sauf aux confluences.
#[test]
fn river_flow_points_downstream() {
    let mut rivers = 0usize;
    let mut checked = 0usize;
    let mut downhill = 0usize;
    for seed in SEEDS {
        let gen = profile(seed).new_generator();
        let mut tables: HashMap<(i32, i32), Box<WaterTable>> = HashMap::new();
        for cz in -10..10 {
            for cx in -10..10 {
                tables.insert((cx, cz), gen.water_table(cx, cz).expect("table"));
            }
        }
        let rest = |x: i32, z: i32| {
            tables
                .get(&(x.div_euclid(16), z.div_euclid(16)))
                .map(|t| t.0[(z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize])
        };
        for (&(cx, cz), t) in &tables {
            for lz in 0..16i32 {
                for lx in 0..16i32 {
                    let r = t.0[(lz * 16 + lx) as usize];
                    if r.kind != RestKind::River {
                        continue;
                    }
                    rivers += 1;
                    let (fx, fz) = (f32::from(r.flow[0]), f32::from(r.flow[1]));
                    let speed = (fx * fx + fz * fz).sqrt() / 16.0;
                    assert!(
                        (0.7..=4.6).contains(&speed),
                        "vitesse hors bornes : {speed} (seed {seed})"
                    );
                    let n = (fx * fx + fz * fz).sqrt();
                    let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                    let (qx, qz) = (x + (8.0 * fx / n).round() as i32, z + (8.0 * fz / n).round() as i32);
                    if let Some(q) = rest(qx, qz) {
                        if q.kind == RestKind::River {
                            checked += 1;
                            if q.level <= r.level {
                                downhill += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(rivers > 200, "trop peu de rivières explorées : {rivers}");
    assert!(checked > 100, "trop peu de colonnes comparables : {checked}");
    let ratio = downhill as f64 / checked as f64;
    assert!(ratio > 0.95, "courant à contre-pente : {downhill}/{checked}");
}
