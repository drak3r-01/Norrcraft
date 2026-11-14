//! Classes de blocs face à l'eau vivante (doc 07 §2.2).

use nb_data::{BlockId, BlockRegistry, FluidKind, UNASSIGNED_ID};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidClass {
    /// Étanche.
    Closed,
    /// Air.
    Empty,
    /// Bloc `water`.
    Water,
    /// Garde sa place, l'eau le traverse.
    Permeable,
    /// Perméable et immergé par défaut.
    Aquatic,
    /// Emporté par l'eau.
    Washable,
}

#[derive(Clone, Debug)]
pub struct FluidBlocks {
    pub class: [FluidClass; 256],
    pub air: BlockId,
    pub water: BlockId,
    pub lava: BlockId,
    /// `kalla`
    pub spring: BlockId,
    /// `nyis`
    pub new_ice: BlockId,
    /// `svallis`
    pub aufeis: BlockId,
    /// `basaltsten`
    pub quench: BlockId,
    /// `vitmossa`
    pub seep: BlockId,
    /// `kalksten`
    pub limestone: BlockId,
    /// `blais`
    pub glacier_ice: BlockId,
}

impl FluidBlocks {
    pub fn from_registry(r: &BlockRegistry) -> Self {
        let ids = r.ids();
        let find = |name: &str| {
            let b = r.get(name);
            if b.name == name {
                b.id
            } else {
                UNASSIGNED_ID
            }
        };
        let mut class = [FluidClass::Closed; 256];
        for b in r.iter() {
            class[usize::from(b.id)] = if b.id == ids.air {
                FluidClass::Empty
            } else if b.id == ids.water {
                FluidClass::Water
            } else {
                match b.fluid {
                    FluidKind::Permeable => FluidClass::Permeable,
                    FluidKind::Aquatic => FluidClass::Aquatic,
                    FluidKind::Washable => FluidClass::Washable,
                    FluidKind::Closed => FluidClass::Closed,
                    FluidKind::Auto => {
                        if b.cross_shaped || (1..8).contains(&b.height) {
                            FluidClass::Washable
                        } else {
                            FluidClass::Closed
                        }
                    }
                }
            };
        }
        // Les identifiants inconnus se lisent comme de l'air (`BlockRegistry::from_id`) mais ne
        // portent jamais d'eau : étanches.
        class[usize::from(ids.lava)] = FluidClass::Closed;
        Self {
            class,
            air: ids.air,
            water: ids.water,
            lava: ids.lava,
            spring: find("kalla"),
            new_ice: find("nyis"),
            aufeis: find("svallis"),
            quench: find("basaltsten"),
            seep: find("vitmossa"),
            limestone: find("kalksten"),
            glacier_ice: find("blais"),
        }
    }

    #[inline]
    pub fn class_of(&self, id: BlockId) -> FluidClass {
        self.class[usize::from(id)]
    }

    /// Niveau d'une cellule sans octet explicite : 8 pour l'eau et les plantes aquatiques.
    #[inline]
    pub fn default_level(&self, id: BlockId) -> u8 {
        match self.class_of(id) {
            FluidClass::Water | FluidClass::Aquatic => 8,
            _ => 0,
        }
    }

    /// Une cellule de ce bloc peut-elle contenir de l'eau ? (tout sauf étanche)
    #[inline]
    pub fn open(&self, id: BlockId) -> bool {
        self.class_of(id) != FluidClass::Closed
    }

    /// Table « ce bloc peut porter de l'eau » : eau, perméable, aquatique.
    pub fn keep_table(&self) -> [bool; 256] {
        let mut t = [false; 256];
        for (i, c) in self.class.iter().enumerate() {
            t[i] = matches!(
                c,
                FluidClass::Water | FluidClass::Permeable | FluidClass::Aquatic
            );
        }
        t
    }
}
