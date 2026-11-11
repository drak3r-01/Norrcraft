//! Générateur de terrain Norrcraft (session 1.2) : le relief de [`HeightFields`] rempli de roche,
//! de moraine et de gravier, la mer à `S`.
//!
//! Les blocs de surface définitifs viennent avec les biomes (session 2.1) ; d'ici là la règle est
//! minimale et lisible : granit partout, moraine (`moran`) sur les pentes douces des terres basses,
//! gravier (`grus`) sur les fonds marins et les grèves, granit nu sur les parois et en altitude.

use nb_data::{BlockId, BlockRegistry};

use super::config::NorrConfig;
use super::fields::FieldGrid;
use super::height::{HeightFields, Relief};
use crate::chunk::{Chunk, CHUNK_SIZE_X, CHUNK_SIZE_Z};
use crate::profile::{BiomeId, Climate, TerrainGenerator};
use crate::world::find_spawn_location;

/// Blocs utilisés par le générateur, lus dans le registre du monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NorrBlocks {
    pub bedrock: BlockId,
    pub granite: BlockId,
    pub till: BlockId,
    pub gravel: BlockId,
    pub water: BlockId,
}

impl NorrBlocks {
    pub fn from_registry(registry: &BlockRegistry) -> Self {
        Self {
            bedrock: registry.get("bedrock").id,
            granite: registry.get("granit").id,
            till: registry.get("moran").id,
            gravel: registry.get("grus").id,
            water: registry.get("water").id,
        }
    }
}

/// Pente (plus grande différence de surface avec les 4 voisines) au-delà de laquelle la roche
/// reste nue.
const BARE_SLOPE: i32 = 2;
/// Au-dessus de S + 50, la moraine laisse place à la roche (en attendant l'étagement de la 2.1).
const TILL_MAX_ABOVE_SEA: i32 = 50;

/// Générateur Norrcraft : fonction pure de (graine, configuration, colonne).
#[derive(Debug, Clone)]
pub struct NorrcraftTerrain {
    heights: HeightFields,
    blocks: NorrBlocks,
}

impl NorrcraftTerrain {
    pub fn new(seed: i64, cfg: NorrConfig, blocks: NorrBlocks) -> Self {
        Self {
            heights: HeightFields::new(seed, cfg),
            blocks,
        }
    }

    pub fn heights(&self) -> &HeightFields {
        &self.heights
    }

    fn sea(&self) -> i32 {
        self.heights.config().sea_level
    }

    fn world_height(&self) -> i32 {
        self.heights
            .config()
            .world_height
            .min(crate::chunk::CHUNK_SIZE_Y as i32)
    }

    /// Relief des colonnes `x0 − 1 ..= x0 + 16` × `z0 − 1 ..= z0 + 16` (le chunk et sa bordure,
    /// pour les pentes), champs lents lus par un treillis 4 × 4 commun.
    fn relief_grid(&self, x0: i32, z0: i32) -> Vec<Relief> {
        let grid = FieldGrid::new(
            self.heights.macro_fields(),
            x0 - 1,
            z0 - 1,
            x0 + CHUNK_SIZE_X as i32,
            z0 + CHUNK_SIZE_Z as i32,
        );
        let n = CHUNK_SIZE_X + 2;
        let mut out = Vec::with_capacity(n * (CHUNK_SIZE_Z + 2));
        for j in 0..(CHUNK_SIZE_Z + 2) as i32 {
            for i in 0..n as i32 {
                let (wx, wz) = (x0 - 1 + i, z0 - 1 + j);
                let m = grid.at(wx, wz);
                out.push(self.heights.relief(f64::from(wx), f64::from(wz), &m));
            }
        }
        out
    }
}

impl TerrainGenerator for NorrcraftTerrain {
    fn fork(&self) -> Box<dyn TerrainGenerator> {
        Box::new(self.clone())
    }

    fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        self.heights.column(wx, wz).top(self.world_height())
    }

    fn biome_at(&self, _wx: i32, _wz: i32) -> BiomeId {
        0
    }

    fn generate_chunk(&self, chunk: &mut Chunk) {
        let pos = chunk.pos();
        let (x0, z0) = (pos.x * CHUNK_SIZE_X as i32, pos.z * CHUNK_SIZE_Z as i32);
        let reliefs = self.relief_grid(x0, z0);
        let hmax = self.world_height();
        let n = CHUNK_SIZE_X + 2;
        let top = |i: usize, j: usize| reliefs[j * n + i].top(hmax);
        let sea = self.sea();
        let b = self.blocks;

        for lz in 0..CHUNK_SIZE_Z {
            for lx in 0..CHUNK_SIZE_X {
                let (i, j) = (lx + 1, lz + 1);
                let h = top(i, j);
                let slope = [top(i - 1, j), top(i + 1, j), top(i, j - 1), top(i, j + 1)]
                    .into_iter()
                    .map(|v| (v - h).abs())
                    .max()
                    .unwrap_or(0);
                // Couverture meuble : gravier sous l'eau et sur les grèves, moraine sur les terres
                // basses ; rien sur les parois ni en altitude.
                let (cover, depth) = if slope > BARE_SLOPE {
                    (b.granite, 0)
                } else if h <= sea + 1 {
                    (b.gravel, 2)
                } else if h < sea + TILL_MAX_ABOVE_SEA {
                    (b.till, 3 - slope)
                } else {
                    (b.granite, 0)
                };
                let (x, z) = (lx as i32, lz as i32);
                for y in 0..h {
                    let id = if y == 0 {
                        b.bedrock
                    } else if y >= h - depth {
                        cover
                    } else {
                        b.granite
                    };
                    chunk.set_block(x, y, z, id);
                }
                for y in h..sea {
                    chunk.set_block(x, y, z, b.water);
                }
            }
        }
    }

    /// Départ cherché en anneaux autour de l'origine, au-dessus du niveau de la mer **du monde
    /// Norrcraft** (la configuration vanilla du monde porte encore son propre niveau).
    fn find_spawn(&self, _sea_level: i32) -> [f64; 3] {
        find_spawn_location(self.sea(), |x, z| self.surface_height(x, z))
    }

    fn climate_at(&self, wx: i32, wz: i32) -> Option<Climate> {
        let m = self.heights.macro_fields().sample_lattice(wx, wz);
        Some(Climate {
            temperature: m.t0,
            humidity: m.hm,
            latitude: None,
        })
    }
}
