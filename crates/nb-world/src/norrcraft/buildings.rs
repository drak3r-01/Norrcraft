//! Bâtiments d'habitat Norrcraft (session 4.2 ; GDD doc 02 §10.1) : torp, by, lada, fäbod,
//! fiskeläge, bastu, fjällstuga, vindskydd et kåta.
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
}

impl Mat {
    pub const COUNT: usize = 19;

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
                if best.map_or(true, |(_, d)| s < d) {
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
