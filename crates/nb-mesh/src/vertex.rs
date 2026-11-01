//! Format de sommet du maillage de chunk et encodage du layer de texture (spec §4).

use bytemuck::{Pod, Zeroable};

/// Décalage ajouté au layer des blocs croisés (`emitCross`).
pub const LAYER_CROSS_OFFSET: f32 = 100_000.0;
/// Décalage ajouté au layer des blocs `leavesTinted` (`emitQuad`).
pub const LAYER_LEAVES_OFFSET: f32 = 200_000.0;

/// Un sommet du maillage : 9 `f32`, 36 octets, identique au `float[]` du Java.
///
/// | location | attribut | offset |
/// |---|---|---|
/// | 0 | position monde | 0 |
/// | 1 | UV (unités bloc) | 12 |
/// | 2 | layer encodé ([`LayerCode`]) | 20 |
/// | 3 | couleur (teinte × lumière × AO × dir) | 24 |
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct ChunkVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub layer: f32,
    pub color: [f32; 3],
}

const _: () = assert!(std::mem::size_of::<ChunkVertex>() == 36);

impl ChunkVertex {
    /// Taille d'un sommet en octets (stride du vertex buffer).
    pub const STRIDE: usize = 36;
    /// Offsets des attributs (locations 0..3).
    pub const OFFSET_POS: usize = 0;
    pub const OFFSET_UV: usize = 12;
    pub const OFFSET_LAYER: usize = 20;
    pub const OFFSET_COLOR: usize = 24;

    /// Le sommet appartient-il à la passe fluide (layer négatif) ?
    #[inline]
    pub fn is_fluid(&self) -> bool {
        self.layer < 0.0
    }
}

/// Layer de texture décodé.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerCode {
    /// Layer brut (blocs ordinaires, nénuphars).
    Plain(u32),
    /// Fluide (eau, lave) : `-layer - 1`.
    Fluid(u32),
    /// Feuilles (`leavesTinted`) : `layer + 200000`.
    Leaves(u32),
    /// Bloc croisé : `layer + 100000`.
    Cross(u32),
}

impl LayerCode {
    /// Valeur flottante écrite dans le sommet (mêmes opérations f32 que le Java).
    pub fn encode(self) -> f32 {
        match self {
            LayerCode::Plain(l) => l as f32,
            LayerCode::Fluid(l) => -(l as f32) - 1.0,
            LayerCode::Leaves(l) => l as f32 + LAYER_LEAVES_OFFSET,
            LayerCode::Cross(l) => l as f32 + LAYER_CROSS_OFFSET,
        }
    }

    /// Décodage dans l'ordre des tests de `main.vert` : `< 0`, `>= 200000`, `>= 100000`.
    pub fn decode(v: f32) -> LayerCode {
        if v < 0.0 {
            LayerCode::Fluid((-v - 1.0) as u32)
        } else if v >= LAYER_LEAVES_OFFSET {
            LayerCode::Leaves((v - LAYER_LEAVES_OFFSET) as u32)
        } else if v >= LAYER_CROSS_OFFSET {
            LayerCode::Cross((v - LAYER_CROSS_OFFSET) as u32)
        } else {
            LayerCode::Plain(v as u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_36_bytes() {
        assert_eq!(std::mem::size_of::<ChunkVertex>(), ChunkVertex::STRIDE);
        assert_eq!(std::mem::align_of::<ChunkVertex>(), 4);
        let v = ChunkVertex {
            pos: [1.0, 2.0, 3.0],
            uv: [4.0, 5.0],
            layer: 6.0,
            color: [7.0, 8.0, 9.0],
        };
        let f: &[f32] = bytemuck::cast_slice(std::slice::from_ref(&v));
        assert_eq!(f, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
        assert_eq!(ChunkVertex::OFFSET_UV, 3 * 4);
        assert_eq!(ChunkVertex::OFFSET_LAYER, 5 * 4);
        assert_eq!(ChunkVertex::OFFSET_COLOR, 6 * 4);
    }

    #[test]
    fn layer_roundtrip() {
        for l in [0u32, 1, 7, 63, 1000, 99_999] {
            for code in [
                LayerCode::Plain(l),
                LayerCode::Fluid(l),
                LayerCode::Leaves(l),
                LayerCode::Cross(l),
            ] {
                if matches!(code, LayerCode::Plain(_)) && l >= 100_000 {
                    continue;
                }
                assert_eq!(LayerCode::decode(code.encode()), code);
            }
        }
        assert_eq!(LayerCode::Fluid(0).encode(), -1.0);
        assert_eq!(LayerCode::Fluid(5).encode(), -6.0);
        assert_eq!(LayerCode::Leaves(3).encode(), 200_003.0);
        assert_eq!(LayerCode::Cross(3).encode(), 100_003.0);
    }
}
