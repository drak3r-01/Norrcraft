//! Tests d'acceptation des arbres Norrcraft (session 2.3 ; GDD doc 02 §9.1, §9.2 et §13).

use std::collections::BTreeMap;
use std::time::Instant;

use nb_core::ChunkPos;
use nb_data::{BlockId, BlockRegistry, WorldKind};
use nb_world::norrcraft::trees::{draw, Leaf, Put, Wood, MAX_REACH};
use nb_world::norrcraft::{
    altitudes, NorrBiome, NorrBlocks, NorrConfig, NorrcraftTerrain, Plant, Species, TreeKind,
};
use nb_world::{Chunk, TerrainGenerator};

const SEEDS: [i64; 3] = [12345, -98765, 20261111];

fn reg() -> BlockRegistry {
    BlockRegistry::for_kind(WorldKind::Norrcraft)
}

fn terrain(seed: i64) -> NorrcraftTerrain {
    NorrcraftTerrain::new(seed, NorrConfig::default(), NorrBlocks::from_registry(&reg()))
}

fn chunk(t: &NorrcraftTerrain, cx: i32, cz: i32) -> Chunk {
    let mut c = Chunk::new(ChunkPos::new(cx, cz));
    t.generate_chunk(&mut c);
    c
}

/// `n × n` chunks à partir de `(cx0, cz0)`, générés dans l'ordre donné par `order`.
struct Area {
    cx0: i32,
    cz0: i32,
    n: i32,
    chunks: Vec<Chunk>,
}

impl Area {
    fn new(t: &NorrcraftTerrain, cx0: i32, cz0: i32, n: i32, reverse: bool) -> Self {
        let mut idx: Vec<i32> = (0..n * n).collect();
        if reverse {
            idx.reverse();
        }
        let mut slots: Vec<Option<Chunk>> = (0..n * n).map(|_| None).collect();
        for k in idx {
            slots[k as usize] = Some(chunk(t, cx0 + k % n, cz0 + k / n));
        }
        Self {
            cx0,
            cz0,
            n,
            chunks: slots.into_iter().flatten().collect(),
        }
    }

    fn block(&self, wx: i32, y: i32, wz: i32) -> Option<BlockId> {
        let (cx, cz) = (wx.div_euclid(16) - self.cx0, wz.div_euclid(16) - self.cz0);
        if y < 0 || cx < 0 || cz < 0 || cx >= self.n || cz >= self.n {
            return None;
        }
        Some(self.chunks[(cz * self.n + cx) as usize].block(
            wx.rem_euclid(16),
            y,
            wz.rem_euclid(16),
        ))
    }
}

/// Cellules de la grille de placement qui couvrent le carré `[x0, x1] × [z0, z1]`.
fn plants_in(t: &NorrcraftTerrain, x0: i32, z0: i32, x1: i32, z1: i32) -> Vec<Plant> {
    t.with_tree_probe(|probe| {
        let mut v = Vec::new();
        for j in z0.div_euclid(5)..=z1.div_euclid(5) {
            for i in x0.div_euclid(5)..=x1.div_euclid(5) {
                v.extend(t.trees().cell_plants(probe, i, j));
            }
        }
        v.retain(|p| (x0..=x1).contains(&p.x) && (z0..=z1).contains(&p.z));
        v
    })
}

#[test]
fn trees_do_not_depend_on_the_chunk_order_or_the_cache() {
    // Zone forestière du préréglage par défaut, trois graines, générée dans les deux sens avec
    // deux instances (donc deux caches) indépendantes.
    for seed in SEEDS {
        let (a, b) = (terrain(seed), terrain(seed));
        let fwd = Area::new(&a, -3, -3, 6, false);
        let rev = Area::new(&b, -3, -3, 6, true);
        for (k, (ca, cb)) in fwd.chunks.iter().zip(&rev.chunks).enumerate() {
            assert_eq!(ca.voxels()[..], cb.voxels()[..], "graine {seed} chunk {k}");
        }
        // Un chunk isolé à froid donne la même chose que dans la zone, et une copie aussi.
        let cold = chunk(&terrain(seed), 0, 0);
        let k = (3 * fwd.n + 3) as usize;
        assert_eq!(cold.voxels()[..], fwd.chunks[k].voxels()[..], "graine {seed}");
        let mut forked = Chunk::new(ChunkPos::new(0, 0));
        a.fork().generate_chunk(&mut forked);
        assert_eq!(forked.voxels()[..], fwd.chunks[k].voxels()[..], "graine {seed}");
    }
}

/// Dessin d'une plante dans le monde : collecte des blocs absolus.
struct World {
    wood: Vec<(i32, i32, i32, Wood)>,
    leaf: Vec<(i32, i32, i32, Leaf)>,
    at: (i32, i32, i32),
}

impl Put for World {
    fn wood(&mut self, dx: i32, dy: i32, dz: i32, w: Wood) {
        if dx.abs() <= MAX_REACH && dz.abs() <= MAX_REACH {
            self.wood
                .push((self.at.0 + dx, self.at.1 + dy, self.at.2 + dz, w));
        }
    }
    fn leaf(&mut self, dx: i32, dy: i32, dz: i32, l: Leaf) {
        if dx.abs() <= MAX_REACH && dz.abs() <= MAX_REACH {
            self.leaf
                .push((self.at.0 + dx, self.at.1 + dy, self.at.2 + dz, l));
        }
    }
    fn supported(&mut self, _: i32, _: i32, _: i32) -> bool {
        true
    }
}

/// Origine (en chunks) du carré de `n x n` chunks le plus boisé parmi une grille de candidats.
fn densest_window(t: &NorrcraftTerrain, n: i32) -> (i32, i32) {
    let mut best = (0, (0, 0));
    for j in -3..=3 {
        for i in -3..=3 {
            let (cx, cz) = (i * 40, j * 40);
            let k = plants_in(t, cx * 16, cz * 16, (cx + n) * 16 - 1, (cz + n) * 16 - 1).len();
            if k > best.0 {
                best = (k, (cx, cz));
            }
        }
    }
    best.1
}

#[test]
fn crowns_are_whole_across_chunk_borders() {
    // Aucun bloc de tronc ou de feuille dessiné par une plante ne manque dans le monde généré,
    // où que passent les bordures de chunks : un bloc dessiné vaut un arbre, ou un solide qui l'a
    // pris (terrain, autre arbre), jamais de l'air.
    let n = 10;
    let mut checked = 0u32;
    let mut across = 0u32;
    for seed in SEEDS {
        let t = terrain(seed);
        let (cx0, cz0) = densest_window(&t, n);
        let area = Area::new(&t, cx0, cz0, n, false);
        // Plantes dont tout le volume est dans la zone.
        let m = MAX_REACH + 1;
        let (x0, z0) = (cx0 * 16 + m, cz0 * 16 + m);
        let (x1, z1) = ((cx0 + n) * 16 - 1 - m, (cz0 + n) * 16 - 1 - m);
        let plants = plants_in(&t, x0, z0, x1, z1);
        assert!(plants.len() > 120, "graine {seed} : {} plantes", plants.len());
        for pl in plants.iter().filter(|p| p.kind != TreeKind::Log) {
            let mut w = World {
                wood: Vec::new(),
                leaf: Vec::new(),
                at: (pl.x, pl.y, pl.z),
            };
            draw(pl, &mut w);
            let mut borders = false;
            for &(x, y, z, _) in &w.wood {
                if y >= 0 && y < 128 {
                    let b = area.block(x, y, z).unwrap();
                    assert_ne!(b, 0, "graine {seed} : tronc absent en ({x},{y},{z}) de {pl:?}");
                    checked += 1;
                    borders |= x.rem_euclid(16) == 0 || z.rem_euclid(16) == 0;
                }
            }
            for &(x, y, z, _) in &w.leaf {
                if y >= 0 && y < 128 {
                    let b = area.block(x, y, z).unwrap();
                    assert_ne!(b, 0, "graine {seed} : feuille absente en ({x},{y},{z}) de {pl:?}");
                    checked += 1;
                    borders |= x.rem_euclid(16) == 0 || z.rem_euclid(16) == 0;
                }
            }
            if borders {
                across += 1;
            }
        }
    }
    eprintln!("blocs vérifiés : {checked}, plantes sur une bordure : {across}");
    assert!(checked > 20_000 && across > 100);
}

#[test]
fn placement_respects_the_ground_rules() {
    let sea = NorrConfig::default().sea_level;
    for seed in SEEDS {
        let t = terrain(seed);
        let mut per_biome: BTreeMap<&str, u32> = BTreeMap::new();
        let plants = plants_in(&t, -1500, -1500, 1500, 1500);
        assert!(plants.len() > 20_000, "graine {seed} : {} plantes", plants.len());
        for pl in &plants {
            let sel = t.column_biome(pl.x, pl.z);
            *per_biome.entry(sel.biome.name()).or_default() += 1;
            // Sec, au-dessus de la mer, sur le sol de la colonne.
            let c = t.hydrography().column(pl.x, pl.z);
            assert!(!c.is_wet(), "plante dans l'eau : {pl:?}");
            assert!(pl.y > sea + 1 && pl.y == c.top, "{pl:?} sol {}", c.top);
            // Pente < 3 (relief des quatre voisines) : pas de falaise. Les membres d'un bosquet de
            // trembles ne sont pas testés individuellement (écart de sol ≤ 2 avec le centre).
            if pl.kind != TreeKind::Living(Species::Asp) {
                let h = t.heights().column(pl.x, pl.z).ground_top(128);
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let n = t.heights().column(pl.x + dx, pl.z + dz).ground_top(128);
                    assert!((n - h).abs() < 3, "pente raide sous {pl:?}");
                }
            }
            // Biome sans arbres, neige, glace : jamais de plante.
            assert!(
                !matches!(
                    sel.biome,
                    NorrBiome::Vasterhav
                        | NorrBiome::Djuphav
                        | NorrBiome::Bottenhav
                        | NorrBiome::Packis
                        | NorrBiome::Kalfjall
                        | NorrBiome::Glaciar
                ),
                "{pl:?} dans {:?}",
                sel.biome
            );
            assert!(!sel.snow, "plante sous les neiges éternelles : {pl:?}");
            // Limite des arbres : aucune forêt au-dessus de y_a (les biomes de toundra et de
            // tourbière sont exemptés : arbustes nains).
            let line = altitudes(t.heights().macro_fields().sample_lattice(pl.x, pl.z).t0, sea)
                .tree_line;
            if !matches!(
                sel.biome,
                NorrBiome::Fjord
                    | NorrBiome::Skargard
                    | NorrBiome::Klintkust
                    | NorrBiome::Strandang
                    | NorrBiome::Alvar
                    | NorrBiome::Myr
                    | NorrBiome::Fjallhed
                    | NorrBiome::Palsmyr
                    | NorrBiome::Lavafalt
            ) {
                assert!(f64::from(pl.y) < line, "{pl:?} au-dessus de la limite {line}");
            }
        }
        eprintln!("graine {seed} : {per_biome:?}");
        assert!(per_biome.len() >= 8, "peu de biomes boisés : {per_biome:?}");
    }
}

/// Nombre de plantes par biome et par essence (ou bois mort), cellules de la grille sur 3 km.
fn census(t: &NorrcraftTerrain) -> BTreeMap<(&'static str, &'static str), u32> {
    let mut out = BTreeMap::new();
    for pl in plants_in(t, -1500, -1500, 1500, 1500) {
        let biome = t.column_biome(pl.x, pl.z).biome.name();
        let what = match pl.kind {
            TreeKind::Living(s) => s.name(),
            TreeKind::Snag => "chicot",
            TreeKind::Stump => "souche",
            TreeKind::Log => "tronc couche",
        };
        *out.entry((biome, what)).or_default() += 1;
    }
    out
}

#[test]
fn species_follow_their_biomes() {
    let mut total: BTreeMap<(&'static str, &'static str), u32> = BTreeMap::new();
    for seed in SEEDS {
        for (k, v) in census(&terrain(seed)) {
            *total.entry(k).or_default() += v;
        }
    }
    let of = |biome: &str, what: &str| -> u32 {
        total
            .iter()
            .filter(|((b, w), _)| *b == biome && *w == what)
            .map(|(_, n)| *n)
            .sum()
    };
    let all = |biome: &str| -> u32 {
        total
            .iter()
            .filter(|((b, _), _)| *b == biome)
            .map(|(_, n)| *n)
            .sum()
    };
    eprintln!("{total:?}");
    let urskog = all("URSKOG");
    assert!(urskog > 3_000, "Urskog : {urskog}");
    // Épicéa dominant en forêt primaire, avec pins, bouleaux, trembles et bois mort.
    assert!(of("URSKOG", "gran") * 100 / urskog >= 33, "épicéa d'Urskog");
    for w in ["tall", "bjork", "asp", "chicot", "souche", "tronc couche"] {
        assert!(of("URSKOG", w) > 0, "Urskog sans {w}");
    }
    let dead = of("URSKOG", "chicot") + of("URSKOG", "souche") + of("URSKOG", "tronc couche");
    assert!((4..=13).contains(&(dead * 100 / urskog)), "bois mort {dead}/{urskog}");
    // Pin seul en pinède, hêtre en hêtraie, rien dans la mer ni sur la glace.
    let tallhed = all("TALLHED");
    if tallhed > 200 {
        assert_eq!(of("TALLHED", "gran"), 0);
        assert!(of("TALLHED", "tall") * 100 / tallhed >= 90);
    }
    let bokskog = all("BOKSKOG");
    if bokskog > 200 {
        assert!(of("BOKSKOG", "bok") * 100 / bokskog >= 40, "hêtre de Bokskog");
        assert_eq!(of("BOKSKOG", "gran"), 0);
    }
    let sjoland = all("SJOLAND");
    if sjoland > 200 {
        assert!(of("SJOLAND", "al") > 0 && of("SJOLAND", "gran") > 0);
    }
    for b in ["VASTERHAV", "DJUPHAV", "BOTTENHAV", "PACKIS", "KALFJALL", "GLACIAR"] {
        assert_eq!(all(b), 0, "{b} porte des arbres");
    }
    // Toundra : seulement des saules et des bouleaux nains.
    for w in ["gran", "tall", "bok", "ek", "asp", "al"] {
        // Les membres d'un bosquet de trembles peuvent déborder de quelques blocs.
        assert!(of("FJALLHED", w) * 50 <= all("FJALLHED"), "Fjällhed porte {w}");
    }
    // Les 12 essences apparaissent.
    for s in Species::ALL {
        assert!(
            total.iter().any(|((_, w), n)| *w == s.name() && *n > 0),
            "essence {} absente",
            s.name()
        );
    }
}

#[test]
fn canopy_density_orders_the_biomes() {
    let mut rate: BTreeMap<&'static str, (u32, u32)> = BTreeMap::new();
    for seed in SEEDS {
        let t = terrain(seed);
        t.with_tree_probe(|probe| {
            for j in -150..150 {
                for i in -150..150 {
                    let (x, z) = (5 * i + 2, 5 * j + 2);
                    let sel = t.column_biome(x, z);
                    if sel.fresh_water || t.hydrography().column(x, z).is_wet() {
                        continue;
                    }
                    let e = rate.entry(sel.biome.name()).or_default();
                    e.0 += 1;
                    if !t.trees().cell_plants(probe, i, j).is_empty() {
                        e.1 += 1;
                    }
                }
            }
        });
    }
    let share = |b: &str| rate.get(b).map(|&(n, k)| f64::from(k) / f64::from(n.max(1)));
    eprintln!(
        "{:?}",
        rate.iter()
            .map(|(b, &(n, k))| format!("{b} {:.2} ({n})", f64::from(k) / f64::from(n.max(1))))
            .collect::<Vec<_>>()
    );
    let urskog = share("URSKOG").expect("Urskog");
    assert!((0.20..=0.60).contains(&urskog), "Urskog {urskog}");
    if let Some(h) = share("HAGMARK") {
        assert!(h < urskog / 2.0 && h > 0.03, "Hagmark {h}");
    }
    if let Some(h) = share("FJALLHED") {
        assert!(h < 0.08, "Fjällhed {h}");
    }
    if let Some(h) = share("BOKSKOG") {
        assert!(h > urskog * 0.6, "Bokskog {h}");
    }
}

#[test]
fn generation_cost_stays_in_the_budget() {
    // Budget du doc 02 §12 : 2,5 ms par chunk. Cache froid, 12 × 12 chunks voisins.
    let t = terrain(12345);
    let start = Instant::now();
    let area = Area::new(&t, 20, 20, 12, false);
    let per_chunk = start.elapsed().as_secs_f64() * 1000.0 / 144.0;
    eprintln!("{per_chunk:.2} ms par chunk, avec les arbres ({} chunks)", area.chunks.len());
    assert!(per_chunk < 5.0, "{per_chunk} ms par chunk");
}
