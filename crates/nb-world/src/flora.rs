//! Flore de la phase 9 : arbres supplémentaires (pommier, saule, baobab, séquoia, saguaro,
//! arbre-parasol, bambouseraie, genévrier) et passe de plantes par biome (`FloraEntry`).
//!
//! Comme le reste du générateur « Norrblock », tout est fonction pure de
//! `(graine, configuration, position)` : un arbre à cheval sur deux chunks est identique des
//! deux côtés.

use nb_data::BlockId;

use crate::biome::{TreeType, SOIL_ASH, SOIL_GRASS, SOIL_SAND, SOIL_SNOW};
use crate::chunk::{Chunk, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};
use crate::generator::WorldGenerator;
use crate::generator_nb::{Painter, Rng};
use crate::noise::hash_position;
use crate::terrain::NbColumn;

const SX: i32 = CHUNK_SIZE_X as i32;
const SY: i32 = CHUNK_SIZE_Y as i32;
const SZ: i32 = CHUNK_SIZE_Z as i32;
const GRID: i32 = SX + 2;

#[inline]
fn gi(lx: i32, lz: i32) -> usize {
    ((lz + 1) * GRID + (lx + 1)) as usize
}

#[inline]
fn unit(h: i32) -> f64 {
    f64::from(h % 10000) / 10000.0
}

impl WorldGenerator {
    /// Arbres de la phase 9 (appelé par `place_tree_nb` pour les formes qu'il ne connaît pas).
    pub(crate) fn place_tree_nature(
        &self,
        p: &mut Painter,
        kind: TreeType,
        x: i32,
        y: i32,
        z: i32,
        rng: &mut Rng,
    ) {
        let ids = self.ids;
        match kind {
            TreeType::Apple => {
                let th = rng.range(3, 4);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.wood);
                }
                p.blob((x, y + th, z), (2.6, 2.1, 2.6), ids.apple_leaves, rng, 0.25);
                p.solid(x + 1, y + th - 1, z, ids.wood);
            }
            TreeType::Willow => {
                let th = rng.range(5, 7);
                let (dx, dz) = [(1, 0), (-1, 0), (0, 1), (0, -1)][rng.range(0, 3) as usize];
                let (mut tx, mut tz) = (x, z);
                for k in 0..th {
                    p.solid(tx, y + k, tz, ids.wood);
                    if k == 2 {
                        tx += dx;
                        tz += dz;
                    }
                }
                let top = y + th;
                p.blob((tx, top, tz), (3.6, 1.9, 3.6), ids.willow_leaves, rng, 0.2);
                // Rideau de feuilles retombantes autour de la couronne.
                for i in 0..16 {
                    let a = f64::from(i) * std::f64::consts::TAU / 16.0;
                    let r = 3.0 + rng.f64() * 0.7;
                    let (sx, sz) = (
                        tx + (a.cos() * r).round() as i32,
                        tz + (a.sin() * r).round() as i32,
                    );
                    for d in 1..=rng.range(2, 5) {
                        p.leaf(sx, top - d, sz, ids.willow_leaves);
                    }
                }
            }
            TreeType::Baobab => {
                let th = rng.range(9, 13);
                for k in 0..th {
                    let r: i32 = if k < th * 6 / 10 { 2 } else { 1 };
                    for dz in -r..=r {
                        for dx in -r..=r {
                            if r == 2 && dx.abs() == 2 && dz.abs() == 2 {
                                continue;
                            }
                            p.solid(x + dx, y + k, z + dz, ids.baobab_wood);
                        }
                    }
                }
                let top = y + th;
                let branches = rng.range(5, 7);
                for i in 0..branches {
                    let a = f64::from(i) * std::f64::consts::TAU / f64::from(branches)
                        + rng.f64() * 0.6;
                    let len = rng.range(4, 6);
                    let ex = x + (a.cos() * f64::from(len)).round() as i32;
                    let ez = z + (a.sin() * f64::from(len)).round() as i32;
                    p.line((x, top - 2, z), (ex, top + 1, ez), ids.baobab_wood);
                    p.blob(
                        (ex, top + 2, ez),
                        (3.4, 1.4, 3.4),
                        ids.acacia_leaves,
                        rng,
                        0.3,
                    );
                }
                p.blob(
                    (x, top + 1, z),
                    (2.6, 1.3, 2.6),
                    ids.acacia_leaves,
                    rng,
                    0.2,
                );
            }
            TreeType::Redwood => {
                let th = rng.range(24, 34);
                for k in 0..th {
                    // Empattement 3×3 sur les trois premiers blocs.
                    let flare: &[(i32, i32)] = if k < 3 {
                        &[
                            (-1, 0),
                            (0, 0),
                            (1, 0),
                            (-1, 1),
                            (0, 1),
                            (1, 1),
                            (2, 0),
                            (2, 1),
                            (0, -1),
                            (1, -1),
                            (0, 2),
                            (1, 2),
                        ]
                    } else {
                        &[(0, 0), (1, 0), (0, 1), (1, 1)]
                    };
                    for &(dx, dz) in flare {
                        p.solid(x + dx, y + k, z + dz, ids.redwood_wood);
                    }
                }
                for d in (th * 2 / 5)..th + 3 {
                    let left = th + 2 - d;
                    let r = 4.min((f64::from(left) * 0.22).floor() as i32 + 1);
                    let r = if d % 4 == 0 && r > 1 { r - 1 } else { r };
                    for dz in -r..=r + 1 {
                        for dx in -r..=r + 1 {
                            let ax = if dx > 0 { dx - 1 } else { dx };
                            let az = if dz > 0 { dz - 1 } else { dz };
                            if ax.abs() + az.abs() > r + r / 2 {
                                continue;
                            }
                            p.leaf(x + dx, y + d, z + dz, ids.spruce_leaves);
                        }
                    }
                }
            }
            TreeType::Saguaro => {
                let th = rng.range(4, 8);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.cactus);
                }
                for side in 0..2 {
                    if side == 1 && !rng.chance(0.7) {
                        continue;
                    }
                    let (dx, dz) = [(1, 0), (-1, 0), (0, 1), (0, -1)][rng.range(0, 3) as usize];
                    let by = y + rng.range(2, th - 2).max(2);
                    let out = rng.range(1, 2);
                    for i in 1..=out {
                        p.solid(x + dx * i, by, z + dz * i, ids.cactus);
                    }
                    for k in 1..=rng.range(2, 3) {
                        p.solid(x + dx * out, by + k, z + dz * out, ids.cactus);
                    }
                }
            }
            TreeType::Umbrella => {
                let th = rng.range(5, 8);
                let (mut tx, mut tz) = (x, z);
                let (dx, dz) = [(1, 0), (-1, 0), (0, 1), (0, -1)][rng.range(0, 3) as usize];
                for k in 0..th {
                    p.solid(tx, y + k, tz, ids.acacia_wood);
                    if k == th / 2 {
                        tx += dx;
                        tz += dz;
                        p.solid(tx, y + k, tz, ids.acacia_wood);
                    }
                }
                let top = y + th;
                for i in 0..3 {
                    let a = f64::from(i) * std::f64::consts::TAU / 3.0 + rng.f64();
                    let ex = tx + (a.cos() * 3.0).round() as i32;
                    let ez = tz + (a.sin() * 3.0).round() as i32;
                    p.line((tx, top - 1, tz), (ex, top + 1, ez), ids.acacia_wood);
                    p.blob(
                        (ex, top + 2, ez),
                        (3.6, 1.2, 3.6),
                        ids.acacia_leaves,
                        rng,
                        0.25,
                    );
                }
                p.blob(
                    (tx, top + 2, tz),
                    (3.0, 1.1, 3.0),
                    ids.acacia_leaves,
                    rng,
                    0.2,
                );
            }
            TreeType::BambooGrove => {
                for _ in 0..rng.range(7, 13) {
                    let bx = x + rng.range(-3, 3);
                    let bz = z + rng.range(-3, 3);
                    let g = self.terrain.column(bx, bz);
                    if g.height < self.cfg.sea_level || g.river < 0.7 {
                        continue;
                    }
                    for k in 0..rng.range(5, 12) {
                        p.solid(bx, g.height + k, bz, ids.bamboo);
                    }
                }
            }
            TreeType::Juniper => {
                let th = rng.range(2, 3);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.spruce_wood);
                }
                p.blob(
                    (x, y + th + 1, z),
                    (1.9, 2.8, 1.9),
                    ids.spruce_leaves,
                    rng,
                    0.25,
                );
            }
            _ => {}
        }
    }

    /// Plantes de `BiomeConfig::flora`, posées avant la végétation d'origine (elles ont donc
    /// la priorité sur les herbes hautes). Retourne sans rien faire si le biome n'a pas de flore.
    pub(crate) fn nb_flora(&self, chunk: &mut Chunk, wx0: i32, wz0: i32, grid: &[NbColumn]) {
        let ids = &self.ids;
        let c = &*self.cfg;
        let sea = c.sea_level;
        for lx in 0..SX {
            for lz in 0..SZ {
                let col = grid[gi(lx, lz)];
                let h = col.height;
                if h < sea || h >= SY - 14 || col.lava_y > 0 || col.river < 0.6 {
                    continue;
                }
                let bc = self.biomes.config(col.biome);
                if bc.flora.is_empty() || chunk.block(lx, h, lz) != ids.air {
                    continue;
                }
                let below = chunk.block(lx, h - 1, lz);
                let soil = soil_of(ids, below);
                if soil == 0 {
                    continue;
                }
                let (wx, wz) = (wx0.wrapping_add(lx), wz0.wrapping_add(lz));
                let hash = hash_position(
                    wx.wrapping_mul(7).wrapping_add(211),
                    wz.wrapping_mul(13).wrapping_sub(97),
                );
                let r = unit(hash);
                let patch = self.noise.fbm2d(
                    f64::from(wx) * 0.05 + 410.0,
                    f64::from(wz) * 0.05 - 130.0,
                    2,
                    2.0,
                    0.5,
                );
                let wet = || {
                    [(-1, 0), (1, 0), (0, -1), (0, 1)]
                        .iter()
                        .any(|&(dx, dz)| grid[gi(lx + dx, lz + dz)].height < sea)
                };
                let mut acc = 0.0;
                for e in &bc.flora {
                    if e.soil & soil == 0 || e.chance <= 0.0 {
                        continue;
                    }
                    if e.wet && !(h <= sea + 1 && wet()) {
                        continue;
                    }
                    let mut chance = e.chance * c.vegetation_density;
                    if e.patchy {
                        // Champs : forte densité dans les plaques, presque rien ailleurs.
                        chance *= if patch > 0.55 { 2.4 } else { 0.08 };
                    }
                    acc += chance;
                    if r >= acc {
                        continue;
                    }
                    let (lo, hi) = e.height;
                    let n = if hi > lo {
                        i32::from(lo) + (hash / 7) % (i32::from(hi - lo) + 1)
                    } else {
                        i32::from(lo)
                    };
                    for k in 0..n {
                        if h + k < SY && chunk.block(lx, h + k, lz) == ids.air {
                            chunk.set_block(lx, h + k, lz, e.block);
                        }
                    }
                    break;
                }
            }
        }
    }
}

fn soil_of(ids: &nb_data::BlockIds, b: BlockId) -> u8 {
    if b == ids.grass || b == ids.podzol || b == ids.moss {
        SOIL_GRASS
    } else if b == ids.sand || b == ids.red_sand || b == ids.terracotta {
        SOIL_SAND
    } else if b == ids.snow {
        SOIL_SNOW
    } else if b == ids.ash || b == ids.basalt {
        SOIL_ASH
    } else {
        0
    }
}
