//! Tests d'acceptation des structures Norrcraft (session 4.1 ; GDD doc 02 §10.2, §10.3 et §13) :
//! grilles, prédicats de site, pistes, cairns.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use nb_core::ChunkPos;
use nb_data::{BlockId, BlockRegistry, WorldKind};
use nb_world::norrcraft::structures::{Mode, Route, Surface, MAX_WET_RUN, RULES, TRACK_RANGE};
use nb_world::norrcraft::{
    Class, NorrBiome, NorrBlocks, NorrConfig, NorrcraftTerrain, StructKind, StructSite,
};
use nb_world::{Chunk, TerrainGenerator};

const SEEDS: [i64; 3] = [12345, -98765, 20261111];
/// Demi-côté de la zone d'étude, en blocs.
const REACH: i32 = 2600;

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

fn sites(t: &NorrcraftTerrain, r: i32) -> Vec<StructSite> {
    t.with_probe(|p| t.structures().sites_in(p, -r, -r, r, r))
}

fn routes(t: &NorrcraftTerrain, r: i32) -> Vec<Arc<Route>> {
    t.with_probe(|p| t.structures().routes_in(p, -r, -r, r, r))
}

#[test]
fn sites_are_pure_and_fit_their_biome_and_class() {
    for seed in SEEDS {
        let t = terrain(seed);
        let all = sites(&t, REACH);
        assert!(all.len() > 200, "graine {seed} : {} sites", all.len());
        // Deuxième instance, parcours inverse : mêmes sites.
        let again = terrain(seed);
        let mut rev = again.with_probe(|p| {
            let mut v = Vec::new();
            for z in (-3..=3).rev() {
                for x in (-3..=3).rev() {
                    let w = REACH / 3;
                    let (x0, z0) = (x * w - w / 2, z * w - w / 2);
                    v.extend(
                        again
                            .structures()
                            .sites_in(p, x0, z0, x0 + w - 1, z0 + w - 1),
                    );
                }
            }
            v
        });
        rev.retain(|s| s.x.abs() <= REACH && s.z.abs() <= REACH);
        rev.sort_by_key(|s| (s.z, s.x, s.kind));
        rev.dedup();
        if all != rev {
            let only_a: Vec<_> = all.iter().filter(|s| !rev.contains(s)).take(5).collect();
            let only_b: Vec<_> = rev.iter().filter(|s| !all.contains(s)).take(5).collect();
            panic!(
                "graine {seed} : sites dépendant de l'ordre ({} contre {}) ; seulement à gauche {only_a:?} ; seulement à droite {only_b:?}",
                all.len(),
                rev.len()
            );
        }
        for s in &all {
            let rule = s.kind.rule();
            let biome = t.column_biome(s.x, s.z).biome;
            assert!(
                rule.biomes.iter().any(|(b, _)| *b == biome),
                "{} en {} ({}, {})",
                rule.name,
                biome.name(),
                s.x,
                s.z
            );
            assert_eq!(s.class(), rule.class);
        }
        // Séparation entre classes : une petite structure ne touche pas une grande.
        for (n, a) in all.iter().enumerate() {
            for b in &all[n + 1..] {
                if a.class() == b.class() {
                    continue;
                }
                let sum = a.rule().radius + b.rule().radius + 4;
                let (dx, dz) = (i64::from(a.x - b.x), i64::from(a.z - b.z));
                assert!(
                    dx * dx + dz * dz >= i64::from(sum) * i64::from(sum),
                    "{} et {} se chevauchent en ({}, {})",
                    a.kind.name(),
                    b.kind.name(),
                    a.x,
                    a.z
                );
            }
        }
    }
}

#[test]
fn site_predicates_hold_for_what_they_promise() {
    let t = terrain(SEEDS[0]);
    let sea = t.heights().config().sea_level;
    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for s in sites(&t, REACH) {
        *by_kind.entry(s.kind.name()).or_default() += 1;
        let col = t.hydrography().column(s.x, s.z);
        assert_eq!(col.top, s.y);
        match s.kind {
            StructKind::Skeppsvrak => assert!(col.is_wet() && col.level - col.top >= 6),
            StructKind::Bro => assert!(col.is_wet()),
            StructKind::Vardkase => {
                for (dx, dz) in [(40, 0), (-40, 0), (0, 40), (0, -40)] {
                    assert!(t.hydrography().column(s.x + dx, s.z + dz).top <= s.y);
                }
                assert!(s.y >= sea + 12);
            }
            _ => assert!(!col.is_wet(), "{} dans l'eau", s.kind.name()),
        }
    }
    println!("sites par type : {by_kind:?}");
}

#[test]
fn most_kinds_appear_within_the_study_area() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in SEEDS {
        let t = terrain(seed);
        for s in sites(&t, 4000) {
            seen.insert(s.kind);
        }
    }
    let missing: Vec<_> = RULES
        .iter()
        .filter(|r| !seen.contains(&r.kind))
        .map(|r| r.name)
        .collect();
    println!("types absents : {missing:?}");
    assert!(seen.len() >= 26, "types vus : {}", seen.len());
}

#[test]
fn routes_are_short_connected_gabriel_edges() {
    for seed in SEEDS {
        let t = terrain(seed);
        let rs = routes(&t, REACH);
        assert!(rs.len() >= 5, "graine {seed} : {} pistes", rs.len());
        let track: Vec<StructSite> = t.with_probe(|p| {
            t.structures()
                .sites_in(p, -REACH - 600, -REACH - 600, REACH + 600, REACH + 600)
                .into_iter()
                .filter(|s| s.rule().track)
                .collect()
        });
        for r in &rs {
            let (dx, dz) = (i64::from(r.b.x - r.a.x), i64::from(r.b.z - r.a.z));
            assert!(dx * dx + dz * dz < i64::from(TRACK_RANGE) * i64::from(TRACK_RANGE));
            assert!(r.a.rule().track && r.b.rule().track);
            // Aucun troisième site de piste dans le disque de diamètre AB.
            for c in &track {
                if *c == r.a || *c == r.b {
                    continue;
                }
                let dot = i64::from(c.x - r.a.x) * i64::from(c.x - r.b.x)
                    + i64::from(c.z - r.a.z) * i64::from(c.z - r.b.z);
                assert!(dot >= 0, "{c:?} dans le disque de {:?}-{:?}", r.a, r.b);
            }
            // Cellules 4-connexes de a à b.
            assert_eq!((r.cells[0].x, r.cells[0].z), (r.a.x, r.a.z));
            let last = r.cells.last().unwrap();
            assert_eq!((last.x, last.z), (r.b.x, r.b.z));
            for w in r.cells.windows(2) {
                let d = (w[0].x - w[1].x).abs() + (w[0].z - w[1].z).abs();
                assert_eq!(d, 1);
                // À sec, la piste ne monte ni ne descend de plus d'un bloc par cellule.
                // (montagne nue et passerelles gardent le relief naturel).
                let graded = |m: Mode| !matches!(m, Mode::Bridge | Mode::Ground(Surface::Bare));
                if graded(w[0].mode) && graded(w[1].mode) {
                    assert!((w[0].y - w[1].y).abs() <= 1, "marche trop haute");
                }
            }
            // Traversées d'eau bornées.
            let mut run = 0;
            for c in &r.cells {
                if matches!(c.mode, Mode::Bridge) {
                    run += 1;
                    assert!(run <= MAX_WET_RUN);
                } else {
                    run = 0;
                }
            }
        }
    }
}

/// Une piste à sec assez longue, avec un revêtement de sol (pas la montagne nue).
fn find_ground_route(t: &NorrcraftTerrain) -> Option<Arc<Route>> {
    routes(t, REACH).into_iter().find(|r| {
        r.cells
            .iter()
            .filter(|c| matches!(c.mode, Mode::Ground(Surface::Trodden | Surface::Boards)))
            .count()
            > 40
    })
}

fn solid_free(reg: &BlockRegistry, id: BlockId) -> bool {
    id == 0 || reg.from_id(id).cross_shaped
}

#[test]
fn route_cells_are_stamped_and_cleared() {
    let r = reg();
    for seed in SEEDS {
        let t = terrain(seed);
        let b = NorrBlocks::from_registry(&r);
        let Some(route) = find_ground_route(&t) else {
            continue;
        };
        let mut checked = 0;
        for c in route
            .cells
            .iter()
            .filter(|c| matches!(c.mode, Mode::Ground(Surface::Trodden | Surface::Boards)))
        {
            let ch = chunk(&t, c.x.div_euclid(16), c.z.div_euclid(16));
            let (lx, lz) = (c.x.rem_euclid(16), c.z.rem_euclid(16));
            let near_end = [route.a, route.b]
                .iter()
                .any(|s| (s.x - c.x).abs() <= 4 && (s.z - c.z).abs() <= 4);
            if near_end {
                continue;
            }
            let under = ch.block(lx, c.y - 1, lz);
            let natural_ok = under == b.structs.path || under == b.structs.boards;
            // Les colonnes à glace ou non naturelles sont ignorées par la pose.
            if !natural_ok {
                let top = ch.block(lx, c.top, lz);
                assert!(
                    !solid_free(&r, top) || ch.block(lx, c.top - 1, lz) == 0,
                    "cellule ({}, {}) : ni piste ni exception ({under})",
                    c.x,
                    c.z
                );
                continue;
            }
            checked += 1;
            assert!(solid_free(&r, ch.block(lx, c.y, lz)), "dégagement 1");
            assert!(solid_free(&r, ch.block(lx, c.y + 1, lz)), "dégagement 2");
            assert!(
                !b.trees.is_tree_block(ch.block(lx, c.y, lz))
                    && !b.trees.is_tree_block(ch.block(lx, c.y + 1, lz))
            );
        }
        assert!(checked > 10, "graine {seed} : {checked} cellules vérifiées");
    }
}

#[test]
fn bridges_sit_flush_with_the_water() {
    let r = reg();
    let b = NorrBlocks::from_registry(&r);
    let mut found = 0;
    for seed in SEEDS {
        let t = terrain(seed);
        for route in routes(&t, REACH) {
            for c in route
                .cells
                .iter()
                .filter(|c| c.mode == Mode::Bridge)
                .take(3)
            {
                let ch = chunk(&t, c.x.div_euclid(16), c.z.div_euclid(16));
                let (lx, lz) = (c.x.rem_euclid(16), c.z.rem_euclid(16));
                let id = ch.block(lx, c.level - 1, lz);
                // Planche, ou bloc de glace/gelé de la banquise.
                if id != b.structs.boards {
                    continue;
                }
                found += 1;
                assert_eq!(ch.block(lx, c.level, lz), 0);
            }
        }
    }
    println!("planches de passerelle vérifiées : {found}");
}

#[test]
fn cairns_stand_on_mountain_routes_and_at_markers() {
    let r = reg();
    let b = NorrBlocks::from_registry(&r);
    let is_stone = |id: BlockId| id == b.structs.cairn[0] || id == b.structs.cairn[1];
    let t = terrain(SEEDS[0]);
    let mut route_cairns = 0;
    for route in routes(&t, REACH) {
        for &(x, z, y) in &route.cairns {
            let ch = chunk(&t, x.div_euclid(16), z.div_euclid(16));
            let (lx, lz) = (x.rem_euclid(16), z.rem_euclid(16));
            // Une faille ouverte sous le centre (colonne vue pleine par `surface_height`) ou
            // une glace au-dessus du sol : le cairn n'est pas posé.
            if ch.block(lx, y - 1, lz) == 0
                || ch.block(lx, y, lz) != 0 && !is_stone(ch.block(lx, y, lz))
            {
                continue;
            }
            assert!(
                is_stone(ch.block(lx, y, lz)),
                "cairn de piste en ({x}, {z}, {y})"
            );
            route_cairns += 1;
        }
    }
    println!("cairns de piste vérifiés : {route_cairns}");
    let mut markers = 0;
    for s in sites(&t, REACH).iter().filter(|s| s.has_cairn()).take(60) {
        let ch = chunk(&t, s.x.div_euclid(16), s.z.div_euclid(16));
        assert!(
            is_stone(ch.block(s.x.rem_euclid(16), s.y, s.z.rem_euclid(16))),
            "{} sans cairn en ({}, {}, {})",
            s.kind.name(),
            s.x,
            s.z,
            s.y
        );
        markers += 1;
    }
    println!("cairns de site vérifiés : {markers}");
    assert!(markers > 20);
}

#[test]
fn chunks_do_not_depend_on_generation_order() {
    let t0 = terrain(SEEDS[1]);
    let route = find_ground_route(&t0).expect("une piste de sol");
    let mid = route.cells[route.cells.len() / 2];
    let (cx, cz) = (mid.x.div_euclid(16), mid.z.div_euclid(16));
    let area = |t: &NorrcraftTerrain, reverse: bool| -> Vec<Chunk> {
        let mut idx: Vec<i32> = (0..9).collect();
        if reverse {
            idx.reverse();
        }
        let mut out: Vec<Option<Chunk>> = (0..9).map(|_| None).collect();
        for k in idx {
            out[k as usize] = Some(chunk(t, cx - 1 + k % 3, cz - 1 + k / 3));
        }
        out.into_iter().flatten().collect()
    };
    let fresh_a = area(&terrain(SEEDS[1]), false);
    let fresh_b = area(&terrain(SEEDS[1]), true);
    // Instance chauffée par d'autres chunks, dont ceux de la piste, dans le désordre.
    let warm = terrain(SEEDS[1]);
    for k in [7, 3, 8, 0, 5] {
        chunk(&warm, cx + 40 - k, cz - 30 + k);
    }
    for c in route.cells.iter().step_by(37) {
        chunk(&warm, c.x.div_euclid(16), c.z.div_euclid(16));
    }
    let warm_c = area(&warm, false);
    for k in 0..9 {
        assert_eq!(
            fresh_a[k].voxels(),
            fresh_b[k].voxels(),
            "chunk {k} avant/après"
        );
        assert_eq!(
            fresh_a[k].voxels(),
            warm_c[k].voxels(),
            "chunk {k} instance chaude"
        );
    }
}

#[test]
fn structures_cost_little_per_chunk() {
    let t = terrain(SEEDS[0]);
    let start = Instant::now();
    let n = 24;
    for cz in 0..n {
        for cx in 0..n {
            chunk(&t, cx * 3 - 30, cz * 3 - 30);
        }
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(n * n);
    println!("génération complète : {ms:.2} ms par chunk");
    assert!(ms < 12.0, "{ms} ms par chunk");
}

#[test]
fn class_table_matches_the_document() {
    // Pas de grille du doc 02 §10.2.
    assert_eq!(Class::Major.pitch(), 384);
    assert_eq!(Class::Medium.pitch(), 96);
    assert_eq!(Class::Minor.pitch(), 32);
    for name in [
        "by",
        "stavkyrka",
        "gruva",
        "hytta",
        "fornborg",
        "bergasal",
        "vårdkase",
    ] {
        let r = RULES.iter().find(|r| r.name == name).unwrap();
        assert_eq!(r.class, Class::Major, "{name}");
    }
    for name in [
        "torp",
        "fäbod",
        "fiskeläge",
        "kåta",
        "fjällstuga",
        "gravhög",
        "fyr",
        "långhus",
    ] {
        let r = RULES.iter().find(|r| r.name == name).unwrap();
        assert_eq!(r.class, Class::Medium, "{name}");
    }
    for name in [
        "runsten",
        "lada",
        "vindskydd",
        "sejte",
        "trojaborg",
        "flyttblock",
        "kolmila",
        "rösen",
    ] {
        let r = RULES.iter().find(|r| r.name == name).unwrap();
        assert_eq!(r.class, Class::Minor, "{name}");
    }
    // Les types de piste du doc : refuges, camps, fäbodar, villages.
    let track: Vec<_> = RULES.iter().filter(|r| r.track).map(|r| r.name).collect();
    assert_eq!(track, ["by", "fäbod", "kåta", "fjällstuga"]);
    // Aucun biome marin pour un type à terre.
    for r in RULES.iter().filter(|r| r.kind != StructKind::Skeppsvrak) {
        for (b, _) in r.biomes {
            assert!(
                !matches!(
                    b,
                    NorrBiome::Vasterhav | NorrBiome::Djuphav | NorrBiome::Bottenhav
                ),
                "{}",
                r.name
            );
        }
    }
}

/// Lieux à visiter pour les aperçus 3D (`-- --ignored --nocapture`) : le premier cairn de piste,
/// la première passerelle, le premier caillebotis et le premier jalon de chaque classe.
#[test]
#[ignore]
fn locate_preview_spots() {
    for seed in SEEDS {
        let t = terrain(seed);
        let rs = routes(&t, REACH);
        let cairn = rs.iter().find_map(|r| r.cairns.first().copied());
        let bridge = rs
            .iter()
            .find_map(|r| r.cells.iter().find(|c| c.mode == Mode::Bridge).copied());
        let boards = rs.iter().find_map(|r| {
            r.cells
                .iter()
                .find(|c| c.mode == Mode::Ground(Surface::Boards))
                .copied()
        });
        let trodden = rs.iter().find_map(|r| {
            r.cells
                .get(r.cells.len() / 2)
                .filter(|c| c.mode == Mode::Ground(Surface::Trodden))
                .copied()
        });
        println!(
            "graine {seed}: cairn {:?} passerelle {:?} caillebotis {:?} sentier {:?}",
            cairn.map(|c| (c.0, c.1)),
            bridge.map(|c| (c.x, c.z)),
            boards.map(|c| (c.x, c.z)),
            trodden.map(|c| (c.x, c.z)),
        );
        for class in Class::ALL {
            let s = sites(&t, REACH)
                .into_iter()
                .find(|s| s.class() == class && s.has_cairn());
            println!(
                "  jalon {}: {:?}",
                class.name(),
                s.map(|s| (s.kind.name(), s.x, s.z))
            );
        }
    }
}
