//! Description d'un rig en données (JSON) : os, rattachement des boîtes de la géométrie aux os,
//! clips (canaux par os, courbes), démarches, idles, actions, regard et pilotes secondaires.
//!
//! Unités : rotations en **degrés**, translations et pivots en **pixels** (16 px = 1 bloc, comme
//! la géométrie des mobs), échelles en facteurs, durées en secondes. Une courbe est soit une
//! constante, soit une liste de clés `[[t, valeur], …]` (`t` entre 0 et 1, interpolation de
//! Catmull-Rom), soit une onde sinusoïdale ([`Wave`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Un os : nom, parent (nom, `None` pour la racine) et pivot au repos (espace du modèle, px).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoneDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub pivot: [f32; 3],
}

/// Onde `o + a·sin(2π(n·u + p)) + a2·sin(2π(n2·u + p2))` où `u ∈ [0, 1[` est la position dans
/// le clip. `n` et `n2` doivent être entiers pour qu'un clip en boucle soit continu. Avec
/// `abs`, le premier terme devient `a·|sin(π(n·u + p))|` (rebonds : `n` rebonds par boucle).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wave {
    #[serde(default)]
    pub a: f32,
    #[serde(default = "one")]
    pub n: f32,
    #[serde(default)]
    pub p: f32,
    #[serde(default)]
    pub o: f32,
    #[serde(default)]
    pub abs: bool,
    #[serde(default)]
    pub a2: f32,
    #[serde(default = "two")]
    pub n2: f32,
    #[serde(default)]
    pub p2: f32,
}

fn one() -> f32 {
    1.0
}
fn two() -> f32 {
    2.0
}

impl Wave {
    pub fn sin(a: f32, n: f32, p: f32) -> Self {
        Self {
            a,
            n,
            p,
            o: 0.0,
            abs: false,
            a2: 0.0,
            n2: 2.0,
            p2: 0.0,
        }
    }
}

/// Courbe d'une propriété d'un os au cours d'un clip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Curve {
    Const(f32),
    Keys(Vec<[f32; 2]>),
    Wave(Wave),
}

/// Canal : les courbes d'un os. Propriétés absentes = repos (0 pour rotation et translation,
/// 1 pour l'échelle). `s` est l'échelle uniforme, `sx/sy/sz` s'y multiplient.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelDef {
    pub bone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rx: Option<Curve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ry: Option<Curve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rz: Option<Curve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx: Option<Curve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<Curve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<Curve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<Curve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sx: Option<Curve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sy: Option<Curve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sz: Option<Curve>,
}

impl ChannelDef {
    pub fn new(bone: &str) -> Self {
        Self {
            bone: bone.to_owned(),
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        [
            &self.rx, &self.ry, &self.rz, &self.tx, &self.ty, &self.tz, &self.s, &self.sx,
            &self.sy, &self.sz,
        ]
        .iter()
        .all(|c| c.is_none())
    }
}

/// Un clip : durée d'une boucle (ou de l'action), mode de mélange et canaux.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipDef {
    pub duration: f32,
    #[serde(default, rename = "loop")]
    pub looping: bool,
    /// Additif : s'ajoute à la pose (rotations composées, translations sommées) ; sinon il la
    /// remplace, os par os, proportionnellement à son poids.
    #[serde(default)]
    pub additive: bool,
    /// Fondu d'entrée et de sortie (s) quand le clip joue en couche (idles, actions).
    #[serde(default = "default_fade")]
    pub fade: [f32; 2],
    pub channels: Vec<ChannelDef>,
}

fn default_fade() -> [f32; 2] {
    [0.12, 0.2]
}

/// Entrée de démarche : à `speed` blocs/s le clip joue à son rythme naturel (une boucle =
/// `speed × duration` blocs parcourus, ce qui évite le patinage des pieds).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GaitDef {
    pub clip: String,
    pub speed: f32,
}

/// Variante d'idle jouée au hasard quand le mob est immobile (brouter, regarder autour…).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdleDef {
    pub clip: String,
    #[serde(default = "one")]
    pub weight: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookBone {
    pub bone: String,
    /// Part de l'angle de regard prise par cet os (les poids se somment à ~1).
    pub weight: f32,
}

/// Regard : les os suivent le joueur à moins de `range` blocs, dans la limite des angles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookDef {
    pub bones: Vec<LookBone>,
    /// Degrés.
    pub yaw: f32,
    pub pitch: f32,
    pub range: f32,
}

/// Pilote secondaire : une propriété d'os suit en continu une grandeur du mouvement
/// (`turn` : vitesse de rotation en rad/s ; `speed` : vitesse normalisée par la démarche la plus
/// rapide ; `vspeed` : vitesse verticale en blocs/s), lissée sur `smooth` secondes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverDef {
    pub bone: String,
    /// `rx`, `ry`, `rz` (degrés) ou `tx`, `ty`, `tz` (pixels).
    pub prop: String,
    pub src: String,
    pub gain: f32,
    #[serde(default = "big")]
    pub max: f32,
    #[serde(default = "default_smooth")]
    pub smooth: f32,
}

fn big() -> f32 {
    1.0e6
}
fn default_smooth() -> f32 {
    0.15
}

/// Rig complet d'un modèle.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigDef {
    /// Parents avant enfants.
    pub bones: Vec<BoneDef>,
    /// Pour chaque boîte de la géométrie (dans l'ordre), l'indice de son os.
    pub boxes: Vec<u16>,
    #[serde(default)]
    pub clips: BTreeMap<String, ClipDef>,
    /// Démarches au sol, par vitesse croissante.
    #[serde(default)]
    pub gaits: Vec<GaitDef>,
    /// Démarches dans l'eau (nage).
    #[serde(default)]
    pub swim: Vec<GaitDef>,
    /// Clip en boucle joué à l'arrêt (respiration…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub idles: Vec<IdleDef>,
    /// Délai (s) min / max entre deux variantes d'idle.
    #[serde(default = "default_gap")]
    pub idle_gap: [f32; 2],
    /// Actions : `hurt`, `attack`, `jump` → nom de clip.
    #[serde(default)]
    pub actions: BTreeMap<String, String>,
    /// Clip superposé quand le mob est en l'air.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub air: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<LookDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drivers: Vec<DriverDef>,
}

fn default_gap() -> [f32; 2] {
    [4.0, 10.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_curve_forms() {
        let json = r#"{
            "duration": 1.0, "loop": true,
            "channels": [
              {"bone": "a", "rx": {"a": 30, "n": 1}, "ty": 0.5, "rz": [[0, 0], [0.5, 10], [1, 0]]}
            ]
        }"#;
        let c: ClipDef = serde_json::from_str(json).unwrap();
        assert!(c.looping && !c.additive);
        let ch = &c.channels[0];
        assert!(matches!(ch.rx, Some(Curve::Wave(w)) if w.a == 30.0 && w.n == 1.0));
        assert_eq!(ch.ty, Some(Curve::Const(0.5)));
        assert!(matches!(&ch.rz, Some(Curve::Keys(k)) if k.len() == 3));
        assert_eq!(c.fade, [0.12, 0.2]);
    }

    #[test]
    fn rejects_unknown_fields() {
        let r: Result<ClipDef, _> =
            serde_json::from_str(r#"{"duration":1,"channels":[],"bogus":1}"#);
        assert!(r.is_err());
    }
}
