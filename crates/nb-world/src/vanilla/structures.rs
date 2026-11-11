//! Structures du générateur « Norrblock » (phase 9) : hameaux, cabanes, moulins, tours,
//! temples, ruines, épaves, phares, cercles de pierres, fossiles…
//!
//! Même principe que les arbres : une structure est une fonction pure de
//! `(graine, configuration, cellule)`. Le monde est découpé en cellules de [`CELL`] blocs ; chaque
//! cellule peut porter au plus une structure, dont la position, le type et la forme sortent d'un
//! hachage de la cellule. Chaque chunk reconstruit **en entier** les structures des cellules
//! voisines qui le touchent et n'en écrit que la part qui tombe chez lui : le résultat ne dépend
//! donc ni de l'ordre de génération ni du contenu des chunks voisins. Les fondations
//! (piliers jusqu'au sol) et le dégagement (arbres, relief) sont recalculés à partir du seul
//! terrain.

use nb_data::{BlockId, BlockIds};

use crate::biome::Biome;
use crate::chunk::{Chunk, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};
use crate::generator::WorldGenerator;
use crate::generator_nb::{Painter, Rng};
use crate::noise::hash_position;
use crate::terrain::{NbColumn, Terrain};

const SX: i32 = CHUNK_SIZE_X as i32;
const SY: i32 = CHUNK_SIZE_Y as i32;
const SZ: i32 = CHUNK_SIZE_Z as i32;

/// Côté (en blocs) d'une cellule de structure.
pub const CELL: i32 = 64;
/// Rayon maximal d'une structure : un chunk examine les cellules dans ce rayon autour de lui.
const MAX_RADIUS: i32 = 20;
/// Probabilité qu'une cellule porte une structure (avant `structure_density`).
const CELL_CHANCE: f64 = 0.55;

#[inline]
fn unit(h: i32) -> f64 {
    f64::from(h % 10000) / 10000.0
}

/// Types de structures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructureKind {
    Hamlet,
    Cabin,
    Windmill,
    Watchtower,
    RuinedTower,
    DesertPyramid,
    JungleTemple,
    Camp,
    IceHall,
    StoneCircle,
    Lighthouse,
    Shipwreck,
    OceanRuin,
    WitchHut,
    CherryShrine,
    Obelisk,
    VolcanoForge,
    Fossil,
    MushroomHouse,
}

impl StructureKind {
    pub const ALL: [StructureKind; 19] = [
        StructureKind::Hamlet,
        StructureKind::Cabin,
        StructureKind::Windmill,
        StructureKind::Watchtower,
        StructureKind::RuinedTower,
        StructureKind::DesertPyramid,
        StructureKind::JungleTemple,
        StructureKind::Camp,
        StructureKind::IceHall,
        StructureKind::StoneCircle,
        StructureKind::Lighthouse,
        StructureKind::Shipwreck,
        StructureKind::OceanRuin,
        StructureKind::WitchHut,
        StructureKind::CherryShrine,
        StructureKind::Obelisk,
        StructureKind::VolcanoForge,
        StructureKind::Fossil,
        StructureKind::MushroomHouse,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            StructureKind::Hamlet => "hameau",
            StructureKind::Cabin => "cabane",
            StructureKind::Windmill => "moulin",
            StructureKind::Watchtower => "tour de guet",
            StructureKind::RuinedTower => "tour en ruine",
            StructureKind::DesertPyramid => "pyramide du désert",
            StructureKind::JungleTemple => "temple de la jungle",
            StructureKind::Camp => "campement",
            StructureKind::IceHall => "igloo",
            StructureKind::StoneCircle => "cercle de pierres",
            StructureKind::Lighthouse => "phare",
            StructureKind::Shipwreck => "épave",
            StructureKind::OceanRuin => "ruines sous-marines",
            StructureKind::WitchHut => "hutte de sorcière",
            StructureKind::CherryShrine => "sanctuaire des cerisiers",
            StructureKind::Obelisk => "obélisque",
            StructureKind::VolcanoForge => "forge du volcan",
            StructureKind::Fossil => "fossile géant",
            StructureKind::MushroomHouse => "maison champignon",
        }
    }

    /// Rayon de l'empreinte au sol (≤ [`MAX_RADIUS`]).
    pub const fn radius(self) -> i32 {
        match self {
            StructureKind::Hamlet => 17,
            StructureKind::DesertPyramid | StructureKind::JungleTemple => 11,
            StructureKind::Shipwreck => 10,
            StructureKind::Fossil => 9,
            StructureKind::OceanRuin | StructureKind::StoneCircle => 8,
            StructureKind::Camp | StructureKind::IceHall | StructureKind::CherryShrine => 7,
            _ => 6,
        }
    }

    fn underwater(self) -> bool {
        matches!(self, StructureKind::Shipwreck | StructureKind::OceanRuin)
    }

    /// Dénivelé toléré sous l'empreinte (le reste est comblé / dégagé).
    fn tolerance(self) -> i32 {
        match self {
            StructureKind::Hamlet => 4,
            StructureKind::DesertPyramid | StructureKind::JungleTemple => 6,
            StructureKind::Fossil | StructureKind::StoneCircle | StructureKind::Camp => 5,
            StructureKind::Obelisk | StructureKind::Lighthouse | StructureKind::WitchHut => 7,
            _ => 5,
        }
    }
}

/// Emplacement d'une structure (pour les tests et les outils).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructureSite {
    pub kind: StructureKind,
    pub x: i32,
    pub z: i32,
    /// Premier bloc libre au-dessus du sol (niveau du sol de la structure).
    pub base_y: i32,
    pub variant: u32,
}

fn weights(b: Biome) -> &'static [(StructureKind, u32)] {
    use StructureKind::*;
    match b {
        Biome::Plains => &[
            (Hamlet, 5),
            (Cabin, 2),
            (Windmill, 3),
            (Watchtower, 2),
            (StoneCircle, 2),
            (Camp, 2),
            (RuinedTower, 2),
            (Fossil, 1),
        ],
        Biome::FlowerMeadow => &[(Hamlet, 3), (Windmill, 4), (Camp, 2), (StoneCircle, 1)],
        Biome::Forest => &[
            (Cabin, 5),
            (Hamlet, 2),
            (Camp, 3),
            (RuinedTower, 3),
            (Watchtower, 2),
        ],
        Biome::BirchForest => &[(Cabin, 4), (Hamlet, 2), (Camp, 2), (RuinedTower, 2)],
        Biome::DarkForest => &[(Cabin, 3), (RuinedTower, 4), (Watchtower, 1), (Camp, 1)],
        Biome::AutumnForest => &[(Hamlet, 3), (Cabin, 3), (Camp, 2), (Windmill, 1)],
        Biome::CherryGrove => &[(CherryShrine, 6), (Cabin, 1), (Hamlet, 1)],
        Biome::Taiga => &[
            (Cabin, 5),
            (Camp, 3),
            (Watchtower, 2),
            (RuinedTower, 2),
            (Hamlet, 1),
        ],
        Biome::SnowyTaiga => &[(Cabin, 4), (Camp, 2), (IceHall, 2), (RuinedTower, 1)],
        Biome::MegaTaiga => &[(Cabin, 3), (Camp, 2), (RuinedTower, 2), (StoneCircle, 1)],
        Biome::SnowyPlains => &[(IceHall, 5), (Camp, 2), (Cabin, 2), (Obelisk, 1)],
        Biome::IceSpikes => &[(Obelisk, 3), (IceHall, 2)],
        Biome::Heath => &[
            (StoneCircle, 5),
            (Cabin, 2),
            (RuinedTower, 3),
            (Camp, 1),
            (Obelisk, 1),
        ],
        Biome::Hills => &[
            (Watchtower, 4),
            (StoneCircle, 3),
            (RuinedTower, 3),
            (Obelisk, 2),
            (Camp, 2),
            (Cabin, 1),
        ],
        Biome::Mountains => &[(Obelisk, 3), (Watchtower, 3), (RuinedTower, 2), (Camp, 1)],
        Biome::SnowyPeaks => &[(Obelisk, 3), (IceHall, 1), (Camp, 1)],
        Biome::Savanna => &[
            (Hamlet, 5),
            (Camp, 3),
            (Fossil, 2),
            (Watchtower, 1),
            (Windmill, 1),
        ],
        Biome::Desert => &[
            (DesertPyramid, 4),
            (Camp, 4),
            (Fossil, 3),
            (Obelisk, 2),
            (Hamlet, 2),
        ],
        Biome::Badlands => &[
            (Fossil, 4),
            (Obelisk, 3),
            (Camp, 3),
            (DesertPyramid, 2),
            (Hamlet, 1),
        ],
        Biome::Jungle => &[(JungleTemple, 5), (RuinedTower, 3), (Camp, 2), (Cabin, 1)],
        Biome::Swamp => &[(WitchHut, 6), (RuinedTower, 3), (Cabin, 1)],
        Biome::Volcano => &[(VolcanoForge, 5), (Obelisk, 2)],
        Biome::Beach => &[(Lighthouse, 5), (Camp, 3)],
        Biome::StonyShore => &[(Lighthouse, 5), (RuinedTower, 2), (Camp, 1)],
        Biome::WarmOcean => &[(OceanRuin, 4), (Shipwreck, 2)],
        Biome::Ocean | Biome::DeepOcean | Biome::FrozenOcean => &[(Shipwreck, 3), (OceanRuin, 3)],
        Biome::MushroomFields => &[(MushroomHouse, 6)],
        Biome::River => &[],
    }
}

// =============================================================================================
// Constructeur : repère local tournable, écriture de blocs, fondations.
// =============================================================================================

#[derive(Clone, Copy)]
struct Style {
    plank: BlockId,
    log: BlockId,
    roof: BlockId,
    base: BlockId,
    glass: BlockId,
}

struct Build<'a, 'b> {
    p: &'a mut Painter<'b>,
    ids: BlockIds,
    terrain: &'a Terrain,
    sea: i32,
    /// Pile de repères `(x, z, rotation)` : le dernier est le plus interne.
    frames: Vec<(i32, i32, u8)>,
    oy: i32,
    rng: Rng,
}

#[inline]
fn rot(x: i32, z: i32, r: u8) -> (i32, i32) {
    match r & 3 {
        0 => (x, z),
        1 => (-z, x),
        2 => (-x, -z),
        _ => (z, -x),
    }
}

impl Build<'_, '_> {
    /// Coordonnées monde d'un point local `(x, z)`.
    fn map(&self, x: i32, z: i32) -> (i32, i32) {
        let (mut x, mut z) = (x, z);
        for &(fx, fz, r) in self.frames.iter().rev() {
            let (rx, rz) = rot(x, z, r);
            x = rx + fx;
            z = rz + fz;
        }
        (x, z)
    }

    fn push(&mut self, x: i32, z: i32, r: u8) {
        self.frames.push((x, z, r));
    }

    fn pop(&mut self) {
        self.frames.pop();
    }

    /// Place `b` en `(x, y, z)` local (`y` relatif au niveau du sol de la structure).
    fn set(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        let (wx, wz) = self.map(x, z);
        self.p.force(wx, self.oy + y, wz, b);
    }

    /// Comme [`set`] mais sans écraser un bloc plein existant.
    fn set_soft(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        let (wx, wz) = self.map(x, z);
        if self.p.get(wx, self.oy + y, wz) == self.ids.air {
            self.p.force(wx, self.oy + y, wz, b);
        }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        let (wx, wz) = self.map(x, z);
        self.p.get(wx, self.oy + y, wz)
    }

    fn fill(&mut self, a: (i32, i32, i32), b: (i32, i32, i32), blk: BlockId) {
        for y in a.1.min(b.1)..=a.1.max(b.1) {
            for z in a.2.min(b.2)..=a.2.max(b.2) {
                for x in a.0.min(b.0)..=a.0.max(b.0) {
                    self.set(x, y, z, blk);
                }
            }
        }
    }

    fn clear(&mut self, a: (i32, i32, i32), b: (i32, i32, i32)) {
        let air = self.ids.air;
        self.fill(a, b, air);
    }

    /// Parois verticales d'un rectangle (sans sol ni toit), de `y0` à `y1`.
    #[allow(clippy::too_many_arguments)]
    fn walls(&mut self, x0: i32, z0: i32, x1: i32, z1: i32, y0: i32, y1: i32, blk: BlockId) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, z0, blk);
                self.set(x, y, z1, blk);
            }
            for z in z0..=z1 {
                self.set(x0, y, z, blk);
                self.set(x1, y, z, blk);
            }
        }
    }

    /// Retire la végétation (arbres, plantes) dans un disque autour de la structure, de
    /// `oy − 4` à `oy + 42` : sans cela les canopées recouvrent les bâtiments.
    fn strip(&mut self, r: i32) {
        let (cx, cz) = (self.frames[0].0, self.frames[0].1);
        let air = self.ids.air;
        for wz in cz - r..=cz + r {
            for wx in cx - r..=cx + r {
                if (wx - cx) * (wx - cx) + (wz - cz) * (wz - cz) > r * r || !self.p.covers(wx, wz) {
                    continue;
                }
                for y in self.oy - 4..self.oy + 42 {
                    if self.p.is_foliage(self.p.get(wx, y, wz)) {
                        self.p.force(wx, y, wz, air);
                    }
                }
            }
        }
    }

    /// Comble sous `(x, z)` jusqu'au sol (au plus 14 blocs) avec `blk`, de `y_top` vers le bas.
    fn pillar(&mut self, x: i32, z: i32, y_top: i32, blk: BlockId) {
        let (wx, wz) = self.map(x, z);
        if !self.p.covers(wx, wz) {
            return;
        }
        let g = self.terrain.column(wx, wz).height;
        let mut y = self.oy + y_top;
        let stop = (g - 2).max(self.oy + y_top - 14);
        while y >= stop {
            self.p.force(wx, y, wz, blk);
            y -= 1;
        }
    }

    /// Fondation d'un rectangle : comble sous le niveau 0 et dégage au-dessus.
    fn pad(&mut self, x0: i32, z0: i32, x1: i32, z1: i32, blk: BlockId, clear_h: i32) {
        for z in z0..=z1 {
            for x in x0..=x1 {
                let (wx, wz) = self.map(x, z);
                if !self.p.covers(wx, wz) {
                    continue;
                }
                self.pillar(x, z, -1, blk);
                for y in 0..clear_h {
                    self.p.force(wx, self.oy + y, wz, self.ids.air);
                }
            }
        }
    }

    /// Dégage un disque (arbres, relief) de rayon `r` sur `h` blocs de haut, comble en dessous.
    fn pad_disc(&mut self, r: i32, blk: BlockId, clear_h: i32) {
        for z in -r..=r {
            for x in -r..=r {
                if x * x + z * z > r * r + r {
                    continue;
                }
                let (wx, wz) = self.map(x, z);
                if !self.p.covers(wx, wz) {
                    continue;
                }
                let g = self.terrain.column(wx, wz).height;
                // Tampon : le sol suit la structure, puis se raccorde au terrain.
                for y in g.min(self.oy)..self.oy {
                    self.p.force(wx, y, wz, blk);
                }
                for y in 0..clear_h {
                    self.p.force(wx, self.oy + y, wz, self.ids.air);
                }
            }
        }
    }

    fn chance(&mut self, p: f64) -> bool {
        self.rng.chance(p)
    }
}

// -- Styles ----------------------------------------------------------------------------------

fn style_for(ids: &BlockIds, biome: Biome, variant: u32) -> Style {
    let v = variant % 3;
    match biome {
        Biome::Desert | Biome::Badlands => Style {
            plank: ids.cut_sandstone,
            log: ids.chiseled_sandstone,
            roof: if v == 0 { ids.wool_red } else { ids.sandstone },
            base: ids.sandstone,
            glass: ids.glass,
        },
        Biome::Savanna => Style {
            plank: ids.planks_acacia,
            log: ids.acacia_wood,
            roof: ids.hay_block,
            base: ids.cobblestone,
            glass: ids.glass,
        },
        Biome::Taiga | Biome::SnowyTaiga | Biome::MegaTaiga | Biome::SnowyPlains => Style {
            plank: ids.planks_spruce,
            log: ids.spruce_wood,
            roof: ids.planks_dark_oak,
            base: ids.cobblestone,
            glass: ids.glass,
        },
        Biome::BirchForest => Style {
            plank: ids.planks_birch,
            log: ids.birch_wood,
            roof: if v == 0 {
                ids.planks_spruce
            } else {
                ids.planks_oak
            },
            base: ids.stone_bricks,
            glass: ids.glass,
        },
        Biome::CherryGrove => Style {
            plank: ids.planks_cherry,
            log: ids.cherry_wood,
            roof: ids.planks_dark_oak,
            base: ids.stone_bricks,
            glass: ids.glass,
        },
        Biome::DarkForest | Biome::Swamp => Style {
            plank: ids.planks_dark_oak,
            log: ids.dark_oak_wood,
            roof: ids.planks_spruce,
            base: ids.mossy_cobblestone,
            glass: ids.glass,
        },
        _ => Style {
            plank: ids.planks_oak,
            log: ids.wood,
            roof: match v {
                0 => ids.planks_spruce,
                1 => ids.bricks,
                _ => ids.hay_block,
            },
            base: ids.cobblestone,
            glass: ids.glass,
        },
    }
}

// =============================================================================================
// Briques de construction
// =============================================================================================

/// Maison : empreinte `(2hw+1) × (2hd+1)`, murs de hauteur `h`, porte face à −Z.
fn house(b: &mut Build, hw: i32, hd: i32, h: i32, s: Style, furnish: bool) {
    let ids = b.ids;
    b.pad(-hw - 1, -hd - 1, hw + 1, hd + 1, s.base, h + hw.min(hd) + 3);
    // Sol, murs, poteaux d'angle.
    b.fill((-hw, -1, -hd), (hw, -1, hd), s.plank);
    b.walls(-hw, -hd, hw, hd, 0, h - 1, s.plank);
    for y in 0..h {
        for (x, z) in [(-hw, -hd), (hw, -hd), (-hw, hd), (hw, hd)] {
            b.set(x, y, z, s.log);
        }
    }
    // Soubassement.
    b.walls(-hw, -hd, hw, hd, -1, -1, s.base);
    // Fenêtres.
    for x in (-hw + 2..=hw - 2).step_by(2) {
        if x != 0 {
            b.set(x, 1, hd, s.glass);
            b.set(x, 1, -hd, s.glass);
        }
    }
    for z in (-hd + 2..=hd - 2).step_by(2) {
        b.set(-hw, 1, z, s.glass);
        b.set(hw, 1, z, s.glass);
    }
    // Porte et torches.
    b.clear((0, 0, -hd), (0, 1, -hd));
    b.set(0, -1, -hd, s.base);
    b.set(1, 0, -hd - 1, ids.torch);
    b.set(-1, 0, -hd - 1, ids.torch);
    // Intérieur.
    b.clear((-hw + 1, 0, -hd + 1), (hw - 1, h - 1, hd - 1));
    // Toit en croupe.
    let mut i = 0;
    loop {
        let (x0, x1, z0, z1) = (-hw - 1 + i, hw + 1 - i, -hd - 1 + i, hd + 1 - i);
        if x0 > x1 || z0 > z1 {
            break;
        }
        for z in z0..=z1 {
            for x in x0..=x1 {
                if x == x0 || x == x1 || z == z0 || z == z1 || x0 == x1 || z0 == z1 {
                    b.set(x, h + i, z, s.roof);
                } else if i > 0 {
                    // Dessous du toit : plancher de grenier plein, évite le ciel visible.
                    b.set(x, h + i, z, s.roof);
                }
            }
        }
        i += 1;
    }
    if furnish {
        b.set(0, h - 1, 0, ids.lantern);
        if hd >= 2 {
            for x in -hw + 1..=(-hw + 2).min(hw - 1) {
                b.set(x, 0, hd - 1, ids.bookshelf);
                b.set(x, 1, hd - 1, ids.bookshelf);
            }
        }
        // Lit : deux blocs de laine contre le mur de droite.
        b.set(hw - 1, 0, hd - 1, ids.wool_red);
        if hd >= 3 {
            b.set(hw - 1, 0, hd - 2, ids.wool_white);
        }
        b.set(hw - 1, 0, -hd + 1, ids.torch);
        if b.chance(0.5) {
            b.set(-hw + 1, 0, -hd + 1, ids.pumpkin);
        }
    }
}

/// Puits en pierre : margelle, eau, deux poteaux et petit toit.
fn well(b: &mut Build, s: Style) {
    let ids = b.ids;
    b.pad(-2, -2, 2, 2, s.base, 6);
    b.walls(-2, -2, 2, 2, 0, 0, ids.cobblestone);
    b.fill((-1, -1, -1), (1, -1, 1), ids.water);
    b.fill((-1, 0, -1), (1, 0, 1), ids.water);
    for (x, z) in [(-2, -2), (2, -2), (-2, 2), (2, 2)] {
        b.fill((x, 1, z), (x, 3, z), s.log);
    }
    b.fill((-2, 4, -2), (2, 4, 2), s.roof);
    b.fill((-1, 5, -1), (1, 5, 1), s.roof);
    b.set(0, 3, 0, ids.lantern);
}

/// Champ cultivé 5×5 irrigué (eau au centre).
fn field(b: &mut Build, x: i32, z: i32, crop: BlockId) {
    let ids = b.ids;
    b.pad(x - 3, z - 3, x + 3, z + 3, ids.dirt, 3);
    for dz in -2..=2 {
        for dx in -2..=2 {
            if dx == 0 && dz == 0 {
                b.set(x, -1, z, ids.water);
            } else {
                b.set(x + dx, -1, z + dz, ids.farmland);
                let c = if b.chance(0.08) { ids.pumpkin } else { crop };
                b.set(x + dx, 0, z + dz, c);
            }
        }
    }
    b.walls(x - 3, z - 3, x + 3, z + 3, -1, -1, ids.planks_oak);
}

/// Lampadaire : poteau de bois et lanterne.
fn lamp(b: &mut Build, x: i32, z: i32, log: BlockId) {
    let l = b.ids.lantern;
    b.pillar(x, z, -1, log);
    b.fill((x, 0, z), (x, 2, z), log);
    b.set(x, 3, z, l);
}

/// Chemin de terre battue au niveau du sol entre deux points.
fn path(b: &mut Build, from: (i32, i32), to: (i32, i32)) {
    let path_block = b.ids.dirt_path;
    let n = (to.0 - from.0).abs().max((to.1 - from.1).abs()).max(1);
    for i in 0..=n {
        let x = from.0 + (to.0 - from.0) * i / n;
        let z = from.1 + (to.1 - from.1) * i / n;
        for (dx, dz) in [(0, 0), (1, 0), (0, 1)] {
            let (wx, wz) = b.map(x + dx, z + dz);
            if !b.p.covers(wx, wz) {
                continue;
            }
            let g = b.terrain.column(wx, wz);
            if g.height >= b.sea {
                b.p.force(wx, g.height - 1, wz, path_block);
                b.p.force(wx, g.height, wz, b.ids.air);
            }
        }
    }
}

/// Sommet de tour : parapet crénelé, torches, lanterne.
fn parapet(b: &mut Build, r: i32, y: i32, blk: BlockId) {
    for z in -r..=r {
        for x in -r..=r {
            if x.abs() == r || z.abs() == r {
                let crenel = (x + z).rem_euclid(2) == 0;
                b.set(x, y, z, blk);
                if crenel {
                    b.set(x, y + 1, z, blk);
                }
            }
        }
    }
    b.set(r, y + 2, r, b.ids.torch);
    b.set(-r, y + 2, -r, b.ids.torch);
}

// =============================================================================================
// Les structures
// =============================================================================================

fn hamlet(b: &mut Build, biome: Biome, variant: u32) {
    let ids = b.ids;
    let s = style_for(&ids, biome, variant);
    b.pad_disc(3, ids.dirt, 8);
    well(b, s);
    let n = 3 + (variant % 3) as i32;
    let mut angle = f64::from(variant % 7);
    for i in 0..n {
        angle += std::f64::consts::TAU / f64::from(n) + (b.rng.f64() - 0.5) * 0.5;
        let dist = 9.0 + b.rng.f64() * 2.0;
        let (hx, hz) = (
            (angle.cos() * dist).round() as i32,
            (angle.sin() * dist).round() as i32,
        );
        let face = if hx.abs() > hz.abs() {
            if hx > 0 {
                3u8
            } else {
                1
            }
        } else if hz > 0 {
            0
        } else {
            2
        };
        // Les maisons font face au puits : repère tourné pour que la porte (−Z) regarde le centre.
        let hw = b.rng.range(2, 3);
        let hd = b.rng.range(2, 4);
        let h = b.rng.range(3, 4);
        let st = if i == 0 {
            Style {
                roof: ids.planks_dark_oak,
                ..s
            }
        } else {
            s
        };
        b.push(hx, hz, face);
        house(b, hw, hd, h, st, true);
        b.pop();
        path(b, (0, 0), (hx, hz));
    }
    // Champs et lampadaires.
    let fa = angle + 0.9;
    for k in 0..2 {
        let a = fa + f64::from(k) * 2.2;
        let (fx, fz) = (
            (a.cos() * 13.0).round() as i32,
            (a.sin() * 13.0).round() as i32,
        );
        field(b, fx, fz, ids.wheat);
    }
    for k in 0..4 {
        let a = f64::from(k) * std::f64::consts::FRAC_PI_2 + 0.78;
        lamp(
            b,
            (a.cos() * 5.0).round() as i32,
            (a.sin() * 5.0).round() as i32,
            s.log,
        );
    }
    // Bottes de paille.
    for _ in 0..3 {
        let (x, z) = (b.rng.range(-6, 6), b.rng.range(-6, 6));
        if x.abs() > 3 || z.abs() > 3 {
            let (wx, wz) = b.map(x, z);
            if b.p.covers(wx, wz) {
                let g = b.terrain.column(wx, wz).height;
                b.p.force(wx, g, wz, ids.hay_block);
            }
        }
    }
}

fn cabin(b: &mut Build, biome: Biome, variant: u32) {
    let ids = b.ids;
    let mut s = style_for(&ids, biome, variant);
    if matches!(
        biome,
        Biome::Plains | Biome::Forest | Biome::DarkForest | Biome::Heath
    ) {
        s.plank = ids.wood;
        s.roof = ids.planks_spruce;
    }
    b.pad_disc(5, ids.dirt, 9);
    house(b, 3, 3, 4, s, true);
    // Cheminée de pierre côté +X, avec feu.
    b.fill((4, 0, 0), (4, 5, 0), ids.cobblestone);
    b.set(4, 6, 0, ids.cobblestone);
    b.set(3, 0, 0, ids.campfire);
    // Tas de bûches.
    for z in -3..=-1 {
        b.set(-5, 0, z, s.log);
    }
    b.set(-5, 1, -2, s.log);
    path(b, (0, -4), (0, -8));
}

fn windmill(b: &mut Build, variant: u32) {
    let ids = b.ids;
    b.pad_disc(5, ids.cobblestone, 18);
    // Tour tronconique en briques / pierre, hauteur 10.
    for y in 0..10 {
        let r = if y < 4 {
            3
        } else if y < 8 {
            2
        } else {
            1
        };
        b.walls(
            -r,
            -r,
            r,
            r,
            y,
            y,
            if y < 3 {
                ids.stone_bricks
            } else {
                ids.planks_birch
            },
        );
        b.clear((-r + 1, y, -r + 1), (r - 1, y, r - 1));
    }
    b.fill((-3, -1, -3), (3, -1, 3), ids.planks_oak);
    b.clear((0, 0, -3), (0, 1, -3));
    b.set(0, 3, 0, ids.lantern);
    // Toit conique.
    for (i, r) in [(10, 2), (11, 1)] {
        b.fill((-r, i, -r), (r, i, r), ids.planks_spruce);
    }
    b.set(0, 12, 0, ids.planks_spruce);
    // Axe et pales (plan XY, face à −Z).
    let hub_y = 8;
    b.set(0, hub_y, -2, ids.wood);
    b.set(0, hub_y, -3, ids.wood);
    let sail = if variant.is_multiple_of(2) {
        ids.wool_white
    } else {
        ids.wool_red
    };
    for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
        for i in 1..=6 {
            b.set(dx * i, hub_y + dy * i, -4, ids.planks_spruce);
        }
        // Voiles : bandes de 3 blocs de long sur le côté de chaque pale.
        for i in 2..=6 {
            let (px, py) = if dx != 0 { (0, 1) } else { (1, 0) };
            b.set(dx * i + px, hub_y + dy * i + py, -4, sail);
        }
    }
    path(b, (0, -4), (0, -9));
}

fn watchtower(b: &mut Build, biome: Biome, variant: u32, ruined: bool) {
    let ids = b.ids;
    let (wall, accent) = if ruined {
        (
            if variant.is_multiple_of(2) {
                ids.mossy_stone_bricks
            } else {
                ids.stone_bricks
            },
            ids.mossy_cobblestone,
        )
    } else if matches!(biome, Biome::Desert | Biome::Badlands) {
        (ids.cut_sandstone, ids.chiseled_sandstone)
    } else {
        (ids.stone_bricks, ids.cobblestone)
    };
    let h = b.rng.range(11, 15);
    b.pad_disc(5, accent, h + 4);
    // Fût 5×5 creux.
    b.fill((-2, -1, -2), (2, -1, 2), accent);
    b.walls(-2, -2, 2, 2, 0, h, wall);
    for y in (0..=h).step_by(5) {
        for (x, z) in [(-2, -2), (2, -2), (-2, 2), (2, 2)] {
            b.set(x, y, z, accent);
        }
    }
    b.clear((-1, 0, -1), (1, h, 1));
    // Escalier en colimaçon (anneau de 8 cases, +1 par case).
    let ring = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
    ];
    for k in 0..=h {
        let (x, z) = ring[(k % 8) as usize];
        b.fill((x, 0, z), (x, k, z), accent);
    }
    // Porte et fenêtres.
    b.clear((0, 0, -2), (0, 1, -2));
    b.set(0, -1, -2, accent);
    for y in (3..h).step_by(4) {
        b.set(0, y, 2, ids.glass);
        b.set(2, y, 0, ids.glass);
        b.set(-2, y, 0, ids.glass);
    }
    // Plateforme et parapet.
    b.fill((-3, h + 1, -3), (3, h + 1, 3), ids.planks_oak);
    b.clear((-1, h + 1, -1), (1, h + 1, 1));
    b.fill((-1, h + 1, -1), (1, h + 1, -1), ids.air);
    // Trappe : l'escalier débouche à h.
    parapet(b, 3, h + 2, wall);
    b.set(0, h + 2, 0, ids.lantern);
    if ruined {
        // Érosion : plus on monte, plus de blocs manquent ; vignes et toile d'araignée.
        for y in 2..=h + 4 {
            let p = (f64::from(y) / f64::from(h + 4)).powi(2) * 0.8;
            for z in -3..=3 {
                for x in -3..=3 {
                    if b.get(x, y, z) != ids.air && b.chance(p) {
                        let air = ids.air;
                        b.set(x, y, z, air);
                    }
                }
            }
        }
        for _ in 0..14 {
            let (x, z) = (b.rng.range(-2, 2), b.rng.range(-2, 2));
            let y = b.rng.range(1, h);
            if b.get(x, y, z) == ids.air {
                let blk = if b.chance(0.5) { ids.cobweb } else { ids.vines };
                b.set(x, y, z, blk);
            }
        }
        b.set(0, 0, 0, ids.gold_block);
        b.set(0, 1, 0, ids.torch);
    }
}

fn pyramid(b: &mut Build, jungle: bool) {
    let ids = b.ids;
    let (wall, accent, floor) = if jungle {
        (
            ids.mossy_cobblestone,
            ids.mossy_stone_bricks,
            ids.stone_bricks,
        )
    } else {
        (ids.cut_sandstone, ids.chiseled_sandstone, ids.sandstone)
    };
    let r0 = 9;
    b.pad_disc(r0 + 2, wall, 14);
    // Gradins : 9 niveaux qui rétrécissent d'un bloc.
    for i in 0..9 {
        let r = r0 - i;
        b.fill(
            (-r, i, -r),
            (r, i, r),
            if i % 3 == 2 { accent } else { wall },
        );
    }
    // Sommet : lanterne / or.
    b.set(
        0,
        9,
        0,
        if jungle {
            ids.mossy_stone_bricks
        } else {
            ids.gold_block
        },
    );
    b.set(0, 10, 0, ids.lantern);
    // Entrée : couloir dans l'axe −Z vers la chambre centrale.
    b.clear((-1, 0, -r0), (1, 2, -2));
    b.fill((-1, -1, -r0), (1, -1, -2), floor);
    // Chambre 7×7×5.
    b.clear((-3, 0, -3), (3, 4, 3));
    b.fill((-3, -1, -3), (3, -1, 3), floor);
    b.fill((-3, 5, -3), (3, 5, 3), wall);
    for (x, z) in [(-3, -3), (3, -3), (-3, 3), (3, 3)] {
        b.fill((x, 0, z), (x, 4, z), accent);
    }
    b.set(0, 0, 0, ids.gold_block);
    b.set(0, 1, 0, ids.torch);
    for (x, z) in [(-2, 2), (2, 2), (-2, -2), (2, -2)] {
        b.set(x, 3, z, ids.lantern);
    }
    // Piège : bloc de TNT absent ; on décore plutôt de toiles / de livres.
    for x in -2..=2 {
        b.set(
            x,
            0,
            3,
            if jungle {
                ids.mossy_stone_bricks
            } else {
                ids.bookshelf
            },
        );
    }
    if jungle {
        // Végétation envahissante : vignes sur les gradins, feuillage au sommet.
        for _ in 0..40 {
            let i = b.rng.range(0, 6);
            let r = r0 - i;
            let (x, z) = (b.rng.range(-r, r), b.rng.range(-r, r));
            if x.abs() == r || z.abs() == r {
                b.set_soft(x, i + 1, z, ids.vines);
            }
        }
        for _ in 0..16 {
            let (x, z) = (b.rng.range(-4, 4), b.rng.range(-4, 4));
            b.set_soft(x, 7, z, ids.hibiscus);
        }
    }
}

fn camp(b: &mut Build, biome: Biome, variant: u32) {
    let ids = b.ids;
    let (c1, c2) = match biome {
        Biome::Desert | Biome::Badlands => (ids.wool_yellow, ids.wool_red),
        Biome::SnowyPlains | Biome::SnowyTaiga | Biome::SnowyPeaks => {
            (ids.wool_blue, ids.wool_white)
        }
        _ => (ids.wool_red, ids.wool_white),
    };
    b.pad_disc(7, ids.dirt, 6);
    // Feu de camp entouré de bûches et de bottes de paille.
    b.set(0, 0, 0, ids.campfire);
    b.set(1, 0, 0, ids.wood);
    b.set(-1, 0, 0, ids.wood);
    b.set(0, 0, 1, ids.wood);
    b.set(0, 0, -1, ids.wood);
    for (x, z) in [(3, 3), (-3, 3), (3, -3)] {
        b.set(x, 0, z, ids.hay_block);
    }
    // Deux tentes en A (prismes de 5 de long).
    for (i, (tx, tz, tr)) in [(-5, 0, 0u8), (5, 1, 2)].into_iter().enumerate() {
        b.push(tx, tz, tr);
        let c = if (i as u32 + variant).is_multiple_of(2) {
            c1
        } else {
            c2
        };
        for z in -2..=2 {
            b.set(-2, 0, z, c);
            b.set(2, 0, z, c);
        }
        for z in -2..=2 {
            b.set(-1, 1, z, c);
            b.set(1, 1, z, c);
            b.set(0, 2, z, c);
        }
        b.clear((-1, 0, -2), (1, 0, 2));
        b.set(0, 0, 2, ids.air);
        b.set(0, 0, 1, ids.wool_red);
        b.set(0, 1, -1, ids.air);
        b.set(0, 0, -1, ids.torch);
        b.pop();
    }
    lamp(b, 0, 5, ids.wood);
    b.set(4, 0, -4, ids.pumpkin);
}

fn ice_hall(b: &mut Build) {
    let ids = b.ids;
    let r = 4;
    b.pad_disc(r + 2, ids.snow, 7);
    // Dôme de neige, intérieur creux, plancher de glace compacte.
    for y in 0..=r {
        for z in -r..=r {
            for x in -r..=r {
                let d = x * x + z * z + y * y;
                if d <= r * r + 1 && d >= (r - 1) * (r - 1) {
                    b.set(x, y, z, ids.snow);
                } else if d < (r - 1) * (r - 1) {
                    b.set(x, y, z, ids.air);
                }
            }
        }
    }
    b.fill((-r, -1, -r), (r, -1, r), ids.packed_ice);
    // Tunnel d'entrée.
    b.fill((-1, 0, -r - 3), (1, 2, -r + 1), ids.snow);
    b.clear((0, 0, -r - 3), (0, 1, -r + 1));
    b.fill((0, -1, -r - 3), (0, -1, -r), ids.packed_ice);
    b.set(0, 3, 0, ids.ice);
    b.set(0, 0, 2, ids.wool_blue);
    b.set(1, 0, 2, ids.wool_white);
    b.set(-2, 0, 0, ids.campfire);
    b.set(0, 3, 1, ids.sea_lantern);
    b.set(2, 0, -1, ids.bookshelf);
}

fn stone_circle(b: &mut Build, variant: u32) {
    let ids = b.ids;
    let n = 8 + (variant % 3) as i32;
    b.pad_disc(8, ids.moss, 9);
    let mut heights = Vec::new();
    for i in 0..n {
        let a = f64::from(i) * std::f64::consts::TAU / f64::from(n);
        let (x, z) = (
            (a.cos() * 6.5).round() as i32,
            (a.sin() * 6.5).round() as i32,
        );
        let h = b.rng.range(3, 5);
        heights.push(((x, z), h));
        let blk = if b.chance(0.35) {
            ids.mossy_cobblestone
        } else {
            ids.stone
        };
        b.pillar(x, z, -1, blk);
        b.fill((x, 0, z), (x, h - 1, z), blk);
        if b.chance(0.5) {
            b.set(x, h, z, ids.moss);
        }
    }
    // Linteaux entre paires voisines (trilithes) quand les piliers sont assez hauts.
    for i in (0..n as usize).step_by(2) {
        let (a, ha) = heights[i];
        let (c, hc) = heights[(i + 1) % n as usize];
        if ha >= 4 && hc >= 4 && b.chance(0.7) {
            let y = ha.min(hc);
            let steps = (c.0 - a.0).abs().max((c.1 - a.1).abs()).max(1);
            for k in 0..=steps {
                let x = a.0 + (c.0 - a.0) * k / steps;
                let z = a.1 + (c.1 - a.1) * k / steps;
                b.set(x, y, z, ids.stone);
            }
        }
    }
    // Autel central.
    b.fill((-1, 0, -1), (1, 0, 1), ids.marble);
    b.set(0, 1, 0, ids.campfire);
    for (x, z) in [(2, 2), (-2, 2), (2, -2), (-2, -2)] {
        b.set(
            x,
            0,
            z,
            if variant.is_multiple_of(2) {
                ids.lavender
            } else {
                ids.allium
            },
        );
    }
}

fn lighthouse(b: &mut Build) {
    let ids = b.ids;
    let h = 17;
    b.pad_disc(5, ids.stone_bricks, h + 6);
    // Fût 5×5 aux angles coupés, bandes rouges et blanches.
    for y in 0..h {
        let r: i32 = 2;
        let blk = if (y / 3) % 2 == 0 {
            ids.wool_white
        } else {
            ids.wool_red
        };
        for z in -r..=r {
            for x in -r..=r {
                if (x.abs() == r || z.abs() == r) && !(r == 2 && x.abs() == 2 && z.abs() == 2) {
                    b.set(x, y, z, blk);
                } else {
                    b.set(x, y, z, ids.air);
                }
            }
        }
    }
    b.fill((-3, -1, -3), (3, -1, 3), ids.stone_bricks);
    b.clear((0, 0, -2), (0, 1, -2));
    // Escalier : colimaçon de 1 bloc sur 3×3 (r = 1 libre au centre dès y 9).
    let ring = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
    ];
    for k in 0..h {
        let (x, z) = ring[(k % 8) as usize];
        b.fill((x, 0, z), (x, k.min(h - 1), z), ids.stone_bricks);
    }
    // Lanterne : verre + lumière, balcon.
    b.fill((-2, h, -2), (2, h, 2), ids.stone_bricks);
    for y in h + 1..=h + 3 {
        b.walls(-1, -1, 1, 1, y, y, ids.glass);
    }
    b.clear((0, h + 1, 0), (0, h + 3, 0));
    b.set(0, h + 1, 0, ids.sea_lantern);
    b.set(0, h + 2, 0, ids.lantern);
    b.fill((-2, h + 4, -2), (2, h + 4, 2), ids.planks_dark_oak);
    b.fill((-1, h + 5, -1), (1, h + 5, 1), ids.planks_dark_oak);
    b.set(0, h + 6, 0, ids.planks_dark_oak);
    for x in -2i32..=2 {
        for z in -2i32..=2 {
            if x.abs() == 2 || z.abs() == 2 {
                b.set(x, h + 1, z, ids.planks_oak);
            }
        }
    }
}

fn shipwreck(b: &mut Build, variant: u32) {
    let ids = b.ids;
    let hull = if variant.is_multiple_of(2) {
        ids.planks_dark_oak
    } else {
        ids.planks_spruce
    };
    let rib = if variant.is_multiple_of(2) {
        ids.dark_oak_wood
    } else {
        ids.spruce_wood
    };
    // Coque le long de X : longueur 17, largeur variable (profil de proue/poupe).
    let len: i32 = 8;
    for x in -len..=len {
        let t = f64::from(x.abs()) / f64::from(len);
        let beam = ((1.0 - t * t) * 3.0 + 1.0).round() as i32; // demi-largeur 1..4
        let rise = ((t * t) * 3.0).round() as i32; // proue et poupe remontent
        for z in -beam..=beam {
            // Fond de la coque.
            b.set(x, rise / 2, z, hull);
            // Bordés (côtés) : plus on monte, plus on s'écarte, avec des trous d'épave.
            if z.abs() == beam {
                for y in 1..=3 + rise {
                    if !b.chance(0.18 + 0.05 * f64::from(y)) {
                        b.set(x, y, z, hull);
                    }
                }
            }
        }
        if x % 4 == 0 {
            for y in 1..=3 + rise {
                b.set(x, y, beam, rib);
                b.set(x, y, -beam, rib);
            }
        }
        // Pont partiel.
        if x.abs() < 5 && b.chance(0.7) {
            for z in -beam + 1..=beam - 1 {
                if b.chance(0.8) {
                    b.set(x, 2, z, ids.planks_oak);
                }
            }
        }
    }
    // Mât brisé, penché, avec voile en lambeaux.
    for y in 1..=7 {
        b.set(y / 4, y, 0, rib);
    }
    for y in 3..=6 {
        for z in -2..=2 {
            if b.chance(0.55) {
                b.set(2, y, z, ids.wool_white);
            }
        }
    }
    // Cabine arrière avec trésor.
    b.fill((-7, 1, -1), (-5, 3, 1), ids.air);
    b.set(-6, 1, 0, ids.gold_block);
    b.set(-6, 2, 0, ids.lantern);
    b.walls(-7, -1, -5, 1, 1, 3, ids.planks_spruce);
    b.fill((-7, 4, -1), (-5, 4, 1), ids.planks_spruce);
    b.clear((-5, 1, 0), (-5, 2, 0));
    b.set(-6, 1, 0, ids.gold_block);
}

fn ocean_ruin(b: &mut Build, biome: Biome, variant: u32) {
    let ids = b.ids;
    let (wall, accent) = if biome == Biome::WarmOcean {
        (ids.prismarine_bricks, ids.sea_lantern)
    } else {
        (ids.mossy_stone_bricks, ids.lantern)
    };
    let r = 5;
    b.fill((-r, 0, -r), (r, 0, r), wall);
    b.fill((-r - 1, -1, -r - 1), (r + 1, -1, r + 1), ids.stone_bricks);
    // Colonnes cassées aux angles et au milieu des côtés.
    for (x, z) in [
        (-r, -r),
        (r, -r),
        (-r, r),
        (r, r),
        (0, -r),
        (0, r),
        (-r, 0),
        (r, 0),
    ] {
        let h = b.rng.range(2, 6);
        b.fill(
            (x, 1, z),
            (x, h, z),
            if (x + z) % 2 == 0 {
                wall
            } else {
                ids.stone_bricks
            },
        );
        if b.chance(0.5) {
            b.set(x, h + 1, z, accent);
        }
    }
    // Pans de mur effondrés entre les colonnes.
    for k in -r + 1..r {
        for (x, z) in [(k, -r), (k, r), (-r, k), (r, k)] {
            if (x == 0 || z == 0) && variant.is_multiple_of(2) {
                continue; // portes
            }
            let h = b.rng.range(0, 3);
            if h > 0 {
                b.fill((x, 1, z), (x, h, z), wall);
            }
        }
    }
    // Autel central et trésor.
    b.fill((-1, 1, -1), (1, 1, 1), ids.obsidian);
    b.set(0, 2, 0, ids.gold_block);
    b.set(0, 3, 0, accent);
    if biome == Biome::WarmOcean {
        for _ in 0..8 {
            let (x, z) = (b.rng.range(-r, r), b.rng.range(-r, r));
            let c = [
                ids.coral_pink,
                ids.coral_purple,
                ids.coral_yellow,
                ids.coral_blue,
            ][b.rng.range(0, 3) as usize];
            b.set_soft(x, 1, z, c);
        }
    }
}

fn witch_hut(b: &mut Build) {
    let ids = b.ids;
    // Maison sur pilotis (sol en y = 3), hutte 5×5, pont vers la rive.
    let floor = 3;
    for (x, z) in [(-3, -3), (3, -3), (-3, 3), (3, 3), (0, 3), (0, -3)] {
        b.pillar(x, z, floor - 1, ids.spruce_wood);
    }
    b.fill((-3, floor, -3), (3, floor, 3), ids.planks_spruce);
    // Dégage l'espace.
    b.clear((-3, floor + 1, -3), (3, floor + 7, 3));
    b.walls(-3, -3, 3, 3, floor + 1, floor + 3, ids.planks_dark_oak);
    for y in floor + 1..=floor + 3 {
        for (x, z) in [(-3, -3), (3, -3), (-3, 3), (3, 3)] {
            b.set(x, y, z, ids.spruce_wood);
        }
    }
    b.clear((0, floor + 1, -3), (0, floor + 2, -3));
    for x in [-1, 1] {
        b.set(x, floor + 2, 3, ids.glass);
    }
    b.set(3, floor + 2, 0, ids.glass);
    b.set(-3, floor + 2, 0, ids.glass);
    // Toit pointu.
    for (i, r) in [
        (floor + 4, 4),
        (floor + 5, 3),
        (floor + 6, 2),
        (floor + 7, 1),
    ] {
        b.fill((-r, i, -r), (r, i, r), ids.planks_spruce);
    }
    // Intérieur : chaudron (mossy), étagère, toiles, lanterne.
    b.set(-2, floor + 1, -2, ids.mossy_cobblestone);
    b.set(2, floor + 1, 2, ids.bookshelf);
    b.set(1, floor + 1, 2, ids.bookshelf);
    b.set(-2, floor + 3, 2, ids.cobweb);
    b.set(2, floor + 3, -2, ids.cobweb);
    b.set(0, floor + 3, 0, ids.lantern);
    b.set(2, floor + 1, -2, ids.mushroom_red);
    b.set(-2, floor + 1, 2, ids.mushroom_brown);
    // Pont de bois vers la porte.
    for z in -9..-3 {
        let (wx, wz) = b.map(0, z);
        if b.p.covers(wx, wz) {
            let g = b.terrain.column(wx, wz).height;
            let y = g.max(b.sea);
            let ly = y - b.oy;
            b.set(0, ly.min(floor), z, ids.planks_spruce);
        }
    }
    b.fill((0, 0, -8), (0, floor, -8), ids.air);
    b.set(1, floor + 1, -4, ids.torch);
    b.set(-1, floor + 1, -4, ids.torch);
}

fn cherry_shrine(b: &mut Build) {
    let ids = b.ids;
    b.pad_disc(7, ids.stone_bricks, 10);
    // Esplanade.
    b.fill((-4, -1, -4), (4, -1, 4), ids.stone_bricks);
    // Pavillon 5×5 à deux toits.
    b.walls(-2, -2, 2, 2, 0, 3, ids.planks_cherry);
    for y in 0..=3 {
        for (x, z) in [(-2, -2), (2, -2), (-2, 2), (2, 2)] {
            b.set(x, y, z, ids.wool_red);
        }
    }
    b.clear((-1, 0, -1), (1, 3, 1));
    b.clear((0, 0, -2), (0, 1, -2));
    b.set(0, 0, 1, ids.gold_block);
    b.set(0, 1, 1, ids.lantern);
    for (y, r) in [(4, 4), (5, 3), (6, 2)] {
        b.fill((-r, y, -r), (r, y, r), ids.planks_dark_oak);
    }
    // Relèvement des coins (toit « en pagode »).
    for (x, z) in [(-4, -4), (4, -4), (-4, 4), (4, 4)] {
        b.set(x, 5, z, ids.planks_dark_oak);
    }
    b.fill((-1, 7, -1), (1, 7, 1), ids.planks_dark_oak);
    b.set(0, 8, 0, ids.lantern);
    // Torii : deux poteaux rouges et deux traverses.
    b.push(0, -8, 0);
    for x in [-3, 3] {
        b.pillar(x, 0, -1, ids.wool_red);
        b.fill((x, 0, 0), (x, 5, 0), ids.wool_red);
    }
    b.fill((-4, 5, 0), (4, 5, 0), ids.planks_dark_oak);
    b.fill((-3, 4, 0), (3, 4, 0), ids.wool_red);
    b.set(-4, 6, 0, ids.planks_dark_oak);
    b.set(4, 6, 0, ids.planks_dark_oak);
    b.pop();
    path(b, (0, -3), (0, -8));
    for (x, z) in [(-3, 3), (3, 3)] {
        lamp(b, x, z, ids.planks_dark_oak);
    }
}

fn obelisk(b: &mut Build, biome: Biome, variant: u32) {
    let ids = b.ids;
    let (body, band) = match biome {
        Biome::Desert | Biome::Badlands => (ids.cut_sandstone, ids.gold_block),
        Biome::SnowyPeaks | Biome::IceSpikes | Biome::SnowyPlains => {
            (ids.packed_ice, ids.sea_lantern)
        }
        _ => (ids.basalt_bricks, ids.lantern),
    };
    let h = 13 + (variant % 6) as i32;
    b.pad_disc(5, ids.stone_bricks, h + 4);
    b.fill((-2, -1, -2), (2, -1, 2), ids.stone_bricks);
    b.fill((-2, 0, -2), (2, 0, 2), body);
    for y in 1..h {
        let r = if y < 4 { 1 } else { 0 };
        b.fill((-r, y, -r), (r, y, r), if y % 5 == 0 { band } else { body });
    }
    b.fill((0, h, 0), (0, h + 1, 0), band);
    // Pierres basses alentour.
    for (x, z) in [(-4, 1), (4, -1), (1, 4), (-1, -4)] {
        let hh = b.rng.range(1, 2);
        b.fill((x, 0, z), (x, hh, z), ids.mossy_cobblestone);
    }
}

fn volcano_forge(b: &mut Build) {
    let ids = b.ids;
    b.pad_disc(8, ids.basalt, 12);
    // Plate-forme et quatre pylônes.
    b.fill((-6, -1, -6), (6, -1, 6), ids.basalt_bricks);
    b.fill((-5, 0, -5), (5, 0, 5), ids.obsidian);
    b.fill((-4, 0, -4), (4, 0, 4), ids.basalt_bricks);
    for (x, z) in [(-5, -5), (5, -5), (-5, 5), (5, 5)] {
        b.fill((x, 0, z), (x, 5, z), ids.basalt_bricks);
        b.set(x, 6, z, ids.magma);
        b.set(x, 7, z, ids.lantern);
    }
    // Fosse de lave centrale et enclume d'obsidienne.
    b.fill((-1, 0, -1), (1, 0, 1), ids.lava);
    b.fill((-2, 1, -2), (2, 1, -2), ids.obsidian);
    b.fill((-2, 1, 2), (2, 1, 2), ids.obsidian);
    b.set(-2, 1, 0, ids.obsidian);
    b.set(2, 1, 0, ids.obsidian);
    b.set(0, 2, -2, ids.gold_block);
    for x in -4..=4 {
        if x % 2 == 0 {
            b.set(x, 1, -5, ids.magma);
            b.set(x, 1, 5, ids.magma);
        }
    }
    b.set(0, 1, -6, ids.torch);
}

fn fossil(b: &mut Build, variant: u32) {
    let ids = b.ids;
    let n = 9 + (variant % 3) as i32; // longueur de la colonne vertébrale
    b.pad(-n, -4, n, 4, ids.sand, 8);
    // Colonne vertébrale légèrement ondulée, en partie enfouie.
    for x in -n..=n {
        let y = ((f64::from(x) * 0.5).sin() * 0.8).round() as i32;
        b.set(x, y, 0, ids.bone_block);
        if x % 2 == 0 && x.abs() < n - 2 {
            // Côtes en arche.
            let r = (4.0 * (1.0 - f64::from(x.abs()) / f64::from(n + 2)) + 1.0).round() as i32;
            for a in 0..=r * 3 {
                let t = f64::from(a) / f64::from(r * 3) * std::f64::consts::PI;
                let (cz, cy) = (
                    (t.cos() * f64::from(r)).round() as i32,
                    (t.sin() * f64::from(r)).round() as i32,
                );
                if b.chance(0.92) {
                    b.set(x, y + cy, cz, ids.bone_block);
                }
            }
        }
    }
    // Crâne à l'extrémité.
    b.fill((n + 1, 0, -1), (n + 3, 2, 1), ids.bone_block);
    b.set(n + 3, 1, -1, ids.obsidian);
    b.set(n + 3, 1, 1, ids.obsidian);
    b.clear((n + 3, 0, 0), (n + 3, 0, 0));
}

fn mushroom_house(b: &mut Build) {
    let ids = b.ids;
    b.pad_disc(6, ids.mycelium, 14);
    for y in 0..6 {
        b.walls(-2, -2, 2, 2, y, y, ids.mushroom_stem);
        b.clear((-1, y, -1), (1, y, 1));
    }
    b.fill((-2, -1, -2), (2, -1, 2), ids.planks_birch);
    b.clear((0, 0, -2), (0, 1, -2));
    b.set(0, 3, 2, ids.glass);
    b.set(2, 3, 0, ids.glass);
    b.set(-2, 3, 0, ids.glass);
    // Chapeau : dôme rouge à pois blancs.
    for y in 0..4i32 {
        let r = [5, 5, 4, 2][y as usize];
        for z in -r..=r {
            for x in -r..=r {
                let d = x * x + z * z;
                if d <= r * r + 1 && (y < 3 || d > 0) {
                    let spot = (x * 3 + z * 5 + y).rem_euclid(7) == 0;
                    b.set(
                        x,
                        6 + y,
                        z,
                        if spot {
                            ids.wool_white
                        } else {
                            ids.mushroom_cap_red
                        },
                    );
                }
            }
        }
    }
    b.clear((-1, 6, -1), (1, 6, 1));
    b.set(0, 5, 0, ids.lantern);
    b.set(1, 0, 1, ids.bookshelf);
    b.set(-1, 0, 1, ids.wool_red);
    b.set(1, 0, -1, ids.torch);
}

// =============================================================================================
// Choix et placement
// =============================================================================================

impl WorldGenerator {
    /// Structure de la cellule `(cx, cz)`, si elle en porte une et que le terrain s'y prête.
    pub fn structure_site(&self, cx: i32, cz: i32) -> Option<StructureSite> {
        let dens = self.cfg.structure_density;
        if dens <= 0.0 {
            return None;
        }
        let h = hash_position(
            cx.wrapping_mul(0x2545_F491u32 as i32).wrapping_add(701),
            cz.wrapping_mul(0x1B87_3593u32 as i32).wrapping_sub(313),
        );
        if unit(h) >= (CELL_CHANCE * dens).min(1.0) {
            return None;
        }
        let margin = MAX_RADIUS;
        let span = CELL - 2 * margin;
        let x = cx * CELL + margin + (h / 64) % span;
        let z = cz * CELL + margin + (h / 8192) % span;
        let col = self.terrain.column(x, z);
        let mut rng = Rng::new(x, z, 0x57c7);
        let table = weights(col.biome);
        let total: u32 = table.iter().map(|e| e.1).sum();
        if total == 0 {
            return None;
        }
        let mut r = rng.next() as u32 % total;
        let mut kind = table[0].0;
        for &(k, w) in table {
            if r < w {
                kind = k;
                break;
            }
            r -= w;
        }
        let variant = (rng.next() >> 8) as u32;
        let base_y = self.site_ground(kind, x, z, &col)?;
        Some(StructureSite {
            kind,
            x,
            z,
            base_y,
            variant,
        })
    }

    /// Niveau du sol retenu si l'emplacement est acceptable (assez plat, hors eau / lave).
    fn site_ground(&self, kind: StructureKind, x: i32, z: i32, center: &NbColumn) -> Option<i32> {
        let sea = self.cfg.sea_level;
        let r = kind.radius();
        let mut hs = vec![center.height];
        let probes: [(i32, i32); 8] = [
            (r, 0),
            (-r, 0),
            (0, r),
            (0, -r),
            (r / 2, r / 2),
            (-r / 2, r / 2),
            (r / 2, -r / 2),
            (-r / 2, -r / 2),
        ];
        for (dx, dz) in probes {
            let c = self.terrain.column(x + dx, z + dz);
            if c.lava_y > 0 && kind != StructureKind::VolcanoForge {
                return None;
            }
            hs.push(c.height);
        }
        let (lo, hi) = (*hs.iter().min()?, *hs.iter().max()?);
        hs.sort_unstable();
        let median = hs[hs.len() / 2];
        if kind.underwater() {
            // Fond marin d'au moins 3 blocs sous la surface, plat.
            if hi > sea - 3 || hi - lo > 4 || median < 6 {
                return None;
            }
            return Some(median);
        }
        if lo < sea + 1 || hi - lo > kind.tolerance() || median > SY - 55 {
            return None;
        }
        if center.lava_y > 0 && kind != StructureKind::VolcanoForge {
            return None;
        }
        if kind == StructureKind::Lighthouse {
            // Au bord de l'eau : au moins un point à 12 blocs sous le niveau de la mer.
            let coastal = [
                (12, 0),
                (-12, 0),
                (0, 12),
                (0, -12),
                (9, 9),
                (-9, -9),
                (9, -9),
                (-9, 9),
            ]
            .iter()
            .any(|&(dx, dz)| self.terrain.column(x + dx, z + dz).height < sea);
            if !coastal {
                return None;
            }
        }
        Some(median)
    }

    /// Emplacements des structures dont la cellule touche le rectangle monde donné.
    pub fn structure_sites(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> Vec<StructureSite> {
        let mut out = Vec::new();
        let (c0x, c1x) = (
            (x0 - MAX_RADIUS).div_euclid(CELL),
            (x1 + MAX_RADIUS).div_euclid(CELL),
        );
        let (c0z, c1z) = (
            (z0 - MAX_RADIUS).div_euclid(CELL),
            (z1 + MAX_RADIUS).div_euclid(CELL),
        );
        for cz in c0z..=c1z {
            for cx in c0x..=c1x {
                if let Some(s) = self.structure_site(cx, cz) {
                    out.push(s);
                }
            }
        }
        out
    }

    /// Passe « structures » d'un chunk (après arbres et décors, avant la flore).
    pub(crate) fn nb_structures(&self, chunk: &mut Chunk, wx0: i32, wz0: i32) {
        if self.cfg.structure_density <= 0.0 {
            return;
        }
        let ids = self.ids;
        let (c0x, c1x) = (
            (wx0 - MAX_RADIUS).div_euclid(CELL),
            (wx0 + SX - 1 + MAX_RADIUS).div_euclid(CELL),
        );
        let (c0z, c1z) = (
            (wz0 - MAX_RADIUS).div_euclid(CELL),
            (wz0 + SZ - 1 + MAX_RADIUS).div_euclid(CELL),
        );
        for cz in c0z..=c1z {
            for cx in c0x..=c1x {
                // Rejet rapide : la position (indépendante du terrain) doit toucher le chunk.
                let h = hash_position(
                    cx.wrapping_mul(0x2545_F491u32 as i32).wrapping_add(701),
                    cz.wrapping_mul(0x1B87_3593u32 as i32).wrapping_sub(313),
                );
                let span = CELL - 2 * MAX_RADIUS;
                let px = cx * CELL + MAX_RADIUS + (h / 64) % span;
                let pz = cz * CELL + MAX_RADIUS + (h / 8192) % span;
                if px + MAX_RADIUS < wx0
                    || px - MAX_RADIUS >= wx0 + SX
                    || pz + MAX_RADIUS < wz0
                    || pz - MAX_RADIUS >= wz0 + SZ
                {
                    continue;
                }
                let Some(site) = self.structure_site(cx, cz) else {
                    continue;
                };
                let r = site.kind.radius() + 4;
                if site.x + r < wx0
                    || site.x - r >= wx0 + SX
                    || site.z + r < wz0
                    || site.z - r >= wz0 + SZ
                {
                    continue;
                }
                let biome = self.terrain.column(site.x, site.z).biome;
                let mut painter = Painter::new(chunk, &ids);
                let mut b = Build {
                    p: &mut painter,
                    ids,
                    terrain: &self.terrain,
                    sea: self.cfg.sea_level,
                    frames: vec![(site.x, site.z, (site.variant % 4) as u8)],
                    oy: site.base_y,
                    rng: Rng::new(site.x, site.z, 0x57c8),
                };
                if !site.kind.underwater() {
                    b.strip(site.kind.radius() + 5);
                }
                build_site(&mut b, site, biome);
            }
        }
    }
}

fn build_site(b: &mut Build, site: StructureSite, biome: Biome) {
    let v = site.variant;
    match site.kind {
        StructureKind::Hamlet => hamlet(b, biome, v),
        StructureKind::Cabin => cabin(b, biome, v),
        StructureKind::Windmill => windmill(b, v),
        StructureKind::Watchtower => watchtower(b, biome, v, false),
        StructureKind::RuinedTower => watchtower(b, biome, v, true),
        StructureKind::DesertPyramid => pyramid(b, false),
        StructureKind::JungleTemple => pyramid(b, true),
        StructureKind::Camp => camp(b, biome, v),
        StructureKind::IceHall => ice_hall(b),
        StructureKind::StoneCircle => stone_circle(b, v),
        StructureKind::Lighthouse => lighthouse(b),
        StructureKind::Shipwreck => shipwreck(b, v),
        StructureKind::OceanRuin => ocean_ruin(b, biome, v),
        StructureKind::WitchHut => witch_hut(b),
        StructureKind::CherryShrine => cherry_shrine(b),
        StructureKind::Obelisk => obelisk(b, biome, v),
        StructureKind::VolcanoForge => volcano_forge(b),
        StructureKind::Fossil => fossil(b, v),
        StructureKind::MushroomHouse => mushroom_house(b),
    }
}
