//! Élevage et compagnons (doc 03 §3.6, session 5.7).
//!
//! Le bloc `"husbandry"` d'un `MobConfig` décrit ce qui fait d'un animal un animal domestique :
//! comment on l'apprivoise, ce qu'il mange l'hiver, ce qu'il donne, comment il se monte ou se
//! comporte à la chasse. Les noms d'objets (`mjolk`, `ull`…) sont des chaînes : les objets
//! eux-mêmes arrivent avec les sessions 6.4 et 6.5.

use serde::{Deserialize, Serialize};

use crate::behavior::Window;

/// Rôle d'un animal domestique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Bétail : parqué près du foyer, nourri au fourrage l'hiver, donne un produit.
    #[default]
    Livestock,
    /// Volaille : à enfermer la nuit (le renard rôde).
    Poultry,
    /// Cheval : trait, bât, monture.
    Mount,
    /// Chien : suit son maître, marque l'arrêt, alerte.
    Hound,
    /// Chat : suit un peu, chasse les souris, ronronne.
    Ratter,
}

/// Fourrage d'hiver : sans réserve, le bétail dépérit (doc 03 §3.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fodder {
    /// Objet qui nourrit (`ho`, `halm`, `agn`…).
    pub item: String,
    /// Réserve (0 à 1) consommée par jour de jeu sans pâture.
    pub per_day: f32,
    /// Part de la réserve rendue par une ration.
    pub ration: f32,
    /// Mois (0 = janvier) où l'herbe pousse : l'animal broute et la réserve se remplit seule.
    pub pasture: Window,
}

/// Production (lait, laine, œufs…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Produce {
    pub item: String,
    /// Unités produites par jour de jeu, bien nourri et en saison.
    pub per_day: f32,
    /// Stock maximal prêt à être récolté.
    pub capacity: f32,
    /// Mois de production ; absent = toute l'année.
    #[serde(default)]
    pub months: Option<Window>,
    /// Facteur de rendement au pâturage d'estive (fäbod) : vaut sur le champ `bonus_months`.
    #[serde(default = "one")]
    pub summer_bonus: f32,
    #[serde(default)]
    pub bonus_months: Option<Window>,
    /// La récolte ne se fait qu'une fois la réserve pleine (tonte : une par an).
    #[serde(default)]
    pub seasonal: bool,
    /// Mois où la récolte est permise (tonte au printemps) ; absent = toute l'année.
    #[serde(default)]
    pub harvest_months: Option<Window>,
}

fn one() -> f32 {
    1.0
}

/// Allure de monture (`tolt` : vive et sans secousse).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gait {
    pub name: GaitName,
    /// Multiple de la vitesse de marche.
    pub speed: f32,
    /// Part de secousse verticale gardée (0 : parfaitement fluide).
    pub jolt: f32,
    /// Peut-on manger ou viser en selle ?
    pub steady: bool,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GaitName {
    Walk,
    Tolt,
    Trot,
}

/// Comportement de chien de chasse et de garde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Hound {
    /// Distance à laquelle il marque l'arrêt devant le gibier.
    pub point_range: f32,
    /// Portée à laquelle il aboie sur un prédateur dangereux.
    pub alert_range: f32,
    /// Espèces ou profils qui déclenchent l'alerte.
    pub alerts: Vec<String>,
}

impl Default for Hound {
    fn default() -> Self {
        Self {
            point_range: 18.0,
            alert_range: 32.0,
            alerts: Vec::new(),
        }
    }
}

/// Fiche d'élevage d'une espèce domestique.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Husbandry {
    pub role: Role,
    /// Objets qui apprivoisent (une friandise suffit), puis servent de récompense.
    #[serde(default)]
    pub tame_with: Vec<String>,
    /// Distance de suivi du maître (compagnons et montures de bât).
    #[serde(default = "follow_default")]
    pub follow: f32,
    #[serde(default)]
    pub fodder: Option<Fodder>,
    #[serde(default)]
    pub produce: Option<Produce>,
    #[serde(default)]
    pub gait: Option<Gait>,
    #[serde(default)]
    pub hound: Option<Hound>,
}

fn follow_default() -> f32 {
    6.0
}

impl Husbandry {
    /// Cet objet apprivoise-t-il l'animal ?
    pub fn tames_with(&self, item: &str) -> bool {
        self.tame_with.iter().any(|i| i == item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minimal_sheet_parses() {
        let h: Husbandry = serde_json::from_str(r#"{"role":"hound"}"#).unwrap();
        assert_eq!(h.role, Role::Hound);
        assert_eq!(h.follow, 6.0);
        assert!(h.fodder.is_none());
    }
}
