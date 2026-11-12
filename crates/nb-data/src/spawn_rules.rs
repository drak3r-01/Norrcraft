//! Règles d'apparition par densité cible (doc 03 §4.1 et §4.2, session 5.2).
//!
//! Un mob dont le `MobConfig` porte un bloc `"spawn"` n'est plus tiré par l'apparition d'origine
//! (plafond + biomes + sols) : le système de densité vise `cible(e)` individus autour du joueur
//! et tire l'espèce au prorata de son déficit. Tous les champs sont optionnels.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::behavior::{Activity, CycleRef};

/// Durée de vie d'une apparition (doc 03 §4.1, `persistence`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Persistence {
    /// Disparaît hors de vue au-delà du rayon d'activité, ou quand la saison le renvoie.
    #[default]
    Ephemeral,
    /// Lié à un lieu : ne disparaît que si son chunk est déchargé.
    Resident,
    /// Membre d'une harde (session 5.3) : l'espèce n'est plus tirée par la densité, elle vit en
    /// hardes persistantes décrites par [`SpawnRules::herd`].
    Herd,
    /// Au plus un exemplaire dans le monde.
    Unique,
}

/// Rayons d'évitement (en blocs) : le point d'apparition doit en être hors. 0 = sans contrainte.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Avoid {
    pub fire: f32,
    pub bell: f32,
    /// Village (structure habitée) : lu quand le monde exposera les sites aux entités.
    pub village: f32,
    pub player: f32,
}

/// Façon dont une harde se déplace sur l'année (doc 03 §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HerdMotion {
    /// Transhumance : va et vient entre un site d'été et un site d'hiver, deux fois par an
    /// (rennes, bancs de harengs).
    #[default]
    Seasonal,
    /// Territoire fixe, parcouru lentement (meutes de loups).
    Territory,
    /// Passage : n'existe dans la cellule que pendant une fenêtre du calendrier et la traverse
    /// (vols de grues et d'oies, remontée des saumons).
    Pass,
}

/// Une harde persistante : objet léger du monde, une par cellule (doc 03 §4.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HerdRules {
    pub motion: HerdMotion,
    /// Côté de la cellule en blocs : une harde de l'espèce par cellule.
    pub cell: i32,
    /// Effectif d'une harde pleine, tiré par cellule entre ces deux bornes.
    pub size_min: i32,
    pub size_max: i32,
    /// Individus regagnés par jour de jeu après des pertes, jusqu'à l'effectif plein.
    pub regrow_per_day: f32,
    /// Biomes du site d'été (transhumance), du territoire (meute) ou d'arrivée (passage).
    pub summer: Vec<String>,
    /// Biomes du site d'hiver (transhumance) ou de départ (passage).
    pub winter: Vec<String>,
    /// Mois réel (0 = januari) de la montée vers le site d'été, et de la descente.
    pub up_month: u8,
    pub down_month: u8,
    /// Fenêtre de passage (`Pass`) : premier et dernier mois réels, inclus.
    pub pass_months: [u8; 2],
    /// Cap d'un passage sans sites (x, z) ; traverse la cellule dans ce sens.
    pub heading: [f32; 2],
    /// Rayon (blocs) du va-et-vient d'une harde autour de sa position calendaire.
    pub wander: f32,
    /// Distance du joueur à laquelle la harde se matérialise en individus.
    pub materialize: f32,
}

impl Default for HerdRules {
    fn default() -> Self {
        Self {
            motion: HerdMotion::Seasonal,
            cell: 1024,
            size_min: 6,
            size_max: 14,
            regrow_per_day: 0.5,
            summer: Vec::new(),
            winter: Vec::new(),
            up_month: 4,
            down_month: 9,
            pass_months: [3, 4],
            heading: [0.0, -1.0],
            wander: 24.0,
            materialize: 96.0,
        }
    }
}

impl HerdRules {
    pub fn validate(&self) -> Result<(), String> {
        if !(256..=8192).contains(&self.cell) {
            return Err(format!("herd.cell : {} (256 à 8192 attendus)", self.cell));
        }
        if self.size_min < 1 || self.size_max < self.size_min {
            return Err("herd.sizeMin/sizeMax incohérents".into());
        }
        if !(self.regrow_per_day.is_finite() && self.regrow_per_day >= 0.0)
            || !(self.wander.is_finite() && self.wander >= 0.0)
            || !(self.materialize.is_finite() && self.materialize >= 32.0)
        {
            return Err("herd : regrowPerDay, wander ou materialize invalide".into());
        }
        if self.up_month > 11 || self.down_month > 11 || self.pass_months.iter().any(|&m| m > 11) {
            return Err("herd : mois hors de 0 à 11".into());
        }
        match self.motion {
            HerdMotion::Seasonal if self.summer.is_empty() && self.winter.is_empty() => {
                Err("herd : aucun biome de site (summer / winter)".into())
            }
            HerdMotion::Territory if self.summer.is_empty() => {
                Err("herd : territoire sans biome (summer)".into())
            }
            HerdMotion::Pass
                if self.summer.is_empty()
                    && self.winter.is_empty()
                    && self.heading[0].hypot(self.heading[1]) < 1e-3 =>
            {
                Err("herd : passage sans cap ni sites".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpawnRules {
    /// Hardes persistantes (obligatoire avec `persistence: "herd"`, interdit sinon).
    pub herd: Option<HerdRules>,
    /// Individus visés pour 64 × 64 blocs, par nom de biome. Un biome absent vaut 0.
    pub density_per_biome: BTreeMap<String, f32>,
    /// 4 à 12 facteurs répartis sur l'année à partir du solstice d'hiver (0 = absent : migrateur parti, hibernant). Vide :
    /// toute l'année. Interpolés linéairement, en boucle.
    pub season_curve: Vec<f32>,
    pub time_of_day: Activity,
    /// Facteurs par état du ciel : `clear`, `rain`, `snow`, `mist`, `wind`. Absent : 1.
    pub weather: BTreeMap<String, f32>,
    /// Hauteur maximale du soleil (−1 à 1) tolérée ; `None` : sans limite.
    pub light: Option<f32>,
    /// Exige un couvert (bloc plein ou feuillage dans les 10 blocs au-dessus).
    pub shelter: bool,
    /// Cycle pluriannuel ; à défaut celui du profil de comportement.
    pub cycle: Option<CycleRef>,
    /// Nom d'un bloc qui doit se trouver à 6 blocs ou moins (trou de glace, corniche, fors…).
    pub near_feature: Option<String>,
    pub avoid: Avoid,
    pub persistence: Persistence,
    /// Densité modulée par la réputation du joueur (doc 04 §7, branchée plus tard).
    pub naturfrid: bool,
    /// Hauteur d'apparition au-dessus du sol pour un volatile (défaut : croisière du profil).
    pub altitude: Option<f32>,
}

impl SpawnRules {
    /// Contrôle de cohérence ; le message nomme le champ fautif.
    pub fn validate(&self) -> Result<(), String> {
        match (&self.herd, self.persistence == Persistence::Herd) {
            (Some(h), true) => h.validate()?,
            (None, true) => return Err("persistence herd sans bloc herd".into()),
            (Some(_), false) => return Err("bloc herd sans persistence herd".into()),
            (None, false) => {}
        }
        if self.density_per_biome.is_empty() && self.herd.is_none() {
            return Err("densityPerBiome vide".into());
        }
        let finite = |v: f32| v.is_finite() && v >= 0.0;
        if self.density_per_biome.values().any(|&d| !finite(d)) {
            return Err("densityPerBiome : densité négative".into());
        }
        if !self.season_curve.is_empty() && !(4..=12).contains(&self.season_curve.len()) {
            return Err(format!(
                "seasonCurve : {} facteurs (4 à 12 attendus)",
                self.season_curve.len()
            ));
        }
        if self.season_curve.iter().any(|&v| !finite(v)) {
            return Err("seasonCurve : facteur négatif".into());
        }
        const KEYS: [&str; 5] = ["clear", "rain", "snow", "mist", "wind"];
        for (k, &v) in &self.weather {
            if !KEYS.contains(&k.as_str()) {
                return Err(format!("weather : état inconnu {k}"));
            }
            if !finite(v) {
                return Err(format!("weather.{k} : facteur négatif"));
            }
        }
        if [
            self.avoid.fire,
            self.avoid.bell,
            self.avoid.village,
            self.avoid.player,
        ]
        .iter()
        .any(|&r| !finite(r))
        {
            return Err("avoid : rayon négatif".into());
        }
        Ok(())
    }

    /// Densité (individus pour 64 × 64) dans ce biome.
    pub fn density(&self, biome: &str) -> f32 {
        self.density_per_biome.get(biome).copied().unwrap_or(0.0)
    }

    /// Densité maximale sur tous les biomes (sert à l'acceptation d'un point tiré).
    pub fn max_density(&self) -> f32 {
        self.density_per_biome.values().copied().fold(0.0, f32::max)
    }

    /// Facteur de saison à la fraction d'année `a` (0 au solstice d'hiver, comme le calendrier) :
    /// le facteur `i` est posé à `i / n` de l'année, les autres s'interpolent en boucle.
    pub fn season_factor(&self, a: f64) -> f32 {
        let n = self.season_curve.len();
        if n == 0 {
            return 1.0;
        }
        let x = a.rem_euclid(1.0) * n as f64;
        let i0 = x.floor();
        let t = (x - i0) as f32;
        let at = |i: f64| self.season_curve[(i.rem_euclid(n as f64)) as usize];
        at(i0) * (1.0 - t) + at(i0 + 1.0) * t
    }

    /// Facteur météo pour des conditions données (produit des états actifs).
    pub fn weather_factor(&self, sky: Sky) -> f32 {
        if self.weather.is_empty() {
            return 1.0;
        }
        let get = |k: &str| self.weather.get(k).copied().unwrap_or(1.0);
        let mut f = 1.0;
        let mut calm = true;
        for (key, active) in [
            ("rain", sky.rain > 0.2),
            ("snow", sky.snow > 0.2),
            ("mist", sky.fog > 0.015),
            ("wind", sky.wind > 0.6),
        ] {
            if active {
                f *= get(key);
                calm = false;
            }
        }
        if calm {
            f *= get("clear");
        }
        f
    }
}

/// Ciel vu par les règles d'apparition (dérivé de `nb_world::weather::Environment`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sky {
    pub rain: f32,
    pub snow: f32,
    pub fog: f32,
    pub wind: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(json: &str) -> SpawnRules {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn parses_and_validates() {
        let r = rules(
            r#"{"densityPerBiome": {"URSKOG": 2.0, "MYR": 0.5},
                "seasonCurve": [0, 0, 1, 1], "timeOfDay": "crepuscular",
                "weather": {"rain": 0.5}, "persistence": "resident",
                "avoid": {"fire": 8}}"#,
        );
        assert_eq!(r.validate(), Ok(()));
        assert_eq!(r.density("URSKOG"), 2.0);
        assert_eq!(r.density("FJORD"), 0.0);
        assert_eq!(r.max_density(), 2.0);
        assert_eq!(r.persistence, Persistence::Resident);
        assert_eq!(r.time_of_day, Activity::Crepuscular);
    }

    #[test]
    fn rejects_incoherent_rules() {
        assert!(rules("{}").validate().is_err());
        let few = rules(r#"{"densityPerBiome": {"MYR": 1}, "seasonCurve": [1, 1, 1]}"#);
        assert!(few.validate().unwrap_err().contains("seasonCurve"));
        let neg = rules(r#"{"densityPerBiome": {"MYR": -1}}"#);
        assert!(neg.validate().is_err());
        let w = rules(r#"{"densityPerBiome": {"MYR": 1}, "weather": {"hail": 1}}"#);
        assert!(w.validate().unwrap_err().contains("hail"));
    }

    #[test]
    fn herd_rules_parse_and_validate() {
        let r = rules(
            r#"{"persistence": "herd", "herd": {"motion": "seasonal",
                "summer": ["FJALLHED"], "winter": ["TALLHED"], "sizeMin": 8, "sizeMax": 20}}"#,
        );
        assert_eq!(r.validate(), Ok(()));
        let h = r.herd.as_ref().unwrap();
        assert_eq!((h.cell, h.up_month, h.down_month), (1024, 4, 9));
        // Une harde sans persistance « herd » (et l'inverse) est refusée.
        let loose = rules(r#"{"herd": {"summer": ["A"]}, "densityPerBiome": {"A": 1}}"#);
        assert!(loose.validate().unwrap_err().contains("persistence"));
        let bare = rules(r#"{"persistence": "herd", "densityPerBiome": {"A": 1}}"#);
        assert!(bare.validate().unwrap_err().contains("herd"));
        // Sans biome de site, effectif incohérent, mois hors calendrier : refus.
        for bad in [
            r#"{"motion": "seasonal"}"#,
            r#"{"summer": ["A"], "sizeMin": 5, "sizeMax": 2}"#,
            r#"{"summer": ["A"], "upMonth": 12}"#,
            r#"{"motion": "pass", "heading": [0, 0]}"#,
            r#"{"motion": "territory", "winter": ["A"]}"#,
        ] {
            let r = rules(&format!(r#"{{"persistence": "herd", "herd": {bad}}}"#));
            assert!(r.validate().is_err(), "accepté à tort : {bad}");
        }
    }

    #[test]
    fn season_curve_interpolates_in_a_loop() {
        // Solstice d'hiver 0, équinoxe 1, solstice d'été 2, équinoxe 1.
        let r = rules(r#"{"densityPerBiome": {"A": 1}, "seasonCurve": [0, 1, 2, 1]}"#);
        assert!((r.season_factor(0.0) - 0.0).abs() < 1e-5);
        assert!((r.season_factor(0.5) - 2.0).abs() < 1e-5);
        assert!((r.season_factor(0.125) - 0.5).abs() < 1e-5);
        // Boucle : entre 0,75 (1) et 1,0 (0), à 0,875 on est à mi-chemin.
        assert!((r.season_factor(0.875) - 0.5).abs() < 1e-5);
        assert_eq!(
            rules(r#"{"densityPerBiome": {"A": 1}}"#).season_factor(0.3),
            1.0
        );
    }

    #[test]
    fn weather_factors_multiply_and_clear_means_calm() {
        let r = rules(
            r#"{"densityPerBiome": {"A": 1},
                "weather": {"rain": 0.5, "wind": 0.5, "clear": 2.0}}"#,
        );
        let calm = Sky::default();
        assert_eq!(r.weather_factor(calm), 2.0);
        let wet = Sky { rain: 0.8, ..calm };
        assert_eq!(r.weather_factor(wet), 0.5);
        let both = Sky {
            rain: 0.8,
            wind: 0.9,
            ..calm
        };
        assert_eq!(r.weather_factor(both), 0.25);
        // Neige sans règle de neige : facteur 1, et ce n'est pas « clair ».
        let snow = Sky { snow: 0.9, ..calm };
        assert_eq!(r.weather_factor(snow), 1.0);
    }
}
