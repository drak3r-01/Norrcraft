//! Réglages du générateur Norrcraft (GDD doc 02 §2 et §11).
//!
//! Pour l'instant ils vivent en code : le relief (sessions 1.2 et suivantes) et la sauvegarde
//! (`norrConfig` dans `world.json`) viendront ensuite. Les décisions D3 du plan fixent la hauteur
//! du monde à 128 et le niveau de la mer à 40.

use serde::{Deserialize, Serialize};

/// Niveau de la mer par défaut (décision D3 : 39 blocs de profondeur aux fjords, 80 aux sommets).
pub const DEFAULT_SEA_LEVEL: i32 = 40;
/// Hauteur du monde (inchangée : chunks de 16 × 128 × 16).
pub const DEFAULT_WORLD_HEIGHT: i32 = 128;

/// Préréglages Norrcraft (doc 02 §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum NorrPreset {
    /// Défaut : biais nord-sud sur T₀ et ouest-est sur O, océan à l'ouest, golfe à l'est.
    #[default]
    #[serde(rename = "norrland")]
    Norrland,
    /// Mer et îles dominantes : socle abaissé de 5 blocs, archipel doublé.
    #[serde(rename = "skargard")]
    Skargard,
    /// Montagne et toundra dominantes : U + 0,25, T₀ − 0,15. Monde rude.
    #[serde(rename = "hogfjall")]
    Hogfjall,
}

impl NorrPreset {
    pub const ALL: [NorrPreset; 3] = [Self::Norrland, Self::Skargard, Self::Hogfjall];

    /// Identifiant stable (ligne de commande, sauvegarde).
    pub fn id(self) -> &'static str {
        match self {
            Self::Norrland => "norrland",
            Self::Skargard => "skargard",
            Self::Hogfjall => "hogfjall",
        }
    }

    /// Nom affiché.
    pub fn label(self) -> &'static str {
        match self {
            Self::Norrland => "Norrland",
            Self::Skargard => "Skärgård",
            Self::Hogfjall => "Högfjäll",
        }
    }

    /// Retrouve un préréglage par identifiant ou par nom (sans accents ni casse).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|p| p.id() == s || p.label().to_lowercase() == s)
    }

    /// Ajout à la surrection U (Högfjäll : +0,25).
    pub fn uplift_bonus(self) -> f64 {
        match self {
            Self::Hogfjall => 0.25,
            _ => 0.0,
        }
    }

    /// Ajout à la température au niveau de la mer T₀ (Högfjäll : −0,15).
    pub fn temperature_offset(self) -> f64 {
        match self {
            Self::Hogfjall => -0.15,
            _ => 0.0,
        }
    }

    /// Décalage du socle B(C) en blocs (Skärgård : −5), utilisé par la fonction de hauteur.
    pub fn socle_offset(self) -> f64 {
        match self {
            Self::Skargard => -5.0,
            _ => 0.0,
        }
    }

    /// Facteur sur la densité d'archipel K (Skärgård : ×2), utilisé par la fonction de hauteur.
    pub fn archipelago_gain(self) -> f64 {
        match self {
            Self::Skargard => 2.0,
            _ => 1.0,
        }
    }
}

/// Configuration d'un monde Norrcraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NorrConfig {
    pub preset: NorrPreset,
    /// Niveau de la mer S (`y` du premier bloc d'air au-dessus de l'eau calme).
    pub sea_level: i32,
    /// Hauteur Hmax du monde.
    pub world_height: i32,
}

impl Default for NorrConfig {
    fn default() -> Self {
        Self {
            preset: NorrPreset::Norrland,
            sea_level: DEFAULT_SEA_LEVEL,
            world_height: DEFAULT_WORLD_HEIGHT,
        }
    }
}

impl NorrConfig {
    pub fn with_preset(preset: NorrPreset) -> Self {
        Self {
            preset,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_decision_d3() {
        let c = NorrConfig::default();
        assert_eq!((c.sea_level, c.world_height), (40, 128));
        assert_eq!(c.preset, NorrPreset::Norrland);
    }

    #[test]
    fn presets_parse_and_roundtrip() {
        for p in NorrPreset::ALL {
            assert_eq!(NorrPreset::parse(p.id()), Some(p));
            assert_eq!(NorrPreset::parse(p.label()), Some(p));
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(serde_json::from_str::<NorrPreset>(&json).unwrap(), p);
        }
        assert_eq!(NorrPreset::parse("Skärgård"), Some(NorrPreset::Skargard));
        assert_eq!(NorrPreset::parse("inconnu"), None);
        let c: NorrConfig = serde_json::from_str(r#"{"preset":"hogfjall"}"#).unwrap();
        assert_eq!(c.preset, NorrPreset::Hogfjall);
        assert_eq!(c.sea_level, 40);
    }
}
