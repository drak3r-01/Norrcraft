//! Octet de la couche fluide d'un chunk (doc 07, §2.1).
//!
//! * bits 0..3 : code. 0 = défaut (niveau de la classe du bloc) ; 1..=9 = niveau explicite + 1.
//! * bits 4..6 : [`Dir`] du flux permanent (0 hors régime permanent).
//! * bit 7 : `STEADY`, la cellule fait partie d'un écoulement en régime permanent.

/// Niveau d'une cellule pleine, en huitièmes.
pub const FULL: u8 = 8;

/// Direction d'un flux permanent.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Dir {
    #[default]
    None = 0,
    PosX = 1,
    NegX = 2,
    PosZ = 3,
    NegZ = 4,
    Down = 5,
}

impl Dir {
    pub fn from_bits(b: u8) -> Dir {
        match b {
            1 => Dir::PosX,
            2 => Dir::NegX,
            3 => Dir::PosZ,
            4 => Dir::NegZ,
            5 => Dir::Down,
            _ => Dir::None,
        }
    }

    /// Vecteur unitaire (x, y, z) de la direction.
    pub fn vector(self) -> (i32, i32, i32) {
        match self {
            Dir::None => (0, 0, 0),
            Dir::PosX => (1, 0, 0),
            Dir::NegX => (-1, 0, 0),
            Dir::PosZ => (0, 0, 1),
            Dir::NegZ => (0, 0, -1),
            Dir::Down => (0, -1, 0),
        }
    }

    /// Direction horizontale correspondant à un pas `(dx, dz)` unitaire.
    pub fn horizontal(dx: i32, dz: i32) -> Dir {
        match (dx, dz) {
            (1, 0) => Dir::PosX,
            (-1, 0) => Dir::NegX,
            (0, 1) => Dir::PosZ,
            (0, -1) => Dir::NegZ,
            _ => Dir::None,
        }
    }

    pub fn is_horizontal(self) -> bool {
        matches!(self, Dir::PosX | Dir::NegX | Dir::PosZ | Dir::NegZ)
    }
}

/// Octet de la couche fluide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FluidCode(pub u8);

impl FluidCode {
    pub const DEFAULT: Self = Self(0);
    const STEADY: u8 = 0x80;

    /// Code explicite ; `level` est borné à 0..=8.
    pub fn explicit(level: u8, dir: Dir, steady: bool) -> Self {
        let mut b = level.min(FULL) + 1;
        b |= (dir as u8) << 4;
        if steady {
            b |= Self::STEADY;
        }
        Self(b)
    }

    /// Niveau de la cellule ; `default_level` est celui de la classe du bloc.
    #[inline]
    pub fn level(self, default_level: u8) -> u8 {
        match self.0 & 0x0F {
            0 => default_level,
            c => (c - 1).min(FULL),
        }
    }

    #[inline]
    pub fn is_default(self) -> bool {
        self.0 & 0x0F == 0
    }

    pub fn dir(self) -> Dir {
        Dir::from_bits((self.0 >> 4) & 0x07)
    }

    #[inline]
    pub fn steady(self) -> bool {
        self.0 & Self::STEADY != 0
    }

    /// Même niveau, sans direction ni régime permanent.
    pub fn calm(self) -> Self {
        Self(self.0 & 0x0F)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_roundtrip() {
        for level in 0..=8u8 {
            for steady in [false, true] {
                let c = FluidCode::explicit(level, Dir::NegZ, steady);
                assert_eq!(c.level(99), level);
                assert_eq!(c.dir(), Dir::NegZ);
                assert_eq!(c.steady(), steady);
                assert!(!c.is_default());
            }
        }
        assert!(FluidCode::DEFAULT.is_default());
        assert_eq!(FluidCode::DEFAULT.level(8), 8);
        assert_eq!(FluidCode::explicit(3, Dir::Down, true).calm().0, 4);
    }
}
