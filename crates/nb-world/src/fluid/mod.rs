//! Eau vivante (doc 07) : couche fluide des chunks, classes de blocs, table d'eau, automate
//! cellulaire conservatif, sources, pluie, gel et courants.
//!
//! Tout le comportement est écrit contre le trait [`FluidWorld`] : la simulation se teste sans
//! chunks avec [`MemFluid`].

pub mod blocks;
pub mod flow;
pub mod managed;
pub mod layer;
pub mod mem;
pub mod sim;
pub mod sources;
pub mod table;

pub use blocks::{FluidBlocks, FluidClass};
pub use layer::{Dir, FluidCode, FULL};
pub use mem::MemFluid;
pub use sim::{FluidSim, FluidStats};
pub use table::{RestKind, WaterRest, WaterTable};

use std::sync::Arc;

use nb_core::glam::{IVec3, Vec3};
use nb_core::ChunkPos;
use nb_data::BlockId;

/// Abstraction du monde pour la simulation (doc 07 §3.1).
pub trait FluidWorld {
    /// `None` : chunk non chargé ou hors limites (la cellule est alors un mur).
    fn cell(&self, p: IVec3) -> Option<(BlockId, FluidCode)>;
    /// Écrit voxel et octet fluide (sans journal d'éditions).
    fn write(&mut self, p: IVec3, id: BlockId, code: FluidCode);
    /// Table d'eau du chunk ; `None` si l'univers n'en a pas.
    fn table(&self, cpos: ChunkPos) -> Option<Arc<WaterTable>>;
    /// Demande le remaillage d'un chunk (fin de pas).
    fn mark_mesh(&mut self, _cpos: ChunkPos) {}
    /// Parcourt un chunk chargé : cellules à octet fluide explicite et positions du bloc
    /// `spring`. Implémentation par défaut lente, surchargée par le gestionnaire de chunks.
    fn scan(
        &self,
        cpos: ChunkPos,
        spring: BlockId,
        explicit: &mut Vec<(IVec3, FluidCode)>,
        springs: &mut Vec<IVec3>,
    ) {
        for y in 0..128 {
            for z in 0..16 {
                for x in 0..16 {
                    let p = IVec3::new(cpos.x * 16 + x, y, cpos.z * 16 + z);
                    if let Some((id, code)) = self.cell(p) {
                        if !code.is_default() {
                            explicit.push((p, code));
                        }
                        if id == spring {
                            springs.push(p);
                        }
                    }
                }
            }
        }
    }
}

/// Événements du gestionnaire de chunks à consommer par la simulation.
#[derive(Clone, Debug, Default)]
pub struct FluidEvents {
    pub edits: Vec<IVec3>,
    pub loaded: Vec<ChunkPos>,
    pub unloaded: Vec<ChunkPos>,
}

impl FluidEvents {
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.loaded.is_empty() && self.unloaded.is_empty()
    }
}

/// Changement d'une cellule pendant un pas (réseau, doc 07 §12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidDelta {
    pub pos: IVec3,
    pub block: BlockId,
    pub code: u8,
}

/// Effets ponctuels pour les particules et le son.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FluidFx {
    /// Eau contre lave : vapeur.
    Quench(IVec3),
}

/// Contexte extérieur d'un tick.
#[derive(Clone, Debug, Default)]
pub struct FluidEnv {
    /// Pas de saison 0..8.
    pub season_step: Option<u8>,
    /// Pluie 0..1.
    pub rain: f32,
    /// °C ressentis au sol près du joueur.
    pub air_temp: f32,
    pub daylight: bool,
    /// Joueurs (local et distants).
    pub centers: Vec<Vec3>,
}

/// Prend 8 h dans la cellule (pleine ou infinie). `false` sinon.
pub fn scoop(w: &mut dyn FluidWorld, sim: &mut FluidSim, p: IVec3) -> bool {
    sim.scoop(w, p)
}

/// Verse `amount` h dans la cellule ouverte `p`. Renvoie ce qui a été versé.
pub fn pour(w: &mut dyn FluidWorld, sim: &mut FluidSim, p: IVec3, amount: u8) -> u8 {
    sim.pour(w, p, amount)
}

/// Générateur pseudo-aléatoire déterministe (SplitMix64).
#[derive(Clone, Debug)]
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        crate::norrcraft::math::mix64(self.0.wrapping_sub(0x9E37_79B9_7F4A_7C15))
    }

    /// Entier dans `0..n` (`n > 0`).
    pub fn below(&mut self, n: u32) -> u32 {
        ((self.next_u64() >> 32) % u64::from(n.max(1))) as u32
    }

    /// Entier dans `-r..=r`.
    pub fn around(&mut self, r: i32) -> i32 {
        self.below(2 * r as u32 + 1) as i32 - r
    }
}
