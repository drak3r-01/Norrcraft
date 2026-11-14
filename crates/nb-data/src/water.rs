//! Règles de l'eau vivante (`assets/norrcraft/water.json`, doc 07 §14).
//!
//! Pas de clé `_doc` dans la table : elle est typée et refuse les champs inconnus.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use nb_core::{Error, Result};
use serde::{Deserialize, Serialize};

pub const WATER_JSON: &str = include_str!("../../../assets/norrcraft/water.json");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Speeds {
    pub fall: f32,
    pub film: f32,
    pub stream: f32,
    pub gradient: f32,
    pub max_free: f32,
    pub plunge_pull: f32,
    pub swirl: f32,
    pub rapid: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpringRule {
    /// Huitièmes versés par pas.
    pub rate: f32,
    /// Facteur par pas de saison (0 = cœur de l'hiver).
    pub season: [f32; 8],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Springs {
    pub mountain: SpringRule,
    pub karst: SpringRule,
    pub glacier: SpringRule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Weather {
    pub steps: u32,
    pub rain_samples: u32,
    pub puddle_max: u8,
    pub freeze_below: f32,
    pub thaw_above: f32,
    pub recharge_steps: u32,
    pub recharge_samples: u32,
    pub seep_steps: u32,
    pub seep_samples: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerRules {
    pub wade: f32,
    pub rock_hit: f32,
    pub upstream_drain: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoatRules {
    pub draught: u8,
    pub fall_water: f32,
    pub raft_breaks: u8,
    pub anchor_holds: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalmonRules {
    pub leap_max: u8,
    pub leap_chance: f32,
    pub leap_every: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlantRule {
    pub water: Vec<String>,
    pub depth: [u8; 2],
    pub current: [f32; 2],
}

/// Règles et constantes de l'eau vivante.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaterRules {
    pub step_ticks: u32,
    pub cells_per_tick: u32,
    pub time_cap_micros: u64,
    pub pressure_searches_per_tick: u32,
    pub pressure_nodes: u32,
    pub spread_max: u8,
    pub seek_radius: i32,
    pub calm_steps: u8,
    pub sim_radius: i32,
    pub boot_wake_max: u32,
    pub speeds: Speeds,
    pub springs: Springs,
    pub weather: Weather,
    pub player: PlayerRules,
    pub boats: BoatRules,
    pub salmon: SalmonRules,
    pub plants: BTreeMap<String, PlantRule>,
}

impl WaterRules {
    /// Règles intégrées (analysées une fois).
    pub fn builtin() -> Arc<WaterRules> {
        static RULES: OnceLock<Arc<WaterRules>> = OnceLock::new();
        RULES
            .get_or_init(|| Arc::new(Self::from_json(WATER_JSON).expect("water.json invalide")))
            .clone()
    }

    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| Error::data("water.json", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_parses() {
        let r = WaterRules::builtin();
        assert_eq!(r.step_ticks, 12);
        assert_eq!(r.cells_per_tick, 512);
        assert_eq!(r.springs.glacier.season[4], 2.0);
        assert!(r.plants.contains_key("nackmossa"));
    }
}
