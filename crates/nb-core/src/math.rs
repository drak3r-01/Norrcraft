//! Positions et dimensions du monde (mêmes valeurs que le Java).

use serde::{Deserialize, Serialize};

/// Largeur d'un chunk en X.
pub const CHUNK_SIZE_X: usize = 16;
/// Hauteur d'un chunk en Y.
pub const CHUNK_SIZE_Y: usize = 128;
/// Profondeur d'un chunk en Z.
pub const CHUNK_SIZE_Z: usize = 16;
/// Nombre de blocs par chunk.
pub const CHUNK_VOLUME: usize = CHUNK_SIZE_X * CHUNK_SIZE_Y * CHUNK_SIZE_Z;

/// Index linéaire d'un bloc local : `x + 16 y + 2048 z` (identique à `Chunk.java` ; la doc Java
/// qui annonce `x + 16 z + 256 y` est inexacte).
#[inline]
pub const fn block_index(x: usize, y: usize, z: usize) -> usize {
    x + CHUNK_SIZE_X * y + CHUNK_SIZE_X * CHUNK_SIZE_Y * z
}

/// Division entière arrondie vers le bas (équivalent de `Math.floorDiv`, diviseur > 0).
#[inline]
pub const fn floor_div(a: i32, b: i32) -> i32 {
    a.div_euclid(b)
}

/// Modulo toujours positif (équivalent de `Math.floorMod`, diviseur > 0).
#[inline]
pub const fn floor_mod(a: i32, b: i32) -> i32 {
    a.rem_euclid(b)
}

/// Coordonnées d'un chunk (équivalent du `record ChunkPos(int x, int z)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Chunk contenant le bloc monde `(wx, wz)`.
    #[inline]
    pub const fn from_block(wx: i32, wz: i32) -> Self {
        Self {
            x: wx.div_euclid(CHUNK_SIZE_X as i32),
            z: wz.div_euclid(CHUNK_SIZE_Z as i32),
        }
    }

    /// Coin minimal (monde) du chunk en X.
    #[inline]
    pub const fn origin_x(self) -> i32 {
        self.x * CHUNK_SIZE_X as i32
    }

    /// Coin minimal (monde) du chunk en Z.
    #[inline]
    pub const fn origin_z(self) -> i32 {
        self.z * CHUNK_SIZE_Z as i32
    }
}

/// Position entière d'un bloc dans le monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn chunk(self) -> ChunkPos {
        ChunkPos::from_block(self.x, self.z)
    }

    /// Coordonnées locales dans le chunk (`x`, `z` dans `0..16`, `y` inchangé).
    pub const fn local(self) -> (usize, i32, usize) {
        (
            self.x.rem_euclid(CHUNK_SIZE_X as i32) as usize,
            self.y,
            self.z.rem_euclid(CHUNK_SIZE_Z as i32) as usize,
        )
    }
}

/// `(int) Math.floor(v)` de Java (le cast `as` sature comme Java).
#[inline]
pub fn floor_i32(v: f64) -> i32 {
    v.floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_matches_java_layout() {
        assert_eq!(block_index(0, 0, 0), 0);
        assert_eq!(block_index(1, 0, 0), 1);
        assert_eq!(block_index(0, 1, 0), 16);
        assert_eq!(block_index(0, 0, 1), 2048);
        assert_eq!(block_index(15, 127, 15), CHUNK_VOLUME - 1);
    }

    #[test]
    fn chunk_from_negative_blocks() {
        assert_eq!(ChunkPos::from_block(-1, -1), ChunkPos::new(-1, -1));
        assert_eq!(ChunkPos::from_block(-16, 15), ChunkPos::new(-1, 0));
        assert_eq!(ChunkPos::from_block(-17, 16), ChunkPos::new(-2, 1));
        assert_eq!(BlockPos::new(-1, 5, -17).local(), (15, 5, 15));
    }

    #[test]
    fn floor_helpers() {
        assert_eq!(floor_div(-1, 16), -1);
        assert_eq!(floor_div(16, 16), 1);
        assert_eq!(floor_mod(-1, 16), 15);
        assert_eq!(floor_i32(-0.5), -1);
    }
}
