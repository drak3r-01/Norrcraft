//! Univers de contenu (plan Norrcraft, sessions 0.2 et 0.3 ; GDD doc 01 §2 et §4.2).
//!
//! Un **univers** est un ensemble étanche de contenu et de règles. Les données embarquées sont
//! rangées par univers : `assets/common/` (socle partagé : `air`, `water`, `lava`, `bedrock`),
//! `assets/vanilla/` et `assets/norrcraft/`. Ce module est le seul endroit qui sait quels
//! fichiers appartiennent à quel univers.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Socle commun : blocs que le moteur référence par leur nom, dans tous les univers.
pub const COMMON_BLOCKS_JSON: &str = include_str!("../../../assets/common/blocks.json");

/// Blocs de Norrcraft (palette provisoire de la phase 0 ; textures empruntées en attendant les
/// textures `norr_*`).
pub const NORRCRAFT_BLOCKS_JSON: &str = include_str!("../../../assets/norrcraft/blocks.json");
/// Objets purs de Norrcraft (aucun pour l'instant).
pub const NORRCRAFT_ITEMS_JSON: &str = include_str!("../../../assets/norrcraft/items.json");
/// Recettes de Norrcraft (aucune pour l'instant).
pub const NORRCRAFT_RECIPES_JSON: &str = include_str!("../../../assets/norrcraft/recipes.json");

/// Univers de contenu : ensemble étanche de contenu et de règles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorldKind {
    /// Tout ce qui existait avant Norrcraft (générateurs Classic et Norrblock).
    #[default]
    Vanilla,
    /// Le nouveau monde nordique.
    Norrcraft,
}

impl WorldKind {
    pub const ALL: [WorldKind; 2] = [WorldKind::Vanilla, WorldKind::Norrcraft];

    /// Nom stable (écrit dans `world.json`, échangé par le LAN).
    pub const fn as_str(self) -> &'static str {
        match self {
            WorldKind::Vanilla => "vanilla",
            WorldKind::Norrcraft => "norrcraft",
        }
    }

    /// Inverse de [`WorldKind::as_str`] (insensible à la casse).
    pub fn parse(s: &str) -> Option<WorldKind> {
        WorldKind::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(s.trim()))
    }

    /// Fichiers de blocs de l'univers, socle compris (`(nom, contenu)`), dans l'ordre de
    /// chargement.
    pub fn block_sources(self) -> Vec<(&'static str, &'static str)> {
        let mut v = vec![("common/blocks.json", COMMON_BLOCKS_JSON)];
        match self {
            WorldKind::Vanilla => v.extend([
                ("vanilla/blocks.json", crate::blocks::BLOCKS_JSON),
                ("vanilla/blocks_craft.json", crate::blocks::CRAFT_BLOCKS_JSON),
                (
                    "vanilla/blocks_survival.json",
                    crate::blocks::SURVIVAL_BLOCKS_JSON,
                ),
            ]),
            WorldKind::Norrcraft => v.push(("norrcraft/blocks.json", NORRCRAFT_BLOCKS_JSON)),
        }
        v
    }

    /// Fichiers d'objets purs de l'univers.
    pub fn item_sources(self) -> Vec<&'static str> {
        match self {
            WorldKind::Vanilla => vec![
                crate::items::ITEMS_JSON,
                crate::items::SURVIVAL_ITEMS_JSON,
            ],
            WorldKind::Norrcraft => vec![NORRCRAFT_ITEMS_JSON],
        }
    }

    /// Fichiers de recettes de l'univers.
    pub fn recipe_sources(self) -> Vec<&'static str> {
        match self {
            WorldKind::Vanilla => vec![
                crate::recipes::RECIPES_JSON,
                crate::recipes::SURVIVAL_RECIPES_JSON,
            ],
            WorldKind::Norrcraft => vec![NORRCRAFT_RECIPES_JSON],
        }
    }
}

impl fmt::Display for WorldKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for k in WorldKind::ALL {
            assert_eq!(WorldKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(WorldKind::parse(" Norrcraft "), Some(WorldKind::Norrcraft));
        assert_eq!(WorldKind::parse("autre"), None);
        assert_eq!(WorldKind::default(), WorldKind::Vanilla);
        assert_eq!(
            serde_json::to_string(&WorldKind::Norrcraft).unwrap(),
            "\"norrcraft\""
        );
    }
}
