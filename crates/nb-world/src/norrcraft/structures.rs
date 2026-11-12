//! Structures Norrcraft : grilles de placement, prédicats de site, pistes et cairns (session 4.1 ;
//! GDD doc 02 §10.2 et §10.3).
//!
//! # Placement (§10.2)
//!
//! Trois grilles hachées indépendantes : [`Class::Major`] (pas 384), [`Class::Medium`] (pas 96) et
//! [`Class::Minor`] (pas 32). Chaque cellule tire (graine, i, j) une position dans la cellule puis
//! un type parmi ceux que le biome de cette position autorise (le poids d'un type dans un biome est
//! sa probabilité par cellule ; la somme par biome et par classe ne dépasse pas 1, le reste est
//! vide). Le **prédicat de site** du type (pente, altitude, eau, maximum local…) lit le relief par
//! des sondes pures ([`SiteProbe`]) ; en cas d'échec la cellule reste vide, sans second tirage.
//! Une structure d'une classe plus petite qui chevauche une plus grande est supprimée : le
//! résultat est une fonction pure de (graine, cellule), indépendante de l'ordre des chunks.
//!
//! La table [`RULES`] décrit déjà tout le catalogue du doc 02 §10.1 ; seuls les **rösen** sont
//! construits par cette session. Les autres sites portent un *jalon* provisoire (un tas de pierres
//! plus ou moins haut selon la classe, voir [`PLACEHOLDERS`]) que les sessions 4.2 à 4.4 remplacent
//! par leur bâtiment (champ [`Rule::built`]).
//!
//! # Pistes (§10.3)
//!
//! Entre deux sites « à piste » (refuges, camps, fäbodar, villages) distants de moins de
//! [`TRACK_RANGE`] blocs, une piste est tracée si l'arête figure dans le graphe de Gabriel des
//! sites (aucun troisième site dans le disque de diamètre AB : calcul local, entier, exact). Le
//! segment droit est échantillonné tous les 8 blocs (50 points au plus), chaque point glisse
//! perpendiculairement vers le minimum de `pente² + pénalité d'eau` dans ±12 blocs, les décalages
//! sont lissés, et les points sont reliés en cellules 4-connexes. Selon le sol : cairns tous les
//! 24 blocs en montagne, caillebotis en tourbière, planches ou gué sur l'eau, sentier tassé
//! ailleurs. Une piste est une fonction de ses deux extrémités : chaque chunk n'écrit que sa part.
//!
//! Ce module n'appelle aucune fonction transcendante (test de `norrcraft::mod`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use nb_data::{BlockId, BlockRegistry};

use super::biomes::NorrBiome;
use super::buildings::{self, Building, Mat};
use super::hydro::{WaterColumn, WaterKind};
use super::math::{hash01, hash64};
use super::subsoil::SiteProbe;
use super::trees::TreeBlocks;
use crate::chunk::{Chunk, AIR, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};

/// Les sites sans bâtiment (sessions 4.2 à 4.4) portent un jalon de pierres : passer à `false`
/// pour ne rien poser d'autre que les rösen et les pistes.
pub const PLACEHOLDERS: bool = true;
/// Longueur maximale d'une piste (blocs) : au-delà, pas de piste.
pub const TRACK_RANGE: i32 = 400;
/// Pas d'échantillonnage du segment, en blocs.
pub const TRACK_STEP: f64 = 8.0;
/// Glissement perpendiculaire maximal d'un point de piste, en blocs.
pub const TRACK_PULL: i32 = 12;
/// Distance des cairns de piste, en cellules de piste.
pub const CAIRN_EVERY: usize = 24;
/// Plus longue traversée d'eau acceptée (cellules) ; au-delà la piste n'existe pas.
pub const MAX_WET_RUN: usize = 16;
/// Dégagement autour d'une piste où aucun arbre ne pousse (Chebyshev).
const PATH_CLEAR: i32 = 2;
/// Marge de séparation entre deux structures de classes différentes, en blocs.
const GAP: i32 = 4;
/// Positions de repli d'un type qui exige un lieu précis (rive, rivière, sommet…).
const SNAP_TRIES: i32 = 15;
/// Cases mémorisées avant que les caches soient vidés.
const CACHE_MAX: usize = 1 << 15;
/// Constante de semence des grilles de structures.
const SEED: u64 = 0x4E52_5743_0000_0801;
/// Constante de semence des cairns de piste.
const CAIRN_SEED: u64 = 0x4E52_5743_0000_0802;
/// Rayon d'un jalon ou d'un röse, en blocs (empreinte réservée aux arbres).
const MARK_RADIUS: i32 = 3;

// ---------------------------------------------------------------------------------------------
// Classes et catalogue
// ---------------------------------------------------------------------------------------------

/// Classe d'une structure : sa grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    /// Pas 384 : villages, églises, mines, hauts fourneaux, enceintes, feux d'alarme.
    Major,
    /// Pas 96 : fermes, hameaux, camps, refuges, tumulus, phares, halles.
    Medium,
    /// Pas 32 : pierres runiques, granges, abris, rochers, cairns.
    Minor,
}

impl Class {
    pub const ALL: [Class; 3] = [Class::Major, Class::Medium, Class::Minor];

    /// Pas de la grille, en blocs.
    pub const fn pitch(self) -> i32 {
        match self {
            Class::Major => 384,
            Class::Medium => 96,
            Class::Minor => 32,
        }
    }

    /// Marge entre la position tirée et le bord de la cellule (au moins le rayon maximal de la
    /// classe : une structure ne déborde jamais de sa cellule).
    pub const fn margin(self) -> i32 {
        match self {
            Class::Major => 48,
            Class::Medium => 18,
            Class::Minor => 7,
        }
    }

    const fn tag(self) -> i32 {
        match self {
            Class::Major => 0,
            Class::Medium => 1,
            Class::Minor => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Class::Major => "majeure",
            Class::Medium => "moyenne",
            Class::Minor => "mineure",
        }
    }
}

/// Type de structure (doc 02 §10.1). L'ordre est celui de [`RULES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum StructKind {
    // Majeures.
    By,
    Stavkyrka,
    Gruva,
    Hytta,
    Fornborg,
    Bergasal,
    Vardkase,
    // Moyennes.
    Torp,
    Fabod,
    Fiskelage,
    Kata,
    Fjallstuga,
    Gravhog,
    Fyr,
    Langhus,
    Bastu,
    Skeppssattning,
    Vaderkvarn,
    Hallristning,
    Skeppsvrak,
    Bro,
    Baverdamm,
    Bjornide,
    Tjardal,
    Geyser,
    // Mineures.
    Runsten,
    Lada,
    Vindskydd,
    Sejte,
    Trojaborg,
    Flyttblock,
    Kolmila,
    Rosen,
}

impl StructKind {
    pub const COUNT: usize = 33;

    pub fn rule(self) -> &'static Rule {
        &RULES[self as usize]
    }

    pub fn name(self) -> &'static str {
        self.rule().name
    }

    pub fn class(self) -> Class {
        self.rule().class
    }
}

/// Condition d'eau d'un site.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shore {
    Any,
    Lake,
    Sea,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Wet {
    /// Centre et empreinte à sec.
    Dry,
    /// Centre à sec, eau de la nature donnée à moins de `d` blocs du centre.
    Shore(Shore, i32),
    /// Centre sous une mer d'une profondeur dans `[min, max]`.
    Afloat(i32, i32),
    /// Sans condition.
    Any,
}

/// Condition supplémentaire du prédicat de site.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Extra {
    None,
    /// Maximum local de la hauteur dans un rayon de `r` blocs (16 sondes).
    LocalMax(i32),
    /// Une marche de `W` (ou un barrage) à moins de `d` blocs.
    StepNear(i32),
    /// Centre dans une rivière dont les deux rives sont à moins de 14 blocs sur un axe.
    RiverCrossing,
    /// Surrection `U` au moins égale à la valeur (sous la chaîne).
    Mountain(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Terms {
    /// Différence de hauteur maximale entre le centre et les sondes sèches de l'empreinte.
    slope: i32,
    /// Altitude du sol au-dessus de la mer (min, max).
    alt: (i32, i32),
    wet: Wet,
    extra: Extra,
}

/// Fiche d'un type de structure.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub kind: StructKind,
    pub name: &'static str,
    pub class: Class,
    /// Rayon d'emprise (blocs) : dégagement, prédicat de pente et conflits entre classes.
    pub radius: i32,
    /// Biomes autorisés et probabilité par cellule dans ce biome.
    pub biomes: &'static [(NorrBiome, f64)],
    terms: Terms,
    /// Sert d'extrémité de piste (refuge, camp, fäbod, village).
    pub track: bool,
    /// Le bâtiment existe (sinon seulement un jalon provisoire).
    pub built: bool,
}

impl Rule {
    /// Le type exige-t-il un lieu précis (eau, sommet, marche…) : positions de repli permises.
    fn snaps(&self) -> bool {
        !matches!(self.terms.wet, Wet::Dry) || !matches!(self.terms.extra, Extra::None)
    }
}

use NorrBiome as B;

const fn t(slope: i32, lo: i32, hi: i32, wet: Wet, extra: Extra) -> Terms {
    Terms {
        slope,
        alt: (lo, hi),
        wet,
        extra,
    }
}

const FAR: i32 = 400;

#[allow(clippy::too_many_arguments)]
const fn rule(
    kind: StructKind,
    name: &'static str,
    class: Class,
    radius: i32,
    biomes: &'static [(NorrBiome, f64)],
    terms: Terms,
    track: bool,
    built: bool,
) -> Rule {
    Rule {
        kind,
        name,
        class,
        radius,
        biomes,
        terms,
        track,
        built,
    }
}

use Class::{Major, Medium, Minor};
use StructKind as K;

/// Le catalogue : une fiche par type, dans l'ordre de [`StructKind`].
pub const RULES: [Rule; StructKind::COUNT] = [
    // --- Majeures (pas 384) -----------------------------------------------------------------
    rule(
        K::By,
        "by",
        Major,
        44,
        &[(B::Hagmark, 0.40), (B::Bokskog, 0.35)],
        t(7, 2, 70, Wet::Dry, Extra::None),
        true,
        true,
    ),
    rule(
        K::Stavkyrka,
        "stavkyrka",
        Major,
        18,
        &[(B::Fjord, 0.10), (B::Hagmark, 0.12)],
        t(4, 2, 70, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Gruva,
        "gruva",
        Major,
        24,
        &[(B::Bergslag, 0.35)],
        t(6, 2, 120, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Hytta,
        "hytta",
        Major,
        20,
        &[(B::Bergslag, 0.25)],
        t(6, 2, 120, Wet::Dry, Extra::StepNear(20)),
        false,
        false,
    ),
    rule(
        K::Fornborg,
        "fornborg",
        Major,
        26,
        &[(B::Alvar, 0.25), (B::Hogkust, 0.15)],
        t(4, 2, 90, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Bergasal,
        "bergasal",
        Major,
        20,
        &[
            (B::Fjallhed, 0.12),
            (B::Kalfjall, 0.10),
            (B::Fjallbjorkskog, 0.05),
        ],
        t(8, 10, FAR, Wet::Dry, Extra::Mountain(0.55)),
        false,
        false,
    ),
    rule(
        K::Vardkase,
        "vårdkase",
        Major,
        6,
        &[
            (B::Hogkust, 0.25),
            (B::Klintkust, 0.15),
            (B::Fjallhed, 0.05),
        ],
        t(6, 12, FAR, Wet::Dry, Extra::LocalMax(40)),
        false,
        false,
    ),
    // --- Moyennes (pas 96) ------------------------------------------------------------------
    rule(
        K::Torp,
        "torp",
        Medium,
        14,
        &[(B::Hagmark, 0.12), (B::Sjoland, 0.10), (B::Urskog, 0.04)],
        t(3, 2, 80, Wet::Dry, Extra::None),
        false,
        true,
    ),
    rule(
        K::Fabod,
        "fäbod",
        Medium,
        16,
        &[(B::Fjallbjorkskog, 0.12), (B::Urskog, 0.04)],
        t(4, 40, FAR, Wet::Dry, Extra::None),
        true,
        true,
    ),
    rule(
        K::Fiskelage,
        "fiskeläge",
        Medium,
        18,
        &[(B::Skargard, 0.22), (B::Fjord, 0.15), (B::Klintkust, 0.12)],
        t(5, 0, 40, Wet::Shore(Shore::Sea, 6), Extra::None),
        false,
        true,
    ),
    rule(
        K::Kata,
        "kåta",
        Medium,
        14,
        &[
            (B::Fjallbjorkskog, 0.05),
            (B::Fjallhed, 0.05),
            (B::Palsmyr, 0.05),
        ],
        t(3, 10, FAR, Wet::Dry, Extra::None),
        true,
        true,
    ),
    rule(
        K::Fjallstuga,
        "fjällstuga",
        Medium,
        8,
        &[(B::Fjallhed, 0.06), (B::Kalfjall, 0.06)],
        t(3, 20, FAR, Wet::Dry, Extra::None),
        true,
        true,
    ),
    rule(
        K::Gravhog,
        "gravhög",
        Medium,
        12,
        &[(B::Hagmark, 0.06), (B::Alvar, 0.08)],
        t(3, 2, 80, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Fyr,
        "fyr",
        Medium,
        8,
        &[
            (B::Skargard, 0.04),
            (B::Klintkust, 0.04),
            (B::Hogkust, 0.04),
        ],
        t(6, 0, 80, Wet::Shore(Shore::Sea, 10), Extra::LocalMax(16)),
        false,
        false,
    ),
    rule(
        K::Langhus,
        "långhus",
        Medium,
        14,
        &[(B::Hagmark, 0.04), (B::Fjord, 0.04)],
        t(3, 2, 60, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Bastu,
        "bastu",
        Medium,
        8,
        &[(B::Sjoland, 0.12), (B::Skargard, 0.08)],
        t(4, 0, 60, Wet::Shore(Shore::Any, 6), Extra::None),
        false,
        true,
    ),
    rule(
        K::Skeppssattning,
        "skeppssättning",
        Medium,
        16,
        &[(B::Alvar, 0.03), (B::Strandang, 0.04), (B::Klintkust, 0.02)],
        t(3, 2, 40, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Vaderkvarn,
        "väderkvarn",
        Medium,
        8,
        &[(B::Alvar, 0.04), (B::Strandang, 0.05)],
        t(3, 2, 60, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Hallristning,
        "hällristning",
        Medium,
        6,
        &[(B::Hogkust, 0.05), (B::Skargard, 0.03), (B::Hagmark, 0.03)],
        t(3, 2, 70, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Skeppsvrak,
        "skeppsvrak",
        Medium,
        14,
        &[(B::Bottenhav, 0.06), (B::Vasterhav, 0.04)],
        t(6, -60, 0, Wet::Afloat(6, 30), Extra::None),
        false,
        false,
    ),
    rule(
        K::Bro,
        "bro",
        Medium,
        10,
        &[(B::Alv, 0.20)],
        t(5, -10, 90, Wet::Any, Extra::RiverCrossing),
        false,
        false,
    ),
    rule(
        K::Baverdamm,
        "bäverdamm",
        Medium,
        12,
        &[(B::Alv, 0.12), (B::Sjoland, 0.06)],
        t(4, 0, 80, Wet::Shore(Shore::Any, 5), Extra::None),
        false,
        false,
    ),
    rule(
        K::Bjornide,
        "björnide",
        Medium,
        6,
        &[(B::Urskog, 0.05)],
        t(5, 2, 100, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Tjardal,
        "tjärdal",
        Medium,
        8,
        &[(B::Tallhed, 0.04)],
        t(4, 2, 100, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Geyser,
        "geyser",
        Medium,
        8,
        &[(B::Lavafalt, 0.12)],
        t(4, 0, 100, Wet::Dry, Extra::None),
        false,
        false,
    ),
    // --- Mineures (pas 32) ------------------------------------------------------------------
    rule(
        K::Runsten,
        "runsten",
        Minor,
        2,
        &[
            (B::Bokskog, 0.03),
            (B::Hagmark, 0.04),
            (B::Alvar, 0.03),
            (B::Sjoland, 0.03),
            (B::Urskog, 0.02),
            (B::Tallhed, 0.02),
            (B::Strandang, 0.02),
            (B::Fjallbjorkskog, 0.02),
        ],
        t(2, 2, 140, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Lada,
        "lada",
        Minor,
        7,
        &[(B::Hagmark, 0.02), (B::Strandang, 0.03), (B::Myr, 0.015)],
        t(2, 1, 60, Wet::Dry, Extra::None),
        false,
        true,
    ),
    rule(
        K::Vindskydd,
        "vindskydd",
        Minor,
        5,
        &[(B::Urskog, 0.012), (B::Sjoland, 0.012)],
        t(3, 2, 100, Wet::Dry, Extra::None),
        false,
        true,
    ),
    rule(
        K::Sejte,
        "sejte",
        Minor,
        3,
        &[(B::Fjallhed, 0.012), (B::Kalfjall, 0.006)],
        t(5, 10, FAR, Wet::Shore(Shore::Lake, 8), Extra::None),
        false,
        false,
    ),
    rule(
        K::Trojaborg,
        "trojaborg",
        Minor,
        7,
        &[(B::Skargard, 0.02), (B::Strandang, 0.02)],
        t(2, 0, 12, Wet::Shore(Shore::Sea, 12), Extra::None),
        false,
        false,
    ),
    rule(
        K::Flyttblock,
        "flyttblock",
        Minor,
        4,
        &[(B::Urskog, 0.02), (B::Tallhed, 0.02), (B::Hagmark, 0.015)],
        t(3, 2, 130, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Kolmila,
        "kolmila",
        Minor,
        5,
        &[(B::Tallhed, 0.015), (B::Urskog, 0.008)],
        t(2, 2, 100, Wet::Dry, Extra::None),
        false,
        false,
    ),
    rule(
        K::Rosen,
        "rösen",
        Minor,
        2,
        &[(B::Fjallhed, 0.04), (B::Kalfjall, 0.04), (B::Myr, 0.006)],
        t(3, 4, FAR, Wet::Dry, Extra::None),
        false,
        true,
    ),
];

// ---------------------------------------------------------------------------------------------
// Sites
// ---------------------------------------------------------------------------------------------

/// Une structure placée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructSite {
    pub kind: StructKind,
    pub x: i32,
    pub z: i32,
    /// Premier `y` non solide de la colonne du centre (le fond sous l'eau).
    pub y: i32,
    /// Orientation 0 à 3 (quart de tour).
    pub rot: u8,
    /// Graine locale (variantes, bâtiments des sessions suivantes).
    pub seed: u64,
}

impl StructSite {
    pub fn rule(&self) -> &'static Rule {
        self.kind.rule()
    }

    pub fn class(&self) -> Class {
        self.rule().class
    }

    /// Le site porte-t-il un cairn (rösen construit, ou jalon provisoire d'un site à venir) ?
    pub fn has_cairn(&self) -> bool {
        let r = self.rule();
        r.kind == K::Rosen
            || (PLACEHOLDERS && !r.built && !matches!(r.terms.wet, Wet::Afloat(..) | Wet::Any))
    }

    /// Couches du cairn du site (rayon de chaque couche).
    fn cairn_profile(&self) -> &'static [i32] {
        match (self.kind, self.class()) {
            (K::Rosen, _) => &[1, 0, 0],
            (_, Major) => &[2, 2, 1, 1, 0, 0],
            (_, Medium) => &[1, 1, 0, 0],
            (_, Minor) => &[1, 0],
        }
    }
}

/// Tirage d'une cellule, avant le prédicat de site.
#[derive(Debug, Clone, Copy)]
struct Drawn {
    kind: StructKind,
    x: i32,
    z: i32,
}

type Key = (Class, i32, i32);

// ---------------------------------------------------------------------------------------------
// Prédicats de site
// ---------------------------------------------------------------------------------------------

const DIRS8: [(i32, i32); 8] = [
    (1, 0),
    (0, 1),
    (-1, 0),
    (0, -1),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, -1),
];

/// Huit points sur le carré de rayon `r` (les diagonales raccourcies à 7/10).
fn ring(r: i32) -> [(i32, i32); 8] {
    let d = (r * 7 + 5) / 10;
    [
        (r, 0),
        (0, r),
        (-r, 0),
        (0, -r),
        (d, d),
        (-d, d),
        (d, -d),
        (-d, -d),
    ]
}

fn shore_matches(s: Shore, k: WaterKind) -> bool {
    match s {
        Shore::Any => k != WaterKind::None,
        Shore::Lake => k == WaterKind::Lake,
        Shore::Sea => k == WaterKind::Sea,
    }
}

/// Prédicat de site : renvoie le `y` du centre si le site convient.
fn site_ok(rule: &Rule, x: i32, z: i32, probe: &SiteProbe, sea: i32) -> Option<i32> {
    let t = &rule.terms;
    let c = (probe.water)(x, z);
    let rel = c.top - sea;
    if rel < t.alt.0 || rel > t.alt.1 {
        return None;
    }
    let r = rule.radius.max(2);
    let mut offsets: Vec<(i32, i32)> = ring(r).to_vec();
    if r >= 10 {
        offsets.extend(ring(r / 2));
    }
    let cols: Vec<WaterColumn> = offsets
        .iter()
        .map(|&(dx, dz)| (probe.water)(x + dx, z + dz))
        .collect();
    match t.wet {
        Wet::Dry => {
            if c.is_wet() || cols.iter().any(WaterColumn::is_wet) {
                return None;
            }
        }
        Wet::Shore(kind, d) => {
            if c.is_wet() || !near_water(x, z, d, kind, probe) {
                return None;
            }
        }
        Wet::Afloat(lo, hi) => {
            let depth = c.level - c.top;
            if !c.is_wet() || c.kind != WaterKind::Sea || depth < lo || depth > hi {
                return None;
            }
        }
        Wet::Any => {}
    }
    // Pente : seules les sondes sèches comptent (sauf sous l'eau, où le fond est la référence).
    let afloat = matches!(t.wet, Wet::Afloat(..));
    if cols
        .iter()
        .filter(|w| afloat || !w.is_wet())
        .any(|w| (w.top - c.top).abs() > t.slope)
    {
        return None;
    }
    match t.extra {
        Extra::None => {}
        Extra::LocalMax(rad) => {
            let mut pts = ring(rad).to_vec();
            pts.extend(ring(rad / 2));
            if pts
                .iter()
                .any(|&(dx, dz)| (probe.water)(x + dx, z + dz).top > c.top)
            {
                return None;
            }
        }
        Extra::StepNear(d) => {
            let mut found = false;
            'rays: for (dx, dz) in DIRS8 {
                let mut s = 4;
                while s <= d {
                    let w = (probe.water)(x + dx * s, z + dz * s);
                    if w.dam || w.stair != c.stair {
                        found = true;
                        break 'rays;
                    }
                    s += 4;
                }
            }
            if !found {
                return None;
            }
        }
        Extra::RiverCrossing => {
            if !(c.is_wet() && c.kind == WaterKind::River) {
                return None;
            }
            let bank = |dx: i32, dz: i32| {
                (1..=14).any(|s| !(probe.water)(x + dx * s, z + dz * s).is_wet())
            };
            if !((bank(1, 0) && bank(-1, 0)) || (bank(0, 1) && bank(0, -1))) {
                return None;
            }
        }
        Extra::Mountain(u) => {
            if (probe.macros)(x, z).u < u {
                return None;
            }
        }
    }
    Some(c.top)
}

/// Une colonne d'eau de la nature demandée à moins de `d` blocs (huit rayons, pas de 2).
fn near_water(x: i32, z: i32, d: i32, kind: Shore, probe: &SiteProbe) -> bool {
    DIRS8.iter().any(|&(dx, dz)| {
        (1..=d / 2).any(|s| {
            let w = (probe.water)(x + dx * 2 * s, z + dz * 2 * s);
            w.is_wet() && shore_matches(kind, w.kind)
        })
    })
}

// ---------------------------------------------------------------------------------------------
// Blocs
// ---------------------------------------------------------------------------------------------

/// Blocs des structures et classes de blocs que le dégagement doit reconnaître.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructBlocks {
    /// Pierres des cairns : `rosesten` et `klappersten`.
    pub cairn: [BlockId; 2],
    /// Planches du caillebotis et des ponts.
    pub boards: BlockId,
    /// Sentier tassé.
    pub path: BlockId,
    /// Gravier des gués.
    pub ford: BlockId,
    pub water: BlockId,
    /// Blocs des bâtiments, indexés par [`Mat`].
    mats: [BlockId; Mat::COUNT],
    /// Plantes et feuilles qu'un chemin ou un cairn écarte.
    clearable: [bool; 256],
}

impl StructBlocks {
    pub fn from_registry(r: &BlockRegistry, trees: &TreeBlocks) -> Self {
        let id = |n: &str| {
            let b = r.get(n);
            assert!(b.name == n, "structures : bloc « {n} » absent de Norrcraft");
            b.id
        };
        let mut clearable = [false; 256];
        for b in r.iter() {
            if b.cross_shaped || trees.is_leaf(b.id) {
                clearable[usize::from(b.id)] = true;
            }
        }
        let mut mats = [AIR; Mat::COUNT];
        for m in Mat::ALL {
            if m != Mat::Air {
                mats[m as usize] = id(m.block());
            }
        }
        Self {
            cairn: [id("rosesten"), id("klappersten")],
            boards: id("planka"),
            path: id("stig"),
            ford: id("grus"),
            water: id("water"),
            mats,
            clearable,
        }
    }

    /// Bloc d'un matériau de bâtiment.
    #[inline]
    pub fn mat(&self, m: Mat) -> BlockId {
        self.mats[m as usize]
    }

    /// Bloc qu'un chemin ou un cairn remplace sans façon : air, plante, feuille.
    #[inline]
    fn soft(&self, id: BlockId) -> bool {
        id == AIR || self.clearable[usize::from(id)]
    }

    /// Bloc de pierre d'un cairn en `(x, y, z)`.
    fn stone(&self, seed: u64, x: i32, y: i32, z: i32) -> BlockId {
        let h = hash01(seed, x, y, z);
        self.cairn[usize::from(h >= 0.72)]
    }
}

// ---------------------------------------------------------------------------------------------
// Pistes
// ---------------------------------------------------------------------------------------------

/// Revêtement d'une cellule de piste à sec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Montagne : le sol n'est pas touché, seuls les cairns marquent la piste.
    Bare,
    /// Sentier tassé.
    Trodden,
    /// Caillebotis de planches (tourbière).
    Boards,
}

/// Ce qu'une cellule de piste devient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ground(Surface),
    /// Gué : gravier au fond d'une eau d'un bloc.
    Ford,
    /// Passerelle de planches à fleur d'eau.
    Bridge,
}

/// Une cellule d'une piste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathCell {
    pub x: i32,
    pub z: i32,
    /// Premier `y` non solide naturel de la colonne et premier `y` d'air au-dessus de l'eau.
    pub top: i32,
    pub level: i32,
    /// Premier `y` libre au-dessus de la piste une fois nivelée.
    pub y: i32,
    pub mode: Mode,
}

/// Une piste entre deux sites.
#[derive(Debug, Clone)]
pub struct Route {
    pub a: StructSite,
    pub b: StructSite,
    pub cells: Vec<PathCell>,
    /// Cairns de bord de piste : `(x, z, y)` du centre.
    pub cairns: Vec<(i32, i32, i32)>,
    /// Boîte englobante des cellules, des cairns et du dégagement : `x0, z0, x1, z1` inclus.
    pub bbox: [i32; 4],
    /// Colonnes interdites aux arbres.
    reserved: HashSet<(i32, i32)>,
}

impl Route {
    /// Nombre de cellules mouillées.
    pub fn wet_cells(&self) -> usize {
        self.cells
            .iter()
            .filter(|c| matches!(c.mode, Mode::Ford | Mode::Bridge))
            .count()
    }

    pub fn reserves(&self, x: i32, z: i32) -> bool {
        let [x0, z0, x1, z1] = self.bbox;
        x >= x0 && x <= x1 && z >= z0 && z <= z1 && self.reserved.contains(&(x, z))
    }

    /// Cellules de piste dont la boîte touche celle du chunk.
    fn touches(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        let [bx0, bz0, bx1, bz1] = self.bbox;
        bx0 <= x1 && bx1 >= x0 && bz0 <= z1 && bz1 >= z0
    }
}

const MOUNTAIN: [NorrBiome; 3] = [B::Fjallhed, B::Kalfjall, B::Glaciar];
const MIRE: [NorrBiome; 2] = [B::Myr, B::Palsmyr];

/// Cellules 4-connexes du segment `(a, b)`, `a` exclu.
fn line4(a: (i32, i32), b: (i32, i32), out: &mut Vec<(i32, i32)>) {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let (nx, nz) = (dx.abs(), dz.abs());
    let (sx, sz) = (dx.signum(), dz.signum());
    let (mut x, mut z) = a;
    let (mut ix, mut iz) = (0, 0);
    while ix < nx || iz < nz {
        if (1 + 2 * ix) * nz < (1 + 2 * iz) * nx {
            x += sx;
            ix += 1;
        } else {
            z += sz;
            iz += 1;
        }
        out.push((x, z));
    }
}

// ---------------------------------------------------------------------------------------------
// Plan d'un chunk
// ---------------------------------------------------------------------------------------------

/// Tout ce qui touche un chunk (plus une marge de 16 blocs).
#[derive(Debug, Default)]
struct Plan {
    routes: Vec<Arc<Route>>,
    sites: Vec<StructSite>,
    buildings: Vec<Arc<Building>>,
}

impl Plan {
    fn reserves(&self, x: i32, z: i32) -> bool {
        self.routes.iter().any(|r| r.reserves(x, z))
            || self.buildings.iter().any(|b| b.reserves(x, z))
            || self.sites.iter().any(|s| {
                s.has_cairn() && (s.x - x).abs() <= MARK_RADIUS && (s.z - z).abs() <= MARK_RADIUS
            })
    }
}

/// Marge du plan autour du chunk, en blocs.
const PLAN_MARGIN: i32 = 16;

type RouteKey = (i32, i32, i32, i32);
type BuildingCache = Arc<Mutex<HashMap<(i32, i32), Option<Arc<Building>>>>>;
type PlanCache = Arc<Mutex<HashMap<(i32, i32), Arc<Plan>>>>;

/// Placement des structures : fonction pure de (graine, colonne), cellules et pistes mémoïsées.
#[derive(Debug, Clone)]
pub struct Structures {
    seed: u64,
    sea: i32,
    blocks: StructBlocks,
    draws: Arc<Mutex<HashMap<Key, Option<Drawn>>>>,
    sites: Arc<Mutex<HashMap<Key, Option<StructSite>>>>,
    routes: Arc<Mutex<HashMap<RouteKey, Option<Arc<Route>>>>>,
    plans: PlanCache,
    buildings: BuildingCache,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn remember<K: std::hash::Hash + Eq, V>(m: &Mutex<HashMap<K, V>>, k: K, v: V) {
    let mut g = lock(m);
    if g.len() >= CACHE_MAX {
        g.clear();
    }
    g.insert(k, v);
}

impl Structures {
    pub fn new(seed: i64, sea_level: i32, blocks: StructBlocks) -> Self {
        Self {
            seed: (seed as u64) ^ SEED,
            sea: sea_level,
            blocks,
            draws: Arc::default(),
            sites: Arc::default(),
            routes: Arc::default(),
            plans: Arc::default(),
            buildings: Arc::default(),
        }
    }

    /// Bâtiment d'un site (mémoïsé) : `None` pour un type sans bâtiment ou un terrain impropre.
    pub fn building(&self, s: &StructSite, probe: &SiteProbe) -> Option<Arc<Building>> {
        if !s.rule().built || s.kind == K::Rosen {
            return None;
        }
        let key = (s.x, s.z);
        if let Some(b) = lock(&self.buildings).get(&key) {
            return b.clone();
        }
        let out = buildings::build(s, probe).map(Arc::new);
        remember(&self.buildings, key, out.clone());
        out
    }

    pub fn blocks(&self) -> &StructBlocks {
        &self.blocks
    }

    // --- Grilles --------------------------------------------------------------------------

    /// Position tirée de la cellule, avant tout biome.
    fn position(&self, class: Class, i: i32, j: i32) -> (i32, i32) {
        let (pitch, margin) = (class.pitch(), class.margin());
        let span = f64::from(pitch - 2 * margin);
        let k = 16 * class.tag();
        (
            pitch * i + margin + (hash01(self.seed, i, j, k) * span) as i32,
            pitch * j + margin + (hash01(self.seed, i, j, k + 1) * span) as i32,
        )
    }

    /// Position de repli numéro `t` (≥ 1) dans la cellule.
    fn position_try(&self, class: Class, i: i32, j: i32, t: i32) -> (i32, i32) {
        let (pitch, margin) = (class.pitch(), class.margin());
        let span = f64::from(pitch - 2 * margin);
        (
            pitch * i + margin + (hash01(self.seed, i, j, 100 + 2 * t) * span) as i32,
            pitch * j + margin + (hash01(self.seed, i, j, 101 + 2 * t) * span) as i32,
        )
    }

    /// Tirage de la cellule : position et type autorisé par le biome du centre.
    fn draw(&self, class: Class, i: i32, j: i32, probe: &SiteProbe) -> Option<Drawn> {
        let key = (class, i, j);
        if let Some(d) = lock(&self.draws).get(&key) {
            return *d;
        }
        let (x, z) = self.position(class, i, j);
        let biome = (probe.biome)(x, z);
        let u = hash01(self.seed, i, j, 16 * class.tag() + 2);
        let mut acc = 0.0;
        let mut found = None;
        'rules: for r in RULES.iter().filter(|r| r.class == class) {
            for &(b, w) in r.biomes {
                if b == biome {
                    acc += w;
                    if u < acc {
                        found = Some(Drawn { kind: r.kind, x, z });
                        break 'rules;
                    }
                }
            }
        }
        remember(&self.draws, key, found);
        found
    }

    /// Type et position tirés pour la cellule, avant le prédicat de site (cartes et tests : le
    /// rapport tirages/sites mesure la sévérité des prédicats).
    pub fn drawn_kind(
        &self,
        class: Class,
        i: i32,
        j: i32,
        probe: &SiteProbe,
    ) -> Option<(StructKind, i32, i32)> {
        self.draw(class, i, j, probe).map(|d| (d.kind, d.x, d.z))
    }

    /// Site résolu de la cellule : tirage, prédicat, puis séparation d'avec les classes plus
    /// grandes.
    pub fn site(&self, class: Class, i: i32, j: i32, probe: &SiteProbe) -> Option<StructSite> {
        let key = (class, i, j);
        if let Some(s) = lock(&self.sites).get(&key) {
            return *s;
        }
        let out = self.resolve(class, i, j, probe);
        remember(&self.sites, key, out);
        out
    }

    fn resolve(&self, class: Class, i: i32, j: i32, probe: &SiteProbe) -> Option<StructSite> {
        let d = self.draw(class, i, j, probe)?;
        let rule = d.kind.rule();
        // Les types qui exigent un lieu précis (rive, rivière, sommet…) ont droit à quelques
        // positions de repli, toujours dans la cellule, tirées de la graine : le calcul reste pur
        // et borné.
        let tries = if rule.snaps() { 1 + SNAP_TRIES } else { 1 };
        let mut found = None;
        for t in 0..tries {
            let (x, z) = if t == 0 {
                (d.x, d.z)
            } else {
                let p = self.position_try(class, i, j, t);
                if !rule
                    .biomes
                    .iter()
                    .any(|(b, _)| *b == (probe.biome)(p.0, p.1))
                {
                    continue;
                }
                p
            };
            if let Some(y) = site_ok(rule, x, z, probe, self.sea) {
                found = Some((x, z, y));
                break;
            }
        }
        let (px, pz, y) = found?;
        for higher in Class::ALL.iter().filter(|c| c.tag() < class.tag()) {
            let reach = Self::max_radius(*higher) + rule.radius + GAP;
            let pitch = higher.pitch();
            for hj in (pz - reach - pitch).div_euclid(pitch)..=(pz + reach).div_euclid(pitch) {
                for hi in (px - reach - pitch).div_euclid(pitch)..=(px + reach).div_euclid(pitch) {
                    if let Some(o) = self.site(*higher, hi, hj, probe) {
                        let sum = o.rule().radius + rule.radius + GAP;
                        let (dx, dz) = (i64::from(o.x - px), i64::from(o.z - pz));
                        if dx * dx + dz * dz < i64::from(sum) * i64::from(sum) {
                            return None;
                        }
                    }
                }
            }
        }
        let h = hash64(self.seed, i, j, 16 * class.tag() + 3);
        Some(StructSite {
            kind: d.kind,
            x: px,
            z: pz,
            y,
            rot: (h & 3) as u8,
            seed: h >> 2,
        })
    }

    fn max_radius(class: Class) -> i32 {
        RULES
            .iter()
            .filter(|r| r.class == class)
            .map(|r| r.radius)
            .max()
            .unwrap_or(0)
    }

    /// Cellules d'une classe dont la position peut tomber dans la boîte.
    fn cells(class: Class, x0: i32, z0: i32, x1: i32, z1: i32) -> impl Iterator<Item = (i32, i32)> {
        let p = class.pitch();
        let (i0, i1) = ((x0 - p).div_euclid(p), x1.div_euclid(p));
        let (j0, j1) = ((z0 - p).div_euclid(p), z1.div_euclid(p));
        (j0..=j1).flat_map(move |j| (i0..=i1).map(move |i| (i, j)))
    }

    /// Sites résolus dont le centre est dans la boîte (bornes incluses), triés par (z, x).
    pub fn sites_in(
        &self,
        probe: &SiteProbe,
        x0: i32,
        z0: i32,
        x1: i32,
        z1: i32,
    ) -> Vec<StructSite> {
        let mut out = Vec::new();
        for class in Class::ALL {
            for (i, j) in Self::cells(class, x0, z0, x1, z1) {
                if let Some(s) = self.site(class, i, j, probe) {
                    if s.x >= x0 && s.x <= x1 && s.z >= z0 && s.z <= z1 {
                        out.push(s);
                    }
                }
            }
        }
        out.sort_by_key(|s| (s.z, s.x, s.kind));
        out
    }

    /// Sites de piste (classes majeure et moyenne) de la boîte : seul le tirage décide d'abord,
    /// le prédicat n'est évalué que pour les types de piste.
    fn track_sites(
        &self,
        probe: &SiteProbe,
        x0: i32,
        z0: i32,
        x1: i32,
        z1: i32,
    ) -> Vec<StructSite> {
        let mut out = Vec::new();
        for class in [Class::Major, Class::Medium] {
            for (i, j) in Self::cells(class, x0, z0, x1, z1) {
                let Some(d) = self.draw(class, i, j, probe) else {
                    continue;
                };
                if !d.kind.rule().track {
                    continue;
                }
                if let Some(s) = self.site(class, i, j, probe) {
                    if s.x >= x0 && s.x <= x1 && s.z >= z0 && s.z <= z1 {
                        out.push(s);
                    }
                }
            }
        }
        out.sort_by_key(|s| (s.x, s.z, s.kind));
        out
    }

    // --- Pistes ---------------------------------------------------------------------------

    /// Pistes dont la boîte touche la boîte `[x0, x1] × [z0, z1]`, dans l'ordre global.
    pub fn routes_in(
        &self,
        probe: &SiteProbe,
        x0: i32,
        z0: i32,
        x1: i32,
        z1: i32,
    ) -> Vec<Arc<Route>> {
        let r = TRACK_RANGE + TRACK_PULL + 4;
        let sites = self.track_sites(probe, x0 - r, z0 - r, x1 + r, z1 + r);
        let slack = TRACK_PULL + PATH_CLEAR + 4;
        let range2 = i64::from(TRACK_RANGE) * i64::from(TRACK_RANGE);
        let mut out = Vec::new();
        for (ia, a) in sites.iter().enumerate() {
            for b in &sites[ia + 1..] {
                let (dx, dz) = (i64::from(b.x - a.x), i64::from(b.z - a.z));
                if dx * dx + dz * dz >= range2 {
                    continue;
                }
                if a.x.min(b.x) - slack > x1
                    || a.x.max(b.x) + slack < x0
                    || a.z.min(b.z) - slack > z1
                    || a.z.max(b.z) + slack < z0
                {
                    continue;
                }
                // Graphe de Gabriel : aucun site C avec un angle ACB obtus.
                let blocked = sites.iter().any(|c| {
                    c != a && c != b && {
                        let dot = i64::from(c.x - a.x) * i64::from(c.x - b.x)
                            + i64::from(c.z - a.z) * i64::from(c.z - b.z);
                        dot < 0
                    }
                });
                if blocked {
                    continue;
                }
                if let Some(route) = self.route(a, b, probe) {
                    if route.touches(x0, z0, x1, z1) {
                        out.push(route);
                    }
                }
            }
        }
        out
    }

    /// Piste entre deux sites (mémoïsée), `None` si le trajet est impraticable.
    pub fn route(&self, a: &StructSite, b: &StructSite, probe: &SiteProbe) -> Option<Arc<Route>> {
        let key = (a.x, a.z, b.x, b.z);
        if let Some(r) = lock(&self.routes).get(&key) {
            return r.clone();
        }
        let out = self.build_route(a, b, probe).map(Arc::new);
        remember(&self.routes, key, out.clone());
        out
    }

    fn build_route(&self, a: &StructSite, b: &StructSite, probe: &SiteProbe) -> Option<Route> {
        let water = |x: i32, z: i32| (probe.water)(x, z);
        let (ax, az, bx, bz) = (
            f64::from(a.x),
            f64::from(a.z),
            f64::from(b.x),
            f64::from(b.z),
        );
        let (dx, dz) = (bx - ax, bz - az);
        let len = (dx * dx + dz * dz).sqrt();
        if len < 1.0 {
            return None;
        }
        let n = ((len / TRACK_STEP).ceil() as usize).clamp(2, 50);
        let (px, pz) = (-dz / len, dx / len);

        // Glissement perpendiculaire de chaque point intérieur vers le coût minimal.
        let cost = |x: i32, z: i32, off: i32| -> f64 {
            let c = water(x, z);
            let hx = water(x + 1, z).top;
            let hz = water(x, z + 1).top;
            let slope = f64::from((c.top - hx) * (c.top - hx) + (c.top - hz) * (c.top - hz));
            let wet = if c.is_wet() {
                30.0 + 6.0 * f64::from(c.level - c.top)
            } else {
                0.0
            };
            slope + wet + 0.05 * f64::from(off * off)
        };
        let mut offsets = vec![0.0f64; n + 1];
        let steps = [-12, -8, -4, 0, 4, 8, 12];
        debug_assert!(steps.iter().all(|s: &i32| s.abs() <= TRACK_PULL));
        for (k, slot) in offsets.iter_mut().enumerate().take(n).skip(1) {
            let t = k as f64 / n as f64;
            let (cx, cz) = (ax + dx * t, az + dz * t);
            let mut best = (f64::INFINITY, 0);
            for off in steps {
                let (x, z) = (
                    (cx + px * f64::from(off)).round() as i32,
                    (cz + pz * f64::from(off)).round() as i32,
                );
                let c = cost(x, z, off);
                if c < best.0 {
                    best = (c, off);
                }
            }
            *slot = f64::from(best.1);
        }
        // Lissage (1, 2, 1) / 4, extrémités fixes.
        let raw = offsets.clone();
        for (k, o) in offsets.iter_mut().enumerate().take(n).skip(1) {
            *o = (raw[k - 1] + 2.0 * raw[k] + raw[k + 1]) / 4.0;
        }
        let mut pts = Vec::with_capacity(n + 1);
        pts.push((a.x, a.z));
        for (k, o) in offsets.iter().enumerate().take(n).skip(1) {
            let t = k as f64 / n as f64;
            pts.push((
                (ax + dx * t + px * o).round() as i32,
                (az + dz * t + pz * o).round() as i32,
            ));
        }
        pts.push((b.x, b.z));
        let mut line = vec![(a.x, a.z)];
        for w in pts.windows(2) {
            line4(w[0], w[1], &mut line);
        }

        // Nature de chaque cellule.
        let mut cells: Vec<PathCell> = Vec::with_capacity(line.len());
        let mut surface = Surface::Trodden;
        for (k, &(x, z)) in line.iter().enumerate() {
            if k % 4 == 0 {
                let biome = (probe.biome)(x, z);
                surface = if MOUNTAIN.contains(&biome) {
                    Surface::Bare
                } else if MIRE.contains(&biome) {
                    Surface::Boards
                } else {
                    Surface::Trodden
                };
            }
            let c = water(x, z);
            cells.push(PathCell {
                x,
                z,
                top: c.top,
                level: c.level,
                y: c.top,
                mode: if c.is_wet() {
                    Mode::Bridge
                } else {
                    Mode::Ground(surface)
                },
            });
        }

        // Traversées d'eau : gué si l'eau est basse et courte, sinon passerelle ; refus des
        // traversées trop longues.
        let mut k = 0;
        let mut wet_total = 0;
        while k < cells.len() {
            if cells[k].mode != Mode::Bridge {
                k += 1;
                continue;
            }
            let start = k;
            while k < cells.len() && cells[k].mode == Mode::Bridge {
                k += 1;
            }
            let run = k - start;
            wet_total += run;
            if run > MAX_WET_RUN {
                return None;
            }
            let shallow = cells[start..k].iter().all(|c| c.level - c.top <= 1);
            if shallow && run <= 3 {
                for c in &mut cells[start..k] {
                    c.mode = Mode::Ford;
                }
            }
        }
        if wet_total * 10 > cells.len() * 4 {
            return None;
        }

        // Nivellement : le sol à sec ne varie jamais de plus d'un bloc d'une cellule à l'autre ;
        // la passerelle reste à fleur d'eau.
        let mut prev: Option<i32> = None;
        for c in &mut cells {
            c.y = match c.mode {
                Mode::Bridge => c.level,
                Mode::Ground(Surface::Bare) => c.top,
                _ => match prev {
                    Some(p) => c.top.clamp(p - 1, p + 1),
                    None => c.top,
                },
            };
            prev = Some(c.y);
            if (c.y - c.top).abs() > 4 && c.mode != Mode::Bridge {
                return None;
            }
        }
        // Le dernier sol avant l'eau doit pouvoir rejoindre la passerelle.
        for w in cells.windows(2) {
            let (p, q) = (&w[0], &w[1]);
            let bridge_edge = (p.mode == Mode::Bridge) != (q.mode == Mode::Bridge);
            if bridge_edge && (p.y - q.y).abs() > 3 {
                return None;
            }
        }

        // Cairns de montagne, tous les 24 blocs, à deux blocs du sentier.
        let mut cairns = Vec::new();
        for k in (CAIRN_EVERY / 2..cells.len().saturating_sub(CAIRN_EVERY / 2)).step_by(CAIRN_EVERY)
        {
            let c = cells[k];
            if c.mode != Mode::Ground(Surface::Bare) {
                continue;
            }
            let (p, q) = (
                cells[k.saturating_sub(2)],
                cells[(k + 2).min(cells.len() - 1)],
            );
            let (tx, tz) = (f64::from(q.x - p.x), f64::from(q.z - p.z));
            let tl = (tx * tx + tz * tz).sqrt().max(1.0);
            let side = if cairns.len() % 2 == 0 { 1.0 } else { -1.0 };
            let (cx, cz) = (
                c.x + (-tz / tl * 2.0 * side).round() as i32,
                c.z + (tx / tl * 2.0 * side).round() as i32,
            );
            let w = water(cx, cz);
            if !w.is_wet() && (w.top - c.top).abs() <= 2 {
                cairns.push((cx, cz, w.top));
            }
        }

        // Emprise réservée aux arbres et boîte englobante.
        let mut reserved = HashSet::new();
        let (mut x0, mut z0, mut x1, mut z1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for c in &cells {
            for dz in -PATH_CLEAR..=PATH_CLEAR {
                for dx in -PATH_CLEAR..=PATH_CLEAR {
                    reserved.insert((c.x + dx, c.z + dz));
                }
            }
            x0 = x0.min(c.x - PATH_CLEAR);
            z0 = z0.min(c.z - PATH_CLEAR);
            x1 = x1.max(c.x + PATH_CLEAR);
            z1 = z1.max(c.z + PATH_CLEAR);
        }
        for &(cx, cz, _) in &cairns {
            for dz in -MARK_RADIUS..=MARK_RADIUS {
                for dx in -MARK_RADIUS..=MARK_RADIUS {
                    reserved.insert((cx + dx, cz + dz));
                }
            }
            x0 = x0.min(cx - MARK_RADIUS);
            z0 = z0.min(cz - MARK_RADIUS);
            x1 = x1.max(cx + MARK_RADIUS);
            z1 = z1.max(cz + MARK_RADIUS);
        }
        Some(Route {
            a: *a,
            b: *b,
            cells,
            cairns,
            bbox: [x0, z0, x1, z1],
            reserved,
        })
    }

    // --- Plan d'un chunk ------------------------------------------------------------------

    fn plan(&self, probe: &SiteProbe, cx: i32, cz: i32) -> Arc<Plan> {
        if let Some(p) = lock(&self.plans).get(&(cx, cz)) {
            return Arc::clone(p);
        }
        let (x0, z0) = (
            cx * CHUNK_SIZE_X as i32 - PLAN_MARGIN,
            cz * CHUNK_SIZE_Z as i32 - PLAN_MARGIN,
        );
        let (x1, z1) = (
            x0 + CHUNK_SIZE_X as i32 - 1 + 2 * PLAN_MARGIN,
            z0 + CHUNK_SIZE_Z as i32 - 1 + 2 * PLAN_MARGIN,
        );
        let plan = Arc::new(Plan {
            routes: self.routes_in(probe, x0, z0, x1, z1),
            sites: self
                .sites_in(probe, x0, z0, x1, z1)
                .into_iter()
                .filter(StructSite::has_cairn)
                .collect(),
            buildings: self
                .sites_in(
                    probe,
                    x0 - buildings::REACH,
                    z0 - buildings::REACH,
                    x1 + buildings::REACH,
                    z1 + buildings::REACH,
                )
                .iter()
                .filter_map(|s| self.building(s, probe))
                .filter(|b| b.touches(x0 - 2, z0 - 2, x1 + 2, z1 + 2))
                .collect(),
        });
        remember(&self.plans, (cx, cz), Arc::clone(&plan));
        plan
    }

    /// La colonne est-elle réservée (piste, cairn, jalon) ? Les arbres s'en écartent.
    pub fn reserved(&self, probe: &SiteProbe, x: i32, z: i32) -> bool {
        let (cx, cz) = (
            x.div_euclid(CHUNK_SIZE_X as i32),
            z.div_euclid(CHUNK_SIZE_Z as i32),
        );
        self.plan(probe, cx, cz).reserves(x, z)
    }

    // --- Écriture -------------------------------------------------------------------------

    /// Écrit dans `chunk` les pistes, cairns et jalons qui le touchent.
    pub fn pass(&self, chunk: &mut Chunk, probe: &SiteProbe) {
        let pos = chunk.pos();
        let plan = self.plan(probe, pos.x, pos.z);
        let (x0, z0) = (pos.x * CHUNK_SIZE_X as i32, pos.z * CHUNK_SIZE_Z as i32);
        for route in &plan.routes {
            self.stamp_route(chunk, route, x0, z0);
        }
        for s in &plan.sites {
            self.stamp_cairn(chunk, s.x, s.y, s.z, s.cairn_profile(), s.seed, x0, z0);
        }
        for bld in &plan.buildings {
            self.stamp_building(chunk, bld, x0, z0);
        }
    }

    /// Écrit les pièces d'un bâtiment qui touchent le chunk : déblai de l'emprise, fondation de
    /// pierre sous le plancher (sentier tassé en surface), blocs, puis pieux.
    fn stamp_building(&self, chunk: &mut Chunk, bld: &Building, x0: i32, z0: i32) {
        let b = &self.blocks;
        let (cw, cd, ch) = (
            CHUNK_SIZE_X as i32,
            CHUNK_SIZE_Z as i32,
            CHUNK_SIZE_Y as i32,
        );
        let inside = |x: i32, z: i32| x >= x0 && x < x0 + cw && z >= z0 && z < z0 + cd;
        for part in &bld.parts {
            let [bx0, bz0, bx1, bz1] = part.bbox;
            if bx0 > x0 + cw - 1 || bx1 < x0 || bz0 > z0 + cd - 1 || bz1 < z0 {
                continue;
            }
            let [px0, pz0, px1, pz1] = part.pad;
            if px0 <= px1 && pz0 <= pz1 {
                for z in pz0.max(z0)..=pz1.min(z0 + cd - 1) {
                    for x in px0.max(x0)..=px1.min(x0 + cw - 1) {
                        let (lx, lz) = (x - x0, z - z0);
                        for y in part.floor.max(0)..(part.floor + part.height).min(ch) {
                            if chunk.block(lx, y, lz) != AIR {
                                chunk.set_block(lx, y, lz, AIR);
                            }
                        }
                        let mut y = part.floor - 1;
                        while y >= 1 && y > part.floor - 9 && y < ch {
                            let cur = chunk.block(lx, y, lz);
                            if !b.soft(cur) && cur != b.water {
                                break;
                            }
                            let id = if y == part.floor - 1 {
                                b.path
                            } else {
                                b.mat(Mat::Stone)
                            };
                            chunk.set_block(lx, y, lz, id);
                            y -= 1;
                        }
                    }
                }
            }
            for &(x, y, z, m) in &part.voxels {
                if inside(x, z) && (0..ch).contains(&y) {
                    chunk.set_block(x - x0, y, z - z0, b.mat(m));
                }
            }
            for &(x, z, top) in &part.piles {
                if !inside(x, z) {
                    continue;
                }
                let mut y = top.min(ch - 1);
                while y >= 1 {
                    let cur = chunk.block(x - x0, y, z - z0);
                    if cur != b.water && !b.soft(cur) {
                        break;
                    }
                    chunk.set_block(x - x0, y, z - z0, b.mat(Mat::Log));
                    y -= 1;
                }
            }
        }
    }

    fn stamp_route(&self, chunk: &mut Chunk, route: &Route, x0: i32, z0: i32) {
        let b = &self.blocks;
        let skip = MARK_RADIUS + 1;
        for c in &route.cells {
            let (lx, lz) = (c.x - x0, c.z - z0);
            if !(0..CHUNK_SIZE_X as i32).contains(&lx) || !(0..CHUNK_SIZE_Z as i32).contains(&lz) {
                continue;
            }
            // Pas de piste dans le jalon des extrémités.
            if [route.a, route.b]
                .iter()
                .any(|s| (s.x - c.x).abs() <= skip && (s.z - c.z).abs() <= skip)
            {
                continue;
            }
            let at = |chunk: &Chunk, y: i32| {
                if (0..CHUNK_SIZE_Y as i32).contains(&y) {
                    chunk.block(lx, y, lz)
                } else {
                    AIR
                }
            };
            match c.mode {
                Mode::Ground(Surface::Bare) => {}
                Mode::Ground(s) => {
                    // La colonne doit être naturelle : sol plein sous un dessus libre.
                    let below = at(chunk, c.top - 1);
                    if !b.soft(at(chunk, c.top)) || b.soft(below) || below == b.water {
                        continue;
                    }
                    let block = if s == Surface::Boards {
                        b.boards
                    } else {
                        b.path
                    };
                    // Déblai au-dessus du niveau de la piste, puis revêtement (et remblai si
                    // la piste est plus haute que le sol).
                    for y in c.y..c.top {
                        chunk.set_block(lx, y, lz, AIR);
                    }
                    for y in (c.y - 1).min(c.top - 1)..c.y {
                        chunk.set_block(lx, y, lz, block);
                    }
                    clear_above(chunk, b, lx, lz, c.y);
                }
                Mode::Ford => {
                    let below = at(chunk, c.top - 1);
                    if !b.soft(below) && below != b.water {
                        chunk.set_block(lx, c.top - 1, lz, b.ford);
                    }
                }
                Mode::Bridge => {
                    let y = c.level - 1;
                    let cur = at(chunk, y);
                    if cur == b.water || b.soft(cur) {
                        chunk.set_block(lx, y, lz, b.boards);
                        clear_above(chunk, b, lx, lz, c.level);
                    }
                }
            }
        }
        for &(cx, cz, cy) in &route.cairns {
            let seed = hash64(CAIRN_SEED, cx, cz, 0);
            self.stamp_cairn(chunk, cx, cy, cz, &[1, 0, 0], seed, x0, z0);
        }
    }

    /// Pose un cairn de centre `(cx, cz)`, sol du centre `cy` : chaque colonne repose sur son
    /// propre sol (jusqu'à 3 blocs plus bas), sommet aligné sur le centre.
    #[allow(clippy::too_many_arguments)]
    fn stamp_cairn(
        &self,
        chunk: &mut Chunk,
        cx: i32,
        cy: i32,
        cz: i32,
        profile: &[i32],
        seed: u64,
        x0: i32,
        z0: i32,
    ) {
        let b = &self.blocks;
        let reach = profile.iter().copied().max().unwrap_or(0);
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let (x, z) = (cx + dx, cz + dz);
                let (lx, lz) = (x - x0, z - z0);
                if !(0..CHUNK_SIZE_X as i32).contains(&lx)
                    || !(0..CHUNK_SIZE_Z as i32).contains(&lz)
                {
                    continue;
                }
                let d2 = dx * dx + dz * dz;
                let mut h = 0;
                for &r in profile {
                    if d2 <= r * r + i32::from(r > 0) {
                        h += 1;
                    } else {
                        break;
                    }
                }
                // Coins du premier tas parfois absents.
                if profile[0] == 1 && dx != 0 && dz != 0 && hash01(seed, dx, dz, 1) < 0.45 {
                    h = 0;
                }
                if h == 0 {
                    continue;
                }
                // Sol de la colonne : on descend sous les plantes jusqu'à un bloc plein.
                let mut y = cy + 1;
                while y > 1 && b.soft(chunk.block(lx, y - 1, lz)) {
                    y -= 1;
                }
                if y < cy - 3 || chunk.block(lx, y - 1, lz) == b.water {
                    continue;
                }
                for yy in y..(cy + h).min(CHUNK_SIZE_Y as i32) {
                    let cur = chunk.block(lx, yy, lz);
                    if b.soft(cur) {
                        chunk.set_block(lx, yy, lz, b.stone(seed, x, yy, z));
                    }
                }
            }
        }
    }
}

/// Dégage deux blocs de plantes ou de feuilles au-dessus de `y`.
fn clear_above(chunk: &mut Chunk, b: &StructBlocks, lx: i32, lz: i32, y: i32) {
    for yy in y..(y + 2).min(CHUNK_SIZE_Y as i32) {
        if b.clearable[usize::from(chunk.block(lx, yy, lz))] {
            chunk.set_block(lx, yy, lz, AIR);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_follow_the_kind_order() {
        for (i, r) in RULES.iter().enumerate() {
            assert_eq!(r.kind as usize, i, "{}", r.name);
        }
    }

    #[test]
    fn biome_weights_never_exceed_one_per_class() {
        for class in Class::ALL {
            for b in NorrBiome::ALL {
                let sum: f64 = RULES
                    .iter()
                    .filter(|r| r.class == class)
                    .flat_map(|r| r.biomes.iter())
                    .filter(|(bb, _)| *bb == b)
                    .map(|(_, w)| w)
                    .sum();
                assert!(sum <= 1.0, "{} {}: {sum}", class.name(), b.name());
            }
        }
    }

    #[test]
    fn radii_fit_in_the_cell_margin() {
        for class in Class::ALL {
            assert!(
                Structures::max_radius(class) <= class.margin(),
                "{}",
                class.name()
            );
        }
    }

    #[test]
    fn cairns_are_rule_driven() {
        assert!(RULES[K::Rosen as usize].built);
        // Rösen + les neuf types d'habitat de la session 4.2.
        assert!(RULES.iter().filter(|r| r.built).count() == 10);
        assert!(RULES.iter().filter(|r| r.track).all(|r| r.class != Minor));
    }

    #[test]
    fn line4_is_four_connected_and_reaches_the_end() {
        for (a, b) in [
            ((0, 0), (10, 3)),
            ((5, 5), (-7, 9)),
            ((0, 0), (0, -6)),
            ((2, 2), (2, 2)),
        ] {
            let mut out = vec![a];
            line4(a, b, &mut out);
            assert_eq!(*out.last().unwrap(), b);
            for w in out.windows(2) {
                let d = (w[0].0 - w[1].0).abs() + (w[0].1 - w[1].1).abs();
                assert_eq!(d, 1);
            }
            assert_eq!(out.len() as i32, (b.0 - a.0).abs() + (b.1 - a.1).abs() + 1);
        }
    }
}
