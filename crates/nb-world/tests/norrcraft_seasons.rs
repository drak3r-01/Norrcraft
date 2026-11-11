//! Tests d'acceptation de la surcouche saisonnière Norrcraft (session 3.2 ; GDD doc 04 §2.3).

use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::{NorrBlocks, NorrConfig, NorrcraftTerrain};
use nb_world::{Chunk, TerrainGenerator};

const SEEDS: [i64; 2] = [12345, 20261111];
/// Pas de saison : fin janvier, mars, août.
const JAN: u8 = 0;
const MARCH: u8 = 1;
const AUG: u8 = 4;

fn registry() -> BlockRegistry {
    BlockRegistry::for_kind(WorldKind::Norrcraft)
}

fn terrain(seed: i64, step: Option<u8>) -> NorrcraftTerrain {
    let t = NorrcraftTerrain::new(
        seed,
        NorrConfig::default(),
        NorrBlocks::from_registry(&registry()),
    );
    t.set_season(step);
    t
}

fn gen(t: &NorrcraftTerrain, cx: i32, cz: i32) -> Chunk {
    let mut c = Chunk::new(ChunkPos::new(cx, cz));
    t.generate_chunk(&mut c);
    c
}

/// Chunks autour de l'origine, y compris ceux qui portent de l'eau.
fn grid() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for cz in (-30..30).step_by(5) {
        for cx in (-30..30).step_by(5) {
            v.push((cx, cz));
        }
    }
    v
}

fn count(c: &Chunk, ids: &[u8]) -> usize {
    c.voxels().iter().filter(|b| ids.contains(b)).count()
}

fn layer_ids(reg: &BlockRegistry) -> Vec<u8> {
    (1..=7)
        .map(|i| reg.get(&format!("snotacke_{i}")).id)
        .collect()
}

#[test]
fn no_season_leaves_the_end_of_summer_state() {
    let reg = registry();
    let ice = [reg.get("nyis").id, reg.get("is").id, reg.get("tjockis").id];
    let seasonal = [reg.get("kvistar").id, reg.get("ris").id];
    let t = terrain(SEEDS[0], None);
    for (cx, cz) in grid().into_iter().take(12) {
        let c = gen(&t, cx, cz);
        assert_eq!(count(&c, &seasonal), 0);
        assert_eq!(count(&c, &ice), 0);
        assert_eq!(count(&c, &layer_ids(&reg)), 0);
    }
}

#[test]
fn the_overlay_only_replaces_soft_cells_with_seasonal_blocks() {
    let reg = registry();
    let blocks = NorrBlocks::from_registry(&reg).season;
    for seed in SEEDS {
        let base = terrain(seed, None);
        for step in [JAN, MARCH, AUG] {
            let seasonal = terrain(seed, Some(step));
            for (cx, cz) in grid() {
                let (a, b) = (gen(&base, cx, cz), gen(&seasonal, cx, cz));
                for (i, (&ba, &bb)) in a.voxels().iter().zip(b.voxels().iter()).enumerate() {
                    assert!(
                        blocks.natural(bb, ba),
                        "seed {seed} pas {step} chunk ({cx}, {cz}) cellule {i} : {ba} devient {bb}"
                    );
                }
            }
        }
    }
}

#[test]
fn winter_brings_snow_that_summer_lacks() {
    let reg = registry();
    let layers = layer_ids(&reg);
    let (mut march, mut jan, mut aug) = (0, 0, 0);
    let (tm, tj, ta) = (
        terrain(SEEDS[0], Some(MARCH)),
        terrain(SEEDS[0], Some(JAN)),
        terrain(SEEDS[0], Some(AUG)),
    );
    for (cx, cz) in grid() {
        march += count(&gen(&tm, cx, cz), &layers);
        jan += count(&gen(&tj, cx, cz), &layers);
        aug += count(&gen(&ta, cx, cz), &layers);
    }
    println!("couches de neige : mars {march}, janvier {jan}, août {aug}");
    assert!(march > 500, "pas de neige en mars : {march}");
    assert!(jan > 500);
    assert!(
        aug * 10 < march,
        "trop de neige en août : {aug} contre {march}"
    );
}

#[test]
fn only_the_top_water_cell_freezes() {
    let reg = registry();
    let ice = [
        reg.get("nyis").id,
        reg.get("is").id,
        reg.get("tjockis").id,
        reg.get("havsis").id,
    ];
    let water = reg.get("water").id;
    let mut frozen = 0;
    let (tb, tw) = (terrain(SEEDS[0], None), terrain(SEEDS[0], Some(JAN)));
    for (cx, cz) in grid() {
        let base = gen(&tb, cx, cz);
        let w = gen(&tw, cx, cz);
        for z in 0..16 {
            for x in 0..16 {
                for y in 1..127 {
                    let (a, b) = (base.block(x, y, z), w.block(x, y, z));
                    if a == b || !ice.contains(&b) {
                        continue;
                    }
                    if a == water {
                        frozen += 1;
                        assert_eq!(
                            base.block(x, y + 1, z),
                            0,
                            "glace sous autre chose que l'air"
                        );
                    }
                }
            }
        }
    }
    println!("cellules gelées en janvier : {frozen}");
    assert!(frozen > 100, "aucun lac ne gèle : {frozen}");
}

#[test]
fn deciduous_leaves_fall_in_winter_and_return() {
    let reg = registry();
    let leaves: Vec<u8> = [
        "bjork_lov",
        "bok_lov",
        "ek_lov",
        "asp_lov",
        "al_lov",
        "ronn_lov",
        "salg_lov",
        "hassel_lov",
    ]
    .iter()
    .map(|n| reg.get(n).id)
    .collect();
    let (mut summer, mut winter) = (0, 0);
    let (ta, tj) = (terrain(SEEDS[1], Some(AUG)), terrain(SEEDS[1], Some(JAN)));
    for (cx, cz) in grid() {
        summer += count(&gen(&ta, cx, cz), &leaves);
        winter += count(&gen(&tj, cx, cz), &leaves);
    }
    println!("feuilles de feuillus : août {summer}, janvier {winter}");
    assert!(summer > 1000);
    assert!(
        winter * 20 < summer,
        "{winter} feuilles en janvier pour {summer} en août"
    );
}

#[test]
fn reseason_turns_one_season_into_another_exactly() {
    for (cx, cz) in [(0, 0), (3, -4), (-7, 9), (12, 12), (-20, -15)] {
        let t = terrain(SEEDS[0], Some(AUG));
        let mut c = gen(&t, cx, cz);
        let summer = c.clone();
        t.set_season(Some(JAN));
        let changed = t.reseason(&mut c);
        let fresh = gen(&t, cx, cz);
        assert_eq!(c.voxels(), fresh.voxels(), "chunk ({cx}, {cz})");
        assert_eq!(changed, summer.voxels() != fresh.voxels());
        assert!(
            !t.reseason(&mut c),
            "une deuxième passe ne doit rien changer"
        );
        // Et retour à l'été.
        t.set_season(Some(AUG));
        t.reseason(&mut c);
        assert_eq!(c.voxels(), summer.voxels());
    }
}

#[test]
fn columns_edited_by_the_player_are_never_overwritten() {
    let reg = registry();
    let stone = reg.get("granit").id;
    let layers = layer_ids(&reg);
    let t = terrain(SEEDS[0], Some(AUG));
    // Un chunk de terre ferme qui reçoit de la neige en mars.
    let tm = terrain(SEEDS[0], Some(MARCH));
    let (cx, cz) = grid()
        .into_iter()
        .find(|&(cx, cz)| count(&gen(&tm, cx, cz), &layers) > 80)
        .expect("un chunk enneigé");
    let mut c = gen(&t, cx, cz);
    // Le joueur pose un bloc de granit au-dessus du sol dans la colonne (8, 8) et creuse (3, 3).
    let ground = |c: &Chunk, x: i32, z: i32| {
        (1..126)
            .rev()
            .find(|&y| c.block(x, y, z) != 0 && c.block(x, y + 1, z) == 0)
            .unwrap()
    };
    let g = ground(&c, 8, 8);
    c.set_block(8, g + 1, 8, stone);
    let d = ground(&c, 3, 3);
    c.set_block(3, d, 3, 0);
    let col = |c: &Chunk, x: i32, z: i32| (0..128).map(|y| c.block(x, y, z)).collect::<Vec<_>>();
    let (before, dug) = (col(&c, 8, 8), col(&c, 3, 3));
    t.set_season(Some(MARCH));
    assert!(
        t.reseason(&mut c),
        "le reste du chunk doit recevoir la neige"
    );
    assert_eq!(col(&c, 8, 8), before);
    assert_eq!(col(&c, 3, 3), dug);
}

#[test]
fn generation_is_deterministic_per_season() {
    for step in [JAN, AUG] {
        let a = gen(&terrain(SEEDS[1], Some(step)), -3, 7);
        let b = gen(&terrain(SEEDS[1], Some(step)), -3, 7);
        assert_eq!(a.voxels(), b.voxels());
    }
}

#[test]
#[ignore]
fn time_per_chunk() {
    let t = terrain(SEEDS[0], Some(MARCH));
    let start = std::time::Instant::now();
    let n = 60;
    for i in 0..n {
        gen(&t, i % 10, i / 10);
    }
    println!(
        "génération avec surcouche : {:?} par chunk",
        start.elapsed() / n as u32
    );
    let mut c = gen(&terrain(SEEDS[0], Some(AUG)), 0, 0);
    let start = std::time::Instant::now();
    for _ in 0..n {
        t.set_season(Some(AUG));
        t.reseason(&mut c);
        t.set_season(Some(MARCH));
        t.reseason(&mut c);
    }
    println!(
        "reseason : {:?} par appel",
        start.elapsed() / (2 * n as u32)
    );
}
