//! Bâtiments Norrcraft (sessions 4.2 à 4.4 ; GDD doc 02 §10.1) : habitat (torp, by, lada, fäbod,
//! fiskeläge, bastu, fjällstuga, vindskydd, kåta), patrimoine (runsten, gravhög…) et industrie et
//! nature (gruva, hytta, kolmila, tjärdal, bäverdamm, björnide, geyser, vårdkase).
//!
//! Chaque site construit est décrit par un [`Building`] : une liste de [`Part`] (un bâtiment, un
//! enclos, un ponton…) en coordonnées absolues du monde. Un `Building` est une fonction pure de
//! (site, relief) : les sondes de [`SiteProbe`] donnent le sol de chaque pièce, qui a donc son
//! propre niveau (un hameau épouse la pente au lieu d'être posé sur une plate-forme unique). Les
//! pièces dont l'emprise est mouillée ou trop inclinée sont omises.
//!
//! La pose (déblai de l'emprise, fondation de pierre sous le plancher, voisinage dégagé de la
//! végétation, pieux des pontons) est faite par `Structures::pass` ; ce module ne sait que dessiner.
//!
//! Repère local d'une pièce : `y = 0` est le premier bloc d'air au-dessus du sol, `y = -1` le
//! plancher ; la porte donne sur `+z` ; la rotation (quart de tour) est appliquée par [`Xf`].

use super::hydro::WaterKind;
use super::math::hash01;
use super::structures::{StructKind, StructSite};
use super::subsoil::SiteProbe;

/// Matériaux des bâtiments (les blocs sont résolus par `StructBlocks`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mat {
    Air,
    /// Rouge de Falun.
    Wall,
    /// Angles et huisseries blancs.
    Trim,
    /// Rondins.
    Log,
    Plank,
    /// Toit de tourbe.
    Turf,
    Hay,
    /// Bois gris des granges.
    Grey,
    /// Bois goudronné.
    Tar,
    /// Foyer ouvert.
    Hearth,
    /// Poêle à pierres.
    Stove,
    Glass,
    Stone,
    Bed,
    Table,
    Birch,
    Torch,
    Path,
    Water,
    /// Pierre runique.
    Rune,
    /// Menhir.
    Menhir,
    /// Feu de phare.
    Beacon,
    /// Voile de moulin.
    Sail,
    /// Roche gravée.
    Carved,
    /// Coffre au trésor.
    Treasure,
    /// Maçonnerie de chambre funéraire.
    Chamber,
    /// Charbon de bois.
    Coal,
    /// Scories bleues de haut fourneau.
    Slag,
    /// Sinter siliceux des geysers.
    Sinter,
    /// Roche brûlante (magma).
    Hot,
    /// Mottes de terre.
    Earth,
    /// Orgues basaltiques.
    Basalt,
}

impl Mat {
    pub const COUNT: usize = 32;

    pub const ALL: [Mat; Mat::COUNT] = [
        Mat::Air,
        Mat::Wall,
        Mat::Trim,
        Mat::Log,
        Mat::Plank,
        Mat::Turf,
        Mat::Hay,
        Mat::Grey,
        Mat::Tar,
        Mat::Hearth,
        Mat::Stove,
        Mat::Glass,
        Mat::Stone,
        Mat::Bed,
        Mat::Table,
        Mat::Birch,
        Mat::Torch,
        Mat::Path,
        Mat::Water,
        Mat::Rune,
        Mat::Menhir,
        Mat::Beacon,
        Mat::Sail,
        Mat::Carved,
        Mat::Treasure,
        Mat::Chamber,
        Mat::Coal,
        Mat::Slag,
        Mat::Sinter,
        Mat::Hot,
        Mat::Earth,
        Mat::Basalt,
    ];

    /// Nom du bloc Norrcraft (vide pour l'air).
    pub const fn block(self) -> &'static str {
        match self {
            Mat::Air => "",
            Mat::Wall => "faluroed",
            Mat::Trim => "vitkant",
            Mat::Log => "stock",
            Mat::Plank => "planka",
            Mat::Turf => "torvtak",
            Mat::Hay => "halm",
            Mat::Grey => "gratra",
            Mat::Tar => "tjarbrada",
            Mat::Hearth => "eldstad",
            Mat::Stove => "kakelugn",
            Mat::Glass => "glasruta",
            Mat::Stone => "grundsten",
            Mat::Bed => "sang",
            Mat::Table => "bord",
            Mat::Birch => "bjorkplanka",
            Mat::Torch => "fackla",
            Mat::Path => "stig",
            Mat::Water => "water",
            Mat::Rune => "runblock",
            Mat::Menhir => "menhir",
            Mat::Beacon => "fyrljus",
            Mat::Sail => "segel",
            Mat::Carved => "hallristad",
            Mat::Treasure => "skatt",
            Mat::Chamber => "kammarsten",
            Mat::Coal => "kol",
            Mat::Slag => "blaslagg",
            Mat::Sinter => "geyserit",
            Mat::Hot => "hetsten",
            Mat::Earth => "torv",
            Mat::Basalt => "basaltsten",
        }
    }
}

/// Une pièce d'un bâtiment, en coordonnées absolues.
#[derive(Debug, Clone)]
pub struct Part {
    /// Premier `y` d'air au-dessus du sol de la pièce (le plancher est à `floor - 1`).
    pub floor: i32,
    /// Emprise déblayée et fondée : `x0, z0, x1, z1` inclus (vide si `x0 > x1`).
    pub pad: [i32; 4],
    /// Hauteur déblayée au-dessus de `floor`.
    pub height: i32,
    /// Blocs posés, dans l'ordre : `(x, y, z, matériau)`.
    pub voxels: Vec<(i32, i32, i32, Mat)>,
    /// Pieux : `(x, z, y)` du haut, descendus jusqu'au fond.
    pub piles: Vec<(i32, i32, i32)>,
    /// Boîte englobante en plan : `x0, z0, x1, z1` inclus.
    pub bbox: [i32; 4],
}

/// Un site construit.
#[derive(Debug, Clone)]
pub struct Building {
    pub parts: Vec<Part>,
    /// Boîte englobante des pièces : `x0, z0, x1, z1` inclus.
    pub bbox: [i32; 4],
}

/// Marge autour d'un bâtiment où les arbres ne poussent pas.
const CLEAR: i32 = 2;

impl Building {
    /// Colonne interdite aux arbres (boîte du bâtiment, marge comprise).
    pub fn reserves(&self, x: i32, z: i32) -> bool {
        let [x0, z0, x1, z1] = self.bbox;
        x >= x0 - CLEAR && x <= x1 + CLEAR && z >= z0 - CLEAR && z <= z1 + CLEAR
    }

    /// La boîte du bâtiment touche-t-elle celle du chunk ?
    pub fn touches(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        let [bx0, bz0, bx1, bz1] = self.bbox;
        bx0 <= x1 && bx1 >= x0 && bz0 <= z1 && bz1 >= z0
    }

    /// Nombre de blocs posés (pour les tests).
    pub fn voxel_count(&self) -> usize {
        self.parts.iter().map(|p| p.voxels.len()).sum()
    }
}

/// Distance maximale d'un bâtiment à son centre, pour les plans de chunk.
pub const REACH: i32 = 48;

// ---------------------------------------------------------------------------------------------
// Repère et pièces
// ---------------------------------------------------------------------------------------------

/// Repère : origine monde et quart de tour (0 à 3).
#[derive(Debug, Clone, Copy)]
struct Xf {
    x: i32,
    z: i32,
    rot: u8,
}

impl Xf {
    fn map(self, x: i32, z: i32) -> (i32, i32) {
        let (dx, dz) = match self.rot & 3 {
            0 => (x, z),
            1 => (-z, x),
            2 => (-x, -z),
            _ => (z, -x),
        };
        (self.x + dx, self.z + dz)
    }

    /// Repère enfant : origine en `(dx, dz)` de celui-ci, tourné de `turn` quarts de tour en plus.
    fn child(self, dx: i32, dz: i32, turn: u8) -> Xf {
        let (x, z) = self.map(dx, dz);
        Xf {
            x,
            z,
            rot: (self.rot + turn) & 3,
        }
    }
}

/// Rotation qui oriente `+z` local vers `(dx, dz)` du monde (axe dominant).
fn rot_toward(dx: i32, dz: i32) -> u8 {
    if dx.abs() >= dz.abs() {
        if dx > 0 {
            3
        } else {
            1
        }
    } else if dz > 0 {
        0
    } else {
        2
    }
}

/// Rotation de la porte vers le centre `(0, 0)` depuis `(dx, dz)`.
fn face_center(dx: i32, dz: i32) -> u8 {
    rot_toward(-dx, -dz)
}

/// Pièce en cours de dessin.
struct P {
    xf: Xf,
    floor: i32,
    pad: [i32; 4],
    height: i32,
    voxels: Vec<(i32, i32, i32, Mat)>,
    piles: Vec<(i32, i32, i32)>,
}

impl P {
    /// Pièce sur la terre : sol au centre de `rect` (local), à sec et pas trop incliné.
    fn land(probe: &SiteProbe, xf: Xf, rect: [i32; 4], height: i32, dev: i32) -> Option<P> {
        let (cx, cz) = (
            (rect[0] + rect[2]).div_euclid(2),
            (rect[1] + rect[3]).div_euclid(2),
        );
        let (ax, az) = xf.map(cx, cz);
        let c = (probe.water)(ax, az);
        if c.is_wet() {
            return None;
        }
        let floor = c.top;
        let pts = [
            (rect[0], rect[1]),
            (rect[2], rect[1]),
            (rect[0], rect[3]),
            (rect[2], rect[3]),
            (cx, rect[1]),
            (cx, rect[3]),
            (rect[0], cz),
            (rect[2], cz),
        ];
        for (px, pz) in pts {
            let (wx, wz) = xf.map(px, pz);
            let w = (probe.water)(wx, wz);
            if w.is_wet() || (w.top - floor).abs() > dev {
                return None;
            }
        }
        let (ax0, az0) = xf.map(rect[0], rect[1]);
        let (ax1, az1) = xf.map(rect[2], rect[3]);
        Some(P {
            xf,
            floor,
            pad: [ax0.min(ax1), az0.min(az1), ax0.max(ax1), az0.max(az1)],
            height,
            voxels: Vec::new(),
            piles: Vec::new(),
        })
    }

    /// Pièce sans emprise, au-dessus de l'eau : le plancher est à fleur d'eau (`level - 1`).
    fn raw(xf: Xf, level: i32) -> P {
        P {
            xf,
            floor: level,
            pad: [1, 1, 0, 0],
            height: 0,
            voxels: Vec::new(),
            piles: Vec::new(),
        }
    }

    fn set(&mut self, x: i32, y: i32, z: i32, m: Mat) {
        let (wx, wz) = self.xf.map(x, z);
        self.voxels.push((wx, self.floor + y, wz, m));
    }

    /// Bloc à un `y` absolu.
    fn set_abs(&mut self, x: i32, y: i32, z: i32, m: Mat) {
        let (wx, wz) = self.xf.map(x, z);
        self.voxels.push((wx, y, wz, m));
    }

    #[allow(clippy::too_many_arguments)]
    fn fill(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, m: Mat) {
        for y in y0..=y1 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    self.set(x, y, z, m);
                }
            }
        }
    }

    /// Contour d'un rectangle, de `y0` à `y1`.
    #[allow(clippy::too_many_arguments)]
    fn ring(&mut self, x0: i32, z0: i32, x1: i32, z1: i32, y0: i32, y1: i32, m: Mat) {
        for y in y0..=y1 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    if x == x0 || x == x1 || z == z0 || z == z1 {
                        self.set(x, y, z, m);
                    }
                }
            }
        }
    }

    /// Pieu dont le haut est à `y` (relatif).
    fn pile(&mut self, x: i32, z: i32, y: i32) {
        let (wx, wz) = self.xf.map(x, z);
        self.piles.push((wx, wz, self.floor + y));
    }

    fn finish(self) -> Part {
        let mut bbox = if self.pad[0] <= self.pad[2] {
            self.pad
        } else {
            [i32::MAX, i32::MAX, i32::MIN, i32::MIN]
        };
        for &(x, _, z, _) in &self.voxels {
            bbox = [bbox[0].min(x), bbox[1].min(z), bbox[2].max(x), bbox[3].max(z)];
        }
        for &(x, z, _) in &self.piles {
            bbox = [bbox[0].min(x), bbox[1].min(z), bbox[2].max(x), bbox[3].max(z)];
        }
        Part {
            floor: self.floor,
            pad: self.pad,
            height: self.height,
            voxels: self.voxels,
            piles: self.piles,
            bbox,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Styles et éléments communs
// ---------------------------------------------------------------------------------------------

struct Style {
    wall: Mat,
    post: Mat,
    roof: Mat,
    gable: Mat,
    floor: Mat,
    glass: bool,
}

/// Ferme rouge de Falun, angles blancs, toit de tourbe.
const FALU: Style = Style {
    wall: Mat::Wall,
    post: Mat::Trim,
    roof: Mat::Turf,
    gable: Mat::Wall,
    floor: Mat::Plank,
    glass: true,
};
/// Rondins nus, toit de tourbe.
const LOGS: Style = Style {
    wall: Mat::Log,
    post: Mat::Log,
    roof: Mat::Turf,
    gable: Mat::Log,
    floor: Mat::Plank,
    glass: true,
};
/// Cabane goudronnée.
const TAR: Style = Style {
    wall: Mat::Tar,
    post: Mat::Tar,
    roof: Mat::Tar,
    gable: Mat::Tar,
    floor: Mat::Plank,
    glass: true,
};
/// Grange grise.
const BARN: Style = Style {
    wall: Mat::Grey,
    post: Mat::Log,
    roof: Mat::Grey,
    gable: Mat::Grey,
    floor: Mat::Hay,
    glass: false,
};
/// Refuge de montagne : rondins, toit goudronné.
const HUT: Style = Style {
    wall: Mat::Log,
    post: Mat::Log,
    roof: Mat::Tar,
    gable: Mat::Log,
    floor: Mat::Plank,
    glass: true,
};
/// Pierre, toit de tourbe (forge, laiterie).
const STONE: Style = Style {
    wall: Mat::Stone,
    post: Mat::Stone,
    roof: Mat::Turf,
    gable: Mat::Stone,
    floor: Mat::Stone,
    glass: false,
};
/// Moulin à pivot : bois blanchi, toit goudronné.
const MILL: Style = Style {
    wall: Mat::Trim,
    post: Mat::Trim,
    roof: Mat::Tar,
    gable: Mat::Trim,
    floor: Mat::Plank,
    glass: false,
};
/// Bâtiment de mine : bois gris, toit goudronné, sol de pierre.
const MINE: Style = Style {
    wall: Mat::Grey,
    post: Mat::Log,
    roof: Mat::Tar,
    gable: Mat::Grey,
    floor: Mat::Stone,
    glass: false,
};
/// Sauna : rondins, plancher de bouleau.
const SAUNA: Style = Style {
    wall: Mat::Log,
    post: Mat::Log,
    roof: Mat::Turf,
    gable: Mat::Log,
    floor: Mat::Birch,
    glass: false,
};

/// Toit à deux pans (faîte le long de `x`), pignons pleins, débord d'un bloc ; `y` est le
/// premier niveau du toit.
fn roof(p: &mut P, x0: i32, z0: i32, x1: i32, z1: i32, y: i32, s: &Style) {
    let mut k = 0;
    loop {
        let (lo, hi) = (z0 - 1 + k, z1 + 1 - k);
        if lo > hi {
            break;
        }
        for x in (x0 - 1)..=(x1 + 1) {
            for z in [lo, lo + 1, hi - 1, hi] {
                if z >= lo && z <= hi {
                    p.set(x, y + k, z, s.roof);
                }
            }
        }
        if z0 + k <= z1 - k {
            for z in (z0 + k)..=(z1 - k) {
                p.set(x0, y + k, z, s.gable);
                p.set(x1, y + k, z, s.gable);
            }
        }
        if lo + 1 >= hi - 1 {
            break;
        }
        k += 1;
    }
}

/// Cabane rectangulaire : plancher, murs de hauteur `h`, porte sur `+z`, fenêtres, toit.
#[allow(clippy::too_many_arguments)]
fn cabin(
    p: &mut P,
    x0: i32,
    z0: i32,
    x1: i32,
    z1: i32,
    h: i32,
    s: &Style,
    door_w: i32,
    door_h: i32,
) {
    p.fill(x0, -1, z0, x1, -1, z1, s.floor);
    p.ring(x0, z0, x1, z1, 0, h - 1, s.wall);
    if s.post != s.wall {
        for (x, z) in [(x0, z0), (x1, z0), (x0, z1), (x1, z1)] {
            p.fill(x, 0, z, x, h - 1, z, s.post);
        }
    }
    let xm = (x0 + x1).div_euclid(2);
    let hw = door_w / 2;
    p.fill(xm - hw, 0, z1, xm + hw, door_h - 1, z1, Mat::Air);
    if s.glass && h >= 3 {
        let zm = (z0 + z1).div_euclid(2);
        p.set(x0, 1, zm, Mat::Glass);
        p.set(x1, 1, zm, Mat::Glass);
        p.set(xm, 1, z0, Mat::Glass);
    }
    roof(p, x0, z0, x1, z1, h, s);
}

/// Poêle contre le coin `(x, z)` avec sa cheminée, qui dépasse du toit.
fn stove(p: &mut P, x: i32, z: i32, top: i32) {
    p.set(x, 0, z, Mat::Stove);
    p.fill(x, 1, z, x, top, z, Mat::Stone);
}

/// Table éclairée.
fn table(p: &mut P, x: i32, z: i32) {
    p.set(x, 0, z, Mat::Table);
    p.set(x, 1, z, Mat::Torch);
}

/// Pose une pièce de terre si elle tient, et la range.
fn land_part(
    probe: &SiteProbe,
    out: &mut Vec<Part>,
    xf: Xf,
    rect: [i32; 4],
    height: i32,
    dev: i32,
    draw: impl FnOnce(&mut P),
) -> bool {
    match P::land(probe, xf, rect, height, dev) {
        Some(mut p) => {
            draw(&mut p);
            out.push(p.finish());
            true
        }
        None => false,
    }
}

// ---------------------------------------------------------------------------------------------
// Les types
// ---------------------------------------------------------------------------------------------

/// Construit le bâtiment d'un site (`None` : type sans bâtiment, ou terrain impropre).
pub fn build(site: &StructSite, probe: &SiteProbe) -> Option<Building> {
    let mut parts = Vec::new();
    let root = Xf {
        x: site.x,
        z: site.z,
        rot: site.rot & 3,
    };
    let ok = match site.kind {
        StructKind::Torp => torp(probe, root, site, &mut parts),
        StructKind::By => by(probe, site, &mut parts),
        StructKind::Lada => lada(probe, root, &mut parts),
        StructKind::Fabod => fabod(probe, root, &mut parts),
        StructKind::Fiskelage => fiskelage(probe, site, &mut parts),
        StructKind::Bastu => bastu(probe, site, &mut parts),
        StructKind::Fjallstuga => fjallstuga(probe, root, &mut parts),
        StructKind::Vindskydd => vindskydd(probe, root, &mut parts),
        StructKind::Kata => kata(probe, site, &mut parts),
        StructKind::Runsten => runsten(probe, root, site, &mut parts),
        StructKind::Gravhog => gravhog(probe, root, &mut parts),
        StructKind::Skeppssattning => skeppssattning(probe, root, site, &mut parts),
        StructKind::Fornborg => fornborg(probe, root, site, &mut parts),
        StructKind::Stavkyrka => stavkyrka(probe, root, &mut parts),
        StructKind::Vaderkvarn => vaderkvarn(probe, root, &mut parts),
        StructKind::Fyr => fyr(probe, site, &mut parts),
        StructKind::Trojaborg => trojaborg(probe, root, &mut parts),
        StructKind::Hallristning => hallristning(probe, root, site, &mut parts),
        StructKind::Langhus => langhus(probe, root, site, &mut parts),
        StructKind::Skeppsvrak => skeppsvrak(probe, root, site, &mut parts),
        StructKind::Kolmila => kolmila(probe, root, site, &mut parts),
        StructKind::Tjardal => tjardal(probe, root, &mut parts),
        StructKind::Bjornide => bjornide(probe, root, site, &mut parts),
        StructKind::Geyser => geyser(probe, root, site, &mut parts),
        StructKind::Vardkase => vardkase(probe, root, &mut parts),
        StructKind::Baverdamm => baverdamm(probe, site, &mut parts),
        StructKind::Gruva => gruva(probe, root, &mut parts),
        StructKind::Hytta => hytta(probe, root, site, &mut parts),
        _ => return None,
    };
    if !ok || parts.is_empty() {
        return None;
    }
    let mut bbox = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for p in &parts {
        bbox = [
            bbox[0].min(p.bbox[0]),
            bbox[1].min(p.bbox[1]),
            bbox[2].max(p.bbox[2]),
            bbox[3].max(p.bbox[3]),
        ];
    }
    Some(Building { parts, bbox })
}

/// Maison paysanne : cabane rouge et son étable ; renvoie `false` si la maison ne tient pas.
fn farm(probe: &SiteProbe, xf: Xf, barn: bool, out: &mut Vec<Part>) -> bool {
    let house = land_part(probe, out, xf, [-5, -3, 3, 3], 8, 4, |p| {
        cabin(p, -4, -2, 2, 2, 3, &FALU, 1, 2);
        stove(p, -3, -1, 6);
        p.set(1, 0, -1, Mat::Bed);
        p.set(1, 0, 0, Mat::Bed);
        table(p, -1, -1);
    });
    if house && barn {
        land_part(probe, out, xf, [4, -4, 11, 2], 7, 4, |p| {
            cabin(p, 5, -3, 10, 1, 3, &BARN, 3, 3);
            p.fill(6, 0, -2, 9, 0, -2, Mat::Hay);
            p.fill(6, 1, -2, 7, 1, -2, Mat::Hay);
        });
    }
    house
}

fn torp(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let house = farm(probe, root, true, out);
    if !house {
        return false;
    }
    // Mât de la Saint-Jean.
    land_part(probe, out, root, [-9, 3, -7, 5], 8, 5, |p| {
        p.fill(-8, 0, 4, -8, 6, 4, Mat::Log);
        p.fill(-9, 5, 4, -7, 5, 4, Mat::Log);
        p.set(-9, 4, 4, Mat::Hay);
        p.set(-7, 4, 4, Mat::Hay);
    });
    // Entrée du cellier : un tertre de tourbe et sa porte de pierre.
    land_part(probe, out, root, [4, 3, 8, 7], 4, 5, |p| {
        p.fill(5, 0, 4, 7, 0, 6, Mat::Turf);
        p.set(6, 1, 5, Mat::Turf);
        p.fill(6, 0, 3, 6, 1, 3, Mat::Stone);
    });
    // Potager : planches de sentier entre des piquets.
    if hash01(site.seed, 1, 2, 3) < 0.9 {
        land_part(probe, out, root, [-5, 4, -1, 6], 3, 5, |p| {
            p.fill(-4, -1, 4, -2, -1, 5, Mat::Path);
            for (x, z) in [(-5, 4), (-1, 4), (-5, 6), (-1, 6)] {
                p.set(x, 0, z, Mat::Log);
            }
        });
    }
    true
}

/// Positions des fermes d'un village, dans l'ordre de remplissage.
const BY_SLOTS: [(i32, i32); 8] = [
    (0, -28),
    (0, 28),
    (28, 0),
    (-28, 0),
    (20, -20),
    (-20, 20),
    (20, 20),
    (-20, -20),
];

fn by(probe: &SiteProbe, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let root = Xf {
        x: site.x,
        z: site.z,
        rot: 0,
    };
    let n = 4 + (site.seed % 5) as usize;
    let mut houses = 0;
    for (i, &(dx, dz)) in BY_SLOTS.iter().take(n).enumerate() {
        let xf = root.child(dx, dz, face_center(dx, dz));
        let barn = hash01(site.seed, i as i32, 7, 7) < 0.5;
        if farm(probe, xf, barn, out) {
            houses += 1;
        }
    }
    if houses < 2 {
        return false;
    }
    // La place : sentier, puits au centre.
    land_part(probe, out, root, [-9, -9, 9, 9], 5, 6, |p| {
        p.fill(-8, -1, -8, 8, -1, 8, Mat::Path);
        p.ring(-1, -1, 1, 1, 0, 0, Mat::Stone);
        p.fill(0, -2, 0, 0, -1, 0, Mat::Water);
        for x in [-1, 1] {
            p.fill(x, 1, 0, x, 3, 0, Mat::Log);
        }
        p.fill(-1, 3, 0, 1, 3, 0, Mat::Plank);
    });
    // Forge.
    let xf = root.child(-15, -4, face_center(-15, -4));
    land_part(probe, out, xf, [-3, -3, 3, 3], 7, 4, |p| {
        cabin(p, -2, -2, 2, 2, 3, &STONE, 1, 2);
        p.set(-1, 0, -1, Mat::Hearth);
        p.set(0, 0, -1, Mat::Stove);
        p.fill(0, 1, -1, 0, 6, -1, Mat::Stone);
        table(p, 1, -1);
    });
    // Four à pain commun.
    let xf = root.child(15, 5, face_center(15, 5));
    land_part(probe, out, xf, [-3, -3, 3, 3], 6, 4, |p| {
        p.fill(-1, 0, -2, 1, 1, 1, Mat::Stone);
        p.fill(0, 0, 1, 0, 0, 1, Mat::Air);
        p.set(0, 0, 0, Mat::Hearth);
        p.set(0, 1, 0, Mat::Air);
        p.fill(-1, 2, -2, 1, 2, 1, Mat::Stone);
        p.fill(0, 3, -1, 0, 4, -1, Mat::Stone);
    });
    // Clocher de bois, séparé.
    let xf = root.child(4, -14, face_center(4, -14));
    land_part(probe, out, xf, [-3, -3, 3, 3], 13, 4, |p| {
        p.fill(-1, -1, -1, 1, -1, 1, Mat::Plank);
        p.ring(-1, -1, 1, 1, 0, 8, Mat::Log);
        p.fill(0, 0, 1, 0, 1, 1, Mat::Air);
        for (x, z) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            p.fill(x, 6, z, x, 7, z, Mat::Air);
        }
        p.set(0, 6, 0, Mat::Stone);
        p.fill(-2, 9, -2, 2, 9, 2, Mat::Tar);
        p.fill(-1, 10, -1, 1, 10, 1, Mat::Tar);
        p.set(0, 11, 0, Mat::Tar);
        p.set(0, 12, 0, Mat::Trim);
    });
    true
}

fn lada(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    land_part(probe, out, root, [-3, -4, 3, 4], 8, 3, |p| {
        cabin(p, -2, -3, 2, 3, 4, &BARN, 3, 3);
        p.fill(-1, 0, -2, 1, 0, -1, Mat::Hay);
        p.fill(-1, 1, -2, 0, 1, -2, Mat::Hay);
    })
}

fn fabod(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    let mut huts = 0;
    for (dx, dz) in [(-8, -5), (0, -7), (8, -4)] {
        let xf = root.child(dx, dz, 0);
        if land_part(probe, out, xf, [-3, -2, 3, 3], 6, 5, |p| {
            cabin(p, -2, -1, 2, 2, 2, &LOGS, 1, 2);
            p.set(-1, 0, 0, Mat::Bed);
            p.set(1, 0, 0, Mat::Hearth);
        }) {
            huts += 1;
        }
    }
    if huts == 0 {
        return false;
    }
    // Laiterie.
    land_part(probe, out, root.child(5, 6, 0), [-3, -3, 3, 3], 7, 5, |p| {
        cabin(p, -2, -2, 2, 2, 3, &STONE, 1, 2);
        p.fill(-1, 0, -1, 1, 0, -1, Mat::Birch);
    });
    // Enclos du bétail.
    land_part(probe, out, root.child(-8, 4, 0), [-6, -4, 6, 4], 4, 5, |p| {
        p.ring(-5, -3, 5, 3, 0, 0, Mat::Log);
        for (x, z) in [(-5, -3), (5, -3), (-5, 3), (5, 3), (0, -3), (0, 3)] {
            p.set(x, 1, z, Mat::Log);
        }
        p.set(5, 0, 0, Mat::Air);
    });
    true
}

/// Direction (rotation) et distance de l'eau la plus proche selon les quatre axes.
fn water_dir(
    probe: &SiteProbe,
    site: &StructSite,
    want: impl Fn(WaterKind) -> bool,
    reach: i32,
) -> Option<(u8, i32)> {
    let mut best: Option<(u8, i32)> = None;
    for (rot, (dx, dz)) in [(0u8, (0, 1)), (1, (-1, 0)), (2, (0, -1)), (3, (1, 0))] {
        for s in 1..=reach {
            let w = (probe.water)(site.x + dx * s, site.z + dz * s);
            if w.is_wet() && want(w.kind) {
                if best.is_none_or(|(_, d)| s < d) {
                    best = Some((rot, s));
                }
                break;
            }
        }
    }
    best
}

/// Ponton de deux blocs de large vers `+z` : planches à fleur d'eau, pieux tous les trois blocs.
/// Renvoie le niveau de l'eau au bout, si le ponton y arrive.
fn pier(probe: &SiteProbe, xf: Xf, from: i32, to: i32) -> (Part, Option<i32>) {
    let mut p = P::raw(xf, 0);
    let mut level = None;
    for z in from..=to {
        for x in 0..=1 {
            let (wx, wz) = xf.map(x, z);
            let w = (probe.water)(wx, wz);
            if w.is_wet() {
                p.set_abs(x, w.level - 1, z, Mat::Plank);
                if z % 3 == 0 {
                    p.piles.push((wx, wz, w.level - 2));
                }
                level = Some(w.level);
            } else {
                p.set_abs(x, w.top - 1, z, Mat::Plank);
            }
        }
    }
    (p.finish(), level)
}

fn fiskelage(probe: &SiteProbe, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let sea = water_dir(probe, site, |k| k == WaterKind::Sea, 14);
    let rot = sea.map_or(site.rot & 3, |(r, _)| r);
    let root = Xf {
        x: site.x,
        z: site.z,
        rot,
    };
    // Cabanes goudronnées (sjöbodar), porte vers l'eau.
    let mut huts = 0;
    for rect in [[-10, -7, -6, -4], [-3, -9, 1, -6], [5, -6, 9, -3]] {
        let pad = [rect[0] - 1, rect[1] - 1, rect[2] + 1, rect[3] + 1];
        if land_part(probe, out, root, pad, 7, 5, |p| {
            cabin(p, rect[0], rect[1], rect[2], rect[3], 3, &TAR, 1, 2);
            p.set(rect[0] + 1, 0, rect[1] + 1, Mat::Table);
            p.set(rect[2] - 1, 0, rect[1] + 1, Mat::Hay);
        }) {
            huts += 1;
        }
    }
    if huts == 0 {
        return false;
    }
    // Fumoir.
    land_part(probe, out, root, [-3, -4, 3, 1], 7, 5, |p| {
        cabin(p, -1, -3, 1, -1, 3, &STONE, 1, 2);
        p.set(0, 0, -3, Mat::Stove);
        p.fill(0, 1, -3, 0, 6, -3, Mat::Stone);
    });
    // Séchoir à poisson.
    land_part(probe, out, root, [5, 0, 11, 2], 4, 5, |p| {
        for x in [6, 10] {
            p.fill(x, 0, 1, x, 2, 1, Mat::Log);
        }
        p.fill(6, 2, 1, 10, 2, 1, Mat::Log);
        p.fill(7, 1, 1, 9, 1, 1, Mat::Hay);
    });
    // Barque tirée sur la grève.
    land_part(probe, out, root, [-7, 1, -2, 4], 3, 5, |p| {
        p.ring(-6, 2, -3, 3, 0, 0, Mat::Tar);
        p.set(-5, 0, 2, Mat::Plank);
    });
    // Ponton et cabane sur pilotis.
    if let Some((_, d)) = sea {
        let (deck, _) = pier(probe, root, 1, d + 3);
        out.push(deck);
        let zm = d + 5;
        let hut_xf = root.child(0, zm, 2);
        let cells = [(-1, zm - 1), (2, zm - 1), (-1, zm + 2), (2, zm + 2), (0, zm), (1, zm + 1)];
        let levels: Vec<_> = cells
            .iter()
            .map(|&(x, z)| {
                let (wx, wz) = root.map(x, z);
                let w = (probe.water)(wx, wz);
                (w.is_wet() && w.kind == WaterKind::Sea).then_some(w.level)
            })
            .collect();
        if let Some(Some(level)) = levels.first().copied() {
            if levels.iter().all(|l| *l == Some(level)) {
                let mut p = P::raw(hut_xf, level);
                cabin(&mut p, -2, -2, 1, 1, 3, &TAR, 1, 2);
                for (x, z) in [(-2, -2), (1, -2), (-2, 1), (1, 1)] {
                    p.pile(x, z, -2);
                }
                out.push(p.finish());
            }
        }
    }
    true
}

fn bastu(probe: &SiteProbe, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let water = water_dir(probe, site, |k| k != WaterKind::None, 9);
    let rot = water.map_or(site.rot & 3, |(r, _)| r);
    let root = Xf {
        x: site.x,
        z: site.z,
        rot,
    };
    let ok = land_part(probe, out, root, [-3, -5, 3, 0], 8, 4, |p| {
        cabin(p, -2, -4, 2, -1, 3, &SAUNA, 1, 2);
        p.set(-1, 0, -3, Mat::Stove);
        p.fill(-1, 1, -3, -1, 6, -3, Mat::Stone);
        p.fill(0, 0, -3, 1, 0, -3, Mat::Birch);
        p.fill(1, 1, -3, 1, 1, -3, Mat::Birch);
        p.set(1, 0, -2, Mat::Torch);
    });
    if ok {
        if let Some((_, d)) = water {
            let (deck, _) = pier(probe, root, 1, d + 3);
            out.push(deck);
        }
    }
    ok
}

fn fjallstuga(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    land_part(probe, out, root, [-3, -3, 6, 3], 7, 3, |p| {
        cabin(p, -2, -2, 2, 1, 3, &HUT, 1, 2);
        stove(p, -1, -1, 5);
        p.set(1, 0, -1, Mat::Bed);
        p.set(1, 1, -1, Mat::Bed);
        table(p, 0, -1);
        // Bois de chauffe contre le mur et fanal à la porte.
        p.fill(3, 0, -1, 4, 1, 0, Mat::Log);
        p.set(1, 0, 2, Mat::Torch);
    })
}

fn vindskydd(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    land_part(probe, out, root, [-3, -3, 3, 4], 6, 3, |p| {
        p.fill(-2, 0, -2, 2, 3, -2, Mat::Log);
        for (z, y) in [(-1, 3), (0, 2), (1, 2)] {
            p.fill(-2, y, z, 2, y, z, Mat::Plank);
        }
        p.fill(-2, 0, -1, -2, 2, -1, Mat::Log);
        p.fill(2, 0, -1, 2, 2, -1, Mat::Log);
        p.fill(-1, 0, -1, 1, 0, -1, Mat::Hay);
        p.ring(-1, 2, 1, 4, 0, 0, Mat::Stone);
        p.set(0, 0, 3, Mat::Hearth);
        p.fill(3, 0, 1, 3, 1, 2, Mat::Log);
    })
}

/// Hutte de tourbe ronde, porte sur `+z` (rayon 3, trou à fumée au sommet).
fn dome(p: &mut P) {
    for z in -3..=3 {
        for x in -3..=3 {
            let d2 = x * x + z * z;
            if d2 > 4 && d2 <= 10 {
                p.set(x, 0, z, Mat::Turf);
                p.set(x, 1, z, Mat::Turf);
            }
            if (2..=5).contains(&d2) {
                p.set(x, 2, z, Mat::Turf);
            }
            if d2 > 0 && d2 <= 2 {
                p.set(x, 3, z, Mat::Turf);
            }
        }
    }
    p.fill(0, 0, 3, 0, 1, 3, Mat::Air);
    p.set(0, 0, 0, Mat::Hearth);
    p.set(-1, 0, -1, Mat::Bed);
    p.set(-1, 0, 0, Mat::Bed);
}

fn kata(probe: &SiteProbe, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let root = Xf {
        x: site.x,
        z: site.z,
        rot: 0,
    };
    let n = 2 + (site.seed % 3) as usize;
    let mut huts = 0;
    for &(dx, dz) in [(-7, 1), (-2, -9), (6, -7), (-5, 8)].iter().take(n) {
        let xf = root.child(dx, dz, face_center(dx, dz));
        if land_part(probe, out, xf, [-4, -4, 4, 4], 6, 4, dome) {
            huts += 1;
        }
    }
    if huts == 0 {
        return false;
    }
    // Foyer commun entouré de sièges.
    land_part(probe, out, root, [-2, -2, 2, 2], 3, 5, |p| {
        p.fill(-1, 0, -1, 1, 0, 1, Mat::Stone);
        p.set(0, 0, 0, Mat::Hearth);
        for (x, z) in [(-2, 0), (2, 0), (0, -2), (0, 2)] {
            p.set(x, 0, z, Mat::Log);
        }
    });
    // Enclos à rennes.
    land_part(probe, out, root, [4, 3, 11, 10], 3, 5, |p| {
        p.ring(5, 4, 10, 9, 0, 0, Mat::Log);
        for (x, z) in [(5, 4), (10, 4), (5, 9), (10, 9)] {
            p.set(x, 1, z, Mat::Log);
        }
        p.set(5, 0, 6, Mat::Air);
    });
    // Njalla : garde-manger sur poteaux.
    land_part(probe, out, root.child(10, 0, 0), [-2, -2, 2, 2], 8, 5, |p| {
        for (x, z) in [(-1, -1), (1, -1), (-1, 1), (1, 1)] {
            p.fill(x, 0, z, x, 2, z, Mat::Log);
        }
        p.fill(-1, 3, -1, 1, 3, 1, Mat::Plank);
        p.ring(-1, -1, 1, 1, 4, 5, Mat::Plank);
        p.set(0, 4, 1, Mat::Air);
        p.fill(-2, 6, -2, 2, 6, 2, Mat::Plank);
    });
    true
}

// ---------------------------------------------------------------------------------------------
// Patrimoine (session 4.3)
// ---------------------------------------------------------------------------------------------

/// Racine entière.
fn isqrt(n: i32) -> i32 {
    let mut r = 0;
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

/// Pose, dalle par dalle (`step` blocs de côté), une forme qui épouse le terrain : chaque dalle a
/// son propre sol, et manque si elle est mouillée ou trop inclinée. Renvoie le nombre de dalles.
#[allow(clippy::too_many_arguments)]
fn tiles(
    probe: &SiteProbe,
    out: &mut Vec<Part>,
    xf: Xf,
    (lo, hi, step): (i32, i32, i32),
    height: i32,
    dev: i32,
    want: &dyn Fn(i32, i32) -> bool,
    cell: &dyn Fn(&mut P, i32, i32),
) -> usize {
    let mut n = 0;
    let mut tz = lo;
    while tz <= hi {
        let mut tx = lo;
        while tx <= hi {
            let rect = [tx, tz, tx + step - 1, tz + step - 1];
            let any = (rect[1]..=rect[3]).any(|z| (rect[0]..=rect[2]).any(|x| want(x, z)));
            if any
                && land_part(probe, out, xf, rect, height, dev, |p| {
                    for z in rect[1]..=rect[3] {
                        for x in rect[0]..=rect[2] {
                            if want(x, z) {
                                cell(p, x, z);
                            }
                        }
                    }
                })
            {
                n += 1;
            }
            tx += step;
        }
        tz += step;
    }
    n
}

/// Colonne isolée de hauteur `h`, posée sur son propre sol.
fn pillar(
    probe: &SiteProbe,
    out: &mut Vec<Part>,
    root: Xf,
    dx: i32,
    dz: i32,
    h: i32,
    m: Mat,
) -> bool {
    land_part(probe, out, root.child(dx, dz, 0), [-1, -1, 1, 1], h + 2, 3, |p| {
        p.fill(0, 0, 0, 0, h - 1, 0, m);
    })
}

/// Pierre runique : un menhir gravé, quelques pierres à son pied.
fn runsten(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let h = 3 + (site.seed % 2) as i32;
    land_part(probe, out, root, [-2, -2, 2, 2], h + 2, 2, |p| {
        p.set(0, 0, 0, Mat::Menhir);
        p.fill(0, 1, 0, 0, h - 1, 0, Mat::Rune);
        for (x, z) in [(1, 1), (-1, 1), (1, -1)] {
            if hash01(site.seed, x, z, 51) < 0.6 {
                p.set(x, 0, z, Mat::Stone);
            }
        }
    })
}

/// Tumulus : dôme de tourbe, chambre funéraire voûtée et couloir d'entrée vers `+z`.
fn gravhog(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    land_part(probe, out, root, [-8, -8, 8, 10], 9, 3, |p| {
        for z in -8..=8 {
            for x in -8..=8 {
                let d2 = x * x + z * z;
                if d2 >= 64 {
                    continue;
                }
                let h = (64 - d2) * 6 / 64;
                if h >= 1 {
                    p.fill(x, 0, z, x, h - 1, z, Mat::Turf);
                }
            }
        }
        p.fill(-2, 0, -2, 2, 2, 2, Mat::Chamber);
        p.fill(-1, 0, -1, 1, 1, 1, Mat::Air);
        for x in [-1, 1] {
            p.fill(x, 0, 3, x, 1, 8, Mat::Chamber);
        }
        p.fill(-1, 2, 3, 1, 2, 8, Mat::Chamber);
        p.fill(0, 0, 2, 0, 1, 8, Mat::Air);
        p.set(0, 0, -1, Mat::Treasure);
        p.set(-1, 0, 1, Mat::Torch);
        p.set(1, 0, 1, Mat::Torch);
        // Pierres de seuil gravées.
        p.set(-2, 0, 9, Mat::Rune);
        p.set(2, 0, 9, Mat::Rune);
    })
}

/// Navire de pierres : deux files de menhirs en coque, étrave et étambot plus hauts, mât au centre.
fn skeppssattning(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let mut n = 0;
    for z in (-12..=12).step_by(2) {
        let w = isqrt((144 - z * z) / 9);
        let end = z.abs() == 12;
        for side in [-1, 1] {
            if w == 0 && side == 1 {
                continue;
            }
            let x = side * w;
            if !end && hash01(site.seed, x, z, 52) < 0.08 {
                continue;
            }
            let h = if end {
                4
            } else {
                2 + (hash01(site.seed, x, z, 53) * 2.0) as i32
            };
            if pillar(probe, out, root, x, z, h, Mat::Menhir) {
                n += 1;
            }
        }
    }
    // Mât central et pierre de proue gravée : le cadran des solstices.
    pillar(probe, out, root, 0, 0, 3, Mat::Menhir);
    pillar(probe, out, root, 0, -8, 1, Mat::Rune);
    n >= 8
}

/// Enceinte circulaire de pierres sèches (ruinée), porte vers `+z`, puits et fondations de maisons.
fn fornborg(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let s = site.seed;
    let want = |x: i32, z: i32| {
        let d2 = x * x + z * z;
        (400..=506).contains(&d2) && !(z > 0 && x.abs() <= 2)
    };
    let cell = |p: &mut P, x: i32, z: i32| {
        // Poteaux de la porte.
        if z > 0 && x.abs() == 3 {
            p.fill(x, 0, z, x, 3, z, Mat::Log);
            return;
        }
        let u = hash01(s, x, z, 54);
        let h = if u < 0.08 {
            0
        } else if u < 0.4 {
            1
        } else if u < 0.85 {
            2
        } else {
            3
        };
        if h > 0 {
            let m = if hash01(s, x, z, 55) < 0.3 {
                Mat::Chamber
            } else {
                Mat::Stone
            };
            p.fill(x, 0, z, x, h - 1, z, m);
        }
    };
    let n = tiles(probe, out, root, (-24, 23, 4), 6, 3, &want, &cell);
    if n < 10 {
        return false;
    }
    // Puits au centre.
    land_part(probe, out, root, [-3, -3, 3, 3], 5, 4, |p| {
        p.ring(-1, -1, 1, 1, 0, 0, Mat::Stone);
        p.fill(0, -2, 0, 0, -1, 0, Mat::Water);
    });
    // Fondations de maisons, à demi arasées.
    for rect in [[-13, -11, -6, -5], [5, -12, 12, -6], [-12, 6, -5, 12]] {
        land_part(probe, out, root, rect, 4, 4, |p| {
            let (x0, z0, x1, z1) = (rect[0] + 1, rect[1] + 1, rect[2] - 1, rect[3] - 1);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    if x == x0 || x == x1 || z == z0 || z == z1 {
                        let u = hash01(s, x, z, 56);
                        if u > 0.3 {
                            p.fill(x, 0, z, x, if u > 0.75 { 1 } else { 0 }, z, Mat::Stone);
                        }
                    } else {
                        p.set(x, -1, z, Mat::Path);
                    }
                }
            }
        });
    }
    true
}

/// Église en bois debout : déambulatoire, nef haute, toits étagés, clocher à cloche, têtes de dragon.
fn stavkyrka(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    land_part(probe, out, root, [-5, -9, 5, 7], 21, 4, |p| {
        let tar = Mat::Tar;
        let log = Mat::Log;
        p.fill(-4, -1, -8, 4, -1, 6, Mat::Plank);
        p.ring(-4, -8, 4, 6, 0, 2, tar);
        for z in (-8..=6).step_by(2) {
            for x in [-4, 4] {
                p.fill(x, 0, z, x, 2, z, log);
            }
        }
        for x in (-4..=4).step_by(2) {
            for z in [-8, 6] {
                p.fill(x, 0, z, x, 2, z, log);
            }
        }
        for z in [-5, -1, 3] {
            p.set(-4, 1, z, Mat::Glass);
            p.set(4, 1, z, Mat::Glass);
        }
        // Toit bas, en gradins ; porche devant la porte.
        for k in 0..4 {
            p.ring(-5 + k, -9 + k, 5 - k, 7 - k, 3 + k, 3 + k, tar);
        }
        for x in [-1, 1] {
            p.fill(x, 0, 7, x, 2, 7, log);
        }
        // Nef haute sur poteaux d'angle.
        p.ring(-2, -5, 2, 3, 3, 6, tar);
        for (x, z) in [(-2, -5), (2, -5), (-2, 3), (2, 3), (-2, -1), (2, -1)] {
            p.fill(x, 0, z, x, 6, z, log);
        }
        for z in [-3, 1] {
            p.set(-2, 4, z, Mat::Glass);
            p.set(2, 4, z, Mat::Glass);
        }
        for k in 0..3 {
            p.fill(-3 + k, 7 + k, -6 + k, 3 - k, 7 + k, 4 - k, tar);
        }
        p.fill(0, 10, -3, 0, 10, 1, tar);
        // Clocher.
        p.ring(-1, -2, 1, 0, 8, 13, tar);
        for (x, z) in [(0, -2), (0, 0), (-1, -1), (1, -1)] {
            p.fill(x, 11, z, x, 12, z, Mat::Air);
        }
        p.set(0, 11, -1, Mat::Treasure);
        p.fill(-2, 14, -3, 2, 14, 1, tar);
        p.fill(-1, 15, -2, 1, 15, 0, tar);
        p.set(0, 16, -1, tar);
        p.fill(0, 17, -1, 0, 19, -1, Mat::Trim);
        // Têtes de dragon aux faîtes.
        for (z, y) in [(-3, 11), (1, 11), (-6, 8), (4, 8)] {
            p.fill(0, y, z, 0, y + 1, z, Mat::Trim);
        }
        // Porte, autel et fanaux.
        p.fill(0, 0, 6, 0, 1, 6, Mat::Air);
        table(p, 0, -6);
        for x in [-3, 3] {
            p.set(x, 0, -6, Mat::Torch);
        }
    })
}

/// Moulin à pivot : bâti sur un pied de bois, voiles en croix de Saint-André vers `+x`.
fn vaderkvarn(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    land_part(probe, out, root, [-3, -3, 5, 6], 13, 3, |p| {
        p.fill(0, 0, 0, 0, 2, 0, Mat::Log);
        p.fill(-2, 0, 0, 2, 0, 0, Mat::Log);
        p.fill(0, 0, -2, 0, 0, 2, Mat::Log);
        for (x, z) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            p.set(x, 1, z, Mat::Log);
        }
        p.fill(-2, 3, -1, 2, 3, 1, Mat::Plank);
        p.ring(-2, -1, 2, 1, 4, 6, Mat::Trim);
        roof(p, -2, -1, 2, 1, 7, &MILL);
        p.fill(0, 4, 1, 0, 5, 1, Mat::Air);
        p.set(-2, 5, 0, Mat::Glass);
        p.set(-1, 4, 0, Mat::Hay);
        p.set(-1, 4, -1, Mat::Hay);
        p.set(1, 4, -1, Mat::Table);
        // Escalier de planches jusqu'à la porte.
        for (i, z) in (2..=5).enumerate() {
            let y = 3 - i as i32;
            p.set(0, y, z, Mat::Plank);
            if y > 0 {
                p.fill(0, 0, z, 0, y - 1, z, Mat::Log);
            }
        }
        // Axe et voiles.
        p.set(2, 6, 0, Mat::Log);
        p.set(3, 6, 0, Mat::Log);
        for (sy, sz) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
            for k in 1..=4 {
                p.set(3, 6 + sy * k, sz * k, Mat::Log);
                if k >= 2 {
                    p.set(3, 6 + sy * k, sz * (k - 1), Mat::Sail);
                    p.set(3, 6 + sy * (k - 1), sz * k, Mat::Sail);
                }
            }
        }
    })
}

/// Cellule du mur de la tour du phare (octogone creux de rayon 2).
fn tower_wall(x: i32, z: i32) -> bool {
    (4..=5).contains(&(x * x + z * z))
}

/// Phare de pierre à bandes, escalier en colimaçon, lanterne à feu, maison du gardien.
fn fyr(probe: &SiteProbe, site: &StructSite, out: &mut Vec<Part>) -> bool {
    const SPIRAL: [(i32, i32); 8] = [
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
    ];
    let root = Xf {
        x: site.x,
        z: site.z,
        rot: 0,
    };
    let tower = land_part(probe, out, root, [-3, -3, 3, 3], 21, 6, |p| {
        p.fill(-2, -1, -2, 2, -1, 2, Mat::Stone);
        for z in -4..=4 {
            for x in -4..=4 {
                let d2 = x * x + z * z;
                if tower_wall(x, z) {
                    for y in 0..13 {
                        let m = if y < 3 {
                            Mat::Stone
                        } else if (y / 2) % 2 == 0 {
                            Mat::Trim
                        } else {
                            Mat::Wall
                        };
                        p.set(x, y, z, m);
                    }
                    // Lanterne vitrée.
                    p.fill(x, 14, z, x, 15, z, Mat::Glass);
                }
                // Galerie : plancher et garde-corps.
                if d2 <= 10 && !(x == 0 && z == -1) {
                    let m = if tower_wall(x, z) { Mat::Stone } else { Mat::Plank };
                    p.set(x, 13, z, m);
                }
                if (9..=10).contains(&d2) && !(z == -3 && x.abs() <= 1) {
                    p.set(x, 14, z, Mat::Log);
                }
            }
        }
        // Escalier en colimaçon dans le puits de la tour.
        for i in 0..13 {
            let (cx, cz) = SPIRAL[(i % 8) as usize];
            p.set(cx, i, cz, Mat::Plank);
        }
        p.fill(0, 0, 2, 0, 1, 2, Mat::Air);
        p.fill(0, 14, -2, 0, 15, -2, Mat::Air);
        p.fill(0, 14, 0, 0, 15, 0, Mat::Beacon);
        // Toit de la lanterne.
        for z in -3..=3 {
            for x in -3..=3 {
                let d2 = x * x + z * z;
                if d2 <= 6 {
                    p.set(x, 16, z, Mat::Tar);
                }
                if d2 <= 2 {
                    p.set(x, 17, z, Mat::Tar);
                }
            }
        }
        p.set(0, 18, 0, Mat::Trim);
    });
    if !tower {
        return false;
    }
    // Maison du gardien, porte vers la tour.
    let xf = root.child(9, 3, face_center(9, 3));
    land_part(probe, out, xf, [-4, -3, 4, 3], 7, 4, |p| {
        cabin(p, -3, -2, 3, 2, 3, &FALU, 1, 2);
        stove(p, -2, -1, 6);
        p.set(2, 0, -1, Mat::Bed);
        p.set(2, 0, 0, Mat::Bed);
        table(p, 0, -1);
    });
    true
}

/// Labyrinthe de pierres (trojaborg) : trois anneaux de galets posés à fleur de sol, aux ouvertures
/// alternées, et une pierre au centre.
fn trojaborg(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    let want = |x: i32, z: i32| {
        if x == 0 && z == 0 {
            return true;
        }
        let r = x.abs().max(z.abs());
        if !matches!(r, 2 | 4 | 6) {
            return false;
        }
        // Entrée en `+z` sur les anneaux 6 et 2, en `-z` sur l'anneau 4.
        let gap = x == 0 && ((r != 4 && z == r) || (r == 4 && z == -r));
        !gap
    };
    let cell = |p: &mut P, x: i32, z: i32| {
        if x == 0 && z == 0 {
            p.fill(0, 0, 0, 0, 1, 0, Mat::Menhir);
        } else {
            p.set(x, -1, z, Mat::Stone);
        }
    };
    tiles(probe, out, root, (-6, 6, 3), 4, 3, &want, &cell) >= 6
}

/// Roche gravée : une dalle d'affleurement, un navire et des cerfs peints en rouge.
fn hallristning(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    const ART: [&str; 7] = [
        "....#....",
        "...###...",
        "....#....",
        ".#######.",
        "..#####..",
        "#.#...#.#",
        ".#.......",
    ];
    land_part(probe, out, root, [-5, -4, 5, 4], 4, 2, |p| {
        for (j, row) in ART.iter().enumerate() {
            for (i, c) in row.bytes().enumerate() {
                let (x, z) = (i as i32 - 4, j as i32 - 3);
                if c == b'#' {
                    p.set(x, 0, z, Mat::Carved);
                } else {
                    p.set(x, 0, z, Mat::Menhir);
                    if hash01(site.seed, x, z, 57) > 0.82 {
                        p.set(x, 1, z, Mat::Menhir);
                    }
                }
            }
        }
    })
}

/// Halle viking en ruine : murs de tourbe ébréchés, poteaux, longue fosse à feu, banquettes, coffre.
fn langhus(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let s = site.seed;
    let half = |z: i32| -> i32 {
        match z.abs() {
            0..=6 => 3,
            7..=8 => 2,
            9 => 1,
            _ => -1,
        }
    };
    let inside = |x: i32, z: i32| {
        let w = half(z);
        w >= 0 && x.abs() <= w
    };
    land_part(probe, out, root, [-5, -11, 5, 11], 8, 4, |p| {
        for z in -9..=9 {
            for x in -3..=3 {
                if !inside(x, z) {
                    continue;
                }
                let edge = !(inside(x + 1, z)
                    && inside(x - 1, z)
                    && inside(x, z + 1)
                    && inside(x, z - 1));
                if edge {
                    if (x == 3 && (1..=2).contains(&z)) || hash01(s, x, z, 31) < 0.2 {
                        continue;
                    }
                    let h = 1 + (hash01(s, x, z, 32) * 3.0) as i32;
                    p.set(x, 0, z, Mat::Turf);
                    if h > 1 {
                        p.fill(x, 1, z, x, h - 1, z, Mat::Log);
                    }
                } else {
                    p.set(x, -1, z, Mat::Path);
                }
            }
        }
        for z in [-6, -2, 2, 6] {
            for x in [-1, 1] {
                if hash01(s, x, z, 33) > 0.2 {
                    let h = 3 + (hash01(s, x, z, 34) * 2.0) as i32;
                    p.fill(x, 0, z, x, h - 1, z, Mat::Log);
                }
            }
        }
        for z in -5_i32..=5 {
            for x in [-2, 2] {
                if z.abs() != 2 && hash01(s, x, z, 35) > 0.3 {
                    p.set(x, 0, z, Mat::Plank);
                }
            }
        }
        p.ring(-1, -1, 1, 1, 0, 0, Mat::Stone);
        p.set(0, 0, 0, Mat::Hearth);
        p.set(1, 0, -7, Mat::Treasure);
    })
}

/// Épave d'un navire de guerre : quille, membrures cassées, ponts épars, mâts rompus, coffre dans
/// la cale. Les cales reposent sur des étais qui descendent jusqu'au fond.
fn skeppsvrak(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let c = (probe.water)(site.x, site.z);
    if !c.is_wet() {
        return false;
    }
    let s = site.seed;
    let mut p = P::raw(root, c.top);
    let half = |z: i32| -> i32 {
        match z.abs() {
            0..=5 => 3,
            6..=8 => 2,
            _ => 1,
        }
    };
    for z in -10..=10 {
        let hw = half(z);
        for x in -hw..=hw {
            let edge = x.abs() == hw || z.abs() == 10;
            let u = hash01(s, x, z, 41);
            if edge {
                if u >= 0.15 {
                    let h = 1 + (hash01(s, x, z, 42) * 3.0) as i32;
                    p.fill(x, 0, z, x, h - 1, z, Mat::Tar);
                    if z % 3 == 0 {
                        p.pile(x, z, -1);
                    }
                }
            } else {
                if u < 0.9 {
                    p.set(x, 0, z, Mat::Tar);
                }
                if hash01(s, x, z, 43) < 0.2 {
                    p.set(x, 2, z, Mat::Plank);
                }
            }
            if x == 0 {
                p.set(0, 0, z, Mat::Log);
                if z % 2 == 0 {
                    p.pile(0, z, -1);
                }
            }
        }
    }
    // Étrave, château arrière, mâts rompus.
    p.fill(0, 1, -10, 0, 4, -10, Mat::Log);
    for x in -1..=1 {
        if hash01(s, x, 10, 45) > 0.2 {
            p.fill(x, 1, 10, x, 3, 10, Mat::Tar);
        }
        for z in 8..=9 {
            p.set(x, 2, z, Mat::Plank);
        }
    }
    for (z, base) in [(-3, 5), (3, 7)] {
        let h = base + (hash01(s, z, 0, 44) * 4.0) as i32;
        p.fill(0, 1, z, 0, h, z, Mat::Log);
        if h >= 7 {
            p.fill(-2, 6, z, 2, 6, z, Mat::Log);
        }
    }
    p.set(0, 1, 5, Mat::Treasure);
    out.push(p.finish());
    true
}

// ---------------------------------------------------------------------------------------------
// Industrie et nature (session 4.4)
// ---------------------------------------------------------------------------------------------

/// Meule de charbonnier éteinte : dôme de bûches sous les mottes, cratère de charbon au sommet,
/// charbon épars et pile de bûches à côté.
fn kolmila(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let s = site.seed;
    land_part(probe, out, root, [-5, -5, 5, 5], 7, 2, |p| {
        for z in -4..=4 {
            for x in -4..=4 {
                let d2 = x * x + z * z;
                if d2 <= 12 {
                    let h = 1 + (12 - d2) / 4;
                    for y in 0..h {
                        let top = y == h - 1;
                        let m = if top && d2 <= 1 {
                            Mat::Coal
                        } else if top || d2 > 6 {
                            Mat::Earth
                        } else {
                            Mat::Log
                        };
                        p.set(x, y, z, m);
                    }
                } else if d2 <= 24 && hash01(s, x, z, 91) < 0.35 {
                    p.set(x, 0, z, Mat::Coal);
                }
            }
        }
        p.fill(3, 0, -5, 5, 1, -4, Mat::Log);
    })
}

/// Four à goudron : talus de terre, entonnoir creusé dont le fond est en goudron, gouttière de
/// planches vers `+z`, tonneaux au bout et bûcher de souches.
fn tjardal(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    land_part(probe, out, root, [-6, -6, 6, 8], 7, 3, |p| {
        for z in -6..=6 {
            for x in -6..=6 {
                let d2 = x * x + z * z;
                if d2 <= 36 {
                    if d2 >= 9 {
                        p.fill(x, 0, z, x, (36 - d2) / 12, z, Mat::Earth);
                    } else {
                        let depth = 3 - d2 / 3;
                        p.fill(x, -depth, z, x, -1, z, Mat::Air);
                        p.set(x, -depth - 1, z, if d2 <= 2 { Mat::Tar } else { Mat::Earth });
                    }
                }
            }
        }
        // Gouttière : tranchée dans le talus, caniveau de planches.
        for z in 3..=6 {
            p.fill(0, 0, z, 0, 2, z, Mat::Air);
        }
        for z in 2..=7 {
            p.set(0, -1, z, Mat::Plank);
        }
        for x in [-1, 1] {
            p.fill(x, 0, 8, x, 1, 8, Mat::Tar);
        }
        p.fill(-6, 0, -2, -5, 1, 2, Mat::Log);
    })
}

/// Tanière d'ours : tertre de terre et de roche sous une grosse souche, chambre garnie de foin,
/// entrée basse vers `+z`.
fn bjornide(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let s = site.seed;
    land_part(probe, out, root, [-5, -5, 5, 6], 7, 3, |p| {
        for z in -5..=5 {
            for x in -5..=5 {
                let d2 = x * x + z * z;
                if d2 < 25 {
                    let h = 1 + (25 - d2) * 4 / 25;
                    let m = if d2 >= 16 || hash01(s, x, z, 61) < 0.25 {
                        Mat::Stone
                    } else {
                        Mat::Earth
                    };
                    p.fill(x, 0, z, x, h - 1, z, m);
                }
            }
        }
        // Chambre et entrée.
        p.fill(-1, 0, -1, 1, 1, 2, Mat::Air);
        p.fill(0, 0, 3, 0, 0, 5, Mat::Air);
        p.set(0, 1, 3, Mat::Air);
        p.fill(-1, -1, -1, 1, -1, 1, Mat::Hay);
        // Souche et racines.
        p.fill(-3, 0, -3, -3, 2, -3, Mat::Log);
        p.set(-2, 0, -3, Mat::Log);
        p.set(-3, 0, -2, Mat::Log);
    })
}

/// Geyser : terrasse de sinter en cône dont le sommet est un bassin d'eau sur un évent de magma,
/// bain chaud au bord, orgues basaltiques en couronne.
fn geyser(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let s = site.seed;
    land_part(probe, out, root, [-7, -7, 7, 7], 9, 3, |p| {
        for z in -7..=7 {
            for x in -7..=7 {
                let d2 = x * x + z * z;
                if d2 <= 42 && (d2 <= 9 || hash01(s, x, z, 71) < 0.88) {
                    let h = if d2 <= 9 { 1 + (9 - d2) / 2 } else { 1 };
                    p.fill(x, 0, z, x, h - 1, z, Mat::Sinter);
                }
                if (30..=42).contains(&d2) && hash01(s, x, z, 72) < 0.14 {
                    let h = 2 + (hash01(s, x, z, 73) * 4.0) as i32;
                    p.fill(x, 0, z, x, h - 1, z, Mat::Basalt);
                }
            }
        }
        // Évent : magma, puis bassin au sommet du cône.
        p.fill(0, -3, 0, 0, 2, 0, Mat::Hot);
        p.fill(0, 3, 0, 0, 4, 0, Mat::Water);
        // Bain chaud : creusé dans la terrasse, fond de magma.
        for z in 0..=6 {
            for x in -7..=-1 {
                let pd = (x + 4) * (x + 4) + (z - 3) * (z - 3);
                if pd <= 4 {
                    p.set(x, 0, z, Mat::Air);
                    p.fill(x, -2, z, x, -1, z, Mat::Water);
                    p.set(x, -3, z, Mat::Hot);
                } else if pd <= 8 {
                    // Margelle étanche, quel que soit le sol naturel.
                    p.fill(x, -2, z, x, -1, z, Mat::Sinter);
                }
            }
        }
    })
}

/// Feu d'alarme : socle de pierres, trépied de rondins, bûcher de rondins et de foin, baril de
/// goudron au sommet ; un foyer de veille au pied.
fn vardkase(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    land_part(probe, out, root, [-5, -5, 5, 5], 14, 5, |p| {
        for z in -4..=4 {
            for x in -4..=4 {
                if x * x + z * z <= 14 {
                    p.set(x, 0, z, Mat::Stone);
                }
            }
        }
        for (bx, bz) in [(-3, -2), (3, -2), (0, 3)] {
            for t in 0..=9 {
                p.set(bx * (9 - t) / 9, 1 + t, bz * (9 - t) / 9, Mat::Log);
            }
        }
        p.fill(-1, 7, -1, 1, 8, 1, Mat::Log);
        p.fill(-1, 9, -1, 1, 9, 1, Mat::Hay);
        p.fill(0, 10, 0, 0, 11, 0, Mat::Tar);
        p.set(3, 1, 3, Mat::Hearth);
        p.set(2, 1, 3, Mat::Torch);
    })
}

/// Barrage de castors : hutte de branches près de l'eau et digue de rondins et de boue en travers
/// de l'eau, à un bloc au-dessus de la surface.
fn baverdamm(probe: &SiteProbe, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let Some((rot, d)) = water_dir(probe, site, |k| k != WaterKind::None, 8) else {
        return false;
    };
    let root = Xf {
        x: site.x,
        z: site.z,
        rot,
    };
    let s = site.seed;
    let lodge = land_part(probe, out, root.child(0, -3, 0), [-3, -3, 3, 3], 6, 3, |p| {
        for z in -3..=3 {
            for x in -3..=3 {
                let d2 = x * x + z * z;
                if d2 <= 9 {
                    let m = if hash01(s, x, z, 81) < 0.3 {
                        Mat::Earth
                    } else {
                        Mat::Log
                    };
                    p.fill(x, 0, z, x, (9 - d2) / 3, z, m);
                }
            }
        }
        p.fill(0, 0, 0, 0, 1, 0, Mat::Air);
        p.set(0, 0, 1, Mat::Air);
    });
    if !lodge {
        return false;
    }
    let mut dam = P::raw(root, 0);
    let mut cells = 0;
    for x in -7..=7 {
        let (wx, wz) = root.map(x, d);
        let w = (probe.water)(wx, wz);
        if !w.is_wet() || w.level - w.top > 6 {
            continue;
        }
        for z in d..=d + 1 {
            for y in w.top..=w.level {
                let m = if hash01(s, x * 3 + z, y, 82) < 0.3 {
                    Mat::Earth
                } else {
                    Mat::Log
                };
                dam.set_abs(x, y, z, m);
            }
        }
        if hash01(s, x, 0, 83) < 0.3 {
            dam.set_abs(x, w.level + 1, d, Mat::Log);
        }
        cells += 1;
    }
    if cells >= 5 {
        out.push(dam.finish());
    }
    true
}

/// Pose le chevalement d'une mine : deux portiques en A de rondins, poutre sommitale et poulie.
fn headframe(p: &mut P) {
    for z in [-1, 1] {
        for t in 0..=9 {
            let x = 3 - (2 * t) / 9;
            p.set(x, t, z, Mat::Log);
            p.set(-x, t, z, Mat::Log);
        }
        p.fill(-3, 4, z, 3, 4, z, Mat::Plank);
    }
    p.fill(-1, 9, -1, 1, 9, 1, Mat::Log);
    p.set(1, 8, 0, Mat::Grey);
    p.set(-1, 8, 0, Mat::Grey);
    p.set(0, 7, 0, Mat::Grey);
}

/// Mine abandonnée : chevalement, puits boisé de plus de dix blocs avec sa mare au fond, deux
/// galeries étayées avec rails et wagonnets, bâtiment des machines et halde de stériles.
fn gruva(probe: &SiteProbe, root: Xf, out: &mut Vec<Part>) -> bool {
    let main = land_part(probe, out, root, [-4, -4, 4, 4], 13, 3, |p| {
        for z in -2i32..=2 {
            for x in -2i32..=2 {
                if x.abs() == 2 || z.abs() == 2 {
                    p.set(x, 0, z, Mat::Stone);
                }
            }
        }
        headframe(p);
        let depth = (p.floor - 10).clamp(0, 14);
        if depth < 8 {
            return;
        }
        let y0 = -depth;
        // Cuvelage du puits, galeries, puis puits et mare.
        p.ring(-2, -2, 2, 2, y0 - 3, -1, Mat::Plank);
        for (x, z) in [(-2, -2), (2, -2), (-2, 2), (2, 2)] {
            p.fill(x, y0 - 3, z, x, -1, z, Mat::Log);
        }
        p.fill(2, y0, -1, 14, y0 + 2, 1, Mat::Air);
        p.fill(-1, y0, -12, 1, y0 + 2, -2, Mat::Air);
        p.fill(-1, y0 - 3, -1, 1, -1, 1, Mat::Air);
        p.fill(-1, y0 - 3, -1, 1, y0 - 1, 1, Mat::Water);
        // Étais, rails et wagonnets.
        for x in [4, 8, 12] {
            for z in [-1, 1] {
                p.fill(x, y0, z, x, y0 + 1, z, Mat::Log);
            }
            p.fill(x, y0 + 2, -1, x, y0 + 2, 1, Mat::Log);
        }
        for z in [-4, -8, -12] {
            for x in [-1, 1] {
                p.fill(x, y0, z, x, y0 + 1, z, Mat::Log);
            }
            p.fill(-1, y0 + 2, z, 1, y0 + 2, z, Mat::Log);
        }
        for x in 2..=14 {
            p.set(x, y0 - 1, 0, Mat::Plank);
        }
        for z in -12..=-2 {
            p.set(0, y0 - 1, z, Mat::Plank);
        }
        p.fill(6, y0, 0, 7, y0, 0, Mat::Tar);
        p.set(0, y0, -7, Mat::Tar);
        // Bout de galerie noyé.
        p.fill(11, y0, -1, 11, y0 + 1, 1, Mat::Plank);
        p.fill(12, y0, -1, 14, y0, 1, Mat::Water);
    });
    if !main {
        return false;
    }
    // Bâtiment des machines.
    land_part(probe, out, root, [5, -4, 12, 3], 9, 4, |p| {
        cabin(p, 6, -3, 11, 2, 4, &MINE, 2, 3);
        p.fill(8, 0, -2, 9, 1, -1, Mat::Stone);
        p.set(7, 0, 1, Mat::Torch);
    });
    // Halde de stériles.
    land_part(probe, out, root, [-14, -9, -5, 0], 8, 4, |p| {
        for z in -9..=0 {
            for x in -14..=-5 {
                let d2 = (x + 9) * (x + 9) + (z + 4) * (z + 4);
                if d2 <= 25 {
                    p.fill(x, 0, z, x, (25 - d2) / 6, z, Mat::Stone);
                }
            }
        }
    });
    true
}

/// Haut fourneau en ruine : cuve de pierre éventrée au sommet, arche de chargement, roue à eau
/// de rondins avec son bief, halle à charbon et tas de scories bleues.
fn hytta(probe: &SiteProbe, root: Xf, site: &StructSite, out: &mut Vec<Part>) -> bool {
    let s = site.seed;
    let main = land_part(probe, out, root, [-4, -4, 7, 4], 13, 4, |p| {
        for y in 0..=11 {
            let half: i32 = if y <= 4 { 3 } else { 2 };
            for z in -half..=half {
                for x in -half..=half {
                    if x.abs() != half && z.abs() != half {
                        continue;
                    }
                    if y >= 6 && hash01(s, x + 31 * y, z, 85) < 0.14 * (y - 5) as f64 {
                        continue;
                    }
                    p.set(x, y, z, Mat::Stone);
                }
            }
        }
        // Sole de charbon et d'écume, arche de chargement.
        p.fill(-2, -1, -2, 2, -1, 2, Mat::Coal);
        p.fill(-1, 0, 3, 1, 2, 3, Mat::Air);
        p.fill(-1, 0, -1, 1, 0, 0, Mat::Slag);
        // Roue à eau : jante, rayons, moyeu, axe, bief sur pilotis.
        for z in -3..=3 {
            for y in 0..=6 {
                let d2 = z * z + (y - 3) * (y - 3);
                if (5..=10).contains(&d2) {
                    p.set(6, y, z, Mat::Log);
                }
            }
        }
        p.fill(6, 3, -2, 6, 3, 2, Mat::Plank);
        p.fill(6, 1, 0, 6, 5, 0, Mat::Plank);
        p.set(6, 3, 0, Mat::Log);
        p.fill(4, 3, 0, 5, 3, 0, Mat::Log);
        p.fill(6, 7, -4, 6, 7, 0, Mat::Plank);
        p.fill(6, 0, -4, 6, 6, -4, Mat::Log);
    });
    if !main {
        return false;
    }
    // Halle à charbon : poteaux, toit de planches en deux niveaux, tas de charbon.
    land_part(probe, out, root, [-14, -5, -6, 5], 8, 4, |p| {
        for x in [-13, -7] {
            for z in [-4, 0, 4] {
                p.fill(x, 0, z, x, 4, z, Mat::Log);
            }
        }
        p.fill(-14, 5, -5, -6, 5, 5, Mat::Plank);
        p.fill(-13, 6, -3, -7, 6, 3, Mat::Plank);
        for z in -3..=3 {
            for x in -12..=-8 {
                if hash01(s, x, z, 86) < 0.8 {
                    let h = 1 + (hash01(s, x, z, 87) * 3.0) as i32;
                    p.fill(x, 0, z, x, h - 1, z, Mat::Coal);
                }
            }
        }
    });
    // Tas de scories bleues.
    land_part(probe, out, root, [-1, -12, 7, -6], 7, 4, |p| {
        for z in -12..=-6 {
            for x in -1..=7 {
                let d2 = (x - 3) * (x - 3) + (z + 9) * (z + 9);
                if d2 <= 16 {
                    let m = if hash01(s, x, z, 88) < 0.15 {
                        Mat::Stone
                    } else {
                        Mat::Slag
                    };
                    p.fill(x, 0, z, x, 1 + (16 - d2) / 4, z, m);
                }
            }
        }
    });
    true
}
