//! nb-ui — interface 2D de Norrblock (voir `docs/spec/nb-ui.md`).
//!
//! Pur CPU : produit des [`DrawList`] (quads colorés / texturés, glyphes, icônes de blocs)
//! rendues par `nb_gfx::ui_pass`.

pub mod draw;
pub mod font;
pub mod textures;

pub use draw::{Batch, DrawList, Rgba, Scissor, TextureRef, Vertex};
pub use textures::UiTexture;
