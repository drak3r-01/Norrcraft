//! Accès aux blocs du monde (spec §19.2) : trait implémenté pour `nb_world::World` et une
//! implémentation en mémoire pour les tests.

use std::collections::{HashMap, HashSet};

use nb_core::glam::IVec3;
use nb_core::math::{floor_div, CHUNK_SIZE_Y};
use nb_data::BlockId;

/// Accès aux blocs utilisé par les systèmes.
pub trait BlockAccess {
    /// Bloc en `(x, y, z)` ; AIR hors chunk chargé ou hors `y ∈ [0, 128)`.
    fn block(&self, x: i32, y: i32, z: i32) -> BlockId;
    /// Écrit un bloc (ignoré hors chunk chargé).
    fn set_block(&mut self, x: i32, y: i32, z: i32, id: BlockId);
    /// Le chunk contenant la colonne monde `(wx, wz)` est-il chargé ?
    fn is_chunk_loaded(&self, wx: i32, wz: i32) -> bool;
    /// Teinte `leavesTint` du biome en `(x, z)` (particules de feuilles).
    fn leaves_tint(&self, x: i32, z: i32) -> [f64; 3];

    /// Raccourci sur un `IVec3`.
    fn block_at(&self, p: IVec3) -> BlockId {
        self.block(p.x, p.y, p.z)
    }
}

/// Adaptateur sur un `nb_world::World` partagé (toutes ses méthodes prennent `&self`), ce qui
/// permet de tenir en même temps `world.config_mut()` pour le [`crate::TickCtx`].
#[derive(Debug, Clone, Copy)]
pub struct WorldBlocks<'a>(pub &'a nb_world::World);

impl BlockAccess for WorldBlocks<'_> {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockId {
        self.0.block(x, y, z)
    }

    fn set_block(&mut self, x: i32, y: i32, z: i32, id: BlockId) {
        self.0.set_block(x, y, z, id);
    }

    fn is_chunk_loaded(&self, wx: i32, wz: i32) -> bool {
        self.0.is_chunk_loaded(wx, wz)
    }

    fn leaves_tint(&self, x: i32, z: i32) -> [f64; 3] {
        let biomes = self.0.biome_system();
        biomes.config(biomes.biome_at(x, z)).leaves_tint
    }
}

/// Monde de blocs en mémoire (tests, outils) : tout ce qui n'est pas écrit est de l'air.
///
/// Par défaut tous les chunks sont « chargés » ; [`MemoryBlocks::set_loaded_chunks`]
/// restreint l'ensemble des chunks chargés.
#[derive(Debug, Clone, Default)]
pub struct MemoryBlocks {
    blocks: HashMap<(i32, i32, i32), BlockId>,
    loaded: Option<HashSet<(i32, i32)>>,
    air: BlockId,
    tint: [f64; 3],
}

impl MemoryBlocks {
    /// Monde vide (air = `air`).
    pub fn new(air: BlockId) -> Self {
        Self {
            air,
            tint: [0.3, 0.5, 0.2],
            ..Self::default()
        }
    }

    /// Restreint les chunks chargés à la liste fournie (coordonnées de chunk).
    pub fn set_loaded_chunks(&mut self, chunks: impl IntoIterator<Item = (i32, i32)>) {
        self.loaded = Some(chunks.into_iter().collect());
    }

    /// Tous les chunks redeviennent chargés.
    pub fn load_all(&mut self) {
        self.loaded = None;
    }

    /// Teinte de feuilles renvoyée partout.
    pub fn set_leaves_tint(&mut self, tint: [f64; 3]) {
        self.tint = tint;
    }

    /// Remplit la boîte `[min, max]` (bornes inclusives).
    pub fn fill(&mut self, min: IVec3, max: IVec3, id: BlockId) {
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                for z in min.z..=max.z {
                    self.put(x, y, z, id);
                }
            }
        }
    }

    /// Écrit un bloc sans tenir compte des chunks chargés.
    pub fn put(&mut self, x: i32, y: i32, z: i32, id: BlockId) {
        if !(0..CHUNK_SIZE_Y as i32).contains(&y) {
            return;
        }
        if id == self.air {
            self.blocks.remove(&(x, y, z));
        } else {
            self.blocks.insert((x, y, z), id);
        }
    }

    fn chunk_loaded(&self, x: i32, z: i32) -> bool {
        self.loaded
            .as_ref()
            .map_or(true, |s| s.contains(&(floor_div(x, 16), floor_div(z, 16))))
    }
}

impl BlockAccess for MemoryBlocks {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockId {
        if !(0..CHUNK_SIZE_Y as i32).contains(&y) || !self.chunk_loaded(x, z) {
            return self.air;
        }
        self.blocks.get(&(x, y, z)).copied().unwrap_or(self.air)
    }

    fn set_block(&mut self, x: i32, y: i32, z: i32, id: BlockId) {
        if self.chunk_loaded(x, z) {
            self.put(x, y, z, id);
        }
    }

    fn is_chunk_loaded(&self, wx: i32, wz: i32) -> bool {
        self.chunk_loaded(wx, wz)
    }

    fn leaves_tint(&self, _x: i32, _z: i32) -> [f64; 3] {
        self.tint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_blocks_respect_height_and_loaded_chunks() {
        let mut m = MemoryBlocks::new(0);
        m.put(1, 2, 3, 5);
        m.put(0, 128, 0, 5);
        assert_eq!(m.block(1, 2, 3), 5);
        assert_eq!(m.block(0, 128, 0), 0);
        assert_eq!(m.block(0, -1, 0), 0);
        m.set_loaded_chunks([(0, 0)]);
        assert!(m.is_chunk_loaded(15, 15));
        assert!(!m.is_chunk_loaded(-1, 0));
        m.set_block(-1, 5, 0, 7);
        assert_eq!(m.block(-1, 5, 0), 0);
        m.set_block(1, 2, 3, 0);
        assert_eq!(m.block(1, 2, 3), 0);
    }
}
