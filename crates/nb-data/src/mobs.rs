//! Configuration des mobs : valeurs par défaut intégrées + surcharges `saves/mobs.json`.
//!
//! Comme en Java, la config est à deux niveaux : les mobs intégrés puis les champs présents dans
//! `mobs.json` qui écrasent un par un les valeurs courantes. Les 3 mobs d'origine (renne, poisson,
//! orque) sont suivis des animaux de la phase 9 (`mobs_nb.json`, généré par `scripts/gen_mobs.py`),
//! qui ajoutent des conditions d'apparition (biomes, sols, altitude, profondeur).
//! Un identifiant inconnu crée un mob personnalisé, initialisé depuis le renne.

use std::collections::HashMap;

use nb_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::universe::WorldKind;

/// Valeurs par défaut des mobs intégrés (même format que `mobs.json`).
const DEFAULTS_JSON: &str = include_str!("../../../assets/vanilla/mobs_defaults.json");
/// Boîtes colorées des mobs intégrés : `[x, y, z, w, h, d, r, g, b, a]` en pixels MC (÷16).
const GEOMETRY_JSON: &str = include_str!("../../../assets/vanilla/mob_geometry.json");
/// Animaux de la phase 9 : réglages et géométrie (même formats).
const NB_DEFAULTS_JSON: &str = include_str!("../../../assets/vanilla/mobs_nb.json");
const NB_GEOMETRY_JSON: &str = include_str!("../../../assets/vanilla/mob_geometry_nb.json");
/// Rigs d'animation (os, clips, démarches) de tous les mobs intégrés, générés par `scripts/gen_mobs.py`.
const RIGS_JSON: &str = include_str!("../../../assets/vanilla/mob_rigs.json");
/// Bestiaire Norrcraft (session 5.4) : réglages, géométrie et rigs, générés par
/// `scripts/gen_norr_mobs.py`.
const NORR_DEFAULTS_JSON: &str = include_str!("../../../assets/norrcraft/mobs.json");
const NORR_GEOMETRY_JSON: &str = include_str!("../../../assets/norrcraft/mob_geometry.json");
const NORR_RIGS_JSON: &str = include_str!("../../../assets/norrcraft/mob_rigs.json");

/// Identifiants des trois mobs d'origine, dans l'ordre de l'enum Java `MobType`.
pub const BUILTIN_MOB_IDS: [&str; 3] = ["reindeer", "fish", "orca"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MobMedium {
    Land,
    Water,
    Air,
    /// Phoque, castor, loutre : nage dans l'eau, marche à terre (session 5.2).
    Amphibian,
    /// Banquise : ne quitte pas la glace et y glisse (session 5.2).
    Ice,
}

/// Allure effective d'un mob à un instant : ce que la physique et l'IA doivent faire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gait {
    /// Marche, avec gravité.
    Walk,
    /// Nage en 3D, sans gravité (eau).
    Swim,
    /// Vole en 3D, sans gravité (air).
    Fly,
}

impl MobMedium {
    /// Allure selon que le mob est dans l'eau ou non.
    pub fn gait(self, in_water: bool) -> Gait {
        match self {
            MobMedium::Land | MobMedium::Ice => Gait::Walk,
            MobMedium::Water => Gait::Swim,
            MobMedium::Air => Gait::Fly,
            MobMedium::Amphibian if in_water => Gait::Swim,
            MobMedium::Amphibian => Gait::Walk,
        }
    }

    /// Le mob se déplace-t-il en 3D (nage ou vol) à cet instant ?
    pub fn is_3d(self, in_water: bool) -> bool {
        self.gait(in_water) != Gait::Walk
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MobBehaviorType {
    Passive,
    Aggressive,
}

/// Géométrie en boîtes colorées (mobs intégrés uniquement).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MobGeometry {
    /// `[x, y, z, w, h, d, r, g, b, a]`, coordonnées en pixels MC (÷16 = unités monde).
    pub boxes: Vec<[f32; 10]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobConfig {
    pub id: String,
    pub display_name: String,
    #[serde(skip)]
    pub geometry: Option<MobGeometry>,
    /// Squelette et animations (rig) du modèle ; hérité par les mobs personnalisés copiés d'un
    /// mob intégré. `None` : modèle statique.
    #[serde(skip)]
    pub rig: Option<std::sync::Arc<nb_anim::RigDef>>,

    pub width_x: f32,
    pub width_z: f32,
    pub height: f32,
    pub max_health: f32,

    pub medium: MobMedium,
    pub spawn_min_water_depth: i32,

    pub max_count: i32,
    pub group_size_min: i32,
    pub group_size_max: i32,
    pub spawn_scatter_radius: i32,

    pub behavior_type: MobBehaviorType,

    pub move_speed_min: f32,
    pub move_speed_max: f32,
    pub move_duration_min: f32,
    pub move_duration_max: f32,
    pub pause_duration_min: f32,
    pub pause_duration_max: f32,

    pub herding: bool,
    pub herd_radius: f32,
    pub herd_bias: f32,

    pub aggressor_detect_range: f32,
    pub aggressor_flee_speed: f32,
    pub player_flee_speed: f32,
    pub player_flee_duration_min: f32,
    pub player_flee_duration_max: f32,

    pub aggro_player_range: f32,
    pub chase_speed: f32,
    pub attack_range: f32,
    pub attack_damage: f32,
    pub attack_cooldown: f32,
    pub knockback_force: f32,

    /// Conditions d'apparition (phase 9). Tout est optionnel : vide / 0 = pas de contrainte, ce
    /// qui est le cas des trois mobs d'origine (leur apparition reste celle du Java).
    /// Noms de biomes (`Biome::name()`, ex. `SNOWY_PLAINS`) où le mob peut apparaître.
    #[serde(default)]
    pub spawn_biomes: Vec<String>,
    /// Noms de blocs sur lesquels il apparaît (sol pour un terrestre, fond pour un aquatique).
    /// Un mob soumis à une condition n'apparaît jamais sur les feuillages.
    #[serde(default)]
    pub spawn_surfaces: Vec<String>,
    #[serde(default)]
    pub spawn_min_y: i32,
    /// `0` = pas de plafond.
    #[serde(default)]
    pub spawn_max_y: i32,
    /// Profondeur d'eau maximale (rivières) ; `0` = pas de plafond.
    #[serde(default)]
    pub spawn_max_water_depth: i32,
    /// Profil de comportement Norrcraft (`behaviors.json`, session 5.1) : le mob est alors mené
    /// par le moteur d'états (`nb-ecs::brain`) au lieu des IA passive et agressive d'origine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<String>,
    /// Apparition par densité cible (session 5.2, `spawn_rules.rs`) : le mob quitte alors
    /// l'apparition d'origine (plafond, biomes, sols) pour celle du système de densité.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn: Option<crate::spawn_rules::SpawnRules>,
    /// Fiche d'élevage d'un animal domestique (session 5.7, `husbandry.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub husbandry: Option<crate::husbandry::Husbandry>,
    /// Interactions avec le monde, traces, piégeage et dépeçage (session 5.8, `ecology.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ecology: Option<crate::ecology::Ecology>,
}

impl MobConfig {
    /// Vrai si le mob a au moins une condition d'apparition (phase 9).
    pub fn has_habitat(&self) -> bool {
        !self.spawn_biomes.is_empty()
            || !self.spawn_surfaces.is_empty()
            || self.spawn_min_y > 0
            || self.spawn_max_y > 0
            || self.spawn_max_water_depth > 0
    }
}

fn num(obj: &Value, key: &str) -> Option<f64> {
    obj.get(key).and_then(Value::as_f64)
}

impl MobConfig {
    /// Applique les champs présents dans un objet JSON ; les absents (ou invalides) sont
    /// conservés, comme les `extractFloat(obj, key, c.field)` du Java.
    fn apply_json(&mut self, obj: &Value) {
        macro_rules! f32_field {
            ($($json:literal => $field:ident),* $(,)?) => {
                $(if let Some(v) = num(obj, $json) { self.$field = v as f32; })*
            };
        }
        macro_rules! i32_field {
            ($($json:literal => $field:ident),* $(,)?) => {
                $(if let Some(v) = obj.get($json).and_then(Value::as_i64) {
                    self.$field = v as i32;
                })*
            };
        }
        if let Some(s) = obj.get("displayName").and_then(Value::as_str) {
            self.display_name = s.to_owned();
        }
        if let Some(s) = obj.get("behavior").and_then(Value::as_str) {
            self.behavior = (!s.is_empty()).then(|| s.to_owned());
        }
        if let Some(v) = obj.get("spawn") {
            if v.is_null() {
                self.spawn = None;
            } else if let Ok(rules) =
                serde_json::from_value::<crate::spawn_rules::SpawnRules>(v.clone())
            {
                // Règles incohérentes : ignorées, comme tout champ invalide.
                if rules.validate().is_ok() {
                    self.spawn = Some(rules);
                }
            }
        }
        f32_field! {
            "widthX" => width_x, "widthZ" => width_z, "height" => height,
            "maxHealth" => max_health,
            "moveSpeedMin" => move_speed_min, "moveSpeedMax" => move_speed_max,
            "moveDurationMin" => move_duration_min, "moveDurationMax" => move_duration_max,
            "pauseDurationMin" => pause_duration_min, "pauseDurationMax" => pause_duration_max,
            "herdRadius" => herd_radius, "herdBias" => herd_bias,
            "aggressorDetectRange" => aggressor_detect_range,
            "aggressorFleeSpeed" => aggressor_flee_speed,
            "playerFleeSpeed" => player_flee_speed,
            "playerFleeDurationMin" => player_flee_duration_min,
            "playerFleeDurationMax" => player_flee_duration_max,
            "aggroPlayerRange" => aggro_player_range, "chaseSpeed" => chase_speed,
            "attackRange" => attack_range, "attackDamage" => attack_damage,
            "attackCooldown" => attack_cooldown, "knockbackForce" => knockback_force,
        }
        i32_field! {
            "spawnMinWaterDepth" => spawn_min_water_depth, "maxCount" => max_count,
            "groupSizeMin" => group_size_min, "groupSizeMax" => group_size_max,
            "spawnScatterRadius" => spawn_scatter_radius,
            "spawnMinY" => spawn_min_y, "spawnMaxY" => spawn_max_y,
            "spawnMaxWaterDepth" => spawn_max_water_depth,
        }
        for (key, slot) in [
            ("spawnBiomes", &mut self.spawn_biomes),
            ("spawnSurfaces", &mut self.spawn_surfaces),
        ] {
            if let Some(a) = obj.get(key).and_then(Value::as_array) {
                *slot = a
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
        }
        if let Some(m) = obj
            .get("medium")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
        {
            self.medium = m;
        }
        if let Some(b) = obj
            .get("behaviorType")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
        {
            self.behavior_type = b;
        }
        if let Some(h) = obj.get("herding").and_then(Value::as_bool) {
            self.herding = h;
        }
    }
}

/// Registre des mobs (liste ordonnée : intégrés d'abord, puis personnalisés).
#[derive(Debug, Clone)]
pub struct MobRegistry {
    configs: Vec<MobConfig>,
    kind: WorldKind,
}

impl MobRegistry {
    /// Les trois mobs intégrés, sans surcharge utilisateur.
    pub fn builtin() -> Self {
        Self {
            configs: builtin_configs().expect("données de mobs intégrées invalides"),
            kind: WorldKind::Vanilla,
        }
    }

    /// Mobs intégrés d'un univers : les 48 de vanilla ; pour Norrcraft, le bestiaire de la
    /// phase 5 (lot terrestre de la session 5.4, les autres lots suivent).
    pub fn builtin_for(kind: WorldKind) -> Self {
        match kind {
            WorldKind::Vanilla => Self::builtin(),
            WorldKind::Norrcraft => Self {
                configs: norr_configs().expect("données de mobs Norrcraft invalides"),
                kind,
            },
        }
    }

    /// Univers auquel appartient ce registre.
    pub fn kind(&self) -> WorldKind {
        self.kind
    }

    /// `MobRegistry.load()` : intégrés puis surcharges de `mobs.json` (si fourni).
    /// Un JSON invalide est ignoré comme en Java (le message part sur stderr).
    pub fn load(mobs_json: Option<&str>) -> Self {
        Self::load_for(WorldKind::Vanilla, mobs_json)
    }

    /// Comme [`MobRegistry::load`] pour l'univers `kind`.
    pub fn load_for(kind: WorldKind, mobs_json: Option<&str>) -> Self {
        let mut reg = Self::builtin_for(kind);
        if let Some(json) = mobs_json {
            if let Err(e) = reg.apply_overrides(json) {
                eprintln!("Could not load mobs.json: {e}");
            }
        }
        reg
    }

    /// Fusionne le contenu d'un `mobs.json`.
    pub fn apply_overrides(&mut self, json: &str) -> Result<()> {
        let root: Value = serde_json::from_str(json).map_err(|e| Error::data("mobs.json", e))?;
        let Some(list) = root.get("mobs").and_then(Value::as_array) else {
            return Ok(());
        };
        for obj in list {
            let Some(id) = obj
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            if let Some(existing) = self.configs.iter_mut().find(|c| c.id == id) {
                existing.apply_json(obj);
            } else {
                // Nouveau mob : on part des valeurs du renne pour les champs physiques.
                let mut cfg = builtin_configs()?.remove(0);
                cfg.id = id.to_owned();
                cfg.display_name = id.to_owned();
                cfg.apply_json(obj);
                self.configs.push(cfg);
            }
        }
        Ok(())
    }

    pub fn all(&self) -> &[MobConfig] {
        &self.configs
    }

    pub fn all_mut(&mut self) -> &mut Vec<MobConfig> {
        &mut self.configs
    }

    pub fn get(&self, id: &str) -> Option<&MobConfig> {
        self.configs.iter().find(|c| c.id == id)
    }

    pub fn is_built_in(id: &str) -> bool {
        BUILTIN_MOB_IDS.contains(&id)
            || builtin_mob_ids().iter().any(|b| b == id)
            || norr_mob_ids().iter().any(|b| b == id)
    }

    pub fn id_exists(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// `resetToDefaults()` : retire les mobs personnalisés et restaure les valeurs intégrées.
    pub fn reset_to_defaults(&mut self) {
        *self = Self::builtin_for(self.kind);
    }

    /// Sérialise comme `MobRegistry.save()` : `{ "mobs": [ ... ] }`.
    pub fn to_json(&self) -> String {
        #[derive(Serialize)]
        struct File<'a> {
            mobs: &'a [MobConfig],
        }
        serde_json::to_string_pretty(&File {
            mobs: &self.configs,
        })
        .expect("sérialisation mobs")
    }
}

type RigMap = HashMap<String, std::sync::Arc<nb_anim::RigDef>>;

/// Rigs intégrés (analysés une seule fois).
fn builtin_rigs() -> Result<&'static RigMap> {
    static RIGS: std::sync::OnceLock<std::result::Result<RigMap, String>> =
        std::sync::OnceLock::new();
    RIGS.get_or_init(|| {
        let raw: HashMap<String, nb_anim::RigDef> =
            serde_json::from_str(RIGS_JSON).map_err(|e| e.to_string())?;
        Ok(raw
            .into_iter()
            .map(|(k, v)| (k, std::sync::Arc::new(v)))
            .collect())
    })
    .as_ref()
    .map_err(|e| Error::data("mob_rigs.json", e.clone()))
}

/// Rig d'un mob intégré, à condition qu'il couvre exactement les boîtes de sa géométrie.
fn rig_for(rigs: &RigMap, id: &str, boxes: usize) -> Option<std::sync::Arc<nb_anim::RigDef>> {
    rigs.get(id).filter(|r| r.boxes.len() == boxes).cloned()
}

fn builtin_configs() -> Result<Vec<MobConfig>> {
    #[derive(Deserialize)]
    struct File {
        mobs: Vec<MobConfig>,
    }
    let file: File =
        serde_json::from_str(DEFAULTS_JSON).map_err(|e| Error::data("mobs_defaults.json", e))?;
    let geometry: HashMap<String, Vec<[f32; 10]>> =
        serde_json::from_str(GEOMETRY_JSON).map_err(|e| Error::data("mob_geometry.json", e))?;
    let mut configs = Vec::with_capacity(BUILTIN_MOB_IDS.len() + 48);
    let rigs = builtin_rigs()?;
    for id in BUILTIN_MOB_IDS {
        let mut cfg = file
            .mobs
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| Error::data("mobs_defaults.json", format!("mob manquant: {id}")))?;
        let boxes = geometry.get(id).cloned().ok_or_else(|| {
            Error::data("mob_geometry.json", format!("géométrie manquante: {id}"))
        })?;
        cfg.rig = rig_for(rigs, &cfg.id, boxes.len());
        cfg.geometry = Some(MobGeometry { boxes });
        configs.push(cfg);
    }
    // Animaux de la phase 9, dans l'ordre du fichier.
    let nb_file: File =
        serde_json::from_str(NB_DEFAULTS_JSON).map_err(|e| Error::data("mobs_nb.json", e))?;
    let nb_geometry: HashMap<String, Vec<[f32; 10]>> = serde_json::from_str(NB_GEOMETRY_JSON)
        .map_err(|e| Error::data("mob_geometry_nb.json", e))?;
    for mut cfg in nb_file.mobs {
        let boxes = nb_geometry.get(&cfg.id).cloned().ok_or_else(|| {
            Error::data(
                "mob_geometry_nb.json",
                format!("géométrie manquante: {}", cfg.id),
            )
        })?;
        cfg.rig = rig_for(rigs, &cfg.id, boxes.len());
        cfg.geometry = Some(MobGeometry { boxes });
        configs.push(cfg);
    }
    Ok(configs)
}

/// Bestiaire de l'univers Norrcraft, dans l'ordre du fichier.
fn norr_configs() -> Result<Vec<MobConfig>> {
    #[derive(Deserialize)]
    struct File {
        mobs: Vec<MobConfig>,
    }
    let file: File = serde_json::from_str(NORR_DEFAULTS_JSON)
        .map_err(|e| Error::data("norrcraft/mobs.json", e))?;
    let geometry: HashMap<String, Vec<[f32; 10]>> = serde_json::from_str(NORR_GEOMETRY_JSON)
        .map_err(|e| Error::data("norrcraft/mob_geometry.json", e))?;
    let raw: HashMap<String, nb_anim::RigDef> = serde_json::from_str(NORR_RIGS_JSON)
        .map_err(|e| Error::data("norrcraft/mob_rigs.json", e))?;
    let rigs: RigMap = raw
        .into_iter()
        .map(|(k, v)| (k, std::sync::Arc::new(v)))
        .collect();
    let mut configs = Vec::with_capacity(file.mobs.len());
    for mut cfg in file.mobs {
        let boxes = geometry.get(&cfg.id).cloned().ok_or_else(|| {
            Error::data(
                "norrcraft/mob_geometry.json",
                format!("géométrie manquante: {}", cfg.id),
            )
        })?;
        cfg.rig = rig_for(&rigs, &cfg.id, boxes.len());
        cfg.geometry = Some(MobGeometry { boxes });
        configs.push(cfg);
    }
    Ok(configs)
}

/// Identifiants des mobs intégrés de Norrcraft.
pub fn norr_mob_ids() -> &'static [String] {
    static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| {
        norr_configs()
            .expect("données de mobs Norrcraft invalides")
            .into_iter()
            .map(|c| c.id)
            .collect()
    })
}

/// Identifiants de tous les mobs intégrés (d'origine puis phase 9).
pub fn builtin_mob_ids() -> &'static [String] {
    static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| {
        builtin_configs()
            .expect("données de mobs intégrées invalides")
            .into_iter()
            .map(|c| c.id)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_order_and_values() {
        let reg = MobRegistry::builtin();
        let ids: Vec<_> = reg.all().iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids[..3], BUILTIN_MOB_IDS);
        assert!(
            ids.len() >= 40,
            "les animaux de la phase 9 manquent : {}",
            ids.len()
        );
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "identifiants en double");
        let reindeer = reg.get("reindeer").unwrap();
        assert_eq!(reindeer.max_health, 5.0);
        assert_eq!(reindeer.medium, MobMedium::Land);
        assert_eq!(reindeer.player_flee_speed, 4.317);
        assert_eq!(
            (
                reindeer.aggressor_detect_range,
                reindeer.aggressor_flee_speed
            ),
            (10.0, 3.0)
        );
        assert_eq!(reindeer.geometry.as_ref().unwrap().boxes.len(), 17);
        let orca = reg.get("orca").unwrap();
        assert_eq!(orca.behavior_type, MobBehaviorType::Aggressive);
        assert_eq!(orca.aggro_player_range, 200.0);
        assert_eq!(orca.spawn_min_water_depth, 5);
        assert!(reg.get("fish").unwrap().herding);
    }

    #[test]
    fn overrides_patch_fields_and_add_custom_mobs() {
        let mut reg = MobRegistry::builtin();
        reg.apply_overrides(
            r#"{ "mobs": [
                { "id": "reindeer", "maxHealth": 12.5, "maxCount": 3 },
                { "id": "zombie", "displayName": "Zombie", "behaviorType": "AGGRESSIVE" }
            ] }"#,
        )
        .unwrap();
        let r = reg.get("reindeer").unwrap();
        assert_eq!(r.max_health, 12.5);
        assert_eq!(r.max_count, 3);
        assert_eq!(r.width_x, 0.75);
        let z = reg.get("zombie").unwrap();
        assert_eq!(z.behavior_type, MobBehaviorType::Aggressive);
        assert_eq!(z.width_z, 1.375); // hérité du renne
        assert!(z.geometry.is_some()); // hérité du renne intégré
        assert!(!MobRegistry::is_built_in("zombie"));
    }

    #[test]
    fn json_roundtrip_keeps_overrides() {
        let mut reg = MobRegistry::builtin();
        reg.all_mut()[1].max_health = 9.0;
        let json = reg.to_json();
        let reloaded = MobRegistry::load(Some(&json));
        assert_eq!(reloaded.get("fish").unwrap().max_health, 9.0);
    }

    #[test]
    fn reset_removes_custom_mobs() {
        let mut reg = MobRegistry::builtin();
        reg.apply_overrides(r#"{ "mobs": [ { "id": "zombie" } ] }"#)
            .unwrap();
        assert!(reg.id_exists("zombie"));
        reg.reset_to_defaults();
        assert!(!reg.id_exists("zombie"));
    }

    /// Tous les mobs intégrés ont un rig valide, couvrant exactement leur géométrie, qui se
    /// compile, et dont l'animation reste saine (valeurs finies, membres près du modèle, pieds
    /// au sol à la marche).
    #[test]
    fn every_builtin_mob_has_a_sound_rig() {
        use nb_anim::{AnimInput, Animator, Mat34, Rig, Scratch};
        let reg = MobRegistry::builtin();
        let norr = MobRegistry::builtin_for(WorldKind::Norrcraft);
        let mut sc = Scratch::new();
        for c in reg.all().iter().chain(norr.all()) {
            let def = c
                .rig
                .as_ref()
                .unwrap_or_else(|| panic!("{} : pas de rig", c.id));
            let boxes = &c.geometry.as_ref().unwrap().boxes;
            assert_eq!(def.boxes.len(), boxes.len(), "{}", c.id);
            let rig = Rig::compile(def).unwrap_or_else(|e| panic!("{} : {e}", c.id));
            // Boîte englobante au repos et rayon de tolérance.
            let (mut lo, mut hi) = (glam_min(), glam_max());
            for b in boxes {
                for k in 0..3 {
                    lo[k] = lo[k].min(b[k] / 16.0);
                    hi[k] = hi[k].max((b[k] + b[k + 3]) / 16.0);
                }
            }
            let size = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]);
            let slack = 0.45 * size + 0.15;
            let mut pal = vec![Mat34::IDENTITY; rig.bone_count()];
            let water = c.medium == MobMedium::Water;
            let top = rig
                .gaits
                .iter()
                .chain(&rig.swim)
                .map(|g| g.speed)
                .fold(0.0, f32::max);
            let walk = rig
                .gaits
                .first()
                .or(rig.swim.first())
                .map_or(0.0, |g| g.speed);
            for (label, speed) in [("repos", 0.0), ("marche", walk), ("course", top)] {
                let mut a = Animator::new(c.id.len() as u32);
                let inp = AnimInput {
                    speed,
                    grounded: true,
                    in_water: water,
                    ..AnimInput::default()
                };
                for step in 0..360 {
                    a.update(&rig, &inp, 1.0 / 60.0);
                    if step % 3 == 0 {
                        a.evaluate(&rig, &mut sc, &mut pal);
                        for (bi, m) in pal.iter().enumerate() {
                            assert!(m.0.iter().all(|v| v.is_finite()), "{} {label}", c.id);
                            // Coins des boîtes de cet os.
                            for (i, b) in boxes.iter().enumerate() {
                                if usize::from(def.boxes[i]) != bi {
                                    continue;
                                }
                                for corner in 0..8 {
                                    let p = [
                                        (b[0] + if corner & 1 != 0 { b[3] } else { 0.0 }) / 16.0,
                                        (b[1] + if corner & 2 != 0 { b[4] } else { 0.0 }) / 16.0,
                                        (b[2] + if corner & 4 != 0 { b[5] } else { 0.0 }) / 16.0,
                                    ];
                                    let q = m.transform_point(p.into());
                                    for k in 0..3 {
                                        assert!(
                                            q[k] > lo[k] - slack && q[k] < hi[k] + slack,
                                            "{} {label} : boîte {i} sort du modèle ({q:?}, modèle {lo:?}–{hi:?})",
                                            c.id
                                        );
                                    }
                                    if !water && label != "repos" && step > 45 {
                                        assert!(
                                            q.y > -0.16 - 0.02 * size,
                                            "{} {label} : s'enfonce dans le sol ({q:?})",
                                            c.id
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Les actions et variantes d'idle jouent sans produire de valeur invalide.
            let mut a = Animator::new(5);
            for act in [
                nb_anim::Action::Hurt,
                nb_anim::Action::Attack,
                nb_anim::Action::Jump,
            ] {
                a.trigger(&rig, act);
            }
            for i in 0..rig.clips.len() {
                a.play_variant(i);
                for _ in 0..30 {
                    a.update(
                        &rig,
                        &AnimInput {
                            grounded: true,
                            ..Default::default()
                        },
                        1.0 / 30.0,
                    );
                    a.evaluate(&rig, &mut sc, &mut pal);
                    assert!(pal.iter().all(|m| m.0.iter().all(|v| v.is_finite())));
                }
            }
        }
    }

    fn glam_min() -> [f32; 3] {
        [f32::MAX; 3]
    }
    fn glam_max() -> [f32; 3] {
        [f32::MIN; 3]
    }

    #[test]
    fn custom_mobs_inherit_the_reindeer_rig() {
        let mut reg = MobRegistry::builtin();
        reg.apply_overrides(r#"{ "mobs": [ { "id": "zombie" } ] }"#)
            .unwrap();
        let z = reg.get("zombie").unwrap();
        assert!(z.rig.is_some(), "rig hérité avec la géométrie du renne");
    }
}
