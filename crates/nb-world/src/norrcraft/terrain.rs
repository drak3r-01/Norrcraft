//! Générateur de terrain Norrcraft (sessions 1.2 et 1.4) : le relief de [`HeightFields`] rempli de
//! roche, de moraine et de gravier, et l'eau de l'hydrographie ([`Hydrography`] : mer, lacs,
//! rivières, toutes étanches).
//!
//! Les blocs de surface définitifs viennent avec les biomes (session 2.1) ; d'ici là la règle est
//! minimale et lisible : granit partout, moraine (`moran`) sur les pentes douces des terres basses,
//! gravier (`grus`) sous l'eau et sur les rives, granit nu sur les parois, les barrages et en
//! altitude.

use nb_data::{BlockId, BlockRegistry};

use super::config::NorrConfig;
use super::fields::FieldGrid;
use super::height::HeightFields;
use super::hydro::{seal, Hydrography, WaterColumn};
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
    hydro: Hydrography,
    blocks: NorrBlocks,
}

impl NorrcraftTerrain {
    pub fn new(seed: i64, cfg: NorrConfig, blocks: NorrBlocks) -> Self {
        Self {
            hydro: Hydrography::new(seed, cfg),
            blocks,
        }
    }

    pub fn heights(&self) -> &HeightFields {
        self.hydro.heights()
    }

    pub fn hydrography(&self) -> &Hydrography {
        &self.hydro
    }

    fn sea(&self) -> i32 {
        self.heights().config().sea_level
    }

    fn world_height(&self) -> i32 {
        self.hydro.hydrology().world_height()
    }

    /// Colonnes d'eau scellées du chunk et de sa bordure (`x0 − 1 ..= x0 + 16`, idem en z) : les
    /// colonnes brutes sont calculées sur 20 × 20 (une bordure de plus pour le scellement), champs
    /// lents lus par un treillis 4 × 4 commun. La bordure scellée sert aux pentes.
    fn water_grid(&self, x0: i32, z0: i32) -> Vec<WaterColumn> {
        let heights = self.hydro.heights();
        let (xa, za) = (x0 - 2, z0 - 2);
        let (xb, zb) = (x0 + CHUNK_SIZE_X as i32 + 1, z0 + CHUNK_SIZE_Z as i32 + 1);
        let grid = FieldGrid::new(heights.macro_fields(), xa, za, xb, zb);
        let lakes = self.hydro.lakes(xa, za, xb, zb);
        let hydro = self.hydro.hydrology();
        let n = (xb - xa + 1) as usize;
        let mut raw = Vec::with_capacity(n * n);
        for wz in za..=zb {
            for wx in xa..=xb {
                let m = grid.at(wx, wz);
                let (x, z) = (f64::from(wx), f64::from(wz));
                let r = heights.relief(x, z, &m);
                raw.push(hydro.column(x, z, &r, &m, &lakes));
            }
        }
        let at = |i: usize, j: usize| raw[j * n + i];
        let mut out = Vec::with_capacity((n - 2) * (n - 2));
        for j in 1..n - 1 {
            for i in 1..n - 1 {
                let nb = [at(i - 1, j), at(i + 1, j), at(i, j - 1), at(i, j + 1)];
                out.push(seal(at(i, j), nb, self.world_height()));
            }
        }
        out
    }
}

impl TerrainGenerator for NorrcraftTerrain {
    fn fork(&self) -> Box<dyn TerrainGenerator> {
        Box::new(self.clone())
    }

    /// Premier `y` non solide de la colonne (le fond, sous une nappe d'eau).
    fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        self.hydro.column(wx, wz).top
    }

    fn biome_at(&self, _wx: i32, _wz: i32) -> BiomeId {
        0
    }

    fn generate_chunk(&self, chunk: &mut Chunk) {
        let pos = chunk.pos();
        let (x0, z0) = (pos.x * CHUNK_SIZE_X as i32, pos.z * CHUNK_SIZE_Z as i32);
        let cols = self.water_grid(x0, z0);
        let n = CHUNK_SIZE_X + 2;
        let top = |i: usize, j: usize| cols[j * n + i].top;
        let sea = self.sea();
        let b = self.blocks;

        for lz in 0..CHUNK_SIZE_Z {
            for lx in 0..CHUNK_SIZE_X {
                let (i, j) = (lx + 1, lz + 1);
                let c = cols[j * n + i];
                let h = c.top;
                let slope = [top(i - 1, j), top(i + 1, j), top(i, j - 1), top(i, j + 1)]
                    .into_iter()
                    .map(|v| (v - h).abs())
                    .max()
                    .unwrap_or(0);
                // Couverture meuble : gravier sous l'eau et sur les rives, moraine sur les terres
                // basses ; rien sur les parois, les barrages ni en altitude.
                let (cover, depth) = if slope > BARE_SLOPE || c.dam {
                    (b.granite, 0)
                } else if c.is_wet() || h <= c.level + 1 {
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
                for y in h..c.level {
                    chunk.set_block(x, y, z, b.water);
                }
            }
        }
    }

    /// Départ cherché en anneaux autour de l'origine, sur la terre sèche au-dessus du niveau de la
    /// mer **du monde Norrcraft** (la configuration vanilla du monde porte encore son propre
    /// niveau) : jamais dans un lac ni une rivière.
    fn find_spawn(&self, _sea_level: i32) -> [f64; 3] {
        find_spawn_location(self.sea(), |x, z| {
            let c = self.hydro.column(x, z);
            if c.is_wet() {
                i32::MIN
            } else {
                c.top
            }
        })
    }

    fn climate_at(&self, wx: i32, wz: i32) -> Option<Climate> {
        let m = self.heights().macro_fields().sample_lattice(wx, wz);
        Some(Climate {
            temperature: m.t0,
            humidity: m.hm,
            latitude: None,
        })
    }
}
