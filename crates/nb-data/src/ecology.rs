//! Interactions de la faune avec le monde, chasse et piégeage (doc 03 §4.5 et §5, session 5.8).
//!
//! Le bloc `"ecology"` d'un `MobConfig` dit ce qu'une espèce **fait au monde** (brouter les
//! pousses, abattre des arbres, laisser des traces, mordre avec du venin) et ce que le joueur
//! **lui fait** (piéger, dépecer, respecter la naturfrid). Les noms de blocs et d'objets sont des
//! chaînes : un nom inconnu du registre est simplement ignoré (les objets arrivent en phase 6).

use serde::{Deserialize, Serialize};

use crate::behavior::Window;

/// Broutage : pousses et cultures mangées (élan, chevreuil).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Browse {
    /// Blocs mangés (plantes, pousses, cultures).
    pub eats: Vec<String>,
    /// Mois où l'animal broute ces plantes ; absent = toute l'année.
    #[serde(default)]
    pub months: Option<Window>,
    /// Blocs de clôture : une plante à moins de `fence_radius` d'une clôture est protégée.
    #[serde(default)]
    pub fences: Vec<String>,
    #[serde(default = "fence_radius")]
    pub fence_radius: i32,
}

fn fence_radius() -> i32 {
    2
}

/// Abattage d'arbres et barrage (castor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fell {
    /// Suffixe des troncs vivants (`gran_stam`).
    #[serde(default = "log_suffix")]
    pub log_suffix: String,
    /// Troncs qui ne sont pas des arbres vivants (arbre mort, souche…).
    #[serde(default)]
    pub spare: Vec<String>,
    /// Secondes de jeu entre deux arbres abattus par une colonie (≈ « toutes les quelques
    /// journées »).
    pub every_secs: f32,
    /// Nombre maximal d'arbres abattus par colonie (plafond).
    pub cap: u32,
    /// Rayon de recherche autour de la hutte.
    #[serde(default = "fell_reach")]
    pub reach: i32,
    /// Blocs du barrage : le casser vide l'étang en amont.
    #[serde(default)]
    pub dam: Vec<String>,
}

fn log_suffix() -> String {
    "_stam".into()
}

fn fell_reach() -> i32 {
    24
}

/// Venin (vipère) : dégâts étalés dans le temps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Venom {
    pub damage_per_sec: f32,
    pub seconds: f32,
    /// Probabilité qu'une morsure inocule du venin.
    #[serde(default = "one")]
    pub chance: f32,
    /// Objets qui guérissent (antidote, plantain…).
    #[serde(default)]
    pub cures: Vec<String>,
}

fn one() -> f32 {
    1.0
}

/// Traces laissées dans la neige fraîche (doc 03 §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tracks {
    /// Forme de l'empreinte (`tass`, `klov`, `fagel`, `orm`).
    pub print: String,
    /// Durée de vie d'une trace, en secondes de jeu.
    pub ttl: f32,
    /// Distance parcourue entre deux empreintes.
    #[serde(default = "stride")]
    pub stride: f32,
    /// Probabilité de laisser des crottes à chaque repas.
    #[serde(default)]
    pub droppings: f32,
    /// Écorce un arbre en broutant (élan, ours).
    #[serde(default)]
    pub debark: bool,
}

fn stride() -> f32 {
    1.2
}

/// Famille de piège.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrapKind {
    /// Collet (lagopède, lièvre).
    Snare,
    /// Nasse (poisson, écrevisse).
    FishTrap,
    /// Filet (poisson, oiseau d'eau).
    Net,
    /// Fosse à élan (structure historique).
    Pit,
}

/// Ce qu'un piège prend de cette espèce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatchRule {
    pub trap: TrapKind,
    /// Probabilité par minute qu'un animal à portée du piège s'y prenne.
    pub chance: f32,
}

/// Naturfrid : ce qui pèse sur la réputation quand on tue l'espèce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Naturfrid {
    /// Mois où les femelles sont suitées : tuer en ce moment est une faute grave.
    #[serde(default)]
    pub breeding: Option<Window>,
    /// Animal sacré ou protégé (baleine, ours, cygne…).
    #[serde(default)]
    pub sacred: bool,
    /// Poids de l'espèce dans la réputation (1 = gibier ordinaire, 0 = nuisible).
    #[serde(default = "one")]
    pub worth: f32,
}

/// Outil tenu pour dépecer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    /// Les mains : on déchire, on perd la plus grande part.
    #[default]
    Hands,
    Knife,
}

/// Un morceau qu'on tire de la carcasse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cut {
    pub item: String,
    pub min: u32,
    pub max: u32,
    #[serde(default = "one")]
    pub chance: f32,
    /// Mois de validité (pelage d'hiver, graisse d'automne…) ; absent = toute l'année.
    #[serde(default)]
    pub months: Option<Window>,
    /// Le couteau est indispensable (peau, bois).
    #[serde(default)]
    pub needs_knife: bool,
    /// Multiple du rendement avec un couteau (à la main : 1).
    #[serde(default = "knife_bonus")]
    pub knife_bonus: f32,
}

fn knife_bonus() -> f32 {
    1.5
}

/// Fiche d'écologie d'une espèce.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ecology {
    #[serde(default)]
    pub browse: Option<Browse>,
    #[serde(default)]
    pub fell: Option<Fell>,
    #[serde(default)]
    pub venom: Option<Venom>,
    #[serde(default)]
    pub tracks: Option<Tracks>,
    #[serde(default)]
    pub catch: Vec<CatchRule>,
    #[serde(default)]
    pub naturfrid: Option<Naturfrid>,
    /// Dépeçage : morceaux de la carcasse.
    #[serde(default)]
    pub loot: Vec<Cut>,
    /// Durée de la carcasse avant qu'elle se perde, en secondes de jeu.
    #[serde(default = "carcass_secs")]
    pub carcass: f32,
}

fn carcass_secs() -> f32 {
    600.0
}

impl Ecology {
    /// Taux de capture par minute pour un piège.
    pub fn catch_rate(&self, trap: TrapKind) -> f32 {
        self.catch
            .iter()
            .find(|c| c.trap == trap)
            .map_or(0.0, |c| c.chance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_sheet_parses() {
        let e: Ecology = serde_json::from_str("{}").unwrap();
        assert!(e.browse.is_none() && e.loot.is_empty());
        assert_eq!(e.carcass, 600.0);
        assert_eq!(e.catch_rate(TrapKind::Snare), 0.0);
    }

    #[test]
    fn a_sheet_with_a_snare_parses() {
        let e: Ecology =
            serde_json::from_str(r#"{"catch":[{"trap":"snare","chance":0.5}]}"#).unwrap();
        assert_eq!(e.catch_rate(TrapKind::Snare), 0.5);
        assert_eq!(e.catch_rate(TrapKind::Pit), 0.0);
    }
}
