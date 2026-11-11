//! nb-gfx — rendu Vulkan de Norrblock (voir `docs/spec/nb-gfx.md`).
//!
//! Organisation :
//! - parties pures, testables sans GPU : [`sky`] (cycle jour/nuit), [`camera`], [`atlas`]
//!   (tableaux de textures CPU), [`packs`] (packs de ressources), [`frame`] (réglages, entrées
//!   de frame, bloc d'uniformes), [`mob_model`] et [`jem`] (modèles de mobs : boîtes, formes
//!   vanilla, parseur JEM, textures d'entité, matrices modèle et billboard) ;
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
//! - rendu sauté pour un extent nul, cibles recréées dès que l'extent change (§3.3) ;
//! - passe B2 (mobs, particules) : seul `colorBuffer` est attaché, le bright buffer reste
//!   inchangé (§14.1 ; équivalent du `colorWriteMask = 0` prescrit, sans sortie indéfinie) ;
//!   passe sautée sans mob ni particule ;
//! - rechargement d'un modèle de mob ou d'un pack : attente du GPU avant destruction ;
//! - textures d'entité : repli « premier `.png` du dossier » trié sans casse (ordre Windows) ;
//!   `Float.parseFloat` du parseur JEM sans flottants hexadécimaux.
//!
//! Bogues Java conservés : seuils 1000/2000 de `shadow.vert` (correctif derrière
//! [`GfxSettings::fix_shadow_layer_thresholds`]), bright buffer effacé à la couleur de
//! brouillard, fluides sans écriture de profondeur, occlusion à une frame (ou plus) de retard,
//! mobs non éclairés et sans gamma, particules tête en bas et effacées par le brouillard devant
//! le ciel (pas d'écriture de profondeur), sous-chaînes JEM débordant sur la boîte suivante,
//! boîte texturée de dernier recours en UV non normalisées (texture répétée 16 fois).

pub mod atlas;
pub mod camera;
pub mod frame;
pub mod jem;
pub mod mob_model;
pub mod packs;
pub mod particle_tex;
pub mod shaders;
pub mod sky;
pub mod ui_pass;
pub mod vk;
#[cfg(feature = "window")]
pub mod window;

pub use atlas::TextureArrayCpu;
pub use camera::Camera;
pub use frame::{
    EyeFluid, FrameInput, FrameUniforms, GfxSettings, MobInstance, NO_WATER, ParticleInstance, ShadowQuality,
};
pub use jem::parse_jem;
pub use mob_model::{
    load_all_mob_models, load_mob_model, player_model_id, ColorBox, MobModelCpu, MobModelDesc,
    MobRoots, MobVertex, PLAYER_MODEL_IDS,
};
pub use nb_core::ChunkPos;
pub use nb_mesh::ChunkVertex;
pub use packs::{find_packs_root, list_resource_packs, ResourcePackMeta};
pub use sky::{heat_from_nearest_lava_dist_sq, sky_params, sky_params_biased, SkyClock, SkyParams};
pub use vk::{GfxError, GpuContext, OffscreenRenderer, SceneAssets, SceneRenderer};
#[cfg(feature = "window")]
pub use window::Renderer;

/// Re-export pour les appelants qui enregistrent des commandes dans la passe overlay.
pub use ash;
