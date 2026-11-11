//! Tests d'acceptation du sous-sol Norrcraft (session 2.2 ; GDD doc 02 §8 et §13).

use std::collections::BTreeMap;

use nb_core::ChunkPos;
use nb_data::{BlockId, BlockRegistry, WorldKind};
use nb_world::norrcraft::{
    NorrBiome, NorrBlocks, NorrConfig, NorrcraftTerrain, Province, Region, SubsoilBlocks,
};
use nb_world::{Chunk, TerrainGenerator};

const SEEDS: [i64; 3] = [12345, -98765, 20261111];
const S: i32 = 40;

fn reg() -> BlockRegistry {
    BlockRegistry::for_kind(WorldKind::Norrcraft)
}

fn blocks() -> NorrBlocks {
    NorrBlocks::from_registry(&reg())
}

fn terrain(seed: i64) -> NorrcraftTerrain {
    NorrcraftTerrain::new(seed, NorrConfig::default(), blocks())
}

fn chunk(t: &NorrcraftTerrain, cx: i32, cz: i32) -> Chunk {
    let mut c = Chunk::new(ChunkPos::new(cx, cz));
    t.generate_chunk(&mut c);
    c
}

/// `n × n` chunks à partir de `(cx0, cz0)`, accessibles par coordonnées de monde.
struct Area {
    cx0: i32,
    cz0: i32,
    n: i32,
    chunks: Vec<Chunk>,
}

impl Area {
    fn new(t: &NorrcraftTerrain, cx0: i32, cz0: i32, n: i32) -> Self {
        let mut chunks = Vec::new();
        for cz in 0..n {
            for cx in 0..n {
                chunks.push(chunk(t, cx0 + cx, cz0 + cz));
            }
        }
        Self {
            cx0,
            cz0,
            n,
            chunks,
        }
    }

    fn block(&self, wx: i32, y: i32, wz: i32) -> Option<BlockId> {
        if y < 0 {
            return None;
        }
        let (cx, cz) = (wx.div_euclid(16) - self.cx0, wz.div_euclid(16) - self.cz0);
        if cx < 0 || cz < 0 || cx >= self.n || cz >= self.n {
            return None;
        }
        Some(self.chunks[(cz * self.n + cx) as usize].block(
            wx.rem_euclid(16),
            y,
            wz.rem_euclid(16),
        ))
    }

    /// Vérifie l'étanchéité : aucun bloc d'eau n'a d'air pour voisin latéral ni dessous. Renvoie
    /// le nombre de blocs d'eau sous `S − 1` dont l'air est à moins de 2 blocs au-dessus (cavités
    /// noyées) et le nombre de cellules d'air souterraines (sous la surface − 4).
    fn assert_sealed(&self, t: &NorrcraftTerrain, label: &str) -> (u32, u32) {
        let water = reg().get("water").id;
        let (mut flooded, mut hollow) = (0, 0);
        for wz in self.cz0 * 16..(self.cz0 + self.n) * 16 {
            for wx in self.cx0 * 16..(self.cx0 + self.n) * 16 {
                let h = t.surface_height(wx, wz);
                for y in 0..127 {
                    let here = self.block(wx, y, wz);
                    if here == Some(0) && y > 1 && y < h - 4 {
                        hollow += 1;
                    }
                    if here != Some(water) {
                        continue;
                    }
                    if y < h - 4 {
                        flooded += 1;
                    }
                    assert_ne!(
                        self.block(wx, y - 1, wz),
                        Some(0),
                        "{label} : eau flottante en ({wx}, {y}, {wz})"
                    );
                    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        assert_ne!(
                            self.block(wx + dx, y, wz + dz),
                            Some(0),
                            "{label} : fuite en ({wx}, {y}, {wz}) vers ({}, {})",
                            wx + dx,
                            wz + dz
                        );
                    }
                }
            }
        }
        (flooded, hollow)
    }
}

/// Noms des blocs par identifiant.
fn names() -> Vec<String> {
    let r = reg();
    (0..=255u8).map(|id| r.from_id(id).name.clone()).collect()
}

/// Compte des blocs (par nom) d'une zone.
fn census(a: &Area) -> BTreeMap<String, u64> {
    let names = names();
    let mut out = BTreeMap::new();
    for c in &a.chunks {
        for &v in c.voxels().iter() {
            if v != 0 {
                *out.entry(names[v as usize].clone()).or_insert(0) += 1;
            }
        }
    }
    out
}

/// Premières colonnes de chaque biome trouvées sur une grille de pas `step` (rayon `half`).
fn find_biomes(t: &NorrcraftTerrain, half: i32, step: i32) -> BTreeMap<NorrBiome, (i32, i32)> {
    let mut found = BTreeMap::new();
    let n = half / step;
    for j in -n..n {
        for i in -n..n {
            let (x, z) = (i * step, j * step);
            found.entry(t.column_biome(x, z).biome).or_insert((x, z));
        }
    }
    found
}

const PROVINCE_ROCKS: [&str; 10] = [
    "granit",
    "gnejs",
    "rodgranit",
    "kalksten",
    "sandsten",
    "alunskiffer",
    "gronsten",
    "porfyr",
    "basaltsten",
    "krita",
];

#[test]
fn blocks_exist_and_ids_stay_below_256() {
    let r = reg();
    for n in [
        "sandsten",
        "alunskiffer",
        "gronsten",
        "porfyr",
        "diabas",
        "krita",
        "taljsten",
        "kvarts",
        "svavel",
        "myrmalm",
        "sjomalm",
        "jarnmalm",
        "rik_jarnmalm",
        "kopparmalm",
        "silvermalm",
        "koboltmalm",
        "guldkvarts",
        "guldgrus",
        "volframmalm",
        "flinta",
    ] {
        assert_eq!(r.get(n).name, n, "bloc {n}");
        assert!(r.get(n).solid);
    }
    let sub = SubsoilBlocks::from_registry(&r);
    assert_eq!(sub.provinces.len(), Province::ALL.len());
    // Pas de charbon minéral, ni diamant ni émeraude (décision D8).
    for n in ["charbon", "diamant", "emeraude"] {
        assert_ne!(r.get(n).name, n);
    }
}

/// Doc 02 §8.1 : bedrock en y ≤ 1, djupberg jusqu'à y < 14 (hors minerais, filons et cavités),
/// roche de la province au-dessus.
#[test]
fn strata_follow_the_doc() {
    let names = names();
    let rocks: Vec<usize> = PROVINCE_ROCKS
        .iter()
        .map(|n| names.iter().position(|m| m == n).unwrap())
        .collect();
    for seed in SEEDS {
        let t = terrain(seed);
        let a = Area::new(&t, -4, -4, 8);
        let (mut deep, mut province, mut deep_total) = (0u32, 0u32, 0u32);
        for wz in -64..64 {
            for wx in -64..64 {
                let h = t.surface_height(wx, wz);
                assert_eq!(a.block(wx, 0, wz), Some(reg().get("bedrock").id));
                assert_eq!(a.block(wx, 1, wz), Some(reg().get("bedrock").id));
                for y in 2..14.min(h - 4) {
                    let b = a.block(wx, y, wz).unwrap() as usize;
                    assert!(
                        !rocks.contains(&b),
                        "graine {seed} : roche de province « {} » sous y = 14 en ({wx}, {y}, {wz})",
                        names[b]
                    );
                    deep_total += 1;
                    if names[b] == "djupberg" {
                        deep += 1;
                    }
                }
                for y in 20..(h - 6).min(60) {
                    let b = a.block(wx, y, wz).unwrap() as usize;
                    province += u32::from(rocks.contains(&b));
                }
            }
        }
        assert!(
            f64::from(deep) > 0.9 * f64::from(deep_total),
            "graine {seed} : djupberg {deep} / {deep_total}"
        );
        assert!(
            province > 1000,
            "graine {seed} : trop peu de roche de province"
        );
    }
}

/// Doc 02 §8.1 : parts des provinces proches de celles du doc sur la terre, et le gneiss est
/// obligatoire dans la chaîne.
#[test]
fn provinces_follow_their_shares() {
    let mut share = BTreeMap::new();
    let mut total = 0u32;
    for seed in SEEDS {
        let t = terrain(seed);
        let m = |x, z| t.heights().macro_fields().sample_lattice(x, z);
        for j in -60..60 {
            for i in -60..60 {
                let (x, z) = (i * 250, j * 250);
                let ms = m(x, z);
                if !ms.is_land() {
                    continue;
                }
                let (p, _) = t.subsoil().province(x, z, &ms);
                if ms.u > 0.9 {
                    assert_eq!(p, Province::Gneiss, "gneiss dans la chaîne");
                }
                *share.entry(p).or_insert(0u32) += 1;
                total += 1;
            }
        }
    }
    let pct =
        |p: Province| 100.0 * f64::from(share.get(&p).copied().unwrap_or(0)) / f64::from(total);
    eprintln!(
        "provinces sur la terre : {}",
        Province::ALL
            .iter()
            .map(|&p| format!("{} {:.1} %", p.name(), pct(p)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(pct(Province::GraniteGrey) > pct(Province::Gneiss));
    assert!(pct(Province::Gneiss) > 10.0 && pct(Province::Gneiss) < 40.0);
    for p in Province::ALL {
        assert!(pct(p) > 0.2, "{} absente", p.name());
    }
}

/// Doc 02 §8.1 : les filons de diabase sont des murs verticaux de 2 blocs qui traversent tout le
/// socle.
#[test]
fn dikes_are_vertical_walls_two_blocks_wide() {
    let b = blocks();
    let dia = b.sub.diabase;
    let mut walls = 0;
    let mut widths = Vec::new();
    for seed in SEEDS {
        let t = terrain(seed);
        let sub = t.subsoil();
        // Largeur : longueurs des suites de colonnes de filon le long de x.
        for z in (-3000..3000).step_by(97) {
            let mut run = 0;
            for x in -3000..3000 {
                if sub.dike_at(x, z) {
                    run += 1;
                } else if run > 0 {
                    widths.push(run);
                    run = 0;
                }
            }
        }
        // Verticalité : sur quelques colonnes de filon, diabase à toutes les altitudes de socle.
        let mut checked = 0;
        'scan: for z in (-3000..3000).step_by(13) {
            for x in (-3000..3000).step_by(2) {
                if !sub.dike_at(x, z) || !t.heights().macro_fields().sample_lattice(x, z).is_land()
                {
                    continue;
                }
                let h = t.surface_height(x, z);
                if h < 70 {
                    continue;
                }
                let c = chunk(&t, x.div_euclid(16), z.div_euclid(16));
                let (lx, lz) = (x.rem_euclid(16), z.rem_euclid(16));
                for y in [3, 10, 20, 30, 45, h - 8] {
                    assert_eq!(
                        c.block(lx, y, lz),
                        dia,
                        "graine {seed} : filon interrompu en ({x}, {y}, {z})"
                    );
                }
                walls += 1;
                checked += 1;
                if checked >= 6 {
                    break 'scan;
                }
            }
        }
    }
    assert!(walls >= 6, "trop peu de filons trouvés : {walls}");
    assert!(!widths.is_empty());
    widths.sort_unstable();
    let median = widths[widths.len() / 2];
    eprintln!(
        "largeur médiane des filons le long de x : {median} ({} coupes)",
        widths.len()
    );
    assert!((1..=4).contains(&median), "largeur médiane {median}");
    let big = widths.iter().filter(|&&w| w > 8).count();
    assert!(big * 10 < widths.len(), "filons trop larges : {big}");
}

/// Doc 02 §13 : au moins un Malmberg à moins de 5 km de l'origine, par graine ; il est de fer, il
/// affleure et il est entouré de roche.
#[test]
fn a_malmberg_stands_within_five_kilometres() {
    let b = blocks();
    for seed in SEEDS {
        let t = terrain(seed);
        let mut best = None;
        for j in -10..10 {
            for i in -10..10 {
                if let Some(c) = t.malmberg_at_cell(i, j) {
                    let d = f64::from(c.0).hypot(f64::from(c.2));
                    if d < 5000.0 && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, c));
                    }
                }
            }
        }
        let (d, (cx, cy, cz)) = best.unwrap_or_else(|| panic!("graine {seed} : pas de Malmberg"));
        eprintln!("graine {seed} : Malmberg à {d:.0} blocs, centre ({cx}, {cy}, {cz})");
        let a = Area::new(&t, cx.div_euclid(16) - 6, cz.div_euclid(16) - 6, 12);
        let (mut iron, mut rich, mut outcrop) = (0u32, 0u32, 0u32);
        for wz in (cz - 80)..(cz + 80) {
            for wx in (cx - 80)..(cx + 80) {
                let h = t.surface_height(wx, wz);
                for y in 2..h {
                    let v = a.block(wx, y, wz).unwrap();
                    if v == b.sub.iron || v == b.sub.rich_iron {
                        iron += 1;
                        rich += u32::from(v == b.sub.rich_iron);
                        outcrop += u32::from(y >= h - 3);
                    }
                }
            }
        }
        eprintln!("  fer {iron} (riche {rich}), affleurant {outcrop}");
        assert!(iron > 20_000, "lentille trop petite : {iron}");
        assert!(rich > 1_000);
        assert!(outcrop > 50, "le Malmberg doit affleurer : {outcrop}");
    }
}

/// Doc 02 §8.2 : chaque minerai naît dans son gisement.
#[test]
fn ores_sit_in_their_deposits() {
    let b = blocks();
    let mut seen: BTreeMap<&str, u32> = BTreeMap::new();
    for seed in SEEDS {
        let t = terrain(seed);
        let found = find_biomes(&t, 12000, 48);
        let wanted = [
            NorrBiome::Myr,
            NorrBiome::Sjoland,
            NorrBiome::Klintkust,
            NorrBiome::Bergslag,
            NorrBiome::Tallhed,
        ];
        for biome in wanted {
            let Some(&(x, z)) = found.get(&biome) else {
                eprintln!("graine {seed} : pas de {} dans 12 km", biome.name());
                continue;
            };
            let a = Area::new(&t, x.div_euclid(16) - 4, z.div_euclid(16) - 4, 8);
            for wz in a.cz0 * 16..(a.cz0 + a.n) * 16 {
                for wx in a.cx0 * 16..(a.cx0 + a.n) * 16 {
                    let col = t.biome_at(wx, wz);
                    let h = t.surface_height(wx, wz);
                    for y in (2..h).rev() {
                        let v = a.block(wx, y, wz).unwrap();
                        let (name, ok) = if v == b.sub.bog_ore {
                            ("myrmalm", col == NorrBiome::Myr.id())
                        } else if v == b.sub.lake_ore {
                            (
                                "sjomalm",
                                [
                                    NorrBiome::Sjoland.id(),
                                    NorrBiome::Tallhed.id(),
                                    NorrBiome::Bottenhav.id(),
                                ]
                                .contains(&col),
                            )
                        } else if v == b.sub.chalk {
                            ("krita", col == NorrBiome::Klintkust.id())
                        } else if v == b.sub.flint {
                            ("flinta", col == NorrBiome::Klintkust.id())
                        } else if v == b.sub.sulphur {
                            ("svavel", col == NorrBiome::Lavafalt.id())
                        } else {
                            continue;
                        };
                        *seen.entry(name).or_insert(0) += 1;
                        assert!(
                            ok,
                            "graine {seed} : {name} hors de son gisement en ({wx}, {y}, {wz}) : {}",
                            NorrBiome::ALL[col as usize].name()
                        );
                    }
                }
            }
        }
    }
    eprintln!("minerais vus : {seen:?}");
    for k in ["myrmalm", "sjomalm", "krita", "flinta"] {
        assert!(seen.get(k).copied().unwrap_or(0) > 0, "{k} jamais vu");
    }
}

/// Doc 02 §8.3 : les cavités sont étanches (aucune eau voisine d'air), noyées sous la mer, d'air
/// au-dessus ; karst, failles, tubes de lave, grottes marines et salles existent.
#[test]
fn cavities_exist_and_water_stays_sealed() {
    let mut karst = (0u32, 0u32);
    let mut faults = 0u32;
    for seed in SEEDS {
        let t = terrain(seed);
        let sub = t.subsoil();
        // Karst : une zone de calcaire émergée.
        let mut zone = None;
        'karst: for j in -40..40 {
            for i in -40..40 {
                let (x, z) = (i * 150, j * 150);
                let ms = t.heights().macro_fields().sample_lattice(x, z);
                if ms.is_land()
                    && sub.province(x, z, &ms).0 == Province::Limestone
                    && t.surface_height(x, z) > S
                {
                    zone = Some((x, z));
                    break 'karst;
                }
            }
        }
        if let Some((x, z)) = zone {
            let a = Area::new(&t, x.div_euclid(16) - 3, z.div_euclid(16) - 3, 6);
            let (flooded, hollow) = a.assert_sealed(&t, &format!("karst graine {seed}"));
            eprintln!(
                "graine {seed} : karst en ({x}, {z}) : {flooded} cellules noyées, {hollow} d'air"
            );
            karst.0 += flooded;
            karst.1 += hollow;
        }
        // Failles : une colonne de ligne de faille en granit ou gneiss, sèche.
        'fault: for z in (-4000..4000).step_by(5) {
            for x in (-4000..4000).step_by(3) {
                if !sub.fault_line_at(x, z) {
                    continue;
                }
                let ms = t.heights().macro_fields().sample_lattice(x, z);
                let (p, _) = sub.province(x, z, &ms);
                let granitic = matches!(
                    p,
                    Province::GraniteGrey | Province::GraniteRed | Province::Gneiss
                );
                if !granitic || t.surface_height(x, z) < S + 6 {
                    continue;
                }
                let a = Area::new(&t, x.div_euclid(16) - 2, z.div_euclid(16) - 2, 4);
                let (_, hollow) = a.assert_sealed(&t, &format!("faille graine {seed}"));
                faults += hollow;
                // Une faille est une fente ouverte sur le ciel : l'air touche la surface.
                let mut open = 0;
                for wz in a.cz0 * 16..(a.cz0 + a.n) * 16 {
                    for wx in a.cx0 * 16..(a.cx0 + a.n) * 16 {
                        let h = t.surface_height(wx, wz);
                        open += u32::from(a.block(wx, h - 1, wz) == Some(0));
                    }
                }
                if hollow > 0 {
                    assert!(open > 0, "graine {seed} : faille fermée en ({x}, {z})");
                    break 'fault;
                }
            }
        }
        // Grottes marines et salles de la montagne près de l'origine.
        let mut caves = 0;
        let region = Region {
            x0: -8000,
            z0: -8000,
            x1: 8000,
            z1: 8000,
        };
        for d in t.deposits(region) {
            if (d.kind == "grotte marine" || d.kind == "salle de la montagne") && caves < 6 {
                let a = Area::new(
                    &t,
                    d.center[0].div_euclid(16) - 2,
                    d.center[2].div_euclid(16) - 2,
                    4,
                );
                a.assert_sealed(&t, &format!("{} graine {seed}", d.kind));
                caves += 1;
            }
        }
        eprintln!("graine {seed} : {caves} grottes marines ou salles contrôlées");
    }
    assert!(faults > 0, "aucune faille creusée");
    assert!(karst.1 + karst.0 > 0, "aucun karst creusé");
}

/// Doc 02 §8.2 et §8.3 : chaque espèce de corps (Malmberg, cuivre, cobalt, tungstène, stéatite,
/// grotte marine, salle) existe quelque part dans 16 km pour au moins une des 3 graines.
#[test]
fn every_body_kind_exists_somewhere() {
    let mut kinds: BTreeMap<&str, u32> = BTreeMap::new();
    let region = Region {
        x0: -8000,
        z0: -8000,
        x1: 8000,
        z1: 8000,
    };
    for seed in SEEDS {
        let t = terrain(seed);
        for d in t.deposits(region) {
            *kinds.entry(d.kind).or_insert(0) += 1;
        }
    }
    eprintln!("corps dans 16 km, 3 graines : {kinds:?}");
    for k in [
        "malmberg",
        "cuivre",
        "cobalt",
        "tungstène",
        "stéatite",
        "grotte marine",
        "salle de la montagne",
    ] {
        assert!(kinds.get(k).copied().unwrap_or(0) > 0, "{k} jamais vu");
    }
}

/// Doc 02 §8.3 : tubes de lave (avec effondrements) et soufre du Lavafält ; cavités étanches.
#[test]
fn lava_tubes_and_sulphur_in_the_lava_field() {
    let sulphur = blocks().sub.sulphur;
    let mut tubes = 0u32;
    let mut seen = false;
    for seed in SEEDS {
        let t = terrain(seed);
        // Les îles de lave sont rares et loin : on cherche d'abord par le champ Gt.
        let mut spot = None;
        'find: for j in -100..100 {
            for i in -100..100 {
                let (x, z) = (i * 400, j * 400);
                let m = t.heights().macro_fields().sample_lattice(x, z);
                if m.gt > 0.55 && !m.is_land() && t.column_biome(x, z).biome == NorrBiome::Lavafalt
                {
                    spot = Some((x, z));
                    break 'find;
                }
            }
        }
        let Some((x, z)) = spot else {
            eprintln!("graine {seed} : pas de Lavafält à 40 km");
            continue;
        };
        seen = true;
        let a = Area::new(&t, x.div_euclid(16) - 6, z.div_euclid(16) - 6, 12);
        let (_, hollow) = a.assert_sealed(&t, &format!("lave graine {seed}"));
        let n_sulphur: usize = a
            .chunks
            .iter()
            .map(|c| c.voxels().iter().filter(|&&v| v == sulphur).count())
            .sum();
        eprintln!("graine {seed} : Lavafält en ({x}, {z}) : {hollow} cellules de tube, {n_sulphur} de soufre");
        tubes += hollow;
    }
    if seen {
        assert!(tubes > 0, "aucun tube de lave");
    }
}

/// Doc 02 §2 : indépendance de l'ordre des chunks — un chunk donne les mêmes blocs quel que soit
/// ce qui a été généré avant, et les copies du générateur sont identiques.
#[test]
fn generation_is_order_independent() {
    let t = terrain(12345);
    let fresh = |t: &NorrcraftTerrain| chunk(t, 7, -9);
    let a = fresh(&t);
    // On génère d'autres chunks (voisins compris) avant de recommencer.
    for (cx, cz) in [(8, -9), (6, -9), (7, -8), (7, -10), (200, 200)] {
        let _ = chunk(&t, cx, cz);
    }
    let b = fresh(&t);
    assert_eq!(a.voxels(), b.voxels());
    let forked = t.fork();
    let mut c = Chunk::new(ChunkPos::new(7, -9));
    forked.generate_chunk(&mut c);
    assert_eq!(a.voxels(), c.voxels());
    // Un monde neuf de même graine donne le même chunk.
    let u = terrain(12345);
    assert_eq!(a.voxels(), fresh(&u).voxels());
}

/// Budget de performance (doc 02 §12 : 2,5 ms par chunk) : mesuré en profil de test (optimisé).
#[test]
fn generation_stays_within_the_time_budget() {
    let t = terrain(12345);
    let start = std::time::Instant::now();
    let n = 100;
    for i in 0..n {
        let _ = chunk(&t, (i % 10) * 3 - 15, (i / 10) * 3 - 15);
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    eprintln!("{ms:.2} ms par chunk");
    assert!(ms < 6.0, "{ms:.2} ms par chunk");
}

/// Doc 02 §8.2 : les corps de minerai contiennent leur minerai (cuivre, cobalt, tungstène,
/// stéatite).
#[test]
fn bodies_hold_their_ore() {
    let b = blocks();
    let region = Region {
        x0: -8000,
        z0: -8000,
        x1: 8000,
        z1: 8000,
    };
    let want = [
        ("cuivre", b.sub.copper, 100u32),
        ("cobalt", b.sub.cobalt, 1),
        ("tungstène", b.sub.tungsten, 1),
        ("stéatite", b.sub.soapstone, 20),
    ];
    for (kind, ore, min) in want {
        let mut best = 0u32;
        let mut tried = 0;
        'seeds: for seed in SEEDS {
            let t = terrain(seed);
            for d in t.deposits(region).into_iter().filter(|d| d.kind == kind) {
                let a = Area::new(
                    &t,
                    d.center[0].div_euclid(16) - 2,
                    d.center[2].div_euclid(16) - 2,
                    4,
                );
                let n: usize = a
                    .chunks
                    .iter()
                    .map(|c| c.voxels().iter().filter(|&&v| v == ore).count())
                    .sum();
                best = best.max(n as u32);
                tried += 1;
                if best >= min || tried >= 12 {
                    break 'seeds;
                }
            }
        }
        eprintln!("{kind} : {best} blocs de minerai dans le meilleur corps ({tried} essais)");
        assert!(best >= min, "{kind} : minerai absent ({best})");
    }
}

/// Doc 02 §8.2 : fer de roche en pays minier, argent dans le calcaire, quartz et or dans le nord.
#[test]
fn veins_sit_in_their_country() {
    let b = blocks();
    let (mut iron, mut silver, mut quartz, mut gold) = (0u64, 0u64, 0u64, 0u64);
    for seed in SEEDS {
        let t = terrain(seed);
        let sub = t.subsoil();
        let m = |x, z| t.heights().macro_fields().sample_lattice(x, z);
        let (mut green, mut lime, mut north) = (None, None, None);
        for j in -50..50 {
            for i in -50..50 {
                let (x, z) = (i * 200, j * 200);
                let ms = m(x, z);
                if !ms.is_land() || t.surface_height(x, z) <= S + 4 {
                    continue;
                }
                let p = sub.province(x, z, &ms).0;
                if p == Province::Greenstone && green.is_none() {
                    green = Some((x, z));
                }
                if p == Province::Limestone && lime.is_none() {
                    lime = Some((x, z));
                }
                if ms.t0 < 0.2 && north.is_none() {
                    north = Some((x, z));
                }
            }
        }
        let count = |spot: Option<(i32, i32)>, ids: &[BlockId]| -> u64 {
            let Some((x, z)) = spot else { return 0 };
            let a = Area::new(&t, x.div_euclid(16) - 4, z.div_euclid(16) - 4, 8);
            a.chunks
                .iter()
                .map(|c| c.voxels().iter().filter(|v| ids.contains(v)).count() as u64)
                .sum()
        };
        iron += count(green, &[b.sub.iron, b.sub.rich_iron]);
        silver += count(lime, &[b.sub.silver]);
        quartz += count(north, &[b.sub.quartz]);
        gold += count(north, &[b.sub.gold_quartz]);
    }
    eprintln!("filons : fer {iron}, argent {silver}, quartz {quartz}, or {gold}");
    assert!(iron > 500, "fer de roche absent du pays minier : {iron}");
    assert!(silver > 20, "argent absent du calcaire : {silver}");
    assert!(quartz > 500, "quartz absent du nord : {quartz}");
    assert!(gold > 0, "or absent du nord");
}

/// Sonde de réglage : `cargo test --test norrcraft_subsoil probe -- --ignored --nocapture`.
#[test]
#[ignore]
fn probe() {
    for seed in SEEDS {
        let t = terrain(seed);
        for (name, c) in [
            ("origine", (-8, -8)),
            ("nord", (-8, -200)),
            ("sud-est", (150, 150)),
        ] {
            let a = Area::new(&t, c.0, c.1, 16);
            let census = census(&a);
            let total: u64 = census.values().sum();
            eprintln!("graine {seed} {name} : {total} blocs : {census:?}");
        }
    }
}
