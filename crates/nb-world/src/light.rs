//! Propagation de la lumière (`ChunkManager.computeLightAsync`, spec §10).
//!
//! Un seul canal 0..15 (ciel + émission), recalcul complet d'un chunk sur un volume étendu
//! de 2 blocs dans les voisins (20 × 130 × 20).

use std::collections::VecDeque;

use nb_data::BlockRegistry;

use crate::chunk::{Chunk, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, CHUNK_VOLUME};

const W: i32 = 20;
const H: i32 = 130;
const AREA: i32 = W * H; // 2600
const VOLUME: usize = (W * H * W) as usize;
const SY: i32 = CHUNK_SIZE_Y as i32;

/// Propriétés lumineuses effectives par identifiant de bloc.
#[derive(Debug, Clone)]
pub struct LightTable {
    emission: [u8; 256],
    /// Atténuation effective : 15 si « bloquant », sinon `max(1, json)`.
    atten: [u8; 256],
}

impl LightTable {
    pub fn new(reg: &BlockRegistry) -> Self {
        let ids = reg.ids();
        let mut emission = [0u8; 256];
        let mut atten = [1u8; 256];
        for id in 0..=255u8 {
            let b = reg.from_id(id);
            let transparent_to_light = b.id == ids.air
                || b.id == ids.water
                || b.id == ids.lava
                || b.cross_shaped
                || b.id == ids.lily_pad
                || b.leaves_tinted;
            emission[usize::from(id)] = b.light_emission;
            atten[usize::from(id)] = if transparent_to_light {
                b.light_attenuation.max(1)
            } else {
                15
            };
        }
        Self { emission, atten }
    }

    #[inline]
    pub fn emission(&self, id: u8) -> u8 {
        self.emission[usize::from(id)]
    }

    #[inline]
    pub fn attenuation(&self, id: u8) -> u8 {
        self.atten[usize::from(id)]
    }
}

/// Tampons de travail réutilisables (un par thread de lumière).
pub struct LightContext {
    light: Vec<u8>,
    atten: Vec<u8>,
    queue: VecDeque<(i32, i32, i32)>,
}

impl Default for LightContext {
    fn default() -> Self {
        Self {
            light: vec![0; VOLUME],
            atten: vec![0; VOLUME],
            queue: VecDeque::with_capacity(65_536),
        }
    }
}

#[inline]
fn idx(lx: i32, y: i32, lz: i32) -> usize {
    ((lx + 2) + (y + 1) * W + (lz + 2) * AREA) as usize
}

/// Bloc dans le voisinage 3×3 (`cache[(dz+1)*3 + (dx+1)]`) ; absent ou hors hauteur → air.
#[inline]
fn block_at(cache: &[Option<&Chunk>; 9], lx: i32, y: i32, lz: i32) -> u8 {
    if !(0..SY).contains(&y) {
        return 0;
    }
    let (sx, sz) = (CHUNK_SIZE_X as i32, CHUNK_SIZE_Z as i32);
    let (cx, lx) = if lx < 0 {
        (0, lx + sx)
    } else if lx >= sx {
        (2, lx - sx)
    } else {
        (1, lx)
    };
    let (cz, lz) = if lz < 0 {
        (0, lz + sz)
    } else if lz >= sz {
        (2, lz - sz)
    } else {
        (1, lz)
    };
    cache[cz * 3 + cx].map_or(0, |c| c.block(lx, y, lz))
}

impl LightContext {
    /// Calcule la lumière du chunk central `cache[4]` et la renvoie (ordre `x + 16y + 2048z`).
    pub fn compute(
        &mut self,
        cache: &[Option<&Chunk>; 9],
        table: &LightTable,
    ) -> Box<[u8; CHUNK_VOLUME]> {
        self.light.fill(0);
        self.atten.fill(0);
        self.queue.clear();

        // Passe colonne (lumière du ciel).
        for lx in -2..=17 {
            for lz in -2..=17 {
                let mut cur: i32 = 15;
                for y in (-1..=SY).rev() {
                    let mut atten: i32 = 1;
                    let mut emission: i32 = 0;
                    if (0..SY).contains(&y) {
                        let b = block_at(cache, lx, y, lz);
                        emission = i32::from(table.emission(b));
                        atten = i32::from(table.attenuation(b));
                    } else if y < 0 {
                        atten = 15;
                    }
                    let i = idx(lx, y, lz);
                    self.atten[i] = atten as u8;
                    if y < SY {
                        cur = if atten == 1 && cur == 15 {
                            15
                        } else {
                            (cur - atten).max(0)
                        };
                        cur = cur.max(emission);
                    }
                    self.light[i] = cur as u8;
                    if cur > 0 {
                        self.queue.push_back((lx, y, lz));
                    }
                }
            }
        }

        // BFS.
        const OFFSETS: [(i32, i32, i32); 6] = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];
        while let Some((cx, cy, cz)) = self.queue.pop_front() {
            let cl = i32::from(self.light[idx(cx, cy, cz)]);
            if cl <= 1 {
                continue;
            }
            for &(ox, oy, oz) in &OFFSETS {
                let (nx, ny, nz) = (cx + ox, cy + oy, cz + oz);
                if !(-2..=17).contains(&nx) || !(-1..=SY).contains(&ny) || !(-2..=17).contains(&nz)
                {
                    continue;
                }
                let ni = idx(nx, ny, nz);
                let at = i32::from(self.atten[ni]);
                if at >= 15 {
                    continue;
                }
                let p = if oy == -1 && cl == 15 && at == 1 {
                    15
                } else {
                    cl - at
                };
                if p > i32::from(self.light[ni]) {
                    self.light[ni] = p as u8;
                    self.queue.push_back((nx, ny, nz));
                }
            }
        }

        let mut out: Box<[u8; CHUNK_VOLUME]> = vec![0u8; CHUNK_VOLUME]
            .into_boxed_slice()
            .try_into()
            .expect("taille de chunk");
        for lz in 0..CHUNK_SIZE_Z as i32 {
            for y in 0..SY {
                for lx in 0..CHUNK_SIZE_X as i32 {
                    out[nb_core::math::block_index(lx as usize, y as usize, lz as usize)] =
                        self.light[idx(lx, y, lz)];
                }
            }
        }
        out
    }
}

/// Version pure et autonome du calcul (§10) : `neighbors[(dz+1)*3 + (dx+1)]`, centre en 4.
pub fn compute_light(
    neighbors: &[Option<&Chunk>; 9],
    table: &LightTable,
) -> Box<[u8; CHUNK_VOLUME]> {
    LightContext::default().compute(neighbors, table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_core::ChunkPos;

    #[test]
    fn effective_attenuations() {
        let reg = BlockRegistry::builtin();
        let t = LightTable::new(&reg);
        let ids = reg.ids();
        assert_eq!(t.attenuation(ids.air), 1);
        assert_eq!(t.attenuation(ids.tall_grass), 1);
        assert_eq!(t.attenuation(ids.water), 2);
        assert_eq!(t.attenuation(ids.leaves), 2);
        assert_eq!(t.attenuation(ids.lava), 3);
        assert_eq!(t.emission(ids.lava), 15);
        assert_eq!(t.attenuation(ids.stone), 15);
        assert_eq!(t.attenuation(ids.cactus), 15);
        assert_eq!(t.attenuation(200), 1);
    }

    #[test]
    fn empty_world_is_fully_lit_and_floor_blocks() {
        let reg = BlockRegistry::builtin();
        let t = LightTable::new(&reg);
        let c = Chunk::new(ChunkPos::new(0, 0));
        let mut cache: [Option<&Chunk>; 9] = [None; 9];
        cache[4] = Some(&c);
        let l = compute_light(&cache, &t);
        assert!(l.iter().all(|&v| v == 15));

        // Un plafond de pierre à y=100 sur tout le voisinage : l'ombre en dessous décroît.
        let mut roof = Chunk::new(ChunkPos::new(0, 0));
        for x in 0..16 {
            for z in 0..16 {
                roof.set_block(x, 100, z, reg.ids().stone);
            }
        }
        let cache: [Option<&Chunk>; 9] = [Some(&roof); 9];
        let l = compute_light(&cache, &t);
        assert_eq!(l[nb_core::math::block_index(5, 101, 5)], 15);
        assert_eq!(l[nb_core::math::block_index(5, 100, 5)], 0);
        assert_eq!(l[nb_core::math::block_index(5, 99, 5)], 0);
    }
}
