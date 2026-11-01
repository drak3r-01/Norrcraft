//! nb-mesh — maillage CPU des chunks de Norrblock (voir `docs/spec/nb-mesh.md`).
//!
//! Pur CPU, aucune dépendance GPU : les sorties sont des tampons de sommets
//! ([`ChunkVertex`], 36 octets, triangles non indexés) prêts à être uploadés.
//!
//! - [`mesh_chunk`] : lumière locale, greedy meshing, AO, teintes, croix, nénuphars ;
//! - [`MeshScheduler`] : ordonnancement des remaillages (`ChunkMeshManager`) sans GPU ;
//! - [`Frustum`] / [`ChunkQuadtree`] : culling des chunks ;
//! - [`camera`] : matrices GL nécessaires au culling ;
//! - [`highlight_lines`] : contour du bloc ciblé.
//!
//! Le crate ne dépend pas de `nb-world` : il lit les blocs via [`Neighborhood`] (tranches
//! d'identifiants) et l'état des chunks via le trait [`ChunkSource`].

pub mod biome;
pub mod block;
pub mod camera;
pub mod culling;
pub mod highlight;
pub mod light;
pub mod mesher;
pub mod neighborhood;
pub mod scheduler;
pub mod vertex;

pub use biome::{java_tints, BiomeId, BiomeSource, BiomeTints, FnBiomes, UniformBiome};
pub use block::{should_render_face, BlockMeshInfo, BlockRole, BlockTable};
pub use camera::{clamp_pitch, mul_mat4, perspective_gl, view_matrix};
pub use culling::{ChunkQuadtree, Frustum};
pub use highlight::highlight_lines;
pub use light::{light_scale, LightVolume};
pub use mesher::{ao_scale, calc_ao, mesh_chunk, Face, MeshOutput, OFFSETS};
pub use neighborhood::{ChunkVoxels, Neighborhood};
pub use scheduler::{default_threads, ChunkSource, MeshAction, MeshResult, MeshScheduler};
pub use vertex::{ChunkVertex, LayerCode, LAYER_CROSS_OFFSET, LAYER_LEAVES_OFFSET};

pub use nb_core::math::{CHUNK_SIZE_X as CHUNK_SX, CHUNK_SIZE_Y as CHUNK_SY, CHUNK_SIZE_Z as CHUNK_SZ};
pub use nb_core::ChunkPos;
