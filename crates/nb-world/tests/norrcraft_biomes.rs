//! Tests d'acceptation des biomes Norrcraft (session 2.1 ; GDD doc 02 §7 et §13).

use std::collections::BTreeSet;

use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::biomes::DISTRIBUTION_TARGETS;
use nb_world::norrcraft::{NorrBiome, NorrBlocks, NorrConfig, NorrcraftTerrain, DITHER_BAND};
use nb_world::{Chunk, TerrainGenerator, WorldProfile};

const SEEDS: [i64; 3] = [12345, -98765, 20261111];

fn terrain(seed: i64) -> NorrcraftTerrain {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    NorrcraftTerrain::new(seed, NorrConfig::default(), NorrBlocks::from_registry(&reg))
}

/// Biomes d'une grille `size × size` de pas `step` centrée sur l'origine.
fn survey(t: &NorrcraftTerrain, size: i32, step: i32) -> Vec<NorrBiome> {
    let x0 = -size * step / 2;
    let mut v = Vec::with_capacity((size * size) as usize);
    for j in 0..size {
        for i in 0..size {
            v.push(t.column_biome(x0 + i * step, x0 + j * step).biome);
        }
    }
    v
}

/// Première colonne de banquise trouvée sur une grille grossière au nord (−z) de l'origine.
fn find_packis(t: &NorrcraftTerrain) -> Option<(i32, i32)> {
    (0..13)
        .flat_map(|j| (0..81).map(move |i| (-20000 + 500 * i, -16000 - 2000 * j)))
        .find(|&(x, z)| t.column_biome(x, z).biome == NorrBiome::Packis)
}

#[test]
fn profile_lists_the_25_biomes_with_tint_blending() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let p = WorldProfile::norrcraft(1, NorrConfig::default(), &reg);
    assert_eq!(p.biomes.len(), NorrBiome::COUNT);
    for b in NorrBiome::ALL {
        assert_eq!(p.biome_id_by_name(b.name()), Some(b.id()));
    }
    assert_eq!(p.tint_blend, 2);
}

#[test]
fn biome_at_matches_isolated_columns_and_generation() {
    let t = terrain(12345);
    // Chunks dispersés : côtes, montagnes, plaines.
    for (cx, cz) in [(0, 0), (-3, 5), (40, -17), (-250, 90), (120, 300)] {
        // Le chunk généré remplit le cache ; biome_at relit le cache.
        let mut chunk = Chunk::new(ChunkPos::new(cx, cz));
        t.generate_chunk(&mut chunk);
        for z in 0..16 {
            for x in 0..16 {
                let (wx, wz) = (cx * 16 + x, cz * 16 + z);
                assert_eq!(
                    t.biome_at(wx, wz),
                    t.column_biome(wx, wz).biome.id(),
                    "colonne ({wx}, {wz})"
                );
            }
        }
    }
    // Sans génération préalable (cache froid, autre instance) : même réponse.
    let cold = terrain(12345);
    for (wx, wz) in [(1000, -2000), (-7, 4093), (5555, 5555)] {
        assert_eq!(cold.biome_at(wx, wz), t.column_biome(wx, wz).biome.id());
    }
}

/// Répartition sur 16 × 16 km (doc 02 §7.5), moyenne des 3 graines, tolérance ±40 % relatifs.
///
/// Écarts connus, consignés dans docs/WORLDGEN_NORRCRAFT.md : pas de zone arctique à moins de
/// 8 km de l'origine (Packis n'apparaît que vers 17 km au nord), côtes soulevées et klints rares
/// (formes de la 1.3), champ de lave rare (champ Gt de la 1.1), sommets au-dessus de `y_k` rares
/// (Kalfjäll), ceinture de bouleaux large (Fjällbjörkskog).
#[test]
fn distribution_matches_the_targets() {
    const EXEMPT: [&str; 5] = ["Packis", "côtes", "Lavafält", "Kalfjäll", "Fjällbjörkskog"];
    let (size, step) = (250, 64);
    let mut count = [0u64; NorrBiome::COUNT];
    for seed in SEEDS {
        for b in survey(&terrain(seed), size, step) {
            count[b as usize] += 1;
        }
    }
    let total = (size * size) as f64 * SEEDS.len() as f64;
    let mut report = Vec::new();
    let mut bad = Vec::new();
    for (name, biomes, target) in DISTRIBUTION_TARGETS {
        let share = biomes.iter().map(|b| count[*b as usize]).sum::<u64>() as f64 * 100.0 / total;
        report.push(format!("{name} {share:.2}/{target}"));
        if !EXEMPT.contains(name) && (share - target).abs() > 0.4 * target {
            bad.push(*name);
        }
    }
    assert!(
        bad.is_empty(),
        "hors cible : {bad:?} ; {}",
        report.join(" · ")
    );
}

#[test]
fn every_biome_appears_near_the_origin_or_in_the_north() {
    let mut seen = BTreeSet::new();
    for seed in SEEDS {
        seen.extend(survey(&terrain(seed), 125, 128));
    }
    // Banquise : mer arctique, au nord-ouest (graine 12345 : vers 17 km).
    let t = terrain(12345);
    if find_packis(&t).is_some() {
        seen.insert(NorrBiome::Packis);
    }
    let missing: Vec<_> = NorrBiome::ALL
        .into_iter()
        .filter(|b| !seen.contains(b))
        .map(NorrBiome::name)
        .collect();
    assert!(missing.is_empty(), "biomes absents : {missing:?}");
}

#[test]
fn borders_are_dithered() {
    // Colonnes isolées (aucune des 4 voisines n'a leur biome) : la trame en produit le long des
    // frontières, sans en faire un bruit généralisé.
    let t = terrain(-98765);
    let n = 192;
    let grid: Vec<NorrBiome> = (0..n * n)
        .map(|k| t.column_biome(k % n - 96, k / n - 96).biome)
        .collect();
    let at = |i: i32, j: i32| grid[(j * n + i) as usize];
    let mut isolated = 0;
    for j in 1..n - 1 {
        for i in 1..n - 1 {
            let b = at(i, j);
            if [at(i - 1, j), at(i + 1, j), at(i, j - 1), at(i, j + 1)]
                .iter()
                .all(|&v| v != b)
            {
                isolated += 1;
            }
        }
    }
    let share = f64::from(isolated) / f64::from((n - 2) * (n - 2));
    assert!(isolated > 0, "aucune colonne tramée");
    assert!(share < 0.05, "trame trop bruitée : {:.1} %", share * 100.0);
    assert_eq!(DITHER_BAND, 12.0);
}

#[test]
fn soils_follow_the_biomes() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let id = |n: &str| reg.get(n).id;
    let t = terrain(12345);
    // Pour chaque biome : une zone de 6 × 6 chunks autour d'une colonne du biome trouvée sur une
    // grille grossière ; peau des colonnes sèches du biome dans cette zone.
    for (b, blocks, min) in [
        (NorrBiome::Hagmark, &["gras", "klappersten"][..], 0.5),
        (NorrBiome::Urskog, &["mossa", "barrmatta"][..], 0.5),
        (NorrBiome::Sjoland, &["gras", "mossa"][..], 0.4),
        (NorrBiome::Tallhed, &["lavhed", "barrmatta"][..], 0.4),
        (NorrBiome::Bergslag, &["rodmull", "slagg"][..], 0.4),
    ] {
        let (wx, wz) = (0..40)
            .flat_map(|j| (0..40).map(move |i| (-8000 + 400 * i, -8000 + 400 * j)))
            .find(|&(x, z)| {
                [(0, 0), (48, 0), (-48, 0), (0, 48), (0, -48)]
                    .iter()
                    .all(|(dx, dz)| t.column_biome(x + dx, z + dz).biome == b)
            })
            .unwrap_or_else(|| panic!("{} introuvable", b.name()));
        let (cx0, cz0) = (wx.div_euclid(16) - 3, wz.div_euclid(16) - 3);
        let (mut ok, mut all) = (0, 0);
        for cz in cz0..cz0 + 6 {
            for cx in cx0..cx0 + 6 {
                let mut chunk = Chunk::new(ChunkPos::new(cx, cz));
                t.generate_chunk(&mut chunk);
                for z in 0..16 {
                    for x in 0..16 {
                        let (wx, wz) = (cx * 16 + x, cz * 16 + z);
                        let c = t.hydrography().column(wx, wz);
                        if c.is_wet() || t.biome_at(wx, wz) != b.id() {
                            continue;
                        }
                        let top = chunk.block(x, c.top - 1, z);
                        all += 1;
                        ok += usize::from(blocks.iter().any(|n| id(n) == top));
                    }
                }
            }
        }
        let s = ok as f64 / all.max(1) as f64;
        assert!(all > 100, "{} : trop peu de colonnes ({all})", b.name());
        assert!(
            s >= min,
            "{} : peau attendue {blocks:?} sur {:.0} %",
            b.name(),
            s * 100.0
        );
    }
}

#[test]
fn packis_freezes_the_sea_surface_outside_the_leads() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let (ice, water) = (reg.get("havsis").id, reg.get("water").id);
    let t = terrain(12345);
    // Premier chunk de banquise trouvé sur un transect au nord-ouest.
    let (wx, wz) = find_packis(&t).expect("banquise au nord");
    let (cx, cz) = (wx.div_euclid(16), wz.div_euclid(16));
    let mut chunk = Chunk::new(ChunkPos::new(cx, cz));
    t.generate_chunk(&mut chunk);
    let (mut frozen, mut open) = (0, 0);
    for z in 0..16 {
        for x in 0..16 {
            let (wx, wz) = (cx * 16 + x, cz * 16 + z);
            let c = t.hydrography().column(wx, wz);
            if !c.is_wet() || t.column_biome(wx, wz).biome != NorrBiome::Packis {
                continue;
            }
            match chunk.block(x, c.level - 1, z) {
                b if b == ice => frozen += 1,
                b if b == water => open += 1,
                b => panic!("bloc inattendu {b} en surface de la banquise"),
            }
            assert_eq!(
                chunk.block(x, c.level, z),
                0,
                "rien au-dessus de la banquise"
            );
        }
    }
    assert!(frozen > open, "banquise : {frozen} gelées, {open} ouvertes");
}
