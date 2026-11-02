//! Réglages du joueur (`saves/settings.json`, format inchangé).
//!
//! Les touches sont stockées sous `key_<ACTION>` avec des codes GLFW.

use std::collections::BTreeMap;

use nb_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::keys;

/// Actions clavier configurables (celles que `GameSettings` enregistre dans le fichier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Action {
    Forward,
    Backward,
    Left,
    Right,
    Jump,
    Sprint,
    Crouch,
    Pause,
    Inventory,
    Drop,
}

impl Action {
    pub const ALL: [Action; 10] = [
        Action::Forward,
        Action::Backward,
        Action::Left,
        Action::Right,
        Action::Jump,
        Action::Sprint,
        Action::Crouch,
        Action::Pause,
        Action::Inventory,
        Action::Drop,
    ];

    /// Nom Java de l'énumération `InputManager.Action`.
    pub fn name(self) -> &'static str {
        match self {
            Action::Forward => "FORWARD",
            Action::Backward => "BACKWARD",
            Action::Left => "LEFT",
            Action::Right => "RIGHT",
            Action::Jump => "JUMP",
            Action::Sprint => "SPRINT",
            Action::Crouch => "CROUCH",
            Action::Pause => "PAUSE",
            Action::Inventory => "INVENTORY",
            Action::Drop => "DROP",
        }
    }

    fn default_key(self) -> i32 {
        match self {
            Action::Forward => keys::KEY_W,
            Action::Backward => keys::KEY_S,
            Action::Left => keys::KEY_A,
            Action::Right => keys::KEY_D,
            Action::Jump => keys::KEY_SPACE,
            Action::Sprint => keys::KEY_LEFT_CONTROL,
            Action::Crouch => keys::KEY_LEFT_SHIFT,
            Action::Inventory => keys::KEY_E,
            Action::Drop => keys::KEY_Q,
            Action::Pause => keys::KEY_ESCAPE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ShadowQuality {
    Off,
    Low,
    High,
}

impl ShadowQuality {
    pub fn name(self) -> &'static str {
        match self {
            ShadowQuality::Off => "OFF",
            ShadowQuality::Low => "LOW",
            ShadowQuality::High => "HIGH",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameSettings {
    pub render_distance: i32,
    pub fov: f32,
    pub sensitivity: f32,
    pub shadow_quality: ShadowQuality,
    pub bloom_enabled: bool,
    pub vsync_enabled: bool,
    pub fullscreen: bool,
    pub resource_pack: String,
    pub window_width: i32,
    pub window_height: i32,
    pub key_bindings: BTreeMap<Action, i32>,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            render_distance: 12,
            fov: 90.0,
            sensitivity: 0.1,
            shadow_quality: ShadowQuality::Low,
            bloom_enabled: true,
            vsync_enabled: false,
            fullscreen: false,
            resource_pack: "Default".to_owned(),
            window_width: 1280,
            window_height: 720,
            key_bindings: Action::ALL.iter().map(|&a| (a, a.default_key())).collect(),
        }
    }
}

impl GameSettings {
    /// Lit un `settings.json`. Comme en Java, tout champ absent ou mal typé garde son défaut.
    pub fn from_json(json: &str) -> Result<Self> {
        let root: Value =
            serde_json::from_str(json).map_err(|e| Error::data("settings.json", e))?;
        let mut s = Self::default();
        let int = |k: &str| root.get(k).and_then(Value::as_i64).map(|v| v as i32);
        let float = |k: &str| root.get(k).and_then(Value::as_f64).map(|v| v as f32);
        let boolean = |k: &str| root.get(k).and_then(Value::as_bool);

        if let Some(v) = int("renderDistance") {
            s.render_distance = v;
        }
        if let Some(v) = float("fov") {
            s.fov = v;
        }
        if let Some(v) = float("sensitivity") {
            s.sensitivity = v;
        }
        if let Some(q) = root
            .get("shadowQuality")
            .and_then(|v| serde_json::from_value::<ShadowQuality>(v.clone()).ok())
        {
            s.shadow_quality = q;
        }
        if let Some(v) = boolean("bloomEnabled") {
            s.bloom_enabled = v;
        }
        if let Some(v) = boolean("vsyncEnabled") {
            s.vsync_enabled = v;
        }
        if let Some(v) = boolean("fullscreen") {
            s.fullscreen = v;
        }
        if let Some(v) = root.get("resourcePack").and_then(Value::as_str) {
            s.resource_pack = v.to_owned();
        }
        if let Some(v) = int("windowWidth") {
            s.window_width = v;
        }
        if let Some(v) = int("windowHeight") {
            s.window_height = v;
        }
        for action in Action::ALL {
            if let Some(code) = int(&format!("key_{}", action.name())) {
                s.key_bindings.insert(action, code);
            }
        }
        Ok(s)
    }

    /// Sérialise dans le format du Java (`key_<ACTION>` à plat, codes GLFW).
    pub fn to_json(&self) -> String {
        let mut map = serde_json::Map::new();
        map.insert("renderDistance".into(), self.render_distance.into());
        map.insert("fov".into(), float_value(self.fov));
        map.insert("sensitivity".into(), float_value(self.sensitivity));
        map.insert("shadowQuality".into(), self.shadow_quality.name().into());
        map.insert("bloomEnabled".into(), self.bloom_enabled.into());
        map.insert("vsyncEnabled".into(), self.vsync_enabled.into());
        map.insert("fullscreen".into(), self.fullscreen.into());
        map.insert("resourcePack".into(), self.resource_pack.clone().into());
        map.insert("windowWidth".into(), self.window_width.into());
        map.insert("windowHeight".into(), self.window_height.into());
        for (action, code) in &self.key_bindings {
            map.insert(format!("key_{}", action.name()), (*code).into());
        }
        serde_json::to_string_pretty(&Value::Object(map)).expect("sérialisation settings")
    }
}

/// Convertit un `f32` en nombre JSON via sa représentation décimale la plus courte
/// (`0.1f32` s'écrit `0.1` et non `0.10000000149011612`).
fn float_value(v: f32) -> Value {
    let mut text = v.to_string();
    if !text.contains(['.', 'e']) {
        text.push_str(".0");
    }
    text.parse::<serde_json::Number>()
        .map_or(Value::Null, Value::Number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_java() {
        let s = GameSettings::default();
        assert_eq!(s.render_distance, 12);
        assert_eq!(s.fov, 90.0);
        assert_eq!(s.shadow_quality, ShadowQuality::Low);
        assert!(s.bloom_enabled && !s.vsync_enabled && !s.fullscreen);
        assert_eq!((s.window_width, s.window_height), (1280, 720));
        assert_eq!(s.key_bindings[&Action::Forward], keys::KEY_W);
        assert_eq!(s.key_bindings[&Action::Sprint], keys::KEY_LEFT_CONTROL);
        assert_eq!(s.key_bindings[&Action::Crouch], keys::KEY_LEFT_SHIFT);
        assert_eq!(s.key_bindings[&Action::Pause], keys::KEY_ESCAPE);
    }

    #[test]
    fn parses_partial_java_file() {
        let s = GameSettings::from_json(
            r#"{
  "renderDistance": 8,
  "fov": 70.5,
  "shadowQuality": "HIGH",
  "resourcePack": "My Pack",
  "windowWidth": 1920
  ,"key_FORWARD": 265
}"#,
        )
        .unwrap();
        assert_eq!(s.render_distance, 8);
        assert_eq!(s.fov, 70.5);
        assert_eq!(s.shadow_quality, ShadowQuality::High);
        assert_eq!(s.resource_pack, "My Pack");
        assert_eq!(s.window_width, 1920);
        assert_eq!(s.window_height, 720);
        assert_eq!(s.key_bindings[&Action::Forward], 265);
        assert_eq!(s.key_bindings[&Action::Backward], keys::KEY_S);
    }

    /// `assets/settings_defaults.json` (copié en `saves/settings.json` par
    /// `scripts/package.sh`) doit rester égal aux réglages par défaut.
    #[test]
    fn packaged_defaults_file_matches() {
        let file = include_str!("../../../assets/settings_defaults.json");
        assert_eq!(
            GameSettings::from_json(file).unwrap(),
            GameSettings::default()
        );
        let a: Value = serde_json::from_str(file).unwrap();
        let b: Value = serde_json::from_str(&GameSettings::default().to_json()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn roundtrip_and_float_formatting() {
        let mut s = GameSettings {
            shadow_quality: ShadowQuality::Off,
            ..Default::default()
        };
        s.key_bindings.insert(Action::Jump, 70);
        let json = s.to_json();
        assert!(json.contains("\"sensitivity\": 0.1"), "{json}");
        assert!(json.contains("\"key_JUMP\": 70"));
        assert_eq!(GameSettings::from_json(&json).unwrap(), s);
    }
}
