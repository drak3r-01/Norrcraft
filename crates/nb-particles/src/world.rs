//! Ce que le moteur sait du monde : une seule requête de cellule, résolue par le jeu.

/// Nature d'une cellule vue par les particules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Air,
    /// Bloc plein : arrête les particules qui collisionnent.
    Solid,
    Water,
    Lava,
    /// Feuillage (solide pour les collisions, source de feuilles mortes).
    Leaves,
    /// Bloc incandescent (magma) : braises.
    Magma,
}

impl Cell {
    #[inline]
    pub fn is_solid(self) -> bool {
        matches!(self, Cell::Solid | Cell::Leaves | Cell::Magma)
    }
}

/// Accès en lecture au monde. Les coordonnées hors chunk chargé doivent répondre `Air`.
pub trait ParticleWorld {
    fn cell(&self, x: i32, y: i32, z: i32) -> Cell;

    /// Aucun bloc plein entre `(x, y, z)` (exclu) et `y + up` : à l abri de la pluie ?
    /// Les mondes disposant d une carte de hauteur peuvent surcharger (réponse en O(1)).
    fn sky_open(&self, x: i32, y: i32, z: i32, up: i32) -> bool {
        (1..=up).all(|d| !self.cell(x, y + d, z).is_solid())
    }

    /// Teinte (RGB 0..1) des feuilles en `(x, z)`.
    fn leaf_tint(&self, _x: i32, _z: i32) -> [f32; 3] {
        [0.45, 0.75, 0.3]
    }
}

/// Monde vide (tests, outils).
pub struct EmptyWorld;

impl ParticleWorld for EmptyWorld {
    fn cell(&self, _: i32, _: i32, _: i32) -> Cell {
        Cell::Air
    }
}
