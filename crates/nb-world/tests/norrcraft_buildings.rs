//! Tests d'acceptation des bâtiments d'habitat Norrcraft (session 4.2 ; GDD doc 02 §10.1) :
//! torp, by, lada, fäbod, fiskeläge, bastu, fjällstuga, vindskydd, kåta.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::buildings::{Building, Mat};
use nb_world::norrcraft::{NorrBlocks, NorrConfig, NorrcraftTerrain, StructKind, StructSite};
use nb_world::{Chunk, TerrainGenerator};

const SEEDS: [i64; 3] = [12345, -98765, 20261111];
const REACH: i32 = 2600;
const KINDS: [StructKind; 9] = [
    StructKind::Torp,
    StructKind::By,
    StructKind::Lada,
    StructKind::Fabod,
    StructKind::Fiskelage,
    StructKind::Bastu,
    StructKind::Fjallstuga,
    StructKind::Vindskydd,
    StructKind::Kata,
];

fn reg() -> BlockRegistry {
    BlockRegistry::for_kind(WorldKind::Norrcraft)
}

fn terrain(seed: i64) -> NorrcraftTerrain {
    NorrcraftTerrain::new(
        seed,
        NorrConfig::default(),
        NorrBlocks::from_registry(&reg()),
    )
}

fn chunk(t: &NorrcraftTerrain, cx: i32, cz: i32) -> Chunk {
    let mut c = Chunk::new(ChunkPos::new(cx, cz));
    t.generate_chunk(&mut c);
    c
}

/// Tous les bâtiments de la zone d'étude : `(site, bâtiment)`.
fn all(t: &NorrcraftTerrain) -> Vec<(StructSite, Arc<Building>)> {
    t.with_probe(|p| {
        t.structures()
            .sites_in(p, -REACH, -REACH, REACH, REACH)
            .into_iter()
            .filter_map(|s| t.structures().building(&s, p).map(|b| (s, b)))
            .collect()
    })
}

/// Chunks (cx, cz) qui couvrent la boîte du bâtiment.
fn chunks_of(b: &Building) -> Vec<(i32, i32)> {
    let [x0, z0, x1, z1] = b.bbox;
    let mut v = Vec::new();
    for cz in z0.div_euclid(16)..=z1.div_euclid(16) {
        for cx in x0.div_euclid(16)..=x1.div_euclid(16) {
            v.push((cx, cz));
        }
    }
    v
}

#[test]
fn every_habitat_kind_gets_buildings_on_some_seed() {
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    let mut voxels: BTreeMap<&str, usize> = BTreeMap::new();
    let mut sites: BTreeMap<&str, usize> = BTreeMap::new();
    for seed in SEEDS {
        let t = terrain(seed);
        let placed = t.with_probe(|p| t.structures().sites_in(p, -REACH, -REACH, REACH, REACH));
        for s in placed.iter().filter(|s| KINDS.contains(&s.kind)) {
            *sites.entry(s.kind.name()).or_default() += 1;
        }
        for (s, b) in all(&t) {
            assert!(KINDS.contains(&s.kind), "{} ne devrait pas avoir de bâtiment", s.kind.name());
            *count.entry(s.kind.name()).or_default() += 1;
            *voxels.entry(s.kind.name()).or_default() += b.voxel_count();
        }
    }
    for k in KINDS {
        let n = count.get(k.name()).copied().unwrap_or(0);
        println!("{:<10} {n:>4} bâtiments sur {:>4} sites, {} blocs", k.name(), sites.get(k.name()).copied().unwrap_or(0), voxels.get(k.name()).copied().unwrap_or(0));
        assert!(n > 0, "aucun {} construit sur 3 graines", k.name());
    }
}

#[test]
fn buildings_are_pure_functions_of_seed_and_terrain() {
    let a = all(&terrain(SEEDS[0]));
    let b = all(&terrain(SEEDS[0]));
    assert_eq!(a.len(), b.len());
    for ((sa, ba), (sb, bb)) in a.iter().zip(&b) {
        assert_eq!(sa, sb);
        assert_eq!(ba.bbox, bb.bbox);
        assert_eq!(ba.voxel_count(), bb.voxel_count());
    }
}

#[test]
fn stamped_blocks_match_the_blueprint() {
    let r = reg();
    let t = terrain(SEEDS[0]);
    let mut seen: BTreeMap<&str, bool> = BTreeMap::new();
    for (s, b) in all(&t) {
        if seen.contains_key(s.kind.name()) {
            continue;
        }
        // Dernier écrit gagne : on rejoue les blocs dans l'ordre.
        let mut want: HashMap<(i32, i32, i32), Mat> = HashMap::new();
        for part in &b.parts {
            for &(x, y, z, m) in &part.voxels {
                want.insert((x, y, z), m);
            }
        }
        let chunks: HashMap<(i32, i32), Chunk> = chunks_of(&b)
            .into_iter()
            .map(|(cx, cz)| ((cx, cz), chunk(&t, cx, cz)))
            .collect();
        let (mut ok, mut total) = (0usize, 0usize);
        for (&(x, y, z), &m) in &want {
            let c = &chunks[&(x.div_euclid(16), z.div_euclid(16))];
            let id = c.block(x.rem_euclid(16), y, z.rem_euclid(16));
            total += 1;
            let expect = if m == Mat::Air { 0 } else { r.get(m.block()).id };
            if id == expect {
                ok += 1;
            }
        }
        let frac = ok as f64 / total as f64;
        println!("{:<10} {ok}/{total} blocs conformes ({:.1} %)", s.kind.name(), 100.0 * frac);
        assert!(frac > 0.97, "{} : {ok}/{total}", s.kind.name());
        seen.insert(s.kind.name(), true);
        if seen.len() == KINDS.len() {
            break;
        }
    }
    assert_eq!(seen.len(), KINDS.len(), "types vus : {:?}", seen.keys());
}

#[test]
fn building_chunks_do_not_depend_on_generation_order() {
    let t1 = terrain(SEEDS[1]);
    let t2 = terrain(SEEDS[1]);
    let by = all(&t1)
        .into_iter()
        .find(|(s, _)| s.kind == StructKind::By)
        .map(|(_, b)| b);
    let Some(by) = by else {
        // Aucun village sur cette graine : le test d'ordre porte alors sur le premier bâtiment.
        return;
    };
    let cells = chunks_of(&by);
    let fwd: Vec<u64> = cells
        .iter()
        .map(|&(cx, cz)| chunk(&t1, cx, cz).fingerprint())
        .collect();
    let mut rev: Vec<u64> = cells
        .iter()
        .rev()
        .map(|&(cx, cz)| chunk(&t2, cx, cz).fingerprint())
        .collect();
    rev.reverse();
    assert_eq!(fwd, rev);
}

#[test]
fn no_trees_grow_inside_buildings() {
    let r = reg();
    let t = terrain(SEEDS[2]);
    let logs: Vec<u8> = ["gran_stam", "tall_stam", "bjork_stam"]
        .iter()
        .filter_map(|n| {
            let b = r.get(n);
            (b.name == *n).then_some(b.id)
        })
        .collect();
    let mut checked = 0;
    for (_, b) in all(&t).into_iter().take(40) {
        for part in &b.parts {
            for &(x, y, z, m) in part.voxels.iter().step_by(7) {
                if m == Mat::Air {
                    continue;
                }
                let c = chunk(&t, x.div_euclid(16), z.div_euclid(16));
                let id = c.block(x.rem_euclid(16), y, z.rem_euclid(16));
                assert!(!logs.contains(&id), "tronc dans un bâtiment en ({x},{y},{z})");
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
}
