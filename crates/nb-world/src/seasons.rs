//! Modèle saisonnier (plan Norrcraft, session 3.2 ; GDD doc 04 §2.3) : neige en couches, glace
//! des eaux, feuillage et flore selon la fraction d'année.
//!
//! Tout est **pur** et calculé à l'exécution (le cosinus est permis ici, pas dans la génération
//! du terrain) : une fonction du pas de saison et d'un [`Site`] (température moyenne, latitude,
//! humidité du lieu). La pose des blocs est dans `norrcraft::seasonal`. Les règles réglables
//! sont dans `assets/norrcraft/seasons.json` ([`SeasonRules`]).
//!
//! # Pas de saison
//!
//! Huit pas par an. Le pas `s` est centré sur la fraction d'année `a = (s + 0,5) / 8` (0 =
//! solstice d'hiver) : pas 0 = fin janvier, 1 = mars, 2 = mai, 3 = juin, 4 = août, 5 = septembre,
//! 6 = octobre, 7 = décembre.
//!
//! # Manteau neigeux et glace
//!
//! On intègre l'année par 64 sous-pas, sur deux années de suite pour atteindre le régime
//! périodique. La neige gagne `accumulate × (seuil − T) × humidité` couches par année tant que
//! `T < seuil`, et perd `melt × (T − seuil)` au-dessus ; la glace s'épaissit sous le seuil de
//! l'eau (lac, golfe saumâtre, mer) et fond plus vite qu'elle ne se forme. Un climat qui ne
//! dépasse jamais le seuil garde son manteau toute l'année (neiges éternelles).

use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::OnceLock;

use serde::Deserialize;

/// Pas de saison par an.
pub const STEPS: u8 = 8;

/// Sous-pas d'intégration par an.
const SUB: usize = 64;

/// Règles par défaut (`assets/norrcraft/seasons.json`).
const RULES_JSON: &str = include_str!("../../../assets/norrcraft/seasons.json");

/// Pas de saison d'une fraction d'année `a ∈ [0, 1)`.
pub fn step_of(a: f64) -> u8 {
    ((a * f64::from(STEPS)).floor() as i64).rem_euclid(i64::from(STEPS)) as u8
}

/// Fraction d'année du centre d'un pas.
pub fn step_center(step: u8) -> f64 {
    (f64::from(step % STEPS) + 0.5) / f64::from(STEPS)
}

/// Nom suédois d'un pas, pour les journaux et l'Almanach.
pub fn step_name(step: u8) -> &'static str {
    [
        "djupvinter",
        "senvinter",
        "vår",
        "försommar",
        "högsommar",
        "tidig höst",
        "höst",
        "förvinter",
    ][usize::from(step % STEPS)]
}

/// Le lieu vu par le modèle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Site {
    /// Température moyenne annuelle (échelle de T0, altitude comprise).
    pub mean: f64,
    /// Latitude λ = 1 − T0 : amplitude saisonnière plus forte au nord.
    pub lambda: f64,
    /// Humidité 0..1 (la neige tombe davantage où il est humide).
    pub wet: f64,
}

/// Nature de l'eau qui gèle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IceWater {
    Lake,
    /// Golfe saumâtre (Bottenhav) : gèle après les lacs.
    Brackish,
    Sea,
}

/// Teinte de saison (cible et part du mélange) d'un pas.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct TintRule {
    pub target: [f64; 3],
    pub mix: f64,
}

/// État d'une plante à saison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloraState {
    /// Présente.
    Present,
    /// Remplacée par l'air.
    Gone,
    /// Remplacée par un rameau sans fruits (`ris`).
    Sprig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemperatureRules {
    pub coldest: f64,
    pub amplitude_base: f64,
    pub amplitude_per_lambda: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnowRules {
    pub threshold: f64,
    pub accumulate: f64,
    pub melt: f64,
    pub cap: f64,
    pub wet_base: f64,
    pub wet_gain: f64,
    /// Part du manteau qui atteint le sol sous les conifères.
    pub canopy: f64,
    /// Couches ajoutées sous le vent d'une crête.
    pub drift_layers: i32,
    /// Dénivelé minimal (blocs) face au vent pour former une congère.
    pub drift_drop: i32,
    /// En dessous, le manteau n'est pas dessiné.
    pub min_visible: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IceRules {
    pub lake_threshold: f64,
    pub brackish_threshold: f64,
    pub sea_threshold: f64,
    pub growth: f64,
    pub melt: f64,
    pub e1: f64,
    pub e2: f64,
    pub e3: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeafRules {
    pub autumn_bare_below: f64,
    pub autumn_full_above: f64,
    pub spring_bare_below: f64,
    pub spring_full_above: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TintTable {
    pub steps: Vec<Option<TintRule>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FloraRule {
    pub steps: Vec<u8>,
    #[serde(rename = "else")]
    pub otherwise: String,
}

/// Règles de la surcouche (`seasons.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct SeasonRules {
    pub temperature: TemperatureRules,
    pub snow: SnowRules,
    pub ice: IceRules,
    pub leaves: LeafRules,
    pub leaf_tints: TintTable,
    pub grass_tints: TintTable,
    pub flora: HashMap<String, FloraRule>,
}

/// Cosinus de l'année aux sous-pas, décalés sur le jour le plus froid.
fn cos_table(coldest: f64) -> [f64; SUB] {
    std::array::from_fn(|k| (TAU * (k as f64 / SUB as f64 - coldest)).cos())
}

fn smooth(lo: f64, hi: f64, v: f64) -> f64 {
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl SeasonRules {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Règles livrées avec le jeu.
    pub fn builtin() -> &'static SeasonRules {
        static RULES: OnceLock<SeasonRules> = OnceLock::new();
        RULES.get_or_init(|| {
            Self::from_json(RULES_JSON).expect("assets/norrcraft/seasons.json invalide")
        })
    }

    fn cos(&self) -> &'static [f64; SUB] {
        static COS: OnceLock<[f64; SUB]> = OnceLock::new();
        COS.get_or_init(|| cos_table(SeasonRules::builtin().temperature.coldest))
    }

    /// Amplitude saisonnière de la température à la latitude λ.
    pub fn amplitude(&self, lambda: f64) -> f64 {
        self.temperature.amplitude_base + self.temperature.amplitude_per_lambda * lambda
    }

    /// Température au sous-pas `k` (0..64) de l'année.
    fn temp_at(&self, site: &Site, k: usize) -> f64 {
        site.mean - self.amplitude(site.lambda) * self.cos()[k % SUB]
    }

    /// Température de saison à la fraction d'année `a`.
    pub fn temperature(&self, a: f64, site: &Site) -> f64 {
        site.mean
            - self.amplitude(site.lambda)
                * (TAU * (a.rem_euclid(1.0) - self.temperature.coldest)).cos()
    }

    /// Sous-pas du centre d'un pas de saison.
    fn center(step: u8) -> usize {
        usize::from(step % STEPS) * (SUB / usize::from(STEPS)) + SUB / usize::from(STEPS) / 2
    }

    /// Épaisseur du manteau neigeux (en couches de 1/8 de bloc) à plat et à découvert, au centre
    /// du pas `step`.
    pub fn snow_layers(&self, step: u8, site: &Site) -> f64 {
        let r = &self.snow;
        let wet = r.wet_base + r.wet_gain * site.wet.clamp(0.0, 1.0);
        let dt = 1.0 / SUB as f64;
        let mut s = 0.0f64;
        let target = Self::center(step);
        // Deux années : la première part de zéro, la seconde est déjà périodique.
        for k in 0..=2 * SUB + target {
            let t = self.temp_at(site, k);
            if t < r.threshold {
                s += r.accumulate * (r.threshold - t) * wet * dt;
            } else {
                s -= r.melt * (t - r.threshold) * dt;
            }
            s = s.clamp(0.0, r.cap);
        }
        s
    }

    /// Rang de la glace (0 libre, 1 neuve, 2 portante, 3 épaisse) au centre du pas `step`.
    pub fn ice(&self, step: u8, site: &Site, water: IceWater) -> u8 {
        let r = &self.ice;
        let thr = match water {
            IceWater::Lake => r.lake_threshold,
            IceWater::Brackish => r.brackish_threshold,
            IceWater::Sea => r.sea_threshold,
        };
        let dt = 1.0 / SUB as f64;
        let target = Self::center(step);
        let mut h = 0.0f64;
        let mut prev = 0.0f64;
        for k in 0..2 * SUB + target + 1 {
            let t = self.temp_at(site, k);
            prev = h;
            if t < thr {
                h += r.growth * (thr - t) * dt;
            } else {
                h -= r.melt * (t - thr) * dt;
            }
            h = h.max(0.0);
        }
        let mut e = if h >= r.e3 {
            3
        } else if h >= r.e2 {
            2
        } else if h >= r.e1 {
            1
        } else {
            0
        };
        // Débâcle : la glace qui fond redevient neuve et fragile.
        if e > 1 && h < prev {
            e = 1;
        }
        e
    }

    /// Part des feuilles de feuillus restée à la branche, 0..1.
    pub fn leaf_fraction(&self, step: u8, site: &Site) -> f64 {
        let k = Self::center(step);
        let t = self.temp_at(site, k);
        let before = self.temp_at(site, k + SUB - SUB / usize::from(STEPS));
        let r = &self.leaves;
        if t < before {
            smooth(r.autumn_bare_below, r.autumn_full_above, t)
        } else {
            smooth(r.spring_bare_below, r.spring_full_above, t)
        }
    }

    /// Teinte des feuillus au pas `step`.
    pub fn leaf_tint(&self, step: u8) -> Option<TintRule> {
        self.leaf_tints
            .steps
            .get(usize::from(step % STEPS))
            .copied()
            .flatten()
    }

    /// Teinte de l'herbe au pas `step`.
    pub fn grass_tint(&self, step: u8) -> Option<TintRule> {
        self.grass_tints
            .steps
            .get(usize::from(step % STEPS))
            .copied()
            .flatten()
    }

    /// État de la plante `name` au pas `step`.
    pub fn flora_state(&self, name: &str, step: u8) -> FloraState {
        match self.flora.get(name) {
            None => FloraState::Present,
            Some(rule) if rule.steps.contains(&(step % STEPS)) => FloraState::Present,
            Some(rule) if rule.otherwise == "ris" => FloraState::Sprig,
            Some(_) => FloraState::Gone,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(mean: f64) -> Site {
        Site {
            mean,
            lambda: 1.0 - mean,
            wet: 0.5,
        }
    }

    #[test]
    fn steps_cover_the_year() {
        assert_eq!(step_of(0.0), 0);
        assert_eq!(step_of(0.124), 0);
        assert_eq!(step_of(0.125), 1);
        assert_eq!(step_of(0.999), 7);
        assert_eq!(step_of(1.0), 0);
        for s in 0..STEPS {
            assert_eq!(step_of(step_center(s)), s);
        }
    }

    #[test]
    fn rules_parse() {
        let r = SeasonRules::builtin();
        assert_eq!(r.leaf_tints.steps.len(), 8);
        assert_eq!(r.grass_tints.steps.len(), 8);
        assert!(r.flora.contains_key("vitsippa"));
    }

    /// Table d'ajustement (`cargo test -- --nocapture seasonal_table`).
    #[test]
    fn seasonal_table() {
        let r = SeasonRules::builtin();
        for mean in [0.05, 0.15, 0.25, 0.35, 0.45, 0.6, 0.75] {
            let s = site(mean);
            let snow: Vec<String> = (0..STEPS)
                .map(|k| format!("{:4.1}", r.snow_layers(k, &s)))
                .collect();
            let lake: Vec<u8> = (0..STEPS).map(|k| r.ice(k, &s, IceWater::Lake)).collect();
            let sea: Vec<u8> = (0..STEPS).map(|k| r.ice(k, &s, IceWater::Sea)).collect();
            let leaf: Vec<String> = (0..STEPS)
                .map(|k| format!("{:.2}", r.leaf_fraction(k, &s)))
                .collect();
            println!("T={mean:.2} neige {snow:?} lac {lake:?} mer {sea:?} feuilles {leaf:?}");
        }
    }

    #[test]
    fn boreal_has_deep_winter_snow_and_no_summer_snow() {
        let r = SeasonRules::builtin();
        let s = site(0.40);
        assert!(r.snow_layers(1, &s) >= 4.0, "{}", r.snow_layers(1, &s));
        assert!(r.snow_layers(0, &s) >= 3.0);
        assert!(r.snow_layers(3, &s) < 0.5);
        assert!(r.snow_layers(4, &s) < 0.5);
    }

    #[test]
    fn snow_is_deeper_in_the_north() {
        let r = SeasonRules::builtin();
        assert!(r.snow_layers(1, &site(0.2)) > r.snow_layers(1, &site(0.5)));
        assert!(r.snow_layers(1, &site(0.5)) >= r.snow_layers(1, &site(0.8)));
        assert!(r.snow_layers(1, &site(0.9)) < 0.5, "pas de neige au sud");
    }

    #[test]
    fn high_peaks_keep_eternal_snow() {
        let r = SeasonRules::builtin();
        let s = Site {
            mean: -0.2,
            lambda: 1.0,
            wet: 0.5,
        };
        for k in 0..STEPS {
            assert!(r.snow_layers(k, &s) > 4.0, "pas {k}");
        }
    }

    #[test]
    fn lakes_freeze_in_winter_and_thaw_in_summer() {
        let r = SeasonRules::builtin();
        let s = site(0.40);
        assert!(r.ice(0, &s, IceWater::Lake) >= 2);
        assert_eq!(r.ice(3, &s, IceWater::Lake), 0);
        assert_eq!(r.ice(4, &s, IceWater::Lake), 0);
        // Les lacs gèlent avant le golfe saumâtre, qui gèle avant la mer.
        for k in 0..STEPS {
            assert!(r.ice(k, &s, IceWater::Lake) >= r.ice(k, &s, IceWater::Brackish));
            assert!(r.ice(k, &s, IceWater::Brackish) >= r.ice(k, &s, IceWater::Sea));
        }
        // La débâcle fragilise la glace au printemps.
        assert!(r.ice(2, &s, IceWater::Lake) <= 1);
    }

    #[test]
    fn deciduous_trees_bare_in_winter() {
        let r = SeasonRules::builtin();
        let s = site(0.40);
        assert!(r.leaf_fraction(0, &s) < 0.05);
        assert!(r.leaf_fraction(7, &s) < 0.05);
        assert!(r.leaf_fraction(4, &s) > 0.95);
        assert!(r.leaf_fraction(6, &s) > 0.2 && r.leaf_fraction(6, &s) < 0.9);
    }

    #[test]
    fn flora_windows() {
        let r = SeasonRules::builtin();
        assert_eq!(r.flora_state("vitsippa", 2), FloraState::Present);
        assert_eq!(r.flora_state("vitsippa", 4), FloraState::Gone);
        assert_eq!(r.flora_state("blabar", 4), FloraState::Present);
        assert_eq!(r.flora_state("blabar", 0), FloraState::Sprig);
        assert_eq!(r.flora_state("ljung", 0), FloraState::Present);
    }
}
