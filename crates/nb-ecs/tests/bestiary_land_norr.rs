//! Habitats du lot terrestre de Norrcraft (session 5.4, doc 03 §6) : chaque espèce trouve des
//! points d'apparition valides dans de vrais mondes générés (3 graines). Les biomes sont
//! d'abord repérés par un balayage grossier, puis un carré de chunks est généré autour de deux
//! sites par biome ; les points sont comptés avec le prédicat réel du système d'apparition.

use std::collections::{BTreeMap, BTreeSet};

use nb_core::glam::IVec3;
use nb_core::ChunkPos;
use nb_data::{BlockId, BlockRegistry, MobMedium, MobRegistry, WorldKind};
use nb_ecs::spawn::spawn_column_y;
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

/// Biomes où une espèce peut apparaître (densités, sinon sites de harde).
fn biomes_of(c: &nb_data::MobConfig) -> BTreeSet<String> {
    let r = c.spawn.as_ref().unwrap();
    let mut out: BTreeSet<String> = r.density_per_biome.keys().cloned().collect();
    if let Some(h) = &r.herd {
        out.extend(h.summer.iter().chain(&h.winter).cloned());
    }
    out
}

#[test]
fn every_land_species_has_viable_habitats_in_generated_worlds() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let mobs = MobRegistry::builtin_for(WorldKind::Norrcraft);
    let wanted: BTreeSet<String> = mobs.all().iter().flat_map(biomes_of).collect();
    // espèce → biome → points valides.
    let mut found: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
    let mut sites_seen: BTreeMap<String, u32> = BTreeMap::new();
    for seed in [12345i64, 7, 2024] {
        let profile = WorldProfile::norrcraft(seed, NorrConfig::default(), &reg);
        let gen = profile.new_generator();
        let winter_profile = WorldProfile::norrcraft(seed, NorrConfig::default(), &reg);
        let winter = winter_profile.new_generator();
        winter.set_season(Some(0));
        // Balayage grossier : deux sites par biome voulu, espacés d'au moins 1 500 blocs.
        let mut sites: BTreeMap<String, Vec<(i32, i32)>> = BTreeMap::new();
        for gz in (-6000..6000).step_by(160) {
            for gx in (-6000..6000).step_by(160) {
                let name = &profile.biomes[usize::from(gen.biome_at(gx, gz))].name;
                if !wanted.contains(name) {
                    continue;
                }
                let list = sites.entry(name.clone()).or_default();
                let far = |s: &(i32, i32)| (s.0 - gx).abs().max((s.1 - gz).abs()) > 1500;
                if list.len() < 2 && list.iter().all(far) {
                    list.push((gx, gz));
                }
            }
        }
        // La banquise est loin de l'origine : balayage plus large, premier site seulement.
        if wanted.contains("PACKIS") {
            let mut best: Option<(i32, i32, i32)> = None;
            for gz in (-20000..20000).step_by(400) {
                for gx in (-20000..20000).step_by(400) {
                    if profile.biomes[usize::from(gen.biome_at(gx, gz))].name == "PACKIS" {
                        let d = gx.abs().max(gz.abs());
                        if best.is_none_or(|b| d < b.2) {
                            best = Some((gx, gz, d));
                        }
                    }
                }
            }
            if let Some((gx, gz, _)) = best {
                sites.entry("PACKIS".into()).or_default().push((gx, gz));
            }
        }
        for (b, list) in &sites {
            *sites_seen.entry(b.clone()).or_default() += list.len() as u32;
        }
        for (_, list) in sites {
            for (sx, sz) in list {
                let (ccx, ccz) = (sx.div_euclid(16), sz.div_euclid(16));
                for cz in ccz - 3..=ccz + 3 {
                    for cx in ccx - 3..=ccx + 3 {
                        let mut summer = Chunk::new(ChunkPos::new(cx, cz));
                        gen.generate_chunk(&mut summer);
                        let mut cold = Chunk::new(ChunkPos::new(cx, cz));
                        winter.generate_chunk(&mut cold);
                        let base = IVec3::new(cx * 16, 0, cz * 16);
                        let air = reg.ids().air;
                        let s = One { chunk: summer, base, air };
                        let w = One { chunk: cold, base, air };
                        for (lx, lz) in [(5, 5), (10, 6), (6, 10), (10, 10)] {
                            let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                            let biome = profile.biomes[usize::from(gen.biome_at(x, z))].name.clone();
                            for c in mobs.all() {
                                let in_biome = biomes_of(c).contains(&biome);
                                if !in_biome {
                                    continue;
                                }
                                let access: &One = if c.medium == MobMedium::Ice { &w } else { &s };
                                let y = spawn_column_y(access, &reg, c, x, z);
                                if y > 0 {
                                    *found.entry(c.id.clone()).or_default().entry(biome.clone()).or_default() += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    println!("sites par biome : {sites_seen:?}");
    let mut failures = Vec::new();
    for c in mobs.all() {
        let per_biome = found.get(&c.id).cloned().unwrap_or_default();
        let total: u32 = per_biome.values().sum();
        println!("{:11} {total:5}  {per_biome:?}", c.id);
        if total < 100 {
            failures.push(format!("{} : {total} points", c.id));
        }
    }
    assert!(failures.is_empty(), "espèces sans habitat : {failures:?}");
}
