//! [`FluidWorld`] sur le gestionnaire de chunks : garde en cache les derniers chunks lus pour
//! éviter la table de hachage à chaque lecture.

use std::cell::RefCell;
use std::sync::Arc;

use nb_core::glam::IVec3;
use nb_core::ChunkPos;
use nb_data::BlockId;

use super::{FluidCode, FluidWorld, WaterTable};
use crate::chunk::{SharedChunk, CHUNK_SIZE_Y};
use crate::manager::ChunkManager;

pub struct ManagerFluid<'a> {
    cm: &'a ChunkManager,
    cache: RefCell<Vec<(ChunkPos, SharedChunk)>>,
}

impl<'a> ManagerFluid<'a> {
    pub fn new(cm: &'a ChunkManager) -> Self {
        Self {
            cm,
            cache: RefCell::new(Vec::with_capacity(4)),
        }
    }

    fn chunk(&self, cp: ChunkPos) -> Option<SharedChunk> {
        let mut cache = self.cache.borrow_mut();
        if let Some((_, c)) = cache.iter().find(|(p, _)| *p == cp) {
            return Some(c.clone());
        }
        let c = self.cm.chunk(cp)?;
        if cache.len() >= 4 {
            cache.remove(0);
        }
        cache.push((cp, c.clone()));
        Some(c)
    }
}

#[inline]
fn split(p: IVec3) -> (ChunkPos, i32, i32) {
    (
        ChunkPos::new(p.x.div_euclid(16), p.z.div_euclid(16)),
        p.x.rem_euclid(16),
        p.z.rem_euclid(16),
    )
}

impl FluidWorld for ManagerFluid<'_> {
    fn cell(&self, p: IVec3) -> Option<(BlockId, FluidCode)> {
        if p.y < 0 || p.y >= CHUNK_SIZE_Y as i32 {
            return None;
        }
        let (cp, lx, lz) = split(p);
        let c = self.chunk(cp)?;
        let g = c.read();
        Some((g.block(lx, p.y, lz), g.fluid_code(lx, p.y, lz)))
    }

    fn write(&mut self, p: IVec3, id: BlockId, code: FluidCode) {
        if p.y < 0 || p.y >= CHUNK_SIZE_Y as i32 {
            return;
        }
        let (cp, lx, lz) = split(p);
        if let Some(c) = self.chunk(cp) {
            c.set_fluid(lx, p.y, lz, id, code);
        }
    }

    fn table(&self, cpos: ChunkPos) -> Option<Arc<WaterTable>> {
        self.cm
            .terrain()
            .water_table(cpos.x, cpos.z)
            .map(|b| Arc::new(*b))
    }

    fn mark_mesh(&mut self, cpos: ChunkPos) {
        self.cm.mark_mesh_dirty(cpos);
    }

    fn scan(
        &self,
        cpos: ChunkPos,
        spring: BlockId,
        explicit: &mut Vec<(IVec3, FluidCode)>,
        springs: &mut Vec<IVec3>,
    ) {
        let Some(c) = self.chunk(cpos) else {
            return;
        };
        let g = c.read();
        let at = |i: usize| {
            IVec3::new(
                cpos.x * 16 + (i % 16) as i32,
                ((i / 16) % CHUNK_SIZE_Y) as i32,
                cpos.z * 16 + (i / (16 * CHUNK_SIZE_Y)) as i32,
            )
        };
        if let Some(layer) = g.fluid_layer() {
            for (i, &b) in layer.iter().enumerate() {
                if b != 0 {
                    explicit.push((at(i), FluidCode(b)));
                }
            }
        }
        if spring != nb_data::UNASSIGNED_ID {
            for (i, &v) in g.voxels().iter().enumerate() {
                if v == spring {
                    springs.push(at(i));
                }
            }
        }
    }
}
