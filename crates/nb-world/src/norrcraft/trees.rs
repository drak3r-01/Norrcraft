//! Arbres Norrcraft (session 2.3 ; GDD doc 02 §9.1 et §9.2, doc 04 §9).
//!
//! * **Douze essences** ([`Species`]) et du bois mort ([`Kind::Snag`], [`Kind::Stump`],
//!   [`Kind::Log`]) : chaque arbre est une **fonction pure** de `(graine, cellule)` ; hauteur,
//!   silhouette et ragged des bords viennent de hachages du `seed` de la plante.
//! * **Placement** (§9.2) : grille jitterée de pas [`CELL`] ; une cellule porte une plante si
//!   `h(cellule) < densité_biome · bosquet(p) · facteurs_continus`, avec
//!   `bosquet(p) = 0,4 + 1,2 · F(p/90; 2)` (clairières naturelles). Rejets : eau, pente ≥ 3,
//!   glace, neige, plage, barrage, sol non fertile. L'essence est tirée dans le mélange pondéré du
//!   biome ; `Vr` favorise l'essence d'une tache (peuplements purs par taches de ~40 blocs).
//! * **Indépendance de l'ordre des chunks** : un chunk évalue toutes les cellules dont une plante
//!   peut le toucher (à moins de [`MAX_REACH`] blocs) dans l'ordre global (rangée par rangée), et
//!   n'écrit que dans son volume. Un bloc donné voit donc toujours la même suite d'arbres :
//!   les troncs remplacent l'air et les feuilles, les feuilles ne se posent que sur de l'air.
//!   Le résultat de chaque cellule est mémoïsé dans un cache partagé par toutes les copies du
//!   générateur (il ne change aucun résultat, seulement le coût).
//!
//! Coordonnées des formes : relatives à la plante, `(0, 0, 0)` = premier bloc libre au-dessus du
//! sol de sa colonne.

use std::collections::HashMap;
use std::f64::consts::FRAC_1_SQRT_2 as R;
use std::sync::{Arc, Mutex};

use nb_data::{BlockId, BlockRegistry};

use super::biomes::{NorrBiome, Selection};
use super::climate::altitudes;
use super::fields::MacroSample;
use super::height::Relief;
use super::hydro::WaterColumn;
use super::landforms::Coast;
use super::math::{hash01, mix64, smooth, GradField, Worley};
use crate::chunk::{Chunk, AIR, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};

/// Pas de la grille de placement, en blocs (§9.2 : 5 pour les forêts denses).
pub const CELL: i32 = 5;
/// Portée horizontale maximale d'une plante depuis son pied : tout bloc au-delà est ignoré.
pub const MAX_REACH: i32 = 9;
/// Constante de semence du placement des arbres ; `0x…06xx` = arbres.
const PLACE_SEED: u64 = 0x4E52_5743_0000_0601;
/// Constante de semence du bruit des clairières (`bosquet`).
const GROVE_SEED: u64 = 0x4E52_5743_0000_0602;
/// Constante de semence des taches de peuplements purs.
const STAND_SEED: u64 = 0x4E52_5743_0000_0603;
/// Longueur d'onde des clairières naturelles, en blocs (§9.2 : `F(p / 90 ; 2)`).
const GROVE_WAVELENGTH: f64 = 90.0;
/// Pas des taches de peuplements purs, en blocs.
const STAND_PITCH: f64 = 40.0;
/// Cellules mémoïsées au plus (cache vidé d'un coup au-delà).
const CACHE_MAX: usize = 1 << 16;

// ---------------------------------------------------------------------------------------------
// Essences et blocs
// ---------------------------------------------------------------------------------------------

/// Les douze essences du doc 02 §9.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Species {
    /// Épicéa commun.
    Gran,
    /// Pin sylvestre.
    Tall,
    /// Bouleau verruqueux.
    Bjork,
    /// Bouleau tortueux de montagne.
    Fjallbjork,
    /// Hêtre.
    Bok,
    /// Chêne.
    Ek,
    /// Sorbier.
    Ronn,
    /// Tremble.
    Asp,
    /// Aulne.
    Al,
    /// Saules (sälg, vide).
    Salg,
    /// Genévrier.
    En,
    /// Tilleul et noisetier.
    Lind,
}

impl Species {
    pub const ALL: [Species; 12] = [
        Self::Gran,
        Self::Tall,
        Self::Bjork,
        Self::Fjallbjork,
        Self::Bok,
        Self::Ek,
        Self::Ronn,
        Self::Asp,
        Self::Al,
        Self::Salg,
        Self::En,
        Self::Lind,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Gran => "gran",
            Self::Tall => "tall",
            Self::Bjork => "bjork",
            Self::Fjallbjork => "fjallbjork",
            Self::Bok => "bok",
            Self::Ek => "ek",
            Self::Ronn => "ronn",
            Self::Asp => "asp",
            Self::Al => "al",
            Self::Salg => "salg",
            Self::En => "en",
            Self::Lind => "lind",
        }
    }
}

/// Ce que la plante est : un arbre vivant ou du bois mort (§9.1, dernier paragraphe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Living(Species),
    /// Chicot : tronc sans houppier.
    Snag,
    /// Souche moussue.
    Stump,
    /// Tronc couché moussu.
    Log,
}

/// Variante de silhouette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    /// Épicéa géant ou chêne vénérable : tronc de 2 × 2.
    Giant,
    /// Pin tordu et bas (Skärgård, corniches).
    Twisted,
    /// Arbuste nain (tourbière, toundra, brûlis).
    Dwarf,
}

/// Une plante placée : tout ce qu'il faut pour la dessiner sans autre calcul.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plant {
    pub kind: Kind,
    pub style: Style,
    /// Colonne de pied.
    pub x: i32,
    pub z: i32,
    /// Premier bloc libre au-dessus du sol.
    pub y: i32,
    /// Hachage de forme.
    pub seed: u64,
    /// Sens du vent (signe de θ), pour les bouleaux de montagne.
    pub lean: [i8; 2],
}

/// Bois de la plante (un bloc de tronc par essence, §9 du doc 04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wood {
    Gran,
    Tall,
    /// Haut du tronc du pin, orangé.
    TallTop,
    Bjork,
    Bok,
    Ek,
    Asp,
    Al,
    Ronn,
    Dead,
    Mossy,
}

impl Wood {
    pub const COUNT: usize = 11;
    pub const NAMES: [&'static str; Self::COUNT] = [
        "gran_stam",
        "tall_stam",
        "tall_ovre",
        "bjork_stam",
        "bok_stam",
        "ek_stam",
        "asp_stam",
        "al_stam",
        "ronn_stam",
        "dod_stam",
        "mossig_stam",
    ];
}

/// Feuillage ou aiguilles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leaf {
    Gran,
    Tall,
    Bjork,
    Bok,
    Ek,
    Asp,
    Al,
    Ronn,
    Salg,
    En,
    Hassel,
}

impl Leaf {
    pub const COUNT: usize = 11;
    pub const NAMES: [&'static str; Self::COUNT] = [
        "gran_barr",
        "tall_barr",
        "bjork_lov",
        "bok_lov",
        "ek_lov",
        "asp_lov",
        "al_lov",
        "ronn_lov",
        "salg_lov",
        "en_barr",
        "hassel_lov",
    ];
}

/// Blocs des arbres, lus dans le registre du monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeBlocks {
    wood: [BlockId; Wood::COUNT],
    leaf: [BlockId; Leaf::COUNT],
    /// `solid` de chaque bloc du registre (appui des troncs couchés).
    solid: [bool; 256],
    /// Identifiants de feuillage (un tronc les remplace).
    is_leaf: [bool; 256],
}

impl TreeBlocks {
    pub fn from_registry(r: &BlockRegistry) -> Self {
        let id = |n: &str| {
            let b = r.get(n);
            assert!(b.name == n, "arbres : bloc « {n} » absent de Norrcraft");
            b.id
        };
        let wood = Wood::NAMES.map(id);
        let leaf = Leaf::NAMES.map(id);
        let mut solid = [false; 256];
        for b in r.iter() {
            solid[usize::from(b.id)] = b.solid;
        }
        let mut is_leaf = [false; 256];
        for l in leaf {
            is_leaf[usize::from(l)] = true;
        }
        Self {
            wood,
            leaf,
            solid,
            is_leaf,
        }
    }

    pub fn wood(&self, w: Wood) -> BlockId {
        self.wood[w as usize]
    }

    pub fn leaf(&self, l: Leaf) -> BlockId {
        self.leaf[l as usize]
    }

    /// `true` pour un bloc de tronc ou de feuillage d'arbre.
    pub fn is_tree_block(&self, id: BlockId) -> bool {
        self.is_leaf[usize::from(id)] || self.wood.contains(&id)
    }

    pub fn is_leaf(&self, id: BlockId) -> bool {
        self.is_leaf[usize::from(id)]
    }
}

// ---------------------------------------------------------------------------------------------
// Dessin : formes
// ---------------------------------------------------------------------------------------------

/// Destination des blocs d'une plante, en coordonnées relatives à son pied.
pub trait Put {
    /// Tronc ou branche : remplace l'air et les feuilles.
    fn wood(&mut self, dx: i32, dy: i32, dz: i32, w: Wood);
    /// Feuillage : seulement sur de l'air.
    fn leaf(&mut self, dx: i32, dy: i32, dz: i32, l: Leaf);
    /// Le bloc sous `(dx, dy, dz)` porte-t-il ? (troncs couchés)
    fn supported(&mut self, dx: i32, dy: i32, dz: i32) -> bool;
}

/// Générateur pseudo-aléatoire local d'une plante (SplitMix64).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(mix64(seed ^ 0xA076_1D64_78BD_642F))
    }

    fn next(&mut self) -> u64 {
        self.0 = mix64(self.0);
        self.0
    }

    /// Flottant uniforme dans [0, 1).
    fn f(&mut self) -> f64 {
        ((self.next() >> 11) as f64) * (1.0 / 9_007_199_254_740_992.0)
    }

    /// Entier uniforme dans `[a, b]`.
    fn int(&mut self, a: i32, b: i32) -> i32 {
        a + (self.f() * f64::from(b - a + 1)) as i32
    }

    fn chance(&mut self, p: f64) -> bool {
        self.f() < p
    }
}

/// Tirage fixe d'un bloc (bords irréguliers) : fonction du `seed` de la plante et de la position.
#[inline]
fn rag(seed: u64, x: i32, y: i32, z: i32) -> f64 {
    hash01(seed, x, y, z)
}

/// Ellipsoïde de feuilles de centre `c` et de demi-axes `r`. Au-delà de 70 % du rayon normalisé,
/// un bloc tombe avec la probabilité `ragged · (d − 0,7) / 0,3`.
fn ellipsoid<P: Put + ?Sized>(
    p: &mut P,
    seed: u64,
    c: (f64, f64, f64),
    r: (f64, f64, f64),
    leaf: Leaf,
    ragged: f64,
) {
    let (x0, x1) = ((c.0 - r.0).floor() as i32, (c.0 + r.0).ceil() as i32);
    let (y0, y1) = ((c.1 - r.1).floor() as i32, (c.1 + r.1).ceil() as i32);
    let (z0, z1) = ((c.2 - r.2).floor() as i32, (c.2 + r.2).ceil() as i32);
    for y in y0..=y1 {
        let ty = (f64::from(y) - c.1) / r.1;
        for z in z0..=z1 {
            let tz = (f64::from(z) - c.2) / r.2;
            for x in x0..=x1 {
                let tx = (f64::from(x) - c.0) / r.0;
                let d = tx * tx + ty * ty + tz * tz;
                if d > 1.0 {
                    continue;
                }
                if d > 0.7 && rag(seed, x, y, z) < ragged * (d - 0.7) / 0.3 {
                    continue;
                }
                p.leaf(x, y, z, leaf);
            }
        }
    }
}

/// Disque de feuilles à la hauteur `y`, de centre `(cx, cz)` et de rayon `r`.
fn disc<P: Put + ?Sized>(
    p: &mut P,
    seed: u64,
    (cx, cz): (f64, f64),
    y: i32,
    r: f64,
    leaf: Leaf,
    ragged: f64,
) {
    let (x0, x1) = ((cx - r).floor() as i32, (cx + r).ceil() as i32);
    let (z0, z1) = ((cz - r).floor() as i32, (cz + r).ceil() as i32);
    for z in z0..=z1 {
        for x in x0..=x1 {
            let (dx, dz) = (f64::from(x) - cx, f64::from(z) - cz);
            let d2 = dx * dx + dz * dz;
            if d2 > r * r {
                continue;
            }
            if r > 1.5 && d2 > (r - 1.0) * (r - 1.0) && rag(seed, x, y, z) < ragged {
                continue;
            }
            p.leaf(x, y, z, leaf);
        }
    }
}

/// Ligne de bois entre deux points (marche en pas entiers sur le plus grand axe).
fn line<P: Put + ?Sized>(p: &mut P, a: (f64, f64, f64), b: (f64, f64, f64), w: Wood) {
    let n = (b.0 - a.0)
        .abs()
        .max((b.1 - a.1).abs())
        .max((b.2 - a.2).abs())
        .ceil()
        .max(1.0) as i32;
    for i in 0..=n {
        let t = f64::from(i) / f64::from(n);
        p.wood(
            (a.0 + (b.0 - a.0) * t).round() as i32,
            (a.1 + (b.1 - a.1) * t).round() as i32,
            (a.2 + (b.2 - a.2) * t).round() as i32,
            w,
        );
    }
}

/// Pied d'un tronc excentré : comble jusqu'à deux blocs d'air sous la plante.
fn foot<P: Put + ?Sized>(p: &mut P, x: i32, z: i32, w: Wood) {
    p.wood(x, -2, z, w);
    p.wood(x, -1, z, w);
}

/// `x^0,75` sans fonction transcendante (`sqrt(x) · sqrt(sqrt(x))`) : exposant approchant 0,8.
fn pow075(x: f64) -> f64 {
    let s = x.max(0.0).sqrt();
    s * s.sqrt()
}

/// Seize directions (cos, sin) de pas 22,5°, en constantes : pas de trigonométrie à l'exécution.
const DIRS: [(f64, f64); 16] = [
    (1.0, 0.0),
    (0.9239, 0.3827),
    (R, R),
    (0.3827, 0.9239),
    (0.0, 1.0),
    (-0.3827, 0.9239),
    (-R, R),
    (-0.9239, 0.3827),
    (-1.0, 0.0),
    (-0.9239, -0.3827),
    (-R, -R),
    (-0.3827, -0.9239),
    (0.0, -1.0),
    (0.3827, -0.9239),
    (R, -R),
    (0.9239, -0.3827),
];

/// Épicéa : cône à verticilles tous les 2 blocs, branches tombantes, géants de 2 × 2.
fn gran<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let giant = pl.style == Style::Giant;
    let h = if giant {
        r.int(19, 25)
    } else {
        {
            let u = r.f();
            (9 + (13.0 * u * u.sqrt()) as i32).min(22)
        }
    };
    let mut big = (0.26 * f64::from(h)).clamp(2.4, 5.2);
    if giant {
        big = (big * 1.3).min(6.4);
    }
    let c = if giant { (0.5, 0.5) } else { (0.0, 0.0) };
    let start = ((f64::from(h) * 0.22).round() as i32).max(2);
    for y in 0..h {
        p.wood(0, y, 0, Wood::Gran);
        if giant && y < h * 7 / 10 {
            p.wood(1, y, 0, Wood::Gran);
            p.wood(0, y, 1, Wood::Gran);
            p.wood(1, y, 1, Wood::Gran);
        }
    }
    if giant {
        for y in h * 7 / 10..h {
            p.wood(0, y, 0, Wood::Gran);
        }
        foot(p, 1, 0, Wood::Gran);
        foot(p, 0, 1, Wood::Gran);
        foot(p, 1, 1, Wood::Gran);
    }
    for y in start..=h {
        let v = f64::from(y - start) / f64::from((h - start).max(1));
        let rr = big * pow075(1.0 - v);
        let whorl = (y - start) % 2 == 0;
        let rd = if whorl { rr } else { (rr - 1.0).max(0.0) };
        disc(p, pl.seed, c, y, rd + 0.35, Leaf::Gran, 0.35);
        if whorl && rd >= 2.0 {
            // Branches tombantes : un bloc en contrebas au bout de certaines pointes.
            let (x0, x1) = ((c.0 - rd).floor() as i32, (c.0 + rd).ceil() as i32);
            let (z0, z1) = ((c.1 - rd).floor() as i32, (c.1 + rd).ceil() as i32);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let (dx, dz) = (f64::from(x) - c.0, f64::from(z) - c.1);
                    let d2 = dx * dx + dz * dz;
                    if d2 > (rd - 1.0) * (rd - 1.0)
                        && d2 <= (rd + 0.35) * (rd + 0.35)
                        && rag(pl.seed ^ 0x51, x, y, z) < 0.5
                    {
                        p.leaf(x, y - 1, z, Leaf::Gran);
                    }
                }
            }
        }
    }
    p.leaf(0, h + 1, 0, Leaf::Gran);
    if giant {
        p.leaf(1, h, 0, Leaf::Gran);
        p.leaf(0, h, 1, Leaf::Gran);
        p.leaf(1, h, 1, Leaf::Gran);
    }
}

/// Pin sylvestre : fût nu sur 60 %, haut orangé, houppier aplati en 2 ou 3 plateaux décalés.
fn tall<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let twisted = pl.style == Style::Twisted;
    let dwarf = pl.style == Style::Dwarf;
    let h = if dwarf {
        r.int(3, 5)
    } else if twisted {
        r.int(5, 9)
    } else {
        r.int(8, 16)
    };
    let plates = if dwarf {
        1
    } else if twisted {
        r.int(1, 2)
    } else {
        r.int(2, 3)
    };
    let (lx, lz) = match r.int(0, 3) {
        0 => (1, 0),
        1 => (-1, 0),
        2 => (0, 1),
        _ => (0, -1),
    };
    let (mut tx, mut tz) = (0, 0);
    let mut path = Vec::with_capacity(h as usize);
    for y in 0..h {
        if twisted && y > 0 && y % 2 == 0 && r.chance(0.7) {
            tx += lx;
            tz += lz;
        }
        path.push((tx, tz));
        let w = if y * 4 >= h * 3 {
            Wood::TallTop
        } else {
            Wood::Tall
        };
        p.wood(tx, y, tz, w);
    }
    for i in 0..plates {
        let y = (h - 1 - 2 * i).max(1);
        let (top_x, top_z) = path[y as usize];
        let (ox, oz) = if i == 0 {
            (0, 0)
        } else if twisted {
            (lx * r.int(1, 2), lz * r.int(1, 2))
        } else {
            (r.int(-2, 2), r.int(-2, 2))
        };
        let rad = if dwarf {
            1.6
        } else {
            (2.1 + r.f() * 1.2 - 0.3 * f64::from(i)).max(1.6)
        };
        let c = (f64::from(top_x + ox), f64::from(top_z + oz));
        disc(p, pl.seed, c, y, rad, Leaf::Tall, 0.4);
        disc(p, pl.seed, c, y + 1, (rad - 0.9).max(0.5), Leaf::Tall, 0.3);
        if ox != 0 || oz != 0 {
            // Branche du tronc au plateau décalé.
            let (sx, sz) = (ox.signum(), oz.signum());
            p.wood(top_x + sx, y - 1, top_z, Wood::TallTop);
            if sz != 0 {
                p.wood(top_x + sx, y - 1, top_z + sz, Wood::TallTop);
            }
        }
    }
}

/// Bouleau verruqueux : tronc fin blanc, houppier ovoïde allongé, rameaux pendants.
fn bjork<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let dwarf = pl.style == Style::Dwarf;
    let h = if dwarf { r.int(3, 5) } else { r.int(7, 13) };
    for y in 0..h - 1 {
        p.wood(0, y, 0, Wood::Bjork);
    }
    let cy = f64::from(h) - if dwarf { 1.6 } else { 3.2 };
    let ry = if dwarf { 1.7 } else { 3.3 + r.f() * 0.6 };
    let rx = if dwarf { 1.6 } else { 2.2 + r.f() * 0.9 };
    let rz = rx * (0.9 + 0.2 * r.f());
    if !dwarf {
        for _ in 0..2 {
            let (bx, bz) = match r.int(0, 3) {
                0 => (1, 0),
                1 => (-1, 0),
                2 => (0, 1),
                _ => (0, -1),
            };
            p.wood(bx, h - 5, bz, Wood::Bjork);
        }
    }
    ellipsoid(p, pl.seed, (0.0, cy, 0.0), (rx, ry, rz), Leaf::Bjork, 0.6);
    // Rameaux pendants : une à deux feuilles sous le bas du houppier.
    let n = if dwarf { -1 } else { rx.ceil() as i32 };
    for z in -n..=n {
        for x in -n..=n {
            let (tx, tz) = (f64::from(x) / rx, f64::from(z) / rz);
            let d = tx * tx + tz * tz;
            if d >= 0.85 {
                continue;
            }
            if rag(pl.seed ^ 0x77, x, 0, z) < 0.3 {
                let low = (cy - ry * (1.0 - d).sqrt()).floor() as i32 - 1;
                p.leaf(x, low, z, Leaf::Bjork);
                if rag(pl.seed ^ 0x78, x, 1, z) < 0.4 {
                    p.leaf(x, low - 1, z, Leaf::Bjork);
                }
            }
        }
    }
}

/// Bouleau tortueux : 2 à 4 tiges depuis la souche, marche biaisée vers le haut et sous le vent,
/// petites touffes.
fn fjallbjork<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let dwarf = pl.style == Style::Dwarf;
    let stems = if dwarf { r.int(1, 2) } else { r.int(2, 4) };
    let h = if dwarf { r.int(1, 2) } else { r.int(3, 6) };
    const STARTS: [(i32, i32); 4] = [(0, 0), (1, 0), (-1, 1), (0, -1)];
    for (s, &start) in STARTS.iter().enumerate().take(stems as usize) {
        let (mut x, mut z) = start;
        let sh = (h - r.int(0, 1)).max(1);
        if (x, z) != (0, 0) {
            foot(p, x, z, Wood::Bjork);
        }
        for y in 0..sh {
            p.wood(x, y, z, Wood::Bjork);
            if y >= 1 && r.chance(0.35) {
                if (pl.lean != [0, 0]) && r.chance(0.5) {
                    x += i32::from(pl.lean[0]);
                    z += i32::from(pl.lean[1]);
                } else if r.chance(0.5) {
                    x += r.int(-1, 1);
                } else {
                    z += r.int(-1, 1);
                }
            }
        }
        let tuft = if dwarf {
            (1.1, 0.8, 1.1)
        } else {
            (1.7, 1.3, 1.7)
        };
        ellipsoid(
            p,
            pl.seed ^ s as u64,
            (f64::from(x), f64::from(sh) - 0.3, f64::from(z)),
            tuft,
            Leaf::Bjork,
            0.5,
        );
        if sh >= 4 && r.chance(0.5) {
            ellipsoid(
                p,
                pl.seed ^ 0x9,
                (
                    f64::from(x) + f64::from(r.int(-1, 1)),
                    f64::from(sh) - 2.0,
                    f64::from(z),
                ),
                (1.2, 1.0, 1.2),
                Leaf::Bjork,
                0.5,
            );
        }
    }
}

/// Hêtre : fût lisse gris, branches hautes en éventail, dôme large à deux couches.
fn bok<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let h = r.int(12, 20);
    for y in 0..h - 2 {
        p.wood(0, y, 0, Wood::Bok);
    }
    let n = r.int(5, 6);
    let a0 = r.int(0, 15);
    for k in 0..n {
        let (ca, sa) = DIRS[((a0 + (16 * k + r.int(0, 2)) / n) % 16) as usize];
        let len = r.int(3, 5);
        let y0 = h - 9 + r.int(0, 2);
        for i in 1..=len {
            p.wood(
                (ca * f64::from(i)).round() as i32,
                y0 + i / 2,
                (sa * f64::from(i)).round() as i32,
                Wood::Bok,
            );
        }
    }
    let hf = f64::from(h);
    ellipsoid(
        p,
        pl.seed,
        (0.0, hf - 4.8, 0.0),
        (5.8, 2.2, 5.8),
        Leaf::Bok,
        0.5,
    );
    ellipsoid(
        p,
        pl.seed ^ 3,
        (0.0, hf - 2.2, 0.0),
        (4.2, 2.6, 4.2),
        Leaf::Bok,
        0.5,
    );
}

/// Chêne : branches maîtresses par colonisation d'espace simplifiée (6 à 9 points d'attraction
/// dans un ellipsoïde aplati), houppier très large ; tronc de 2 × 2 pour les vénérables.
fn ek<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let giant = pl.style == Style::Giant;
    let h = if giant { r.int(11, 15) } else { r.int(8, 14) };
    let hf = f64::from(h);
    let tk = (hf * 0.45).round() as i32;
    for y in 0..tk {
        p.wood(0, y, 0, Wood::Ek);
        if giant {
            p.wood(1, y, 0, Wood::Ek);
            p.wood(0, y, 1, Wood::Ek);
            p.wood(1, y, 1, Wood::Ek);
        }
    }
    if giant {
        foot(p, 1, 0, Wood::Ek);
        foot(p, 0, 1, Wood::Ek);
        foot(p, 1, 1, Wood::Ek);
    }
    let off = if giant { 0.5 } else { 0.0 };
    let mut nodes: Vec<(f64, f64, f64)> = vec![(off, f64::from(tk), off)];
    let count = r.int(6, 9);
    let (rx, ry) = (5.5, 2.6);
    let mut done = 0;
    let mut tries = 0;
    while done < count && tries < 80 {
        tries += 1;
        let (u, v, w) = (2.0 * r.f() - 1.0, 2.0 * r.f() - 1.0, 2.0 * r.f() - 1.0);
        if u * u + v * v + w * w > 1.0 {
            continue;
        }
        let a = (
            off + u * rx,
            (hf - 2.5 + v * ry).max(f64::from(tk) + 1.0),
            off + w * rx,
        );
        // Plus proche nœud du squelette.
        let near = nodes
            .iter()
            .copied()
            .min_by(|n, m| {
                let d = |q: &(f64, f64, f64)| {
                    let (dx, dy, dz) = (q.0 - a.0, q.1 - a.1, q.2 - a.2);
                    dx * dx + dy * dy + dz * dz
                };
                d(n).total_cmp(&d(m))
            })
            .unwrap_or(nodes[0]);
        line(p, near, a, Wood::Ek);
        nodes.push(a);
        ellipsoid(p, pl.seed ^ done as u64, a, (2.6, 1.9, 2.6), Leaf::Ek, 0.5);
        done += 1;
    }
    ellipsoid(
        p,
        pl.seed ^ 0xE1,
        (off, hf - 1.0, off),
        (3.2, 2.0, 3.2),
        Leaf::Ek,
        0.5,
    );
}

/// Sorbier : petit arbre rond.
fn ronn<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let h = r.int(4, 7);
    for y in 0..h - 1 {
        p.wood(0, y, 0, Wood::Ronn);
    }
    ellipsoid(
        p,
        pl.seed,
        (0.0, f64::from(h) - 1.6, 0.0),
        (2.3, 2.0, 2.3),
        Leaf::Ronn,
        0.5,
    );
}

/// Tremble : colonne étroite.
fn asp<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let h = r.int(9, 15);
    for y in 0..h - 1 {
        p.wood(0, y, 0, Wood::Asp);
    }
    ellipsoid(
        p,
        pl.seed,
        (0.0, f64::from(h) - 4.2, 0.0),
        (1.9, 4.6, 1.9),
        Leaf::Asp,
        0.7,
    );
}

/// Aulne : cône arrondi.
fn al<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let h = r.int(6, 10);
    for y in 0..h - 1 {
        p.wood(0, y, 0, Wood::Al);
    }
    let start = (f64::from(h) * 0.4).round() as i32;
    for y in start..=h {
        let v = f64::from(y - start) / f64::from((h - start).max(1));
        disc(
            p,
            pl.seed,
            (0.0, 0.0),
            y,
            3.0 * pow075(1.0 - v) + 0.3,
            Leaf::Al,
            0.4,
        );
    }
}

/// Saules : buissons en boule.
fn salg<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let dwarf = pl.style == Style::Dwarf;
    let stems = if dwarf { 1 } else { r.int(1, 3) };
    let sh = if dwarf { 1 } else { r.int(1, 3) };
    for y in 0..sh {
        p.wood(0, y, 0, Wood::Ronn);
    }
    for s in 1..stems {
        let (x, z) = if s == 1 { (1, 0) } else { (0, -1) };
        foot(p, x, z, Wood::Ronn);
        for y in 0..sh {
            p.wood(x, y, z, Wood::Ronn);
        }
    }
    let (rx, ry) = if dwarf {
        (1.2, 0.9)
    } else {
        (1.5 + 0.6 * r.f(), 1.3 + 0.4 * r.f())
    };
    ellipsoid(
        p,
        pl.seed,
        (0.0, f64::from(sh) - 0.2, 0.0),
        (rx, ry, rx),
        Leaf::Salg,
        0.5,
    );
}

/// Genévrier : colonne étroite ou coussin.
fn en<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    if r.chance(0.35) {
        disc(p, pl.seed, (0.0, 0.0), 0, 2.0 + r.f() * 0.5, Leaf::En, 0.4);
        disc(p, pl.seed, (0.0, 0.0), 1, 1.2, Leaf::En, 0.3);
        return;
    }
    let h = r.int(2, 4);
    for y in 0..h {
        p.leaf(0, y, 0, Leaf::En);
        if h >= 3 && y >= 1 && y < h - 1 {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if rag(pl.seed, dx, y, dz) < 0.6 {
                    p.leaf(dx, y, dz, Leaf::En);
                }
            }
        }
    }
}

/// Tilleul (houppier ovoïde de feuilles claires) ou noisetier (buisson de 3 à 5 tiges).
fn lind<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    if (pl.seed >> 8) % 5 < 2 && pl.style != Style::Dwarf {
        let h = r.int(8, 12);
        for y in 0..h - 2 {
            p.wood(0, y, 0, Wood::Bok);
        }
        ellipsoid(
            p,
            pl.seed,
            (0.0, f64::from(h) - 3.6, 0.0),
            (3.8, 3.3, 3.8),
            Leaf::Hassel,
            0.5,
        );
        return;
    }
    let stems = r.int(3, 5);
    const RING: [(i32, i32); 5] = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)];
    for (s, &(ox, oz)) in RING.iter().enumerate().take(stems as usize) {
        let sh = r.int(3, 5);
        let (mut x, mut z) = (ox, oz);
        if (ox, oz) != (0, 0) {
            foot(p, x, z, Wood::Ronn);
        }
        for y in 0..sh {
            p.wood(x, y, z, Wood::Ronn);
            if y % 2 == 1 {
                x += ox.signum();
                z += oz.signum();
            }
        }
        ellipsoid(
            p,
            pl.seed ^ s as u64,
            (f64::from(x), f64::from(sh) - 0.2, f64::from(z)),
            (1.5, 1.4, 1.5),
            Leaf::Hassel,
            0.5,
        );
    }
}

/// Chicot : tronc sans houppier, quelques chicots de branches.
fn snag<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let h = r.int(4, 12);
    for y in 0..h {
        p.wood(0, y, 0, Wood::Dead);
    }
    for _ in 0..r.int(1, 3) {
        let y = r.int(h / 2, (h - 1).max(h / 2));
        let (dx, dz) = match r.int(0, 3) {
            0 => (1, 0),
            1 => (-1, 0),
            2 => (0, 1),
            _ => (0, -1),
        };
        p.wood(dx, y, dz, Wood::Dead);
        if r.chance(0.4) {
            p.wood(dx * 2, y, dz * 2, Wood::Dead);
        }
    }
}

/// Souche moussue.
fn stump<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    p.wood(0, 0, 0, Wood::Mossy);
    if r.chance(0.5) {
        p.wood(0, 1, 0, Wood::Mossy);
    }
}

/// Tronc couché moussu : 4 à 9 blocs le long de x ou de z, posé là où le sol le porte.
fn log<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    let mut r = Rng::new(pl.seed);
    let along_x = r.chance(0.5);
    let len = r.int(4, 9);
    let start = -(len / 2);
    for k in 0..len {
        let (dx, dz) = if along_x {
            (start + k, 0)
        } else {
            (0, start + k)
        };
        if p.supported(dx, 0, dz) {
            p.wood(dx, 0, dz, Wood::Mossy);
        }
    }
}

/// Dessine la plante dans `p`.
pub fn draw<P: Put + ?Sized>(pl: &Plant, p: &mut P) {
    match pl.kind {
        Kind::Living(s) => match s {
            Species::Gran => gran(pl, p),
            Species::Tall => tall(pl, p),
            Species::Bjork => bjork(pl, p),
            Species::Fjallbjork => fjallbjork(pl, p),
            Species::Bok => bok(pl, p),
            Species::Ek => ek(pl, p),
            Species::Ronn => ronn(pl, p),
            Species::Asp => asp(pl, p),
            Species::Al => al(pl, p),
            Species::Salg => salg(pl, p),
            Species::En => en(pl, p),
            Species::Lind => lind(pl, p),
        },
        Kind::Snag => snag(pl, p),
        Kind::Stump => stump(pl, p),
        Kind::Log => log(pl, p),
    }
}

// ---------------------------------------------------------------------------------------------
// Écriture dans un chunk
// ---------------------------------------------------------------------------------------------

/// Écrit une plante dans un chunk : tout ce qui sort du volume du chunk ou de [`MAX_REACH`] est
/// ignoré.
struct ChunkPut<'a> {
    chunk: &'a mut Chunk,
    blocks: &'a TreeBlocks,
    /// Pied de la plante, en coordonnées locales au chunk (peuvent sortir de `0..16`).
    ox: i32,
    oy: i32,
    oz: i32,
}

impl ChunkPut<'_> {
    /// Coordonnées locales du bloc relatif, si dans la portée et dans le chunk.
    #[inline]
    fn locate(&self, dx: i32, dy: i32, dz: i32) -> Option<(i32, i32, i32)> {
        if dx.abs() > MAX_REACH || dz.abs() > MAX_REACH {
            return None;
        }
        let (x, y, z) = (self.ox + dx, self.oy + dy, self.oz + dz);
        let inside = (0..CHUNK_SIZE_X as i32).contains(&x)
            && (0..CHUNK_SIZE_Z as i32).contains(&z)
            && (0..CHUNK_SIZE_Y as i32).contains(&y);
        inside.then_some((x, y, z))
    }
}

impl Put for ChunkPut<'_> {
    fn wood(&mut self, dx: i32, dy: i32, dz: i32, w: Wood) {
        if let Some((x, y, z)) = self.locate(dx, dy, dz) {
            let cur = self.chunk.block(x, y, z);
            if cur == AIR || self.blocks.is_leaf(cur) {
                self.chunk.set_block(x, y, z, self.blocks.wood(w));
            }
        }
    }

    fn leaf(&mut self, dx: i32, dy: i32, dz: i32, l: Leaf) {
        if let Some((x, y, z)) = self.locate(dx, dy, dz) {
            if self.chunk.block(x, y, z) == AIR {
                self.chunk.set_block(x, y, z, self.blocks.leaf(l));
            }
        }
    }

    fn supported(&mut self, dx: i32, dy: i32, dz: i32) -> bool {
        match self.locate(dx, dy - 1, dz) {
            Some((x, y, z)) => self.blocks.solid[usize::from(self.chunk.block(x, y, z))],
            None => false,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------------------------

/// Une colonne isolée vue par le placement : tout ce que le terrain en calcule en une fois.
#[derive(Debug, Clone, Copy)]
pub struct TreeColumn {
    /// Colonne d'eau scellée.
    pub water: WaterColumn,
    /// Biome de la colonne (identique à celui du chunk généré).
    pub sel: Selection,
    /// Champs macroscopiques.
    pub m: MacroSample,
    /// Relief et formes.
    pub r: Relief,
    /// Plus grande différence de sol avec les quatre voisines.
    pub slope: i32,
}

/// Ce que le placement demande au terrain : des sondes pures sur des colonnes isolées.
pub struct TreeProbe<'a> {
    pub column: &'a dyn Fn(i32, i32) -> TreeColumn,
    /// Colonne d'eau scellée (membres d'un bosquet).
    pub water: &'a dyn Fn(i32, i32) -> WaterColumn,
    /// Y a-t-il de l'eau à moins de 4 blocs ? (aulne)
    pub wet_near: &'a dyn Fn(i32, i32) -> bool,
    /// Colonne réservée à une piste, un cairn ou un jalon : aucun arbre n'y pousse (session 4.1).
    pub blocked: &'a dyn Fn(i32, i32) -> bool,
}

/// Choix du mélange d'un biome.
#[derive(Debug, Clone, Copy)]
enum Pick {
    T(Species, Style),
}

/// Règles d'un biome (§7.3, colonne « Arbres », et §9.1).
struct Profile {
    /// Densité de base : probabilité qu'une cellule porte une plante.
    density: f64,
    /// Mélange pondéré.
    mix: &'static [(Pick, f64)],
    /// Part de bois mort parmi les plantes.
    dead: f64,
    /// Sans raréfaction à l'approche de la limite des arbres (toundra, tourbière, archipel).
    ignore_tree_line: bool,
}

use Species as S;

use Style::{Dwarf, Normal, Twisted};

macro_rules! t {
    ($s:expr, $st:expr, $w:expr) => {
        (Pick::T($s, $st), $w)
    };
}

fn profile(b: NorrBiome) -> Option<Profile> {
    let p = |density, mix, dead, ignore_tree_line| {
        Some(Profile {
            density,
            mix,
            dead,
            ignore_tree_line,
        })
    };
    match b {
        NorrBiome::Fjord => p(0.05, &[t!(S::Tall, Twisted, 1.0)], 0.0, true),
        NorrBiome::Skargard => p(
            0.20,
            &[
                t!(S::Tall, Twisted, 0.45),
                t!(S::Ronn, Normal, 0.15),
                t!(S::En, Normal, 0.20),
                t!(S::Al, Normal, 0.20),
            ],
            0.0,
            true,
        ),
        NorrBiome::Klintkust => p(
            0.07,
            &[t!(S::En, Normal, 0.55), t!(S::Tall, Twisted, 0.45)],
            0.0,
            true,
        ),
        NorrBiome::Hogkust => p(
            0.32,
            &[t!(S::Gran, Normal, 0.5), t!(S::Tall, Normal, 0.5)],
            0.0,
            false,
        ),
        NorrBiome::Strandang => p(0.05, &[t!(S::Salg, Normal, 1.0)], 0.0, true),
        NorrBiome::Bokskog => p(
            0.46,
            &[
                t!(S::Bok, Normal, 0.62),
                t!(S::Ek, Normal, 0.13),
                t!(S::Lind, Normal, 0.17),
                t!(S::Bjork, Normal, 0.08),
            ],
            0.04,
            false,
        ),
        NorrBiome::Hagmark => p(
            0.13,
            &[
                t!(S::Ek, Normal, 0.30),
                t!(S::Bjork, Normal, 0.30),
                t!(S::Ronn, Normal, 0.14),
                t!(S::Lind, Normal, 0.16),
                t!(S::En, Normal, 0.10),
            ],
            0.0,
            false,
        ),
        NorrBiome::Alvar => p(0.04, &[t!(S::En, Normal, 1.0)], 0.0, true),
        NorrBiome::Sjoland => p(
            0.36,
            &[
                t!(S::Tall, Normal, 0.30),
                t!(S::Gran, Normal, 0.30),
                t!(S::Bjork, Normal, 0.22),
                t!(S::Al, Normal, 0.18),
            ],
            0.04,
            false,
        ),
        NorrBiome::Urskog => p(
            0.56,
            &[
                t!(S::Gran, Normal, 0.50),
                t!(S::Tall, Normal, 0.14),
                t!(S::Asp, Normal, 0.02),
                t!(S::Bjork, Normal, 0.22),
            ],
            0.08,
            false,
        ),
        NorrBiome::Tallhed => p(0.30, &[t!(S::Tall, Normal, 1.0)], 0.03, false),
        NorrBiome::Myr => p(
            0.06,
            &[t!(S::Tall, Dwarf, 0.55), t!(S::Fjallbjork, Dwarf, 0.45)],
            0.02,
            true,
        ),
        NorrBiome::Alv => p(
            0.42,
            &[
                t!(S::Al, Normal, 0.40),
                t!(S::Salg, Normal, 0.25),
                t!(S::Asp, Normal, 0.35),
            ],
            0.02,
            false,
        ),
        NorrBiome::Brannfalt => p(0.30, &[t!(S::Bjork, Dwarf, 1.0)], 0.62, false),
        NorrBiome::Bergslag => p(
            0.40,
            &[t!(S::Gran, Normal, 0.70), t!(S::Bjork, Normal, 0.30)],
            0.0,
            false,
        ),
        NorrBiome::Fjallbjorkskog => p(
            0.30,
            &[t!(S::Fjallbjork, Normal, 0.80), t!(S::Ronn, Normal, 0.20)],
            0.0,
            false,
        ),
        NorrBiome::Fjallhed => p(
            0.045,
            &[t!(S::Salg, Dwarf, 0.70), t!(S::Fjallbjork, Dwarf, 0.30)],
            0.0,
            true,
        ),
        NorrBiome::Palsmyr => p(0.05, &[t!(S::Fjallbjork, Dwarf, 1.0)], 0.0, true),
        NorrBiome::Lavafalt => p(0.04, &[t!(S::Fjallbjork, Dwarf, 1.0)], 0.0, true),
        NorrBiome::Vasterhav
        | NorrBiome::Djuphav
        | NorrBiome::Bottenhav
        | NorrBiome::Packis
        | NorrBiome::Kalfjall
        | NorrBiome::Glaciar => None,
    }
}

/// Cellule mémoïsée : position dans la grille et plantes.
type Cell = ((i32, i32), Arc<[Plant]>);

type CellCache = Arc<Mutex<HashMap<(i32, i32), Arc<[Plant]>>>>;

/// Placement des arbres : fonction pure de (graine, colonne), cellules mémoïsées.
#[derive(Debug, Clone)]
pub struct Trees {
    seed: u64,
    sea: i32,
    blocks: TreeBlocks,
    grove: GradField,
    stands: Worley,
    cache: CellCache,
}

impl Trees {
    pub fn new(seed: i64, sea_level: i32, blocks: TreeBlocks) -> Self {
        Self {
            seed: (seed as u64) ^ PLACE_SEED,
            sea: sea_level,
            blocks,
            grove: GradField::new(seed, GROVE_SEED),
            stands: Worley::new(seed, STAND_SEED, STAND_PITCH),
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn blocks(&self) -> &TreeBlocks {
        &self.blocks
    }

    /// Position (jitterée) de la cellule de la grille.
    fn cell_pos(&self, i: i32, j: i32) -> (i32, i32) {
        (
            CELL * i + 1 + (3.0 * hash01(self.seed, i, j, 0)) as i32,
            CELL * j + 1 + (3.0 * hash01(self.seed, i, j, 1)) as i32,
        )
    }

    /// Plantes de la cellule `(i, j)` (aucune, une, ou un bosquet de trembles).
    pub fn cell_plants(&self, probe: &TreeProbe, i: i32, j: i32) -> Vec<Plant> {
        let mut out = Vec::new();
        let (x, z) = self.cell_pos(i, j);
        let col = (probe.column)(x, z);
        let (w, sel, m, r) = (col.water, col.sel, col.m, col.r);
        if w.is_wet() || w.dam || w.top <= self.sea + 1 || sel.snow || sel.fresh_water {
            return out;
        }
        let Some(prof) = profile(sel.biome) else {
            return out;
        };
        let f = &r.forms;
        // Sol non fertile : glace, plage, pente.
        if f.ice > 0.0 || (f.coast == Coast::Dunes && w.top <= self.sea + 6) || col.slope >= 3 {
            return out;
        }
        // Densité : base du biome, clairières, limite des arbres, bord de tourbière.
        let (fx, fz) = (f64::from(x), f64::from(z));
        let grove = 0.4
            + 1.2
                * self
                    .grove
                    .fbm(fx / GROVE_WAVELENGTH, fz / GROVE_WAVELENGTH, 2);
        let mut density = prof.density * grove;
        if !prof.ignore_tree_line {
            let line = altitudes(m.t0, self.sea).tree_line;
            density *= smooth(line, line - 10.0, f64::from(w.top));
        }
        if sel.biome != NorrBiome::Myr {
            density *= 1.0 - f.mire.clamp(0.0, 1.0);
        }
        if hash01(self.seed, i, j, 2) >= density {
            return out;
        }
        if (probe.blocked)(x, z) {
            return out;
        }
        let shape = hash01(self.seed, i, j, 5);
        let seed = (hash01(self.seed, i, j, 6) * 9_007_199_254_740_992.0) as u64;
        let lean = [
            if m.theta[0].abs() > 0.38 {
                m.theta[0].signum() as i8
            } else {
                0
            },
            if m.theta[1].abs() > 0.38 {
                m.theta[1].signum() as i8
            } else {
                0
            },
        ];
        let plant = |kind, style, x, z, y, seed| Plant {
            kind,
            style,
            x,
            z,
            y,
            seed,
            lean,
        };
        // Bois mort.
        if hash01(self.seed, i, j, 7) < prof.dead {
            let kind = match (shape * 4.0) as u32 {
                0 | 1 => Kind::Snag,
                2 => Kind::Stump,
                _ => Kind::Log,
            };
            out.push(plant(kind, Normal, x, z, w.top, seed));
            return out;
        }
        // Essence : mélange du biome, l'essence de la tache est favorisée par Vr.
        let hit = self.stands.eval(fx, fz);
        let n = prof.mix.len();
        let favoured = (self.stands.cell_rand(hit.cell.0, hit.cell.1, 0) * n as f64) as usize;
        let boost = 1.0 + 5.0 * m.vr;
        let weight = |k: usize| prof.mix[k].1 * if k == favoured { boost } else { 1.0 };
        let total: f64 = (0..n).map(weight).sum();
        let mut u = hash01(self.seed, i, j, 3) * total;
        let mut chosen = n - 1;
        for k in 0..n {
            u -= weight(k);
            if u < 0.0 {
                chosen = k;
                break;
            }
        }
        let Pick::T(species, mut style) = prof.mix[chosen].0;
        if species == S::Gran && sel.biome == NorrBiome::Urskog && shape < 0.05 {
            style = Style::Giant;
        }
        if species == S::Ek
            && matches!(sel.biome, NorrBiome::Hagmark | NorrBiome::Bokskog)
            && shape < 0.10
        {
            style = Style::Giant;
        }
        if species == S::Al && !(probe.wet_near)(x, z) {
            return out;
        }
        // Bosquet clonal de trembles : 5 à 12 troncs serrés.
        if species == S::Asp && matches!(sel.biome, NorrBiome::Urskog | NorrBiome::Alv) {
            let n = 5 + (hash01(self.seed, i, j, 8) * 8.0) as i32;
            for k in 0..n {
                let (mx, mz) = if k == 0 {
                    (x, z)
                } else {
                    // Point tiré dans le disque de rayon 3,5 par rejet.
                    let mut off = (0, 0);
                    for t in 0..8 {
                        let u = 7.0 * hash01(self.seed, i, j, 100 + 16 * k + 2 * t) - 3.5;
                        let v = 7.0 * hash01(self.seed, i, j, 101 + 16 * k + 2 * t) - 3.5;
                        if u * u + v * v <= 12.25 {
                            off = (u.round() as i32, v.round() as i32);
                            break;
                        }
                    }
                    (x + off.0, z + off.1)
                };
                let mw = if k == 0 { w } else { (probe.water)(mx, mz) };
                if mw.is_wet() || (mw.top - w.top).abs() > 2 || mw.top <= self.sea + 1 {
                    continue;
                }
                if k > 0 && (probe.blocked)(mx, mz) {
                    continue;
                }
                if out.iter().any(|q: &Plant| q.x == mx && q.z == mz) {
                    continue;
                }
                let ms = (hash01(self.seed, i, j, 300 + k) * 9_007_199_254_740_992.0) as u64;
                out.push(plant(Kind::Living(S::Asp), Normal, mx, mz, mw.top, ms));
            }
            return out;
        }
        out.push(plant(Kind::Living(species), style, x, z, w.top, seed));
        out
    }

    /// Une plante pousse-t-elle à moins de `r` blocs (en x et en z) de la colonne `(x, z)` ?
    pub fn plant_within(&self, probe: &TreeProbe, x: i32, z: i32, r: i32) -> bool {
        // Les membres d'un bosquet s'écartent jusqu'à 4 blocs de la position de leur cellule.
        let m = r + 4;
        for j in (z - m - 3 + CELL - 1).div_euclid(CELL)..=(z + m - 1).div_euclid(CELL) {
            for i in (x - m - 3 + CELL - 1).div_euclid(CELL)..=(x + m - 1).div_euclid(CELL) {
                if self
                    .cell_plants(probe, i, j)
                    .iter()
                    .any(|p| (p.x - x).abs() <= r && (p.z - z).abs() <= r)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Plantes de toutes les cellules qui peuvent toucher le chunk d'origine `(x0, z0)`, dans
    /// l'ordre global (rangées de z croissantes, puis x croissants).
    pub fn plants_near(&self, probe: &TreeProbe, x0: i32, z0: i32) -> Vec<Arc<[Plant]>> {
        let span = |o: i32, len: i32| {
            // Position de cellule dans [5i + 1, 5i + 3] : i de ceil((o − R − 3) / 5) à floor((o + len − 1 + R − 1) / 5).
            let lo = (o - MAX_REACH - 3 + CELL - 1).div_euclid(CELL);
            let hi = (o + len - 1 + MAX_REACH - 1).div_euclid(CELL);
            lo..=hi
        };
        let cells: Vec<(i32, i32)> = span(z0, CHUNK_SIZE_Z as i32)
            .flat_map(|j| span(x0, CHUNK_SIZE_X as i32).map(move |i| (i, j)))
            .collect();
        let mut found: Vec<Option<Arc<[Plant]>>> = {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cells.iter().map(|c| cache.get(c).cloned()).collect()
        };
        let mut fresh: Vec<Cell> = Vec::new();
        for (k, c) in cells.iter().enumerate() {
            if found[k].is_none() {
                let plants: Arc<[Plant]> = self.cell_plants(probe, c.0, c.1).into();
                fresh.push((*c, Arc::clone(&plants)));
                found[k] = Some(plants);
            }
        }
        if !fresh.is_empty() {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() + fresh.len() > CACHE_MAX {
                cache.clear();
            }
            cache.extend(fresh);
        }
        found.into_iter().flatten().collect()
    }

    /// Écrit dans `chunk` les plantes des cellules voisines, dans l'ordre global.
    pub fn stamp(&self, chunk: &mut Chunk, plants: &[Arc<[Plant]>]) {
        let pos = chunk.pos();
        let (x0, z0) = (pos.x * CHUNK_SIZE_X as i32, pos.z * CHUNK_SIZE_Z as i32);
        for cell in plants {
            for pl in cell.iter() {
                let (ox, oz) = (pl.x - x0, pl.z - z0);
                if ox < -MAX_REACH
                    || ox >= CHUNK_SIZE_X as i32 + MAX_REACH
                    || oz < -MAX_REACH
                    || oz >= CHUNK_SIZE_Z as i32 + MAX_REACH
                {
                    continue;
                }
                let mut put = ChunkPut {
                    chunk,
                    blocks: &self.blocks,
                    ox,
                    oy: pl.y,
                    oz,
                };
                draw(pl, &mut put);
            }
        }
    }

    /// Passe d'arbres d'un chunk.
    pub fn pass(&self, chunk: &mut Chunk, probe: &TreeProbe) {
        let pos = chunk.pos();
        let plants = self.plants_near(
            probe,
            pos.x * CHUNK_SIZE_X as i32,
            pos.z * CHUNK_SIZE_Z as i32,
        );
        self.stamp(chunk, &plants);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Collecte les blocs d'une plante sans les recouper.
    #[derive(Default)]
    struct Collect {
        wood: HashSet<(i32, i32, i32)>,
        leaf: HashSet<(i32, i32, i32)>,
    }

    impl Put for Collect {
        fn wood(&mut self, dx: i32, dy: i32, dz: i32, _: Wood) {
            self.wood.insert((dx, dy, dz));
        }
        fn leaf(&mut self, dx: i32, dy: i32, dz: i32, _: Leaf) {
            self.leaf.insert((dx, dy, dz));
        }
        fn supported(&mut self, _: i32, _: i32, _: i32) -> bool {
            true
        }
    }

    fn plant(kind: Kind, style: Style, seed: u64) -> Plant {
        Plant {
            kind,
            style,
            x: 0,
            z: 0,
            y: 50,
            seed,
            lean: [1, 0],
        }
    }

    fn all_plants() -> Vec<(Kind, Style)> {
        let mut v = Vec::new();
        for s in Species::ALL {
            v.push((Kind::Living(s), Style::Normal));
            v.push((Kind::Living(s), Style::Dwarf));
        }
        v.push((Kind::Living(Species::Tall), Style::Twisted));
        v.push((Kind::Living(Species::Gran), Style::Giant));
        v.push((Kind::Living(Species::Ek), Style::Giant));
        v.extend([
            (Kind::Snag, Style::Normal),
            (Kind::Stump, Style::Normal),
            (Kind::Log, Style::Normal),
        ]);
        v
    }

    #[test]
    fn every_shape_stays_within_the_reach_and_is_deterministic() {
        for (kind, style) in all_plants() {
            for seed in 0..200u64 {
                let pl = plant(kind, style, seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let (mut a, mut b) = (Collect::default(), Collect::default());
                draw(&pl, &mut a);
                draw(&pl, &mut b);
                assert_eq!(a.wood, b.wood, "{kind:?} {style:?}");
                assert_eq!(a.leaf, b.leaf, "{kind:?} {style:?}");
                for &(dx, dy, dz) in a.wood.iter().chain(a.leaf.iter()) {
                    assert!(
                        dx.abs() <= MAX_REACH && dz.abs() <= MAX_REACH,
                        "{kind:?} {style:?} seed {seed} sort de la portée : {dx} {dz}"
                    );
                    assert!((-2..=40).contains(&dy), "{kind:?} {style:?} dy = {dy}");
                }
            }
        }
    }

    #[test]
    fn living_trees_have_a_trunk_and_foliage_of_plausible_size() {
        for s in Species::ALL {
            let (mut tall_sum, mut n) = (0, 0);
            for seed in 0..100u64 {
                let pl = plant(Kind::Living(s), Style::Normal, seed * 7919 + 1);
                let mut c = Collect::default();
                draw(&pl, &mut c);
                assert!(!c.leaf.is_empty(), "{s:?} sans feuillage");
                let top = c
                    .wood
                    .iter()
                    .chain(c.leaf.iter())
                    .map(|p| p.1)
                    .max()
                    .unwrap();
                tall_sum += top;
                n += 1;
                if !matches!(s, Species::En) {
                    assert!(c.wood.contains(&(0, 0, 0)), "{s:?} sans pied de tronc");
                }
            }
            let mean = f64::from(tall_sum) / f64::from(n);
            let (lo, hi) = match s {
                Species::Gran => (9.0, 24.0),
                Species::Tall => (8.0, 17.0),
                Species::Bjork => (7.0, 14.0),
                Species::Fjallbjork => (3.0, 7.0),
                Species::Bok => (12.0, 21.0),
                Species::Ek => (8.0, 16.0),
                Species::Ronn => (4.0, 8.0),
                Species::Asp => (9.0, 16.0),
                Species::Al => (6.0, 11.0),
                Species::Salg => (1.0, 5.0),
                Species::En => (1.0, 5.0),
                Species::Lind => (3.0, 13.0),
            };
            assert!((lo..=hi).contains(&mean), "{s:?} : hauteur moyenne {mean}");
        }
    }

    #[test]
    fn spruce_is_a_cone_wider_at_the_bottom() {
        let pl = plant(Kind::Living(Species::Gran), Style::Normal, 4242);
        let mut c = Collect::default();
        draw(&pl, &mut c);
        let width = |y: i32| {
            c.leaf
                .iter()
                .filter(|p| p.1 == y)
                .map(|p| p.0.abs().max(p.2.abs()))
                .max()
                .unwrap_or(0)
        };
        let top = c.leaf.iter().map(|p| p.1).max().unwrap();
        let low = c.leaf.iter().map(|p| p.1).min().unwrap();
        assert!(
            width(low + 1) > width(top - 2),
            "{} {}",
            width(low + 1),
            width(top - 2)
        );
    }

    #[test]
    fn giant_spruce_and_veteran_oak_have_a_two_by_two_trunk() {
        for (s, y) in [(Species::Gran, 3), (Species::Ek, 1)] {
            let pl = plant(Kind::Living(s), Style::Giant, 99);
            let mut c = Collect::default();
            draw(&pl, &mut c);
            for p in [(0, y, 0), (1, y, 0), (0, y, 1), (1, y, 1)] {
                assert!(c.wood.contains(&p), "{s:?} sans {p:?}");
            }
        }
    }
}
