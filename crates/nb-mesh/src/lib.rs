//! nb-mesh — maillage CPU des chunks de Norrblock (voir `docs/spec/nb-mesh.md`).

pub mod biome;
pub mod block;
pub mod light;
pub mod mesher;
pub mod neighborhood;
pub mod vertex;

pub use biome::{java_tints, BiomeId, BiomeSource, BiomeTints, FnBiomes, UniformBiome};
pub use block::{should_render_face, BlockMeshInfo, BlockRole, BlockTable};
pub use light::{light_scale, LightVolume};
pub use mesher::{ao_scale, calc_ao, mesh_chunk, Face, MeshOutput, OFFSETS};
pub use neighborhood::{ChunkVoxels, Neighborhood};
pub use vertex::{ChunkVertex, LayerCode, LAYER_CROSS_OFFSET, LAYER_LEAVES_OFFSET};

pub use nb_core::ChunkPos;
