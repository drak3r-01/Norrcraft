//! Moteur d'animation squelettique des mobs de Norrblock.
//!
//! * [`def`] : description d'un rig en données JSON (os, clips, démarches, idles, regard…) ;
//! * [`rig`] : compilation en tables d'échantillons (`Rig`, partagé par tous les mobs d'un
//!   modèle) ;
//! * [`animator`] : état d'animation d'un mob (`Animator`) et évaluation de sa palette de
//!   matrices de peau (`Mat34`), que le vertex shader applique aux sommets (peau rigide : chaque
//!   boîte de la géométrie suit un os) ;
//! * [`humanoid`] : rig de l'avatar des joueurs d'une partie LAN.
//!
//! Le crate est pur CPU et ne dépend que de `glam` et `serde`.

pub mod animator;
pub mod def;
pub mod humanoid;
pub mod rig;
pub mod xf;

pub use animator::{AnimInput, Animator, Scratch};
pub use def::RigDef;
pub use rig::{Action, Rig, RigError, MAX_BONES};
pub use xf::{Mat34, Xf};
