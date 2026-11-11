//! Zones climatiques, étagement et température à l'altitude (GDD doc 02 §4 et §7.1).
//!
//! Ces fonctions ne dépendent que de T₀ et de O (champs de [`super::fields`]) : la sélection des
//! biomes (session 2.1) s'appuiera dessus.

use super::math::clamp01;

/// Zones de température au niveau de la mer (doc 02 §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClimateZone {
    /// T₀ < 0,12.
    Arctic,
    /// 0,12 ≤ T₀ < 0,28.
    Subarctic,
    /// 0,28 ≤ T₀ < 0,50.
    Boreal,
    /// 0,50 ≤ T₀ < 0,70.
    Hemiboreal,
    /// T₀ ≥ 0,70.
    Nemoral,
}

impl ClimateZone {
    pub const ALL: [ClimateZone; 5] = [
        Self::Arctic,
        Self::Subarctic,
        Self::Boreal,
        Self::Hemiboreal,
        Self::Nemoral,
    ];

    pub fn from_t0(t0: f64) -> Self {
        if t0 >= 0.70 {
            Self::Nemoral
        } else if t0 >= 0.50 {
            Self::Hemiboreal
        } else if t0 >= 0.28 {
            Self::Boreal
        } else if t0 >= 0.12 {
            Self::Subarctic
        } else {
            Self::Arctic
        }
    }

    /// Nom suédois de la zone (glose française dans le doc 02).
    pub fn name(self) -> &'static str {
        match self {
            Self::Arctic => "arctique",
            Self::Subarctic => "subarctique",
            Self::Boreal => "boréale",
            Self::Hemiboreal => "hémiboréale",
            Self::Nemoral => "némorale",
        }
    }
}

/// Altitudes caractéristiques d'un climat (doc 02 §7.1), en blocs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Altitudes {
    /// `y_a` : limite des arbres.
    pub tree_line: f64,
    /// `y_b` : ceinture de bouleaux (`y_a − 8`).
    pub birch_belt: f64,
    /// `y_k` : roche nue (`y_a + 16`).
    pub bare_rock: f64,
    /// `y_n` : neiges éternelles (`y_a + 30`).
    pub snow_line: f64,
}

/// Étagement pour une température T₀ et un niveau de mer S :
/// `y_a = S + 14 + 70 · clamp(T₀ − 0,12, 0, 0,6)`.
pub fn altitudes(t0: f64, sea_level: i32) -> Altitudes {
    let y_a = f64::from(sea_level) + 14.0 + 70.0 * (t0 - 0.12).clamp(0.0, 0.6);
    Altitudes {
        tree_line: y_a,
        birch_belt: y_a - 8.0,
        bare_rock: y_a + 16.0,
        snow_line: y_a + 30.0,
    }
}

/// Température réelle à l'altitude `y` : `T = T₀ − 0,006 · max(0, y − S) + 0,05 · (O − 0,5)`.
pub fn temperature_at(t0: f64, ocean: f64, y: f64, sea_level: i32) -> f64 {
    t0 - 0.006 * (y - f64::from(sea_level)).max(0.0) + 0.05 * (ocean - 0.5)
}

/// Même température ramenée dans [0, 1] (pour les cartes).
pub fn temperature_at_clamped(t0: f64, ocean: f64, y: f64, sea_level: i32) -> f64 {
    clamp01(temperature_at(t0, ocean, y, sea_level))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_boundaries() {
        assert_eq!(ClimateZone::from_t0(0.0), ClimateZone::Arctic);
        assert_eq!(ClimateZone::from_t0(0.119), ClimateZone::Arctic);
        assert_eq!(ClimateZone::from_t0(0.12), ClimateZone::Subarctic);
        assert_eq!(ClimateZone::from_t0(0.28), ClimateZone::Boreal);
        assert_eq!(ClimateZone::from_t0(0.50), ClimateZone::Hemiboreal);
        assert_eq!(ClimateZone::from_t0(0.70), ClimateZone::Nemoral);
        assert_eq!(ClimateZone::from_t0(1.0), ClimateZone::Nemoral);
    }

    #[test]
    fn tree_line_examples_from_the_doc() {
        // T₀ = 0,40 (boréal) : arbres jusqu'à S + 34 ; T₀ = 0,15 : dès S + 16.
        let a = altitudes(0.40, 40);
        assert!((a.tree_line - (40.0 + 34.0)).abs() < 0.5, "{}", a.tree_line);
        let b = altitudes(0.15, 40);
        assert!((b.tree_line - (40.0 + 16.0)).abs() < 0.5, "{}", b.tree_line);
        assert_eq!(a.birch_belt, a.tree_line - 8.0);
        assert_eq!(a.bare_rock, a.tree_line + 16.0);
        assert_eq!(a.snow_line, a.tree_line + 30.0);
        // Au-delà de 0,72 la limite des arbres plafonne.
        assert_eq!(altitudes(0.9, 40).tree_line, altitudes(0.72, 40).tree_line);
    }

    #[test]
    fn temperature_drops_with_altitude() {
        assert_eq!(temperature_at(0.5, 0.5, 30.0, 40), 0.5);
        assert!((temperature_at(0.5, 0.5, 140.0, 40) - 0.5 + 0.6).abs() < 1e-12);
        assert!(temperature_at(0.5, 1.0, 40.0, 40) > temperature_at(0.5, 0.0, 40.0, 40));
    }
}
