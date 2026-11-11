//! Chunk de 16×128×16 blocs (`Chunk.java`) et copies immuables pour le mailleur.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use nb_core::math::block_index;
pub use nb_core::math::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, CHUNK_VOLUME};
use nb_core::ChunkPos;
use nb_data::BlockId;

/// Identifiant de l'air (toujours 0 dans `blocks.json`).
pub const AIR: BlockId = 0;

/// Lumière renvoyée hors limites et valeur initiale d'un chunk.
pub const FULL_LIGHT: u8 = 15;

/// Index Java d'un voxel : `x + 16·y + 2048·z` (identique au layout interne Rust).
#[inline]
pub const fn java_index(x: usize, y: usize, z: usize) -> usize {
    block_index(x, y, z)
}

/// Index local si `(x, y, z)` est dans le chunk.
#[inline]
fn local_index(x: i32, y: i32, z: i32) -> Option<usize> {
    if (0..CHUNK_SIZE_X as i32).contains(&x)
        && (0..CHUNK_SIZE_Y as i32).contains(&y)
        && (0..CHUNK_SIZE_Z as i32).contains(&z)
    {
        Some(block_index(x as usize, y as usize, z as usize))
    } else {
        None
    }
}

fn boxed_volume(fill: u8) -> Box<[u8; CHUNK_VOLUME]> {
    vec![fill; CHUNK_VOLUME]
        .into_boxed_slice()
        .try_into()
        .expect("taille de chunk")
}

/// Données d'un chunk : voxels et carte de lumière (les drapeaux sont dans [`ChunkFlags`]).
#[derive(Clone, PartialEq, Eq)]
pub struct Chunk {
    pos: ChunkPos,
    voxels: Box<[BlockId; CHUNK_VOLUME]>,
    light: Box<[u8; CHUNK_VOLUME]>,
}

impl std::fmt::Debug for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chunk")
            .field("pos", &self.pos)
            .finish_non_exhaustive()
    }
}

impl Chunk {
    /// Chunk vide (air), lumière à 15 partout.
    pub fn new(pos: ChunkPos) -> Self {
        Self {
            pos,
            voxels: boxed_volume(AIR),
            light: boxed_volume(FULL_LIGHT),
        }
    }

    /// Chunk à partir de voxels (copiés, ordre `x + 16y + 2048z`) ; un tableau court est complété
    /// par de l'air, les octets en trop sont ignorés.
    pub fn from_voxels(pos: ChunkPos, voxels: &[BlockId]) -> Self {
        let mut c = Self::new(pos);
        let n = voxels.len().min(CHUNK_VOLUME);
        c.voxels[..n].copy_from_slice(&voxels[..n]);
        c
    }

    pub fn pos(&self) -> ChunkPos {
        self.pos
    }

    /// Remplace tous les voxels (réseau : chunk modifié reçu de l'hôte).
    pub fn replace_voxels(&mut self, voxels: &[BlockId; CHUNK_VOLUME]) {
        self.voxels.copy_from_slice(voxels);
    }

    /// Identifiant brut du bloc ; hors limites → air.
    #[inline]
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId {
        local_index(x, y, z).map_or(AIR, |i| self.voxels[i])
    }

    /// Écrit un bloc ; `false` (et rien n'est fait) si hors limites.
    #[inline]
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: BlockId) -> bool {
        match local_index(x, y, z) {
            Some(i) => {
                self.voxels[i] = id;
                true
            }
            None => false,
        }
    }

    /// Lumière 0..15 ; hors limites → 15.
    #[inline]
    pub fn light(&self, x: i32, y: i32, z: i32) -> u8 {
        local_index(x, y, z).map_or(FULL_LIGHT, |i| self.light[i])
    }

    /// Écrit la lumière (hors limites ignoré).
    #[inline]
    pub fn set_light(&mut self, x: i32, y: i32, z: i32, l: u8) {
        if let Some(i) = local_index(x, y, z) {
            self.light[i] = l;
        }
    }

    pub fn voxels(&self) -> &[BlockId; CHUNK_VOLUME] {
        &self.voxels
    }

    pub fn light_map(&self) -> &[u8; CHUNK_VOLUME] {
        &self.light
    }

    /// Remplace toute la carte de lumière.
    pub fn set_light_map(&mut self, light: &[u8; CHUNK_VOLUME]) {
        self.light.copy_from_slice(light);
    }

    /// Positions locales des blocs de lave (« heat sources » du Java, calculées à la demande).
    pub fn heat_sources(&self, lava: BlockId) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
        self.voxels
            .iter()
            .enumerate()
            .filter(move |(_, &b)| b == lava)
            .map(|(i, _)| {
                (
                    i % CHUNK_SIZE_X,
                    (i / CHUNK_SIZE_X) % CHUNK_SIZE_Y,
                    i / (CHUNK_SIZE_X * CHUNK_SIZE_Y),
                )
            })
    }

    /// Empreinte FNV-1a 64 bits des voxels dans l'ordre Java (vecteurs de référence §16).
    pub fn fingerprint(&self) -> u64 {
        fnv1a(self.voxels.as_slice())
    }
}

/// FNV-1a 64 bits.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Drapeaux partagés hors verrou (équivalents des `volatile boolean` Java).
#[derive(Debug)]
pub struct ChunkFlags {
    /// À persister.
    pub dirty: AtomicBool,
    /// Maillage à reconstruire.
    pub mesh_dirty: AtomicBool,
    /// Lumière à recalculer.
    pub light_dirty: AtomicBool,
}

impl Default for ChunkFlags {
    /// Les trois drapeaux à `true`, comme après `new Chunk(...)`.
    fn default() -> Self {
        Self {
            dirty: AtomicBool::new(true),
            mesh_dirty: AtomicBool::new(true),
            light_dirty: AtomicBool::new(true),
        }
    }
}

impl ChunkFlags {
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }
    pub fn is_mesh_dirty(&self) -> bool {
        self.mesh_dirty.load(Ordering::Acquire)
    }
    pub fn is_light_dirty(&self) -> bool {
        self.light_dirty.load(Ordering::Acquire)
    }
    pub fn set_dirty(&self, v: bool) {
        self.dirty.store(v, Ordering::Release);
    }
    pub fn set_mesh_dirty(&self, v: bool) {
        self.mesh_dirty.store(v, Ordering::Release);
    }
    pub fn set_light_dirty(&self, v: bool) {
        self.light_dirty.store(v, Ordering::Release);
    }
    /// Effet de `Chunk.setBlock` sur les drapeaux.
    pub fn mark_all(&self) {
        self.set_dirty(true);
        self.set_light_dirty(true);
        self.set_mesh_dirty(true);
    }
}

/// Entrée de la table des chunks actifs.
#[derive(Debug)]
pub struct ChunkEntry {
    pub chunk: RwLock<Chunk>,
    pub flags: ChunkFlags,
    /// Incrémenté à chaque écriture de bloc : un calcul de lumière commencé avant une écriture
    /// ne remet pas `light_dirty` à `false` (correction de la course du Java).
    edits: AtomicU64,
}

/// Chunk actif partagé entre threads.
pub type SharedChunk = Arc<ChunkEntry>;

impl ChunkEntry {
    pub fn new(chunk: Chunk) -> Self {
        Self {
            chunk: RwLock::new(chunk),
            flags: ChunkFlags::default(),
            edits: AtomicU64::new(0),
        }
    }

    /// Lecture (un verrou empoisonné reste lisible : les données sont des octets).
    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, Chunk> {
        self.chunk.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, Chunk> {
        self.chunk.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Écrit un bloc et pose les trois drapeaux, même si le bloc ne change pas (`Chunk.setBlock`).
    /// Hors limites : rien n'est écrit et les drapeaux restent inchangés.
    pub fn set_block(&self, x: i32, y: i32, z: i32, id: BlockId) -> bool {
        let mut c = self.write();
        let ok = c.set_block(x, y, z, id);
        if ok {
            self.edits.fetch_add(1, Ordering::AcqRel);
            self.flags.mark_all();
        }
        ok
    }

    /// Remplace tous les voxels et pose les trois drapeaux (chunk reçu par le réseau).
    pub fn replace_voxels(&self, voxels: &[BlockId; CHUNK_VOLUME]) {
        self.write().replace_voxels(voxels);
        self.edits.fetch_add(1, Ordering::AcqRel);
        self.flags.mark_all();
    }

    /// Remplace tous les voxels **si aucune écriture n'a eu lieu** depuis que le compteur valait
    /// `expected` (surcouche saisonnière calculée sur une copie). Le verrou en écriture est tenu
    /// pendant le test : une écriture concurrente attend, puis ne se perd pas.
    pub fn replace_voxels_if_unchanged(
        &self,
        voxels: &[BlockId; CHUNK_VOLUME],
        expected: u64,
    ) -> bool {
        let mut g = self.write();
        if self.edit_count() != expected {
            return false;
        }
        g.replace_voxels(voxels);
        self.edits.fetch_add(1, Ordering::AcqRel);
        self.flags.mark_all();
        true
    }

    /// Compteur d'écritures (voir [`ChunkEntry::edits`]).
    pub fn edit_count(&self) -> u64 {
        self.edits.load(Ordering::Acquire)
    }

    pub fn pos(&self) -> ChunkPos {
        self.read().pos()
    }
}

/// Copie immuable (voxels + lumière) pour le maillage en thread. `None` → air / lumière 15.
#[derive(Clone, Default)]
pub struct ChunkSnapshot {
    voxels: Option<Box<[BlockId; CHUNK_VOLUME]>>,
    light: Option<Box<[u8; CHUNK_VOLUME]>>,
}

impl std::fmt::Debug for ChunkSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkSnapshot")
            .field("present", &self.voxels.is_some())
            .finish()
    }
}

impl ChunkSnapshot {
    pub fn of(chunk: Option<&Chunk>) -> Self {
        match chunk {
            Some(c) => Self {
                voxels: Some(c.voxels.clone()),
                light: Some(c.light.clone()),
            },
            None => Self::default(),
        }
    }

    pub fn is_present(&self) -> bool {
        self.voxels.is_some()
    }

    #[inline]
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId {
        match (&self.voxels, local_index(x, y, z)) {
            (Some(v), Some(i)) => v[i],
            _ => AIR,
        }
    }

    #[inline]
    pub fn light(&self, x: i32, y: i32, z: i32) -> u8 {
        match (&self.light, local_index(x, y, z)) {
            (Some(l), Some(i)) => l[i],
            _ => FULL_LIGHT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_chunk_defaults_and_bounds() {
        let mut c = Chunk::new(ChunkPos::new(1, -2));
        assert_eq!(c.block(0, 0, 0), AIR);
        assert_eq!(c.light(3, 3, 3), 15);
        assert_eq!(c.light(-1, 0, 0), 15);
        assert_eq!(c.block(0, 128, 0), AIR);
        assert!(!c.set_block(16, 0, 0, 3));
        assert!(c.set_block(15, 127, 15, 3));
        assert_eq!(c.voxels()[CHUNK_VOLUME - 1], 3);
        assert!(c.set_block(1, 2, 3, 7));
        assert_eq!(c.voxels()[java_index(1, 2, 3)], 7);
        assert_eq!(java_index(1, 2, 3), 1 + 32 + 3 * 2048);
        c.set_light(1, 2, 3, 4);
        assert_eq!(c.light(1, 2, 3), 4);
        c.set_block(4, 5, 6, 33);
        let hs: Vec<_> = c.heat_sources(33).collect();
        assert_eq!(hs, vec![(4, 5, 6)]);
    }

    #[test]
    fn from_voxels_pads_with_air() {
        let c = Chunk::from_voxels(ChunkPos::new(0, 0), &[3, 3, 3]);
        assert_eq!(c.block(2, 0, 0), 3);
        assert_eq!(c.block(3, 0, 0), AIR);
        assert_eq!(c.light(0, 0, 0), 15);
    }

    #[test]
    fn entry_flags_and_snapshot() {
        let e = ChunkEntry::new(Chunk::new(ChunkPos::new(0, 0)));
        assert!(e.flags.is_dirty() && e.flags.is_light_dirty() && e.flags.is_mesh_dirty());
        e.flags.set_dirty(false);
        e.flags.set_light_dirty(false);
        assert!(!e.set_block(0, -1, 0, 3));
        assert!(!e.flags.is_dirty());
        assert!(e.set_block(0, 0, 0, 3));
        assert!(e.flags.is_dirty() && e.flags.is_light_dirty());
        assert_eq!(e.edit_count(), 1);
        let s = ChunkSnapshot::of(Some(&e.read()));
        assert_eq!(s.block(0, 0, 0), 3);
        let empty = ChunkSnapshot::of(None);
        assert_eq!((empty.block(0, 0, 0), empty.light(0, 0, 0)), (AIR, 15));
    }

    #[test]
    fn fnv_of_empty_chunk() {
        // 32768 zéros : valeur calculée indépendamment de l'ordre.
        let c = Chunk::new(ChunkPos::new(0, 0));
        assert_eq!(c.fingerprint(), fnv1a(&[0u8; CHUNK_VOLUME]));
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
    }
}
