//! Configuration des mobs : valeurs par défaut intégrées + surcharges `saves/mobs.json`.
//!
//! Comme en Java, la config est à deux niveaux : les 3 mobs intégrés (renne, poisson, orque)
//! puis les champs présents dans `mobs.json` qui écrasent un par un les valeurs courantes.
//! Un identifiant inconnu crée un mob personnalisé, initialisé depuis le renne.

use std::collections::HashMap;

use nb_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Valeurs par défaut des mobs intégrés (même format que `mobs.json`).
const DEFAULTS_JSON: &str = include_str!("../../../assets/mobs_defaults.json");
/// Boîtes colorées des mobs intégrés : `[x, y, z, w, h, d, r, g, b, a]` en pixels MC (÷16).
const GEOMETRY_JSON: &str = include_str!("../../../assets/mob_geometry.json");

/// Identifiants des mobs intégrés, dans l'ordre de l'enum Java `MobType`.
pub const BUILTIN_MOB_IDS: [&str; 3] = ["reindeer", "fish", "orca"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MobMedium {
    Land,
    Water,
    Air,
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
        }
        if let Some(m) = obj.get("medium").and_then(|v| serde_json::from_value(v.clone()).ok()) {
            self.medium = m;
        }
        if let Some(b) =
            obj.get("behaviorType").and_then(|v| serde_json::from_value(v.clone()).ok())
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
}

impl MobRegistry {
    /// Les trois mobs intégrés, sans surcharge utilisateur.
    pub fn builtin() -> Self {
        Self { configs: builtin_configs().expect("données de mobs intégrées invalides") }
    }

    /// `MobRegistry.load()` : intégrés puis surcharges de `mobs.json` (si fourni).
    /// Un JSON invalide est ignoré comme en Java (le message part sur stderr).
    pub fn load(mobs_json: Option<&str>) -> Self {
        let mut reg = Self::builtin();
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
            let Some(id) = obj.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) else {
                continue;
            };
            if let Some(existing) = self.configs.iter_mut().find(|c| c.id == id) {
                existing.apply_json(obj);
            } else {
                // Nouveau mob : on part des valeurs du renne pour les champs physiques.
                let mut cfg = self.configs[0].clone();
                cfg.id = id.to_owned();
                cfg.display_name = id.to_owned();
                cfg.geometry = None;
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
    }

    pub fn id_exists(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// `resetToDefaults()` : retire les mobs personnalisés et restaure les valeurs intégrées.
    pub fn reset_to_defaults(&mut self) {
        *self = Self::builtin();
    }

    /// Sérialise comme `MobRegistry.save()` : `{ "mobs": [ ... ] }`.
    pub fn to_json(&self) -> String {
        #[derive(Serialize)]
        struct File<'a> {
            mobs: &'a [MobConfig],
        }
        serde_json::to_string_pretty(&File { mobs: &self.configs }).expect("sérialisation mobs")
    }
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
    let mut configs = Vec::with_capacity(BUILTIN_MOB_IDS.len());
    for id in BUILTIN_MOB_IDS {
        let mut cfg = file
            .mobs
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| Error::data("mobs_defaults.json", format!("mob manquant: {id}")))?;
        let boxes = geometry
            .get(id)
            .cloned()
            .ok_or_else(|| Error::data("mob_geometry.json", format!("géométrie manquante: {id}")))?;
        cfg.geometry = Some(MobGeometry { boxes });
        configs.push(cfg);
    }
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_order_and_values() {
        let reg = MobRegistry::builtin();
        let ids: Vec<_> = reg.all().iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, BUILTIN_MOB_IDS);
        let reindeer = reg.get("reindeer").unwrap();
        assert_eq!(reindeer.max_health, 5.0);
        assert_eq!(reindeer.medium, MobMedium::Land);
        assert_eq!(reindeer.player_flee_speed, 4.317);
        assert_eq!(reindeer.geometry.as_ref().unwrap().boxes.len(), 16);
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
        assert!(z.geometry.is_none());
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
        reg.apply_overrides(r#"{ "mobs": [ { "id": "zombie" } ] }"#).unwrap();
        assert!(reg.id_exists("zombie"));
        reg.reset_to_defaults();
        assert!(!reg.id_exists("zombie"));
    }
}
