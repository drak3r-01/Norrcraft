//! Couche Vulkan (`ash` + `gpu-allocator`). Rien ici ne dépend d'une fenêtre : le
//! [`SceneRenderer`] enregistre une frame dans une image couleur quelconque (image de la
//! swapchain ou image hors écran de l'[`OffscreenRenderer`]).

mod context;
mod mobs;
mod offscreen;
mod pipelines;
mod resources;
mod scene;
mod targets;

pub use context::{validation_error_count, GpuContext};
pub use offscreen::OffscreenRenderer;
pub use resources::{Buffer, Image, ImageDesc, Transition};
pub use scene::{
    FrameToken, OutputTarget, OverlayPass, SceneAssets, SceneRenderer, FRAMES_IN_FLIGHT,
    HDR_DEPTH_FORMAT, OUTPUT_FORMAT,
};

/// Erreurs du rendu.
#[derive(Debug, thiserror::Error)]
pub enum GfxError {
    #[error("chargement de la bibliothèque Vulkan impossible : {0}")]
    Loading(#[from] ash::LoadingError),
    #[error("erreur Vulkan : {0}")]
    Vk(#[from] ash::vk::Result),
    #[error("allocation GPU : {0}")]
    Alloc(#[from] gpu_allocator::AllocationError),
    #[error("aucun GPU compatible (Vulkan 1.3, dynamic rendering, synchronization2)")]
    NoDevice,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GfxError>;
