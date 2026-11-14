//! Table d'eau d'un chunk : nappes générées au repos et courant analytique (doc 07 §2.3).

/// Nature d'une nappe générée.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RestKind {
    #[default]
    Dry = 0,
    Sea = 1,
    Lake = 2,
    River = 3,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaterRest {
    /// Fond : premier y non solide (`WaterColumn::top`).
    pub top: u8,
    /// Premier y d'air au-dessus de la nappe (`WaterColumn::level`).
    pub level: u8,
    pub kind: RestKind,
    /// Courant de la nappe en seizièmes de bloc par seconde `(x, z)`.
    pub flow: [i8; 2],
}

/// Table des 16 × 16 colonnes d'un chunk, `[lz * 16 + lx]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaterTable(pub [WaterRest; 256]);

impl Default for WaterTable {
    fn default() -> Self {
        Self([WaterRest::default(); 256])
    }
}

impl WaterRest {
    /// La cellule `y` de cette colonne est-elle dans une nappe infinie ?
    pub fn infinite(&self, y: i32) -> bool {
        matches!(self.kind, RestKind::Sea | RestKind::River)
            && y >= i32::from(self.top)
            && y < i32::from(self.level)
    }
}
