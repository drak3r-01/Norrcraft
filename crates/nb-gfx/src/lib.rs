//! nb-gfx — rendu Vulkan de Norrblock (voir `docs/spec/nb-gfx.md`).
//!
//! Organisation :
//! - parties pures, testables sans GPU : [`sky`] (cycle jour/nuit), [`camera`], [`atlas`]
//!   (tableaux de textures CPU), [`packs`] (packs de ressources), [`frame`] (réglages, entrées
//!   de frame, bloc d'uniformes) ;
//! - [`vk`] : contexte Vulkan (`ash` + `gpu-allocator`), ressources et [`vk::SceneRenderer`],
//!   qui enregistre une frame complète dans n'importe quelle image couleur ;
//!   [`vk::OffscreenRenderer`] l'utilise sans fenêtre (tests headless, lavapipe) ;
//! - [`window`] (feature `window`) : surface, swapchain et intégration winit.
//!
//! Les shaders GLSL 450 (`shaders/glsl450`) sont précompilés en SPIR-V (`shaders/spv`, script
//! `scripts/compile-shaders.sh`).

pub mod atlas;
pub mod camera;
pub mod frame;
pub mod packs;
pub mod shaders;
pub mod sky;
pub mod vk;
#[cfg(feature = "window")]
pub mod window;

pub use atlas::TextureArrayCpu;
pub use camera::Camera;
pub use frame::{EyeFluid, FrameInput, FrameUniforms, GfxSettings, ShadowQuality};
pub use nb_core::ChunkPos;
pub use nb_mesh::ChunkVertex;
pub use packs::{find_packs_root, list_resource_packs, ResourcePackMeta};
pub use sky::{heat_from_nearest_lava_dist_sq, sky_params, SkyClock, SkyParams};
pub use vk::{GfxError, GpuContext, OffscreenRenderer, SceneAssets, SceneRenderer};
#[cfg(feature = "window")]
pub use window::Renderer;

/// Re-export pour les appelants qui enregistrent des commandes dans la passe overlay.
pub use ash;
