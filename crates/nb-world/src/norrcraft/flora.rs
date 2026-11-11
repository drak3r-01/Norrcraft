//! Strate basse de Norrcraft (session 2.4, doc 02 §9.3) : baies, champignons, lichens, fleurs,
//! hautes herbes, fougères, plantes de rive et algues posées sur le sol.
//!
//! Chaque plante est une **fonction pure de (graine, colonne)** : un tirage de hachage
//! `h(graine, plante, x, z)` comparé à `densité du biome × tache(p)`, où `tache(p)` est un fBm de
//! 20 à 60 blocs (une tache par plante, seuil propre). Le premier tirage gagnant, dans l'ordre de
//! la table [`SPECS`] (les plantes rares d'abord, l'herbe haute en dernier), fournit le bloc de la
//! colonne. Aucune lecture des colonnes voisines au-delà de ce que le chunk connaît déjà : le
//! résultat ne dépend pas de l'ordre de génération.
//!
//! Le bloc se pose sur le sol meuble (la nature du sol est lue dans le bloc posé : mousse,
//! aiguilles, lichen, sphaigne…), jamais sur la roche nue, la neige, la glace ni la banquise. Les
//! plantes d'eau se posent au fond (algues, roseaux) ou flottent en surface (nénuphars).
//!
//! La disponibilité saisonnière (baies mûres, champignons d'automne, floraison de printemps) est
//! pour la phase 3 : tous les états sont ici « en pleine saison ».

use nb_data::{BlockId, BlockRegistry};

use super::biomes::NorrBiome;
use super::math::{hash01, smooth, GradField};

/// Nature du sol sous une plante, lue dans le bloc posé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Ground {
    /// Air, eau, neige, glace, bloc inconnu : rien n'y pousse.
    None,
    Grass,
    Moss,
    Needles,
    Leaves,
    Lichen,
    Sphagnum,
    Peat,
    Soil,
    Ash,
    Mull,
    Sand,
    Gravel,
    Till,
    Clay,
    Rock,
}

impl Ground {
    const fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

const FOREST: u32 = Ground::Moss.bit() | Ground::Needles.bit() | Ground::Leaves.bit();
const MEADOW: u32 = Ground::Grass.bit();
const BOG: u32 = Ground::Sphagnum.bit() | Ground::Peat.bit();
const COARSE: u32 = Ground::Gravel.bit() | Ground::Till.bit() | Ground::Sand.bit();
const BARE: u32 = Ground::Ash.bit() | Ground::Mull.bit() | Ground::Soil.bit() | Ground::Gravel.bit()
    | Ground::Till.bit();
const ANY: u32 = u32::MAX & !Ground::None.bit();
/// Fond où les algues et roseaux s'accrochent.
const SEABED: u32 = Ground::Sand.bit() | Ground::Gravel.bit() | Ground::Till.bit() | Ground::Clay.bit()
    | Ground::Rock.bit();

/// Eau de la colonne, pour les plantes aquatiques.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Water {
    Fresh,
    Brackish,
    Salt,
}

impl Water {
    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

const FRESH: u8 = Water::Fresh.bit();
const FRESH_BRACKISH: u8 = Water::Fresh.bit() | Water::Brackish.bit();
const SALT: u8 = Water::Salt.bit() | Water::Brackish.bit();

/// Où une plante pousse.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Habitat {
    /// Colonne sèche.
    Land,
    /// Colonne sèche dont une voisine est en eau douce ou saumâtre.
    Shore,
    /// Posée au fond d'une eau de `min..=max` blocs de profondeur.
    Bed { min: i32, max: i32, water: u8 },
    /// Flotte à la surface d'une eau douce de `min` blocs de profondeur ou plus.
    Floating { min: i32 },
}

/// Fiche d'une plante : où, sur quel sol, dans quels biomes et à quelle densité.
struct Spec {
    block: &'static str,
    habitat: Habitat,
    grounds: u32,
    /// `(biome, densité par colonne dans une tache pleine)` ; vide = tous les biomes.
    biomes: &'static [(NorrBiome, f32)],
    /// Longueur d'onde des taches et seuil du fBm (0 = partout, 0,6 = taches rares).
    wavelength: f64,
    threshold: f64,
}

use NorrBiome as B;

const fn spec(
    block: &'static str,
    habitat: Habitat,
    grounds: u32,
    biomes: &'static [(NorrBiome, f32)],
    wavelength: f64,
    threshold: f64,
) -> Spec {
    Spec {
        block,
        habitat,
        grounds,
        biomes,
        wavelength,
        threshold,
    }
}

/// Table des plantes, par priorité (rares et précieuses d'abord).
/// Nombre de fiches (le roseau en a deux : sur la rive et dans l'eau).
const SPEC_COUNT: usize = 38;

static SPECS: [Spec; SPEC_COUNT] = [
    // --- Champignons et lichens d'ombre ---
    spec("flugsvamp", Habitat::Land, FOREST, &[(B::Urskog, 0.012), (B::Fjallbjorkskog, 0.008)], 24.0, 0.5),
    spec("karljohan", Habitat::Land, FOREST | Ground::Lichen.bit(),
         &[(B::Urskog, 0.012), (B::Bokskog, 0.015), (B::Tallhed, 0.008)], 28.0, 0.5),
    spec("kantarell", Habitat::Land, FOREST | MEADOW,
         &[(B::Urskog, 0.02), (B::Bokskog, 0.02), (B::Sjoland, 0.01)], 26.0, 0.5),
    spec("skagglav", Habitat::Land, FOREST, &[(B::Urskog, 0.03)], 30.0, 0.45),
    // --- Baies ---
    spec("hjortron", Habitat::Land, BOG, &[(B::Myr, 0.06), (B::Palsmyr, 0.08)], 30.0, 0.5),
    spec("tranbar", Habitat::Land, BOG | Ground::Lichen.bit() | Ground::Moss.bit(),
         &[(B::Myr, 0.08), (B::Palsmyr, 0.05), (B::Fjallhed, 0.05)], 34.0, 0.45),
    spec("blabar", Habitat::Land, FOREST | MEADOW | Ground::Till.bit(),
         &[(B::Urskog, 0.10), (B::Sjoland, 0.08), (B::Fjallbjorkskog, 0.08)], 34.0, 0.45),
    spec("lingon", Habitat::Land, FOREST | Ground::Lichen.bit(),
         &[(B::Tallhed, 0.12), (B::Urskog, 0.06), (B::Hogkust, 0.05)], 32.0, 0.45),
    spec("smultron", Habitat::Land, MEADOW,
         &[(B::Hagmark, 0.04), (B::Sjoland, 0.02), (B::Strandang, 0.02)], 24.0, 0.5),
    spec("nypon", Habitat::Land, MEADOW | COARSE | Ground::Moss.bit(),
         &[(B::Klintkust, 0.04), (B::Hogkust, 0.04), (B::Strandang, 0.03), (B::Skargard, 0.03), (B::Alvar, 0.02)],
         26.0, 0.5),
    spec("havtorn", Habitat::Land, COARSE | MEADOW, &[(B::Strandang, 0.03), (B::Skargard, 0.02)], 26.0, 0.5),
    // --- Landes et toundra ---
    spec("ljung", Habitat::Land, COARSE | MEADOW | FOREST | Ground::Lichen.bit(),
         &[(B::Tallhed, 0.15), (B::Skargard, 0.10), (B::Hogkust, 0.08), (B::Alvar, 0.06)], 36.0, 0.4),
    spec("renlav", Habitat::Land, Ground::Lichen.bit() | COARSE | Ground::Moss.bit(),
         &[(B::Tallhed, 0.10), (B::Fjallhed, 0.12), (B::Kalfjall, 0.02)], 40.0, 0.45),
    spec("angsull", Habitat::Land, BOG | Ground::Moss.bit(),
         &[(B::Myr, 0.10), (B::Palsmyr, 0.10), (B::Fjallhed, 0.03)], 34.0, 0.4),
    spec("dvargbjork", Habitat::Land, Ground::Lichen.bit() | Ground::Moss.bit() | Ground::Till.bit() | Ground::Peat.bit(),
         &[(B::Fjallhed, 0.06), (B::Fjallbjorkskog, 0.03), (B::Myr, 0.015)], 30.0, 0.5),
    spec("fjallsippa", Habitat::Land, Ground::Lichen.bit() | Ground::Moss.bit() | COARSE,
         &[(B::Fjallhed, 0.03), (B::Kalfjall, 0.015)], 28.0, 0.55),
    spec("isranunkel", Habitat::Land, COARSE | Ground::Lichen.bit(), &[(B::Kalfjall, 0.02), (B::Glaciar, 0.01)], 28.0, 0.5),
    // --- Fleurs ---
    spec("vitsippa", Habitat::Land, Ground::Leaves.bit() | MEADOW, &[(B::Bokskog, 0.20)], 32.0, 0.4),
    spec("blasippa", Habitat::Land, Ground::Leaves.bit(), &[(B::Bokskog, 0.06)], 28.0, 0.5),
    spec("ramslok", Habitat::Land, Ground::Leaves.bit() | Ground::Soil.bit() | MEADOW,
         &[(B::Bokskog, 0.08), (B::Sjoland, 0.03)], 26.0, 0.5),
    spec("midsommarblomster", Habitat::Land, MEADOW,
         &[(B::Hagmark, 0.10), (B::Fjallbjorkskog, 0.06), (B::Strandang, 0.04)], 28.0, 0.45),
    spec("prastkrage", Habitat::Land, MEADOW, &[(B::Hagmark, 0.06), (B::Strandang, 0.03), (B::Alvar, 0.02)], 26.0, 0.5),
    spec("blaklocka", Habitat::Land, MEADOW, &[(B::Hagmark, 0.05), (B::Alvar, 0.05), (B::Fjallhed, 0.02)], 26.0, 0.5),
    spec("rodklover", Habitat::Land, MEADOW, &[(B::Hagmark, 0.05), (B::Strandang, 0.04)], 26.0, 0.5),
    spec("lupin", Habitat::Land, MEADOW, &[(B::Hagmark, 0.02), (B::Strandang, 0.02)], 40.0, 0.55),
    spec("mjodort", Habitat::Land, MEADOW | Ground::Moss.bit(),
         &[(B::Sjoland, 0.05), (B::Alv, 0.05), (B::Strandang, 0.04)], 30.0, 0.45),
    spec("rallarros", Habitat::Land, BARE,
         &[(B::Brannfalt, 0.10), (B::Bergslag, 0.02), (B::Kalfjall, 0.03)], 30.0, 0.4),
    spec("kvanne", Habitat::Land, MEADOW | Ground::Moss.bit() | COARSE,
         &[(B::Alv, 0.06), (B::Fjallbjorkskog, 0.03), (B::Lavafalt, 0.02)], 28.0, 0.5),
    spec("orkide", Habitat::Land, MEADOW | Ground::Gravel.bit(), &[(B::Alvar, 0.02), (B::Klintkust, 0.015)], 30.0, 0.55),
    spec("glasort", Habitat::Land, Ground::Sand.bit() | Ground::Clay.bit() | Ground::Gravel.bit() | MEADOW,
         &[(B::Strandang, 0.06), (B::Skargard, 0.03)], 24.0, 0.45),
    // --- Rives, lacs et mers ---
    spec("vass", Habitat::Shore, ANY, &[], 30.0, 0.35),
    spec("vass", Habitat::Bed { min: 1, max: 2, water: FRESH_BRACKISH }, SEABED | Ground::Peat.bit() | Ground::Soil.bit(),
         &[], 30.0, 0.35),
    spec("kaveldun", Habitat::Bed { min: 1, max: 1, water: FRESH }, ANY, &[], 26.0, 0.5),
    spec("nackros", Habitat::Floating { min: 2 }, ANY, &[], 24.0, 0.55),
    spec("blastang", Habitat::Bed { min: 1, max: 6, water: SALT },
         SEABED,
         &[(B::Skargard, 0.15), (B::Klintkust, 0.12), (B::Hogkust, 0.15), (B::Fjord, 0.08), (B::Vasterhav, 0.05)],
         36.0, 0.4),
    spec("sockertang", Habitat::Bed { min: 3, max: 18, water: Water::Salt.bit() }, SEABED,
         &[(B::Vasterhav, 0.14), (B::Hogkust, 0.10), (B::Fjord, 0.14), (B::Skargard, 0.12)], 40.0, 0.4),
    // --- Strate herbacée ---
    spec("ormbunke", Habitat::Land, FOREST | MEADOW | Ground::Till.bit(),
         &[(B::Urskog, 0.14), (B::Bokskog, 0.12), (B::Sjoland, 0.08)], 36.0, 0.4),
    spec("hogt_gras", Habitat::Land, MEADOW | Ground::Moss.bit(),
         &[(B::Hagmark, 0.35), (B::Strandang, 0.35), (B::Sjoland, 0.25), (B::Alv, 0.20), (B::Klintkust, 0.20),
           (B::Fjallbjorkskog, 0.15), (B::Skargard, 0.15), (B::Alvar, 0.10), (B::Bokskog, 0.06), (B::Urskog, 0.04),
           (B::Tallhed, 0.03)],
         44.0, 0.25),
];

/// Ce qu'une colonne offre à la strate basse.
#[derive(Debug, Clone, Copy)]
pub struct Column {
    pub wx: i32,
    pub wz: i32,
    pub biome: NorrBiome,
    /// Nature du sol (dernier bloc posé) ; fond pour une colonne sous l'eau.
    pub ground: Ground,
    /// Eau de la colonne ; `None` si elle est sèche.
    pub water: Option<Water>,
    /// Épaisseur d'eau (0 au sec).
    pub depth: i32,
    /// Eau douce ou saumâtre sur l'une des 4 voisines (colonne sèche).
    pub shore: bool,
}

/// Plante choisie : bloc et position verticale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    pub block: BlockId,
    /// `true` : flotte au dernier niveau d'eau ; `false` : première cellule libre sur le sol.
    pub floating: bool,
}

/// Blocs de la strate basse, lus dans le registre : un par fiche de [`SPECS`], et la nature du sol
/// que porte chaque bloc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloraBlocks {
    plants: [BlockId; SPEC_COUNT],
    ground: [Ground; 256],
}

/// Générateur de la strate basse.
#[derive(Debug, Clone)]
pub struct Understory {
    seed: u64,
    patches: GradField,
    blocks: FloraBlocks,
    /// Indices dans [`SPECS`] par biome, dans l'ordre de priorité.
    by_biome: Vec<Vec<u8>>,
}

const SEED_CONST: u64 = 0x4E52_5743_0000_0701;

impl FloraBlocks {
    pub fn from_registry(registry: &BlockRegistry) -> Self {
        let plants = std::array::from_fn(|i| registry.get(SPECS[i].block).id);
        let mut ground = [Ground::None; 256];
        let table: [(&str, Ground); 29] = [
            ("gras", Ground::Grass),
            ("mossa", Ground::Moss),
            ("barrmatta", Ground::Needles),
            ("lovforna", Ground::Leaves),
            ("lavhed", Ground::Lichen),
            ("vitmossa", Ground::Sphagnum),
            ("torv", Ground::Peat),
            ("jord", Ground::Soil),
            ("aska", Ground::Ash),
            ("rodmull", Ground::Mull),
            ("flygsand", Ground::Sand),
            ("svartsand", Ground::Sand),
            ("grus", Ground::Gravel),
            ("klappersten", Ground::Gravel),
            ("moran", Ground::Till),
            ("blalera", Ground::Clay),
            ("granit", Ground::Rock),
            ("rodgranit", Ground::Rock),
            ("gnejs", Ground::Rock),
            ("kalksten", Ground::Rock),
            ("krita", Ground::Rock),
            ("sandsten", Ground::Rock),
            ("basaltsten", Ground::Rock),
            ("sprucket_berg", Ground::Rock),
            ("slagg", Ground::Rock),
            ("diabas", Ground::Rock),
            ("gronsten", Ground::Rock),
            ("porfyr", Ground::Rock),
            ("djupberg", Ground::Rock),
        ];
        for (name, g) in table {
            let id = registry.get(name).id;
            if id != 0 {
                ground[usize::from(id)] = g;
            }
        }
        Self { plants, ground }
    }
}

impl Understory {
    pub fn new(seed: i64, blocks: FloraBlocks) -> Self {
        let by_biome = NorrBiome::ALL
            .iter()
            .map(|b| {
                SPECS
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.biomes.is_empty() || s.biomes.iter().any(|(sb, _)| sb == b))
                    .map(|(i, _)| i as u8)
                    .collect()
            })
            .collect();
        Self {
            seed: (seed as u64) ^ SEED_CONST,
            patches: GradField::new(seed, SEED_CONST),
            blocks,
            by_biome,
        }
    }

    /// Nature du sol que porte le bloc `id`.
    #[inline]
    pub fn ground_of(&self, id: BlockId) -> Ground {
        self.blocks.ground[usize::from(id)]
    }

    /// Tache de la plante `i` en `(wx, wz)`, dans [0, 1].
    fn patch(&self, i: usize, wx: i32, wz: i32) -> f64 {
        let s = &SPECS[i];
        let k = i as f64;
        let v = self.patches.fbm(
            f64::from(wx) / s.wavelength + 13.7 * k,
            f64::from(wz) / s.wavelength - 9.1 * k,
            2,
        );
        smooth(s.threshold - 0.1, s.threshold + 0.1, v)
    }

    /// Plante de la colonne, s'il y en a une.
    pub fn pick(&self, c: &Column) -> Option<Pick> {
        if c.ground == Ground::None {
            return None;
        }
        for &i in &self.by_biome[c.biome as usize] {
            let i = i as usize;
            let s = &SPECS[i];
            let (ok, floating) = match (s.habitat, c.water) {
                (Habitat::Land, None) => (true, false),
                (Habitat::Shore, None) => (c.shore, false),
                (Habitat::Bed { min, max, water }, Some(w)) => {
                    ((min..=max).contains(&c.depth) && water & w.bit() != 0, false)
                }
                (Habitat::Floating { min }, Some(Water::Fresh)) => (c.depth >= min, true),
                _ => (false, false),
            };
            if !ok || s.grounds & c.ground.bit() == 0 {
                continue;
            }
            let density = if s.biomes.is_empty() {
                // Plantes d'eau et de rive : densité propre à l'habitat.
                match s.habitat {
                    Habitat::Shore => 0.25,
                    Habitat::Bed { .. } => 0.28,
                    _ => 0.12,
                }
            } else {
                s.biomes
                    .iter()
                    .find(|(b, _)| *b == c.biome)
                    .map_or(0.0, |(_, d)| f64::from(*d))
            };
            let u = hash01(self.seed, c.wx, c.wz, i as i32);
            if u >= density || u >= density * self.patch(i, c.wx, c.wz) {
                continue;
            }
            return Some(Pick {
                block: self.blocks.plants[i],
                floating,
            });
        }
        None
    }

    /// Blocs de la strate basse, dans l'ordre de [`SPECS`] (tests et cartes).
    pub fn block_names() -> impl Iterator<Item = &'static str> {
        SPECS.iter().map(|s| s.block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_data::WorldKind;

    #[test]
    fn every_spec_names_a_registered_block() {
        let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
        for n in Understory::block_names() {
            assert_ne!(reg.get(n).id, 0, "{n}");
            assert!(reg.get(n).cross_shaped, "{n} doit être en croix");
            assert!(!reg.get(n).solid, "{n}");
        }
    }

    const ALL_GROUNDS: [Ground; 15] = [
        Ground::Grass, Ground::Moss, Ground::Needles, Ground::Leaves, Ground::Lichen, Ground::Sphagnum,
        Ground::Peat, Ground::Soil, Ground::Ash, Ground::Mull, Ground::Sand, Ground::Gravel, Ground::Till,
        Ground::Clay, Ground::Rock,
    ];

    /// Chaque fiche est atteignable : un biome, un sol et une eau qu'elle admet, une colonne où
    /// le tirage la choisit.
    #[test]
    fn every_spec_can_be_picked() {
        let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
        let u = Understory::new(7, FloraBlocks::from_registry(&reg));
        for (i, s) in SPECS.iter().enumerate() {
            let biomes: Vec<NorrBiome> = if s.biomes.is_empty() {
                vec![NorrBiome::Sjoland]
            } else {
                s.biomes.iter().map(|b| b.0).collect()
            };
            let ground = ALL_GROUNDS
                .into_iter()
                .find(|g| s.grounds & g.bit() != 0)
                .unwrap();
            let (water, depth, shore) = match s.habitat {
                Habitat::Land => (None, 0, false),
                Habitat::Shore => (None, 0, true),
                Habitat::Bed { min, water, .. } => (
                    Some(if water & Water::Salt.bit() != 0 {
                        Water::Salt
                    } else if water & Water::Brackish.bit() != 0 {
                        Water::Brackish
                    } else {
                        Water::Fresh
                    }),
                    min,
                    false,
                ),
                Habitat::Floating { min } => (Some(Water::Fresh), min, false),
            };
            let found = biomes.iter().any(|&biome| {
                (0..40_000).any(|k| {
                    u.pick(&Column {
                        wx: k % 200,
                        wz: k / 200,
                        biome,
                        ground,
                        water,
                        depth,
                        shore,
                    })
                    .is_some_and(|p| p.block == u.blocks.plants[i])
                })
            });
            assert!(found, "{} ({:?}) jamais choisie", s.block, s.habitat);
        }
    }
    #[test]
    fn specs_use_only_declared_biomes_once() {
        for s in &SPECS {
            let mut seen = Vec::new();
            for (b, d) in s.biomes {
                assert!(!seen.contains(b), "{} : {:?} deux fois", s.block, b);
                seen.push(*b);
                assert!(*d > 0.0 && *d <= 1.0, "{}", s.block);
            }
        }
    }
}
