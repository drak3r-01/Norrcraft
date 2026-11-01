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
//!
//! Écarts appliqués par rapport au Java (tous prévus par la spec) :
//! - shadow map effacée à 1.0 au démarrage et au passage à OFF (§10, décision 2026-11-01) ;
//! - `LESS_OR_EQUAL` pour toutes les pipelines 3D (§6.2) ; surbrillance sans écriture de
//!   profondeur (profondeur HDR attachée en lecture seule, §18.4.7) ;
//! - nuages échantillonnés en `(cellule + 0.25) / 256` (§5.3) ; bruit à graine fixe ;
//! - mips de l'atlas calculés sur CPU (boîte 2×2) ; redimensionnement Catmull-Rom (§5.1) ;
//! - rendu sauté pour un extent nul, cibles recréées dès que l'extent change (§3.3).
//!
//! Bogues Java conservés : seuils 1000/2000 de `shadow.vert` (correctif derrière
//! [`GfxSettings::fix_shadow_layer_thresholds`]), bright buffer effacé à la couleur de
//! brouillard, fluides sans écriture de profondeur, occlusion à une frame (ou plus) de retard.
//!
//! Pas encore porté : mobs, particules (passe scène B2, §14–§15).

pub mod atlas;
pub mod camera;
pub mod frame;
pub mod jem;
pub mod mob_model;
pub mod packs;
pub mod shaders;
pub mod sky;
pub mod vk;
#[cfg(feature = "window")]
pub mod window;

pub use atlas::TextureArrayCpu;
pub use camera::Camera;
pub use frame::{EyeFluid, FrameInput, FrameUniforms, GfxSettings, ShadowQuality};
pub use jem::parse_jem;
pub use mob_model::{
    load_all_mob_models, load_mob_model, ColorBox, MobModelCpu, MobModelDesc, MobRoots, MobVertex,
};
pub use nb_core::ChunkPos;
pub use nb_mesh::ChunkVertex;
pub use packs::{find_packs_root, list_resource_packs, ResourcePackMeta};
pub use sky::{heat_from_nearest_lava_dist_sq, sky_params, SkyClock, SkyParams};
pub use vk::{GfxError, GpuContext, OffscreenRenderer, SceneAssets, SceneRenderer};
#[cfg(feature = "window")]
pub use window::Renderer;

/// Re-export pour les appelants qui enregistrent des commandes dans la passe overlay.
pub use ash;
