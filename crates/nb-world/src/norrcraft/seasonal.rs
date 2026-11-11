//! Surcouche saisonnière d'un chunk (session 3.2 ; GDD doc 04 §2.3).
//!
//! La génération produit l'état de fin d'été. Pour un pas de saison donné
//! ([`crate::seasons`]), [`apply_column`] pose sur une colonne **naturelle** : le manteau neigeux
//! en couches, la glace des eaux, les feuillus effeuillés (ramilles nues) et la flore de saison.
//! Les colonnes dont un bloc a été posé ou retiré par le joueur ne sont jamais touchées
//! ([`pristine`]).
//!
//! Ce module n'appelle aucune fonction transcendante (test de `norrcraft::mod`) : tout le calcul
//! de température, de neige et de glace est dans [`crate::seasons`].

use nb_data::{BlockId, BlockRegistry};

use super::biomes::NorrBiome;
use super::fields::MacroSample;
use super::hydro::{WaterColumn, WaterKind};
use crate::chunk::{Chunk, AIR, CHUNK_SIZE_Y};
use crate::seasons::{FloraState, IceWater, SeasonRules, Site, STEPS};

/// Rayon de recherche des feuillages au-dessus du sol (hauteur maximale d'un arbre).
const CANOPY_REACH: i32 = 48;

/// Blocs de la surcouche et classes de blocs que le registre du monde permet de reconnaître.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeasonBlocks {
    /// `snotacke_1` à `snotacke_7`.
    pub layers: [BlockId; 7],
    /// Neige pleine (`sno`).
    pub snow: BlockId,
    pub water: BlockId,
    /// Glace neuve, portante, épaisse (lacs) et glace de mer.
    pub new_ice: BlockId,
    pub ice: BlockId,
    pub thick_ice: BlockId,
    pub sea_ice: BlockId,
    /// Ramilles nues et rameau sans fruits.
    pub twig: BlockId,
    pub sprig: BlockId,
    /// Cellules que la surcouche peut remplacer (air, eau, plantes, feuilles caduques).
    soft: [bool; 256],
    /// Sorties de la surcouche : un bloc ainsi posé n'est pas une modification du joueur.
    output: [bool; 256],
    plant: [bool; 256],
    deciduous: [bool; 256],
    conifer: [bool; 256],
    /// État de chaque plante à saison à chaque pas.
    flora: [[FloraState; STEPS as usize]; 256],
}

impl SeasonBlocks {
    pub fn from_registry(r: &BlockRegistry) -> Self {
        let id = |n: &str| {
            let b = r.get(n);
            assert!(b.name == n, "saisons : bloc « {n} » absent de Norrcraft");
            b.id
        };
        let layers = std::array::from_fn(|i| id(&format!("snotacke_{}", i + 1)));
        let me = Self {
            layers,
            snow: id("sno"),
            water: id("water"),
            new_ice: id("nyis"),
            ice: id("is"),
            thick_ice: id("tjockis"),
            sea_ice: id("havsis"),
            twig: id("kvistar"),
            sprig: id("ris"),
            soft: [false; 256],
            output: [false; 256],
            plant: [false; 256],
            deciduous: [false; 256],
            conifer: [false; 256],
            flora: [[FloraState::Present; STEPS as usize]; 256],
        };
        let rules = SeasonRules::builtin();
        let mut out = me;
        out.soft[usize::from(AIR)] = true;
        out.soft[usize::from(out.water)] = true;
        for l in layers {
            out.output[usize::from(l)] = true;
        }
        for b in [
            out.snow,
            out.new_ice,
            out.ice,
            out.thick_ice,
            out.sea_ice,
            out.twig,
            out.sprig,
            AIR,
        ] {
            out.output[usize::from(b)] = true;
        }
        for b in r.iter() {
            let i = usize::from(b.id);
            if b.cross_shaped {
                out.soft[i] = true;
                out.plant[i] = true;
                for (s, slot) in out.flora[i].iter_mut().enumerate() {
                    *slot = rules.flora_state(&b.name, s as u8);
                }
            }
            if b.leaves_tinted && b.seasonal {
                out.soft[i] = true;
                out.deciduous[i] = true;
            } else if b.leaves_tinted {
                out.conifer[i] = true;
            }
        }
        out
    }

    /// Bloc de couche de neige de `eighths` huitièmes (1..=7).
    fn layer(&self, eighths: i32) -> BlockId {
        self.layers[(eighths.clamp(1, 7) - 1) as usize]
    }

    /// Un bloc de la surcouche peut-il remplacer le bloc de base `base` par `cur` sans que ce
    /// soit une modification du joueur ?
    #[inline]
    pub fn natural(&self, cur: BlockId, base: BlockId) -> bool {
        cur == base || (self.soft[usize::from(base)] && self.output[usize::from(cur)])
    }
}

/// Une colonne du chunk est-elle encore naturelle ? `cur` est le contenu actuel du chunk, `base`
/// le chunk de fin d'été régénéré : chaque cellule est identique ou remplacée par une sortie de
/// la surcouche à la place d'un bloc remplaçable.
pub fn pristine(b: &SeasonBlocks, cur: &Chunk, base: &Chunk, x: i32, z: i32) -> bool {
    (0..CHUNK_SIZE_Y as i32).all(|y| b.natural(cur.block(x, y, z), base.block(x, y, z)))
}

/// Ce que la surcouche sait d'une colonne.
pub struct ColumnInput<'a> {
    /// Coordonnées monde.
    pub wx: i32,
    pub wz: i32,
    pub water: WaterColumn,
    /// `y` de la cellule posée sur le sol (dessus de la glace d'un glacier compris).
    pub ground: i32,
    pub macros: &'a MacroSample,
    pub biome: NorrBiome,
    /// Hauteur du sol de la colonne amont du vent dominant (ouest).
    pub upwind_top: i32,
    pub sea_level: i32,
    pub world_height: i32,
}

/// Hachage d'une cellule en [0, 1[ (décide quelles feuilles tombent).
fn hash01(wx: i32, y: i32, wz: i32) -> f64 {
    let mut h = (wx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (wz as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Température moyenne et site d'une colonne à l'altitude `y` (T = T₀ − 0,006·max(0, y − S) +
/// 0,05·(O − 0,5), doc 02 §4).
fn site_at(m: &MacroSample, y: i32, sea_level: i32) -> Site {
    Site {
        mean: super::climate::temperature_at(m.t0, m.o, f64::from(y), sea_level),
        lambda: 1.0 - m.t0,
        wet: m.hm,
    }
}

/// Pose la surcouche du pas `step` sur la colonne `(x, z)` du chunk (coordonnées locales).
pub fn apply_column(
    chunk: &mut Chunk,
    b: &SeasonBlocks,
    rules: &SeasonRules,
    step: u8,
    x: i32,
    z: i32,
    c: &ColumnInput,
) {
    let w = c.water;
    if w.is_wet() {
        freeze(chunk, b, rules, step, x, z, c);
        return;
    }
    let y0 = c.ground;
    if y0 < 1 || y0 >= c.world_height - 1 {
        return;
    }
    let site = site_at(c.macros, y0, c.sea_level);

    // Feuillus : la part de feuilles gardée à la branche ; le reste tombe (moitié ramilles).
    let keep = rules.leaf_fraction(step, &site);
    let mut conifer = false;
    for y in y0..(y0 + CANOPY_REACH).min(c.world_height) {
        let id = chunk.block(x, y, z);
        if b.conifer[usize::from(id)] {
            conifer = true;
        } else if b.deciduous[usize::from(id)] && keep < 1.0 {
            let h = hash01(c.wx, y, c.wz);
            if h >= keep {
                // Les cellules qui tombent: un bloc sur deux devient une ramille, l'autre rien.
                let half = keep + (1.0 - keep) * 0.5;
                chunk.set_block(x, y, z, if h < half { b.twig } else { AIR });
            }
        }
    }

    // Flore de saison de la cellule posée sur le sol.
    let here = chunk.block(x, y0, z);
    let mut cell = here;
    if b.plant[usize::from(here)] {
        match b.flora[usize::from(here)][usize::from(step % STEPS)] {
            FloraState::Present => {}
            FloraState::Gone => cell = AIR,
            FloraState::Sprig => cell = b.sprig,
        }
        if cell != here {
            chunk.set_block(x, y0, z, cell);
        }
    }

    // Manteau neigeux.
    let sr = &rules.snow;
    let mut depth = rules.snow_layers(step, &site);
    if depth < sr.min_visible {
        return;
    }
    if conifer {
        depth *= sr.canopy;
    }
    // Congère : le sol à découvert tombe d'au moins `drift_drop` blocs derrière une crête amont.
    let drift = c.upwind_top - w.top >= sr.drift_drop;
    let mut layers = depth.round() as i32 + if drift { sr.drift_layers } else { 0 };
    if layers < 1 {
        return;
    }
    // Une plante n'est enfouie qu'à partir de deux couches ; sous cela, elle émerge.
    if b.plant[usize::from(cell)] && layers < 2 {
        return;
    }
    layers = layers.min(8 * 3);
    let mut y = y0;
    // Une cellule occupée par autre chose qu'une plante ou de l'air (tronc, bloc) arrête la pile.
    let free = |chunk: &Chunk, y: i32| {
        y < c.world_height && {
            let id = chunk.block(x, y, z);
            id == AIR || (b.plant[usize::from(id)] && !b.deciduous[usize::from(id)])
        }
    };
    while layers >= 8 && free(chunk, y) {
        chunk.set_block(x, y, z, b.snow);
        layers -= 8;
        y += 1;
    }
    if layers > 0 && free(chunk, y) {
        chunk.set_block(x, y, z, b.layer(layers));
    }
}

/// Glace d'une colonne d'eau : la cellule du dessus gèle selon le rang de la glace.
fn freeze(
    chunk: &mut Chunk,
    b: &SeasonBlocks,
    rules: &SeasonRules,
    step: u8,
    x: i32,
    z: i32,
    c: &ColumnInput,
) {
    let w = c.water;
    // Rivières et rapides ne gèlent pas ; la banquise l'est déjà.
    let kind = match w.kind {
        WaterKind::Lake => IceWater::Lake,
        WaterKind::Sea if c.biome == NorrBiome::Bottenhav => IceWater::Brackish,
        WaterKind::Sea if c.biome != NorrBiome::Packis => IceWater::Sea,
        _ => return,
    };
    let y = w.level - 1;
    let cur = chunk.block(x, y, z);
    if cur != b.water && !b.plant[usize::from(cur)] {
        return;
    }
    let site = site_at(c.macros, w.level, c.sea_level);
    let e = rules.ice(step, &site, kind);
    let block = match (kind, e) {
        (_, 0) => return,
        (IceWater::Lake, 1) => b.new_ice,
        (IceWater::Lake, 2) => b.ice,
        (IceWater::Lake, _) => b.thick_ice,
        _ => b.sea_ice,
    };
    chunk.set_block(x, y, z, block);
}
