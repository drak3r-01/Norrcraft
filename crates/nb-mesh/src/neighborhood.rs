//! Voisinage 3×3 de chunks reçu par le mesher (spec §3.1–3.2).
//!
//! Le mesher ne dépend pas de `nb-world` : il reçoit des tranches d'identifiants de blocs
//! (une par chunk, indexées `x + 16·y + 2048·z`).

use nb_core::math::{block_index, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, CHUNK_VOLUME};

use crate::block::BlockTable;

/// Copie des voxels d'un chunk (équivalent de `ChunkSnapshot`, sans la lightmap que le mesher
/// n'utilise jamais).
#[derive(Clone, PartialEq, Eq)]
pub struct ChunkVoxels(Box<[u8]>);

impl std::fmt::Debug for ChunkVoxels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChunkVoxels(..)")
    }
}

impl ChunkVoxels {
    /// Chunk rempli d'un seul identifiant.
    pub fn filled(id: u8) -> Self {
        Self(vec![id; CHUNK_VOLUME].into_boxed_slice())
    }

    /// Depuis un tableau de `CHUNK_VOLUME` octets.
    ///
    /// # Panics
    /// Si la longueur n'est pas `CHUNK_VOLUME`.
    pub fn from_vec(v: Vec<u8>) -> Self {
        assert_eq!(v.len(), CHUNK_VOLUME, "ChunkVoxels: taille invalide");
        Self(v.into_boxed_slice())
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        self.0[block_index(x, y, z)]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, id: u8) {
        self.0[block_index(x, y, z)] = id;
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

/// 9 chunks, index `cz*3 + cx` (`cx, cz ∈ 0..3`, centre = 4). `None` = chunk absent (air).
#[derive(Clone, Copy, Debug)]
pub struct Neighborhood<'a> {
    chunks: [Option<&'a [u8]>; 9],
}

impl<'a> Neighborhood<'a> {
    /// # Panics
    /// Si une tranche n'a pas `CHUNK_VOLUME` éléments.
    pub fn new(chunks: [Option<&'a [u8]>; 9]) -> Self {
        for c in chunks.iter().flatten() {
            assert_eq!(
                c.len(),
                CHUNK_VOLUME,
                "Neighborhood: tranche de taille invalide"
            );
        }
        Self { chunks }
    }

    /// Depuis des [`ChunkVoxels`].
    pub fn from_voxels(chunks: [Option<&'a ChunkVoxels>; 9]) -> Self {
        Self::new(chunks.map(|c| c.map(ChunkVoxels::as_slice)))
    }

    /// Le chunk d'index `cz*3 + cx`.
    #[inline]
    pub fn chunk(&self, index: usize) -> Option<&'a [u8]> {
        self.chunks[index]
    }

    /// `getBlockCached` : identifiant **canonique** du bloc en coordonnées locales au chunk
    /// central (`lx`, `lz` peuvent déborder de ±16). Hors `0..128` en Y, chunk absent ou
    /// identifiant inconnu → air.
    #[inline]
    pub fn block(&self, table: &BlockTable, lx: i32, y: i32, lz: i32) -> u8 {
        let air = table.air_id();
        if y < 0 || y >= CHUNK_SIZE_Y as i32 {
            return air;
        }
        let (mut cx, mut cz, mut lx, mut lz) = (1usize, 1usize, lx, lz);
        let sx = CHUNK_SIZE_X as i32;
        let sz = CHUNK_SIZE_Z as i32;
        if lx < 0 {
            cx = 0;
            lx += sx;
        } else if lx >= sx {
            cx = 2;
            lx -= sx;
        }
        if lz < 0 {
            cz = 0;
            lz += sz;
        } else if lz >= sz {
            cz = 2;
            lz -= sz;
        }
        match self.chunks[cz * 3 + cx] {
            Some(c) if (0..sx).contains(&lx) && (0..sz).contains(&lz) => {
                table.canonical(c[block_index(lx as usize, y as usize, lz as usize)])
            }
            _ => air,
        }
    }
}

/// Cache des identifiants canoniques de la région `lx, lz ∈ [-2, 17]`, `y ∈ [0, 128)` :
/// toutes les lectures du mesher y tombent (optimisation sans effet observable).
pub(crate) struct BlockCache<'a> {
    ids: Vec<u8>,
    n: Neighborhood<'a>,
    table: &'a BlockTable,
}

const PAD: i32 = 2;
const CW: usize = CHUNK_SIZE_X + 4;
const CD: usize = CHUNK_SIZE_Z + 4;

impl<'a> BlockCache<'a> {
    pub(crate) fn new(n: Neighborhood<'a>, table: &'a BlockTable) -> Self {
        let mut ids = vec![table.air_id(); CW * CHUNK_SIZE_Y * CD];
        for ax in 0..CW {
            for y in 0..CHUNK_SIZE_Y {
                for az in 0..CD {
                    ids[(ax * CHUNK_SIZE_Y + y) * CD + az] =
                        n.block(table, ax as i32 - PAD, y as i32, az as i32 - PAD);
                }
            }
        }
        Self { ids, n, table }
    }

    #[inline]
    pub(crate) fn table(&self) -> &'a BlockTable {
        self.table
    }

    /// Identifiant canonique (même sémantique que [`Neighborhood::block`]).
    #[inline]
    pub(crate) fn get(&self, lx: i32, y: i32, lz: i32) -> u8 {
        if y < 0 || y >= CHUNK_SIZE_Y as i32 {
            return self.table.air_id();
        }
        let ax = lx + PAD;
        let az = lz + PAD;
        if ax < 0 || ax >= CW as i32 || az < 0 || az >= CD as i32 {
            return self.n.block(self.table, lx, y, lz);
        }
        self.ids[(ax as usize * CHUNK_SIZE_Y + y as usize) * CD + az as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_data::BlockRegistry;

    #[test]
    fn get_block_cached_crosses_borders() {
        let table = BlockTable::from_registry(&BlockRegistry::builtin(), |_| 0);
        let mut west = ChunkVoxels::filled(0);
        west.set(15, 10, 3, 3);
        let mut south = ChunkVoxels::filled(0);
        south.set(4, 20, 0, 1);
        let mut center = ChunkVoxels::filled(0);
        center.set(0, 0, 0, 200); // inconnu → air
        center.set(1, 127, 1, 5);
        let mut chunks: [Option<&ChunkVoxels>; 9] = [None; 9];
        chunks[3] = Some(&west); // cx=0, cz=1
        chunks[7] = Some(&south); // cx=1, cz=2
        chunks[4] = Some(&center);
        let n = Neighborhood::from_voxels(chunks);
        assert_eq!(n.block(&table, -1, 10, 3), 3);
        assert_eq!(n.block(&table, 4, 20, 16), 1);
        assert_eq!(n.block(&table, 0, 0, 0), 0);
        assert_eq!(n.block(&table, 1, 127, 1), 5);
        assert_eq!(n.block(&table, 1, 128, 1), 0);
        assert_eq!(n.block(&table, 1, -1, 1), 0);
        assert_eq!(n.block(&table, 17, 5, 5), 0); // chunk absent
        let cache = BlockCache::new(n, &table);
        for (x, y, z) in [
            (-1, 10, 3),
            (4, 20, 16),
            (1, 127, 1),
            (-2, 0, 17),
            (40, 3, 3),
        ] {
            assert_eq!(cache.get(x, y, z), n.block(&table, x, y, z));
        }
    }
}
