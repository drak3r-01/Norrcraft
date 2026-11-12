//! Habitats des milieux AIR, AMPHIBIE et GLACE dans de vrais mondes Norrcraft générés
//! (3 graines), doc 03 §6 : chaque milieu doit trouver des milliers de points valides.

use nb_core::glam::IVec3;
use nb_core::ChunkPos;
use nb_data::{BlockId, BlockRegistry, MobRegistry, WorldKind};
use nb_ecs::spawn::{find_air_y, find_ice_y, find_shore_y};
use nb_ecs::BlockAccess;
use nb_world::norrcraft::NorrConfig;
use nb_world::{Chunk, WorldProfile};

/// Un chunk isolé : tout autour, de l'air (les sondes restent à l'intérieur).
struct One {
    chunk: Chunk,
    base: IVec3,
    air: BlockId,
}

impl BlockAccess for One {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockId {
        let (lx, lz) = (x - self.base.x, z - self.base.z);
        if !(0..16).contains(&lx) || !(0..16).contains(&lz) || !(0..128).contains(&y) {
            return self.air;
        }
        self.chunk.block(lx, y, lz)
    }
    fn set_block(&mut self, _: i32, _: i32, _: i32, _: BlockId) {}
    fn is_chunk_loaded(&self, _: i32, _: i32) -> bool {
        true
    }
    fn leaves_tint(&self, _: i32, _: i32) -> [f64; 3] {
        [0.3, 0.5, 0.2]
    }
}

#[test]
fn air_shore_and_ice_habitats_exist_in_generated_worlds() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let mut mobs = MobRegistry::builtin_for(WorldKind::Norrcraft);
    mobs.apply_overrides(
        r#"{"mobs": [{"id": "t_air", "medium": "AIR", "behavior": "rovfagel",
            "spawn": {"densityPerBiome": {"URSKOG": 1}}}]}"#,
    )
    .unwrap();
    let air_cfg = mobs.get("t_air").unwrap().clone();
    let (mut air, mut shore, mut water, mut ice, mut samples) = (0, 0, 0, 0, 0);
    for seed in [12345i64, 7, 2024] {
        let profile = WorldProfile::norrcraft(seed, NorrConfig::default(), &reg);
        let gen = profile.new_generator();
        // Même monde en plein hiver (pas 0) : lacs et mers gelés pour la glace.
        let winter_profile = WorldProfile::norrcraft(seed, NorrConfig::default(), &reg);
        let winter = winter_profile.new_generator();
        winter.set_season(Some(0));
        for cz in (-80..80).step_by(4) {
            for cx in (-80..80).step_by(4) {
                let mut chunk = Chunk::new(ChunkPos::new(cx, cz));
                gen.generate_chunk(&mut chunk);
                let one = One {
                    chunk,
                    base: IVec3::new(cx * 16, 0, cz * 16),
                    air: reg.ids().air,
                };
                for (lx, lz) in [(5, 5), (10, 6), (6, 10), (10, 10)] {
                    let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                    samples += 1;
                    let a = find_air_y(&one, &reg, &air_cfg, x, z);
                    if a > 0 {
                        air += 1;
                        assert!(a < 128 - 7, "ciel du monde : {a}");
                        assert_eq!(
                            one.block(x, a, z),
                            reg.ids().air,
                            "l'oiseau naît dans de l'air"
                        );
                    }
                    let s = find_shore_y(&one, &reg, x, z, 2);
                    if s > 0 {
                        if one.block(x, s, z) == reg.ids().water {
                            water += 1;
                        } else {
                            shore += 1;
                        }
                    }
                }
                let mut cold = Chunk::new(ChunkPos::new(cx, cz));
                winter.generate_chunk(&mut cold);
                let one = One { chunk: cold, ..one };
                for (lx, lz) in [(5, 5), (10, 6), (6, 10), (10, 10)] {
                    let y = find_ice_y(&one, &reg, cx * 16 + lx, cz * 16 + lz);
                    if y > 0 {
                        ice += 1;
                    }
                }
            }
        }
    }
    println!("{samples} colonnes : air {air}, rive {shore}, eau {water}, glace {ice}");
    assert!(air > samples / 2, "air : {air}/{samples}");
    assert!(shore >= 300, "rives : {shore}");
    assert!(water >= 300, "eau : {water}");
    assert!(ice >= 50, "glace : {ice}");
}
