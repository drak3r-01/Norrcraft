//! Instance GPU d'une particule : 32 octets, un seul draw instancié pour toute la scène.

use bytemuck::{Pod, Zeroable};

/// Échantillonne l'atlas des blocs (set 0, binding 1) au lieu de l'atlas des particules.
pub const FLAG_BLOCK_ATLAS: u32 = 1;
/// Alimente le bloom (la lave, les braises, les lucioles…).
pub const FLAG_EMISSIVE: u32 = 2;
/// Billboard cylindrique : le grand axe reste vertical (pluie, neige, flammes).
pub const FLAG_VERTICAL: u32 = 4;
/// Bits 8..16 : étirement du grand axe ×8 (0 = carré).
pub const STRETCH_SHIFT: u32 = 8;

/// Instance GPU (attributs d'instance du pipeline `particle`).
///
/// | offset | taille | contenu |
/// |-------:|-------:|---------|
/// | 0 | 12 | position monde |
/// | 12 | 4 | taille (côté du quad, en blocs) |
/// | 16 | 4 | couleur RGBA8 (alpha prémultiplié par le fondu) |
/// | 20 | 2 | rotation, 0..65535 → 0..2π |
/// | 22 | 2 | couche d'atlas |
/// | 24 | 4 | sous-rectangle UV `(u0, v0, w, h)` en 1/64 (0 = tuile entière) |
/// | 28 | 4 | drapeaux `FLAG_*` |
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuParticle {
    pub position: [f32; 3],
    pub size: f32,
    pub color: u32,
    pub rotation: u16,
    pub layer: u16,
    pub uv: [u8; 4],
    pub flags: u32,
}

const _: () = assert!(std::mem::size_of::<GpuParticle>() == 32);

impl GpuParticle {
    /// Particule simple : tuile entière, sans rotation, couleur `rgba` (0..1).
    pub fn new(position: [f32; 3], size: f32, layer: u16, rgba: [f32; 4]) -> Self {
        Self {
            position,
            size,
            color: pack_rgba(rgba),
            rotation: 0,
            layer,
            uv: [0; 4],
            flags: 0,
        }
    }
}

/// RGBA flottant 0..1 → RGBA8 (octet 0 = rouge, comme `R8G8B8A8_UNORM`).
#[inline]
pub fn pack_rgba(c: [f32; 4]) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    b(c[0]) | b(c[1]) << 8 | b(c[2]) << 16 | b(c[3]) << 24
}

#[inline]
pub fn unpack_rgba(c: u32) -> [f32; 4] {
    let f = |s: u32| ((c >> s) & 255) as f32 * (1.0 / 255.0);
    [f(0), f(8), f(16), f(24)]
}
