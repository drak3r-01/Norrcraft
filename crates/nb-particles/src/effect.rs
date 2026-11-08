//! Description déclarative d'un effet : tout est `const`, les effets de [`crate::presets`] sont
//! de simples valeurs statiques (pas d'allocation, pas de parsing).

use crate::gpu::{FLAG_BLOCK_ATLAS, FLAG_EMISSIVE, FLAG_VERTICAL, STRETCH_SHIFT};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub min: f32,
    pub max: f32,
}

pub const fn r(min: f32, max: f32) -> Range {
    Range { min, max }
}

/// Noms des sprites de l'atlas des particules, dans l'ordre des identifiants [`sprites`].
/// nb-gfx génère les textures procédurales à partir de ces noms ; `leaf` et `lava_drop` sont
/// les PNG historiques.
pub const SPRITE_NAMES: [&str; 11] = [
    "leaf",
    "lava_drop",
    "soft",
    "spark",
    "smoke",
    "drop",
    "snow",
    "bubble",
    "flame",
    "streak",
    "pixel",
];

/// Identifiants de sprites (index dans [`SPRITE_NAMES`]).
pub mod sprites {
    pub const LEAF: u16 = 0;
    pub const LAVA_DROP: u16 = 1;
    pub const SOFT: u16 = 2;
    pub const SPARK: u16 = 3;
    pub const SMOKE: u16 = 4;
    pub const DROP: u16 = 5;
    pub const SNOW: u16 = 6;
    pub const BUBBLE: u16 = 7;
    pub const FLAME: u16 = 8;
    pub const STREAK: u16 = 9;
    pub const PIXEL: u16 = 10;
}

/// Comportements par particule (bits de `Particle::behav`).
pub mod behav {
    /// Collision avec les blocs pleins (rebond / arrêt).
    pub const COLLIDE: u32 = 1;
    /// Meurt en entrant dans l'eau.
    pub const DIE_IN_WATER: u32 = 2;
    /// Meurt en sortant de l'eau (bulles).
    pub const BUBBLE: u32 = 4;
    /// Meurt dans la lave.
    pub const DIE_IN_LAVA: u32 = 8;
    /// Meurt au premier contact avec un bloc.
    pub const DIE_ON_HIT: u32 = 16;
    /// Éclaboussure de pluie à l'impact (sol ou eau).
    pub const RAIN_SPLASH: u32 = 32;
    /// Fondu d'entrée rapide (premiers 12 % de la vie).
    pub const FADE_IN: u32 = 64;
    /// Couleur/alpha interpolés en `t⁴` : opaque presque jusqu à la fin (éclats de blocs).
    pub const FADE_LATE: u32 = 128;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectDef {
    pub sprite: u16,
    pub life: Range,
    pub speed: Range,
    /// 0 = tous le long de `dir`, 1 = dans toutes les directions.
    pub spread: f32,
    /// Blocs/s², positif = chute (la lave remonte avec une valeur négative).
    pub gravity: f32,
    /// Amortissement par seconde (`v *= 1 - drag·dt`).
    pub drag: f32,
    /// Amplitude du balancement horizontal (feuilles, neige).
    pub sway: f32,
    pub size: Range,
    /// Facteur de taille en fin de vie.
    pub size_end: f32,
    pub color: [f32; 4],
    pub color_end: [f32; 4],
    /// Variation aléatoire de la luminosité de la couleur (±).
    pub color_jitter: f32,
    pub spin: Range,
    /// `FLAG_*` GPU.
    pub gpu_flags: u32,
    /// `behav::*`.
    pub behav: u32,
    pub bounce: f32,
    pub friction: f32,
    /// Demi-côtés de la boîte d'apparition.
    pub jitter: [f32; 3],
}

impl EffectDef {
    pub const DEFAULT: EffectDef = EffectDef {
        sprite: sprites::SOFT,
        life: r(1.0, 1.0),
        speed: r(0.0, 0.0),
        spread: 1.0,
        gravity: 0.0,
        drag: 0.0,
        sway: 0.0,
        size: r(0.1, 0.1),
        size_end: 1.0,
        color: [1.0, 1.0, 1.0, 1.0],
        color_end: [1.0, 1.0, 1.0, 0.0],
        color_jitter: 0.0,
        spin: r(0.0, 0.0),
        gpu_flags: 0,
        behav: 0,
        bounce: 0.0,
        friction: 0.0,
        jitter: [0.0; 3],
    };

    pub const fn emissive(mut self) -> Self {
        self.gpu_flags |= FLAG_EMISSIVE;
        self
    }

    /// Billboard cylindrique étiré `stretch` fois (1..31).
    pub const fn vertical(mut self, stretch: f32) -> Self {
        self.gpu_flags |= FLAG_VERTICAL | (((stretch * 8.0) as u32) << STRETCH_SHIFT);
        self
    }

    pub const fn with_behav(mut self, b: u32) -> Self {
        self.behav |= b;
        self
    }

    /// Indique que l'effet échantillonne l'atlas des blocs (mis par `spawn` si `block_layer`).
    pub const fn block(mut self) -> Self {
        self.gpu_flags |= FLAG_BLOCK_ATLAS;
        self
    }
}
