//! Monde en mémoire pour tester la simulation sans chunks (doc 07 §3.1).

use std::collections::HashMap;
use std::sync::Arc;

use nb_core::glam::IVec3;
use nb_core::ChunkPos;
use nb_data::BlockId;

use super::layer::FluidCode;
use super::table::{WaterRest, WaterTable};
use super::FluidWorld;

/// Boîte de cellules `[min, max)` ; tout ce qui est hors de la boîte est « non chargé » (mur).
#[derive(Debug, Clone)]
pub struct MemFluid {
    cells: HashMap<IVec3, (BlockId, FluidCode)>,
    min: IVec3,
    max: IVec3,
    air: BlockId,
    rests: HashMap<ChunkPos, WaterTable>,
    has_rests: bool,
    /// Nombre d'écritures reçues (test « plus aucune écriture »).
    pub writes: u64,
    pub meshed: Vec<ChunkPos>,
}

impl MemFluid {
    /// Boîte remplie d'air.
    pub fn new(min: IVec3, max: IVec3, air: BlockId) -> Self {
        Self {
            cells: HashMap::new(),
            min,
            max,
            air,
            rests: HashMap::new(),
            has_rests: false,
            writes: 0,
            meshed: Vec::new(),
        }
    }

    /// Agrandit la boîte (un chunk voisin se charge).
    pub fn grow(&mut self, min: IVec3, max: IVec3) {
        self.min = self.min.min(min);
        self.max = self.max.max(max);
    }

    fn inside(&self, p: IVec3) -> bool {
        p.cmpge(self.min).all() && p.cmplt(self.max).all()
    }

    /// Pose un bloc sans compter l'écriture.
    pub fn set(&mut self, p: IVec3, id: BlockId, code: FluidCode) {
        if self.inside(p) {
            self.cells.insert(p, (id, code));
        }
    }

    /// Remplit la boîte `[a, b)` d'un bloc.
    pub fn fill(&mut self, a: IVec3, b: IVec3, id: BlockId) {
        for x in a.x..b.x {
            for y in a.y..b.y {
                for z in a.z..b.z {
                    self.set(IVec3::new(x, y, z), id, FluidCode::DEFAULT);
                }
            }
        }
    }

    /// Nappe générée de la colonne `(x, z)`.
    pub fn set_rest(&mut self, x: i32, z: i32, rest: WaterRest) {
        self.has_rests = true;
        let cp = ChunkPos::new(x.div_euclid(16), z.div_euclid(16));
        let t = self.rests.entry(cp).or_default();
        t.0[(z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize] = rest;
    }

    pub fn has_rests(&self) -> bool {
        self.has_rests
    }

    pub fn get(&self, p: IVec3) -> Option<(BlockId, FluidCode)> {
        self.cell(p)
    }

    /// Empreinte de tout le contenu (déterminisme).
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for x in self.min.x..self.max.x {
            for y in self.min.y..self.max.y {
                for z in self.min.z..self.max.z {
                    let (id, c) = self.cell(IVec3::new(x, y, z)).unwrap_or((0, FluidCode(0)));
                    out.push(id);
                    out.push(c.0);
                }
            }
        }
        out
    }
}

impl FluidWorld for MemFluid {
    fn cell(&self, p: IVec3) -> Option<(BlockId, FluidCode)> {
        if !self.inside(p) {
            return None;
        }
        Some(
            self.cells
                .get(&p)
                .copied()
                .unwrap_or((self.air, FluidCode::DEFAULT)),
        )
    }

    fn write(&mut self, p: IVec3, id: BlockId, code: FluidCode) {
        if self.inside(p) {
            self.writes += 1;
            self.cells.insert(p, (id, code));
        }
    }

    fn table(&self, cpos: ChunkPos) -> Option<Arc<WaterTable>> {
        if !self.has_rests {
            return None;
        }
        Some(Arc::new(self.rests.get(&cpos).cloned().unwrap_or_default()))
    }

    fn mark_mesh(&mut self, cpos: ChunkPos) {
        self.meshed.push(cpos);
    }
}
