//! Animaux de la phase 9 : données valides et conditions d'apparition.

mod common;

use std::collections::HashSet;

use common::Harness;
use nb_core::glam::DVec3;
use nb_data::{BlockRegistry, MobMedium, MobRegistry, BUILTIN_MOB_IDS};
use nb_ecs::GameMode;
use nb_world::{Biome, BiomeInfo};

fn new_mobs() -> Vec<nb_data::MobConfig> {
    MobRegistry::builtin()
        .all()
        .iter()
        .filter(|c| !BUILTIN_MOB_IDS.contains(&c.id.as_str()))
        .cloned()
        .collect()
}

#[test]
fn fauna_data_is_valid() {
    let reg = BlockRegistry::builtin();
    let biome_names: HashSet<&str> = Biome::ALL.iter().map(|b| b.name()).collect();
    let mobs = new_mobs();
    assert!(mobs.len() >= 40, "{} animaux", mobs.len());
    for c in &mobs {
        let id = &c.id;
        assert!(
            c.geometry.as_ref().is_some_and(|g| g.boxes.len() >= 4),
            "{id} : géométrie"
        );
        assert!(
            c.width_x > 0.2 && c.width_z > 0.2 && c.height > 0.2,
            "{id} : taille"
        );
        assert!(
            c.width_x < 4.0 && c.width_z < 4.0 && c.height < 6.0,
            "{id} : taille démesurée"
        );
        assert!(c.max_health > 0.0 && c.max_count > 0, "{id}");
        assert!(
            c.group_size_min >= 1 && c.group_size_min <= c.group_size_max,
            "{id} : groupe"
        );
        assert!(c.group_size_max <= c.max_count, "{id} : groupe > plafond");
        assert!(
            c.move_speed_min <= c.move_speed_max && c.move_duration_min <= c.move_duration_max,
            "{id}"
        );
        assert!(c.pause_duration_min <= c.pause_duration_max, "{id}");
        assert!(
            !c.spawn_biomes.is_empty(),
            "{id} : aucun biome d'apparition"
        );
        for b in &c.spawn_biomes {
            assert!(biome_names.contains(b.as_str()), "{id} : biome inconnu {b}");
        }
        for s in &c.spawn_surfaces {
            assert_ne!(reg.get(s).id, reg.ids().air, "{id} : bloc inconnu {s}");
        }
        assert!(c.has_habitat(), "{id}");
        match c.medium {
            MobMedium::Land => assert_eq!(c.spawn_min_water_depth, 0, "{id}"),
            MobMedium::Water => assert!(c.spawn_min_water_depth >= 2, "{id} : profondeur"),
            MobMedium::Air => panic!("{id} : milieu aérien non géré"),
        }
        if c.behavior_type == nb_data::MobBehaviorType::Aggressive {
            assert!(
                c.aggro_player_range > 0.0 && c.chase_speed > 0.0 && c.attack_damage > 0.0,
                "{id}"
            );
            assert!(c.attack_range > 0.0 && c.attack_cooldown > 0.0, "{id}");
        }
        for b in &c.geometry.as_ref().unwrap().boxes {
            assert!(b[3] > 0.0 && b[4] > 0.0 && b[5] > 0.0, "{id} : boîte vide");
            assert!(
                b[6..9].iter().all(|v| (0.0..=1.0).contains(v)) && b[9] > 0.0 && b[9] <= 1.0,
                "{id}"
            );
        }
    }
}

#[test]
fn every_biome_hosts_some_land_or_sea_life() {
    let mobs = new_mobs();
    // Hors rivières / mer, chaque biome terrestre accueille au moins un animal.
    for b in Biome::ALL {
        if b == Biome::River {
            continue;
        }
        assert!(
            mobs.iter()
                .any(|c| c.spawn_biomes.iter().any(|n| n == b.name())),
            "aucun animal pour {b:?}"
        );
    }
}

#[test]
fn original_mobs_keep_their_java_spawn_rules() {
    for id in BUILTIN_MOB_IDS {
        let reg = MobRegistry::builtin();
        let c = reg.get(id).unwrap();
        assert!(
            !c.has_habitat(),
            "{id} ne doit pas avoir de condition d'apparition"
        );
    }
}

/// Sol de 160×160 en `y = 64` d'un bloc donné ; joueur au centre.
fn land_harness(floor: &str, biome: Option<Biome>, mob: &str) -> Harness {
    let mut h = Harness::new();
    let b = h.reg.get(floor).id;
    h.slab(-80, 80, 64, -80, 80, b);
    h.blocks
        .set_biome(biome.map(|b| BiomeInfo::named(b.name())));
    h.enable_spawn(mob);
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Creative);
    h
}

#[test]
fn land_mob_spawns_only_in_its_biome_and_on_its_surface() {
    // Renard polaire : neige + biome arctique.
    let mut h = land_harness("snow", Some(Biome::SnowyPlains), "arctic_fox");
    h.ticks(60 * 20);
    assert!(
        h.count_mobs("arctic_fox") > 0,
        "doit apparaître en plaine enneigée"
    );

    let mut h = land_harness("snow", Some(Biome::Desert), "arctic_fox");
    h.ticks(60 * 20);
    assert_eq!(
        h.count_mobs("arctic_fox"),
        0,
        "pas de renard polaire dans le désert"
    );

    // Bon biome mais mauvais sol : le chameau veut du sable.
    let mut h = land_harness("stone", Some(Biome::Desert), "camel");
    h.ticks(60 * 20);
    assert_eq!(h.count_mobs("camel"), 0);
    let mut h = land_harness("sand", Some(Biome::Desert), "camel");
    h.ticks(60 * 20);
    assert!(h.count_mobs("camel") > 0);
    // Sans biomes dans le monde (tests, mondes de blocs), la condition de biome est ignorée.
    let mut h = land_harness("sand", None, "camel");
    h.ticks(60 * 20);
    assert!(h.count_mobs("camel") > 0);
}

#[test]
fn land_mobs_never_spawn_on_leaves() {
    let mut h = land_harness("leaves", Some(Biome::Forest), "wild_boar");
    h.ticks(60 * 20);
    assert_eq!(
        h.count_mobs("wild_boar"),
        0,
        "pas de sanglier dans la canopée"
    );
    let mut h = land_harness("grass", Some(Biome::Forest), "wild_boar");
    h.ticks(60 * 20);
    assert!(h.count_mobs("wild_boar") > 0);
}

#[test]
fn altitude_condition_is_respected() {
    // Le bouquetin n'apparaît qu'en altitude (y ≥ 46) : le sol de 64 convient, un sol bas non.
    let mut h = land_harness("stone", Some(Biome::Mountains), "mountain_goat");
    h.ticks(60 * 20);
    assert!(h.count_mobs("mountain_goat") > 0);
    let mut h = Harness::new();
    let stone = h.reg.get("stone").id;
    h.slab(-80, 80, 30, -80, 80, stone);
    h.blocks
        .set_biome(Some(BiomeInfo::named(Biome::Mountains.name())));
    h.enable_spawn("mountain_goat");
    h.ticks(60 * 20);
    assert_eq!(h.count_mobs("mountain_goat"), 0);
}

/// Colonne d'eau de `depth` blocs sur du sable (fond en `y = 39`).
fn sea_harness(depth: i32, biome: Biome, mob: &str) -> Harness {
    let mut h = Harness::new();
    let ids = h.ids();
    h.slab(-80, 80, 39, -80, 80, ids.sand);
    for y in 40..40 + depth {
        h.slab(-80, 80, y, -80, 80, ids.water);
    }
    h.blocks.set_biome(Some(BiomeInfo::named(biome.name())));
    h.enable_spawn(mob);
    h
}

#[test]
fn water_mobs_follow_biome_and_depth() {
    let mut h = sea_harness(4, Biome::River, "salmon");
    h.ticks(60 * 20);
    assert!(h.count_mobs("salmon") > 0, "saumon en rivière peu profonde");
    // Trop profond pour une rivière (max 6) : rien.
    let mut h = sea_harness(14, Biome::River, "salmon");
    h.ticks(60 * 20);
    assert_eq!(h.count_mobs("salmon"), 0);
    // Mauvais biome.
    let mut h = sea_harness(4, Biome::Ocean, "salmon");
    h.ticks(60 * 20);
    assert_eq!(h.count_mobs("salmon"), 0);
    // Thon : océan profond seulement.
    let mut h = sea_harness(12, Biome::Ocean, "tuna");
    h.ticks(60 * 20);
    assert!(h.count_mobs("tuna") > 0);
    let mut h = sea_harness(2, Biome::Ocean, "tuna");
    h.ticks(60 * 20);
    assert_eq!(h.count_mobs("tuna"), 0, "trop peu d'eau");
}

#[test]
fn fauna_walks_without_panicking_for_a_while() {
    // Tous les animaux actifs ensemble, monde mixte : la simulation tient et reste bornée.
    let mut h = Harness::new();
    let ids = h.ids();
    h.slab(-90, 90, 64, -90, 90, ids.grass);
    h.blocks
        .set_biome(Some(BiomeInfo::named(Biome::Forest.name())));
    let all: Vec<String> = new_mobs().iter().map(|c| c.id.clone()).collect();
    for id in &all {
        h.enable_spawn(id);
    }
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    h.ticks(60 * 60);
    let total: usize = all.iter().map(|i| h.count_mobs(i)).sum();
    assert!(total > 0 && total < 400, "{total} animaux");
}

/// Chaque animal doit pouvoir apparaître quelque part dans de vrais mondes générés : sinon sa
/// condition de biome / sol / altitude est contradictoire avec le terrain.
#[test]
fn every_animal_has_a_viable_habitat_in_generated_worlds() {
    use std::collections::HashMap;
    use std::sync::Arc;

    use nb_core::ChunkPos;
    use nb_world::{BiomeSystem, Chunk, Noise, WorldConfig, WorldGenerator, WorldType};

    let reg = BlockRegistry::builtin();
    let ids = *reg.ids();
    let mobs = new_mobs();
    let mut viable: HashMap<String, usize> = mobs.iter().map(|c| (c.id.clone(), 0)).collect();

    for (seed, ty) in [
        (12345i64, WorldType::Normal),
        (7, WorldType::Archipelago),
        (2024, WorldType::Highlands),
    ] {
        let cfg = Arc::new(WorldConfig::preset(ty));
        let generator = WorldGenerator::new(seed, cfg.clone(), &ids);
        let bs = BiomeSystem::new(Arc::new(Noise::new(seed)), cfg, &ids);
        for cz in (-60..60).step_by(2) {
            for cx in (-60..60).step_by(2) {
                let mut chunk = Chunk::new(ChunkPos::new(cx, cz));
                generator.generate_chunk(&mut chunk);
                for (lx, lz) in [(3, 3), (12, 4), (5, 11), (13, 13)] {
                    let (wx, wz) = (cx * 16 + lx, cz * 16 + lz);
                    let biome = bs.biome_at(wx, wz);
                    // Surface terrestre (premier solide sous de l'air), comme find_surface_y.
                    let mut land: Option<(i32, String)> = None;
                    for y in (1..=127).rev() {
                        if reg.from_id(chunk.block(lx, y, lz)).solid
                            && chunk.block(lx, y + 1, lz) == ids.air
                        {
                            land = Some((y + 1, reg.from_id(chunk.block(lx, y, lz)).name.clone()));
                            break;
                        }
                    }
                    // Colonne d'eau : sommet, profondeur, fond.
                    let mut sea: Option<(i32, i32, String)> = None;
                    if let Some(top) = (1..=127)
                        .rev()
                        .find(|&y| chunk.block(lx, y, lz) == ids.water)
                    {
                        let mut f = top;
                        while f > 0 && chunk.block(lx, f, lz) == ids.water {
                            f -= 1;
                        }
                        sea = Some((
                            top,
                            top - f,
                            reg.from_id(chunk.block(lx, f, lz)).name.clone(),
                        ));
                    }
                    for c in &mobs {
                        if !c.spawn_biomes.iter().any(|n| n == biome.name()) {
                            continue;
                        }
                        let ok = match c.medium {
                            MobMedium::Land => land.as_ref().is_some_and(|(y, name)| {
                                *y >= c.spawn_min_y
                                    && (c.spawn_max_y <= 0 || *y <= c.spawn_max_y)
                                    && !name.ends_with("leaves")
                                    && name != "cactus"
                                    && (c.spawn_surfaces.is_empty()
                                        || c.spawn_surfaces.contains(name))
                            }),
                            MobMedium::Water => sea.as_ref().is_some_and(|(y, depth, floor)| {
                                *depth >= c.spawn_min_water_depth
                                    && (c.spawn_max_water_depth <= 0
                                        || *depth <= c.spawn_max_water_depth)
                                    && *y >= c.spawn_min_y
                                    && (c.spawn_surfaces.is_empty()
                                        || c.spawn_surfaces.contains(floor))
                            }),
                            MobMedium::Air => false,
                        };
                        if ok {
                            *viable.get_mut(&c.id).unwrap() += 1;
                        }
                    }
                }
            }
        }
    }
    let mut report: Vec<_> = viable.iter().collect();
    report.sort();
    let none: Vec<_> = report
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(k, _)| k.as_str())
        .collect();
    println!("{report:?}");
    assert!(none.is_empty(), "habitats introuvables : {none:?}");
}
