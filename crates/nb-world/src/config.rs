//! Paramètres du monde (`WorldConfig.java`) et préréglages.

use serde::{Deserialize, Deserializer, Serialize};

use crate::biome::CLASSIC_BIOME_COUNT;

/// Type de monde choisi à la création (non sauvegardé, seul le `WorldConfig` résultant l'est).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorldType {
    Normal,
    Flat,
    Amplified,
    Island,
    /// Beaucoup de petites îles dans un océan tropical (générateur « Norrblock »).
    Archipelago,
    /// Hautes terres : reliefs massifs, peu d'océan (générateur « Norrblock »).
    Highlands,
    /// Terrain du jeu Java d'origine, 8 biomes (générateur classique).
    Classic,
}

impl WorldType {
    pub const ALL: [WorldType; 7] = [
        WorldType::Normal,
        WorldType::Flat,
        WorldType::Amplified,
        WorldType::Island,
        WorldType::Archipelago,
        WorldType::Highlands,
        WorldType::Classic,
    ];

    /// Type suivant dans le cycle de l'UI.
    pub fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }
}

/// Algorithme de génération du terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GeneratorKind {
    /// Port exact du générateur Java (8 biomes) — conservé pour les vecteurs de référence et
    /// pour les mondes créés avant l'arrivée du générateur « Norrblock ».
    Classic,
    /// Générateur « Norrblock » : relief continu, 30 biomes, arbres et décors variés.
    Norrblock,
}

/// Un `world.json` sans champ `generator` date d'avant le nouveau générateur : il garde le
/// terrain classique pour ne pas créer de raccord avec les chunks déjà explorés.
fn classic_generator() -> GeneratorKind {
    GeneratorKind::Classic
}

/// Plage de génération d'un minerai (bornes inclusives).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OreRange {
    pub min_y: i32,
    pub max_y: i32,
    pub chance: f64,
}

impl OreRange {
    pub const fn new(min_y: i32, max_y: i32, chance: f64) -> Self {
        Self {
            min_y,
            max_y,
            chance,
        }
    }
}

/// Minerais, dans l'ordre de tirage du générateur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OreConfig {
    pub coal: OreRange,
    pub iron: OreRange,
    pub gold: OreRange,
    pub diamond: OreRange,
    pub emerald: OreRange,
}

impl Default for OreConfig {
    fn default() -> Self {
        Self {
            coal: OreRange::new(5, 80, 0.012),
            iron: OreRange::new(5, 60, 0.008),
            gold: OreRange::new(5, 32, 0.004),
            diamond: OreRange::new(5, 16, 0.002),
            emerald: OreRange::new(5, 30, 0.001),
        }
    }
}

/// Configuration complète d'un monde (spec §5).
///
/// Désérialisation tolérante : tout champ absent garde sa valeur par défaut.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorldConfig {
    /// Algorithme de génération (voir [`GeneratorKind`]).
    #[serde(default = "classic_generator")]
    pub generator: GeneratorKind,
    // Terrain
    pub sea_level: i32,
    pub base_height: i32,
    pub height_scale: i32,
    pub height_noise_octaves: i32,
    pub height_noise_lacunarity: f64,
    pub height_noise_gain: f64,
    pub height_noise_frequency: f64,
    // Océan
    pub ocean_frequency: f64,
    pub ocean_threshold: f64,
    pub coast_threshold: f64,
    pub ocean_fbm_octaves: i32,
    pub ocean_lacunarity: f64,
    pub ocean_gain: f64,
    // Rivières
    pub river_frequency: f64,
    pub river_width: f64,
    pub river_depth: i32,
    pub river_bank_width_mult: f64,
    // Grottes
    pub caves_enabled: bool,
    pub cave_threshold: f64,
    pub cavern_threshold: f64,
    pub cave_depth_max: i32,
    // Biomes
    pub temp_scale: f64,
    pub humid_scale: f64,
    pub mountain_frequency: f64,
    pub mountain_threshold: f64,
    // Features
    pub snow_line_mountains: i32,
    pub snow_line_taiga: i32,
    pub vegetation_density: f64,
    /// Densité des structures (villages, tours, ruines…) du générateur « Norrblock » ; 0 = aucune.
    pub structure_density: f64,
    /// Indexé par ordinal de [`crate::Biome`] ; `< 0` = densité par défaut du biome.
    /// À la lecture, un tableau plus court est complété par `-1.0` et un plus long tronqué
    /// (écart volontaire : le Java plantait la génération).
    #[serde(deserialize_with = "deserialize_tree_overrides")]
    pub tree_density_overrides: [f64; CLASSIC_BIOME_COUNT],
    // Gameplay
    pub gravity: f32,
    pub jump_force: f32,
    pub fall_damage_enabled: bool,
    pub day_length_seconds: f32,
    pub time_of_day: f32,
    /// Non persisté (comme le Java) : revient au défaut au rechargement.
    #[serde(skip)]
    pub walk_speed: f32,
    /// Non persisté (comme le Java).
    #[serde(skip)]
    pub sprint_speed: f32,
    /// Non persisté (comme le Java).
    #[serde(skip)]
    pub fly_speed: f32,
    /// Non persisté (comme le Java).
    #[serde(skip)]
    pub attack_damage: f32,
    // Mobs
    pub mob_spawn_radius: i32,
    pub mob_despawn_radius: i32,
    /// Point d'apparition (pieds). `None` = non défini (triplet NaN du Java).
    pub spawn: Option<[f64; 3]>,
    pub ores: OreConfig,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            generator: GeneratorKind::Norrblock,
            sea_level: 34,
            base_height: 30,
            height_scale: 38,
            height_noise_octaves: 5,
            height_noise_lacunarity: 2.0,
            height_noise_gain: 0.45,
            height_noise_frequency: 0.01,
            ocean_frequency: 0.0012,
            ocean_threshold: 0.38,
            coast_threshold: 0.48,
            ocean_fbm_octaves: 4,
            ocean_lacunarity: 2.0,
            ocean_gain: 0.45,
            river_frequency: 0.003,
            river_width: 0.045,
            river_depth: 8,
            river_bank_width_mult: 1.556,
            caves_enabled: true,
            cave_threshold: 0.42,
            cavern_threshold: 0.65,
            cave_depth_max: 60,
            temp_scale: 0.0025,
            humid_scale: 0.003,
            mountain_frequency: 0.006,
            mountain_threshold: 0.72,
            snow_line_mountains: 58,
            snow_line_taiga: 52,
            vegetation_density: 1.0,
            structure_density: 1.0,
            tree_density_overrides: [-1.0; CLASSIC_BIOME_COUNT],
            gravity: -32.0,
            jump_force: 8.4,
            fall_damage_enabled: true,
            day_length_seconds: 1200.0,
            time_of_day: 900.0,
            walk_speed: 4.317,
            sprint_speed: 5.612,
            fly_speed: 20.0,
            attack_damage: 1.0,
            mob_spawn_radius: 64,
            mob_despawn_radius: 64,
            spawn: None,
            ores: OreConfig::default(),
        }
    }
}

impl WorldConfig {
    /// `WorldConfig.preset(type)` du Java : défauts puis modifications du préréglage (spec §5.2),
    /// avec le générateur **classique**. Sert aux vecteurs de référence ; les types ajoutés
    /// après le Java (Archipelago, Highlands, Classic) y valent NORMAL.
    pub fn preset_java(t: WorldType) -> Self {
        let mut c = Self {
            generator: GeneratorKind::Classic,
            ..Self::default()
        };
        match t {
            WorldType::Flat => {
                c.sea_level = 5;
                c.base_height = 5;
                c.height_scale = 1;
                c.caves_enabled = false;
                c.river_width = 0.0;
            }
            WorldType::Amplified => {
                c.height_scale = 80;
                c.base_height = 45;
                c.ocean_threshold = 0.50;
            }
            WorldType::Island => {
                c.ocean_frequency = 0.003;
                c.ocean_threshold = 0.20;
                c.coast_threshold = 0.30;
            }
            _ => {}
        }
        c
    }

    /// Préréglage d'un type de monde. Les types Java (Normal, Flat, Amplified, Island) reprennent
    /// les réglages du Java avec le générateur « Norrblock » ; AMPLIFIED et ISLAND sont
    /// réajustés pour ce générateur (le seuil d'océan du Java, > au seuil de côte, y donnait un
    /// monde noyé à côtes verticales ; ISLAND y supprimait presque tout l'océan).
    pub fn preset(t: WorldType) -> Self {
        let mut c = Self::preset_java(t);
        c.generator = GeneratorKind::Norrblock;
        match t {
            WorldType::Normal | WorldType::Flat => {}
            WorldType::Amplified => {
                c.ocean_threshold = 0.44;
                c.coast_threshold = 0.52;
            }
            WorldType::Island => {
                c.ocean_frequency = 0.0016;
                c.ocean_threshold = 0.50;
                c.coast_threshold = 0.58;
            }
            WorldType::Archipelago => {
                c.ocean_frequency = 0.0042;
                c.ocean_threshold = 0.48;
                c.coast_threshold = 0.55;
                c.temp_scale = 0.0015;
                c.river_width = 0.02;
            }
            WorldType::Highlands => {
                c.height_scale = 50;
                c.base_height = 38;
                c.mountain_threshold = 0.58;
                c.ocean_threshold = 0.30;
                c.coast_threshold = 0.38;
                c.snow_line_mountains = 74;
            }
            WorldType::Classic => c.generator = GeneratorKind::Classic,
        }
        c
    }

    /// Position de réapparition : le spawn enregistré, sinon `(0.5, seaLevel + 5, 0.5)`
    /// (`World.respawnPlayer`).
    pub fn respawn_position(&self) -> [f64; 3] {
        self.spawn
            .unwrap_or([0.5, f64::from(self.sea_level + 5), 0.5])
    }
}

/// Lit un tableau de longueur quelconque ; complète à 8 avec `-1.0` (ou tronque).
/// Un élément non numérique (`null`…) vaut `-1.0`.
fn deserialize_tree_overrides<'de, D>(d: D) -> Result<[f64; CLASSIC_BIOME_COUNT], D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Vec<serde_json::Value> = Vec::deserialize(d)?;
    let mut out = [-1.0; CLASSIC_BIOME_COUNT];
    for (slot, v) in out.iter_mut().zip(raw) {
        *slot = v.as_f64().unwrap_or(-1.0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets() {
        let f = WorldConfig::preset_java(WorldType::Flat);
        assert_eq!((f.sea_level, f.base_height, f.height_scale), (5, 5, 1));
        assert!(!f.caves_enabled);
        assert_eq!(f.river_width, 0.0);
        let a = WorldConfig::preset_java(WorldType::Amplified);
        assert_eq!((a.height_scale, a.base_height), (80, 45));
        assert_eq!((a.ocean_threshold, a.coast_threshold), (0.50, 0.48));
        assert_eq!(a.generator, GeneratorKind::Classic);
        let i = WorldConfig::preset_java(WorldType::Island);
        assert_eq!(
            (i.ocean_frequency, i.ocean_threshold, i.coast_threshold),
            (0.003, 0.20, 0.30)
        );
        assert_eq!(
            WorldConfig::preset(WorldType::Normal),
            WorldConfig::default()
        );
        assert_eq!(WorldType::Classic.next(), WorldType::Normal);
        assert_eq!(WorldType::Island.next(), WorldType::Archipelago);
        assert_eq!(
            WorldConfig::preset(WorldType::Classic).generator,
            GeneratorKind::Classic
        );
        assert_eq!(
            WorldConfig::preset(WorldType::Highlands).generator,
            GeneratorKind::Norrblock
        );
        assert_eq!(WorldType::Normal.next(), WorldType::Flat);
    }

    #[test]
    fn tolerant_json_and_override_padding() {
        let c: WorldConfig =
            serde_json::from_str(r#"{"seaLevel": 40, "treeDensityOverrides": [0.5, -1.0, 0.1]}"#)
                .unwrap();
        assert_eq!(c.sea_level, 40);
        assert_eq!(c.base_height, 30);
        assert_eq!(
            c.tree_density_overrides,
            [0.5, -1.0, 0.1, -1.0, -1.0, -1.0, -1.0, -1.0]
        );
        let long: WorldConfig =
            serde_json::from_str(r#"{"treeDensityOverrides": [1,2,3,4,5,6,7,8,9,10]}"#).unwrap();
        assert_eq!(
            long.tree_density_overrides,
            [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
        );
    }

    #[test]
    fn roundtrip_skips_non_persisted_fields() {
        let mut c = WorldConfig::preset(WorldType::Amplified);
        c.walk_speed = 99.0;
        c.spawn = Some([1.5, 40.0, -2.5]);
        c.tree_density_overrides[3] = 0.2;
        let json = serde_json::to_string(&c).unwrap();
        let back: WorldConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.walk_speed, 4.317);
        let mut expected = c.clone();
        expected.walk_speed = 4.317;
        assert_eq!(back, expected);
        assert_eq!(WorldConfig::default().respawn_position(), [0.5, 39.0, 0.5]);
        assert_eq!(back.respawn_position(), [1.5, 40.0, -2.5]);
    }
}
