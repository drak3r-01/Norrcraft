//! Sauvegardes (`WorldSaveManager.java`, `WorldMetadata.java`) au nouveau format versionné.
//!
//! ```text
//! <racine>/<nom-assaini>/world.json   (serde_json, "formatVersion": 1)
//! <racine>/<nom-assaini>/chunks/c_<cx>_<cz>.nbc
//! ```
//!
//! Aucune compatibilité avec l'ancien format Java (`world.json` sans version, `.bin` bruts) :
//! un `world.json` sans `formatVersion` est refusé (`Error::SaveVersion { found: 0, .. }`).
//! L'état du joueur est fourni par `nb-game` ([`PlayerState`]) ; `player: None` remplace la
//! sentinelle Java `playerY == 100.0` (« jamais joué » → recherche du spawn).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nb_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::config::WorldConfig;
use crate::storage::write_atomic;

/// Version courante de `world.json`.
pub const WORLD_FORMAT_VERSION: u32 = 1;
/// Nom du fichier de métadonnées.
pub const WORLD_FILE: &str = "world.json";
/// Sous-répertoire des chunks.
pub const CHUNKS_DIR: &str = "chunks";

/// Horodatage courant en millisecondes depuis l'époque Unix.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// État du joueur sauvegardé (pieds, angles en radians comme le Java).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayerState {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub flying: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 100.0,
            z: 0.5,
            yaw: 0.0,
            pitch: 0.0,
            health: 10.0,
            flying: false,
        }
    }
}

/// Case d'inventaire non vide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventorySlot {
    pub slot: i32,
    pub block_name: String,
    pub count: i32,
}

/// État de survie sauvegardé (absent des anciens mondes : valeurs par défaut au chargement).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SurvivalSave {
    pub hunger: f32,
    pub saturation: f32,
    pub warmth: f32,
    pub stamina: f32,
    /// Usure des outils et armures de l'inventaire : `(case, usure)`.
    pub wear: Vec<(i32, u16)>,
    /// Armure portée : `(emplacement 0..4, objet, usure)`.
    pub armor: Vec<(u8, String, u16)>,
}

impl Default for SurvivalSave {
    fn default() -> Self {
        Self {
            hunger: 20.0,
            saturation: 5.0,
            warmth: 100.0,
            stamina: 100.0,
            wear: Vec::new(),
            armor: Vec::new(),
        }
    }
}

fn default_game_mode() -> String {
    "CREATIVE".to_owned()
}

/// Contenu de `world.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldMetadata {
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub seed: i64,
    #[serde(default = "default_game_mode")]
    pub game_mode: String,
    #[serde(default)]
    pub allow_cheats: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub last_played_at: i64,
    /// `None` = jamais joué (le spawn sera recherché puis écrit dans `world_config.spawn`).
    #[serde(default)]
    pub player: Option<PlayerState>,
    #[serde(default)]
    pub inventory_slots: Vec<InventorySlot>,
    #[serde(default)]
    pub world_config: WorldConfig,
    /// Faim, chaleur, endurance, usure et armure (mode survie).
    #[serde(default)]
    pub survival: Option<SurvivalSave>,
}

impl WorldMetadata {
    /// `new WorldMetadata(name, seed, gameMode)` : `createdAt = lastPlayedAt = maintenant`.
    pub fn new(name: impl Into<String>, seed: i64, game_mode: impl Into<String>) -> Self {
        let now = now_millis();
        Self {
            format_version: WORLD_FORMAT_VERSION,
            name: name.into(),
            seed,
            game_mode: game_mode.into(),
            allow_cheats: false,
            created_at: now,
            last_played_at: now,
            player: None,
            inventory_slots: Vec::new(),
            world_config: WorldConfig::default(),
            survival: None,
        }
    }

    /// Le monde n'a jamais été joué (équivalent de `playerY == 100.0`).
    pub fn never_played(&self) -> bool {
        self.player.is_none()
    }
}

/// `name.replaceAll("[^a-zA-Z0-9_\\- ]", "_").trim()`.
pub fn sanitize(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    replaced.trim_matches(|c: char| c <= ' ').to_owned()
}

/// Nom de répertoire d'un monde ; un nom assaini vide donne `world` (écart : le Java écrivait
/// alors directement dans `saves/`).
fn dir_name(name: &str) -> String {
    let s = sanitize(name);
    if s.is_empty() {
        "world".to_owned()
    } else {
        s
    }
}

/// `<racine>/<nom-assaini>`.
pub fn world_dir(root: &Path, name: &str) -> PathBuf {
    root.join(dir_name(name))
}

/// Mondes de `root` (sous-répertoires contenant `world.json`), du plus récemment joué au plus
/// ancien. Les fichiers illisibles ou d'une autre version sont ignorés.
pub fn list_worlds(root: &Path) -> Vec<WorldMetadata> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join(WORLD_FILE).is_file() {
            continue;
        }
        let dir = entry.file_name().to_string_lossy().into_owned();
        match load_metadata(root, &dir) {
            Ok(m) => out.push(m),
            Err(e) => eprintln!("métadonnées illisibles dans {dir} : {e}"),
        }
    }
    out.sort_by_key(|m| std::cmp::Reverse(m.last_played_at));
    out
}

/// Crée le répertoire d'un nouveau monde ; en cas de collision, prend le premier `base_n`
/// libre (n = 2, 3, …) et renomme `meta.name` en conséquence. Écrit `world.json`.
pub fn create_world_directory(root: &Path, meta: &mut WorldMetadata) -> Result<PathBuf> {
    let base = dir_name(&meta.name);
    let mut dir = root.join(&base);
    if dir.exists() {
        let mut n = 2u32;
        while root.join(format!("{base}_{n}")).exists() {
            n += 1;
        }
        let name = format!("{base}_{n}");
        dir = root.join(&name);
        meta.name = name;
    }
    let chunks = dir.join(CHUNKS_DIR);
    fs::create_dir_all(&chunks).map_err(|e| Error::io(&chunks, e))?;
    save_metadata(root, meta)?;
    Ok(dir)
}

/// Écrit `world.json` (crée le répertoire si besoin), atomiquement.
pub fn save_metadata(root: &Path, meta: &WorldMetadata) -> Result<()> {
    let dir = world_dir(root, &meta.name);
    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let mut m = meta.clone();
    m.format_version = WORLD_FORMAT_VERSION;
    let json = serde_json::to_string_pretty(&m).map_err(|e| Error::data(WORLD_FILE, e))?;
    write_atomic(&dir.join(WORLD_FILE), json.as_bytes())
}

/// Analyse le contenu d'un `world.json` (vérifie `formatVersion`).
pub fn parse_metadata(text: &str) -> Result<WorldMetadata> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| Error::data(WORLD_FILE, e))?;
    let found = value
        .get("formatVersion")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    if found != WORLD_FORMAT_VERSION {
        return Err(Error::SaveVersion {
            found,
            expected: WORLD_FORMAT_VERSION,
        });
    }
    serde_json::from_value(value).map_err(|e| Error::data(WORLD_FILE, e))
}

/// Lit `<racine>/<dir_name>/world.json`.
pub fn load_metadata(root: &Path, dir_name: &str) -> Result<WorldMetadata> {
    let path = root.join(dir_name).join(WORLD_FILE);
    let text = fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
    parse_metadata(&text)
}

pub fn world_name_exists(root: &Path, name: &str) -> bool {
    world_dir(root, name).exists()
}

/// Suppression récursive (échecs ignorés).
pub fn delete_world(root: &Path, name: &str) {
    let _ = fs::remove_dir_all(world_dir(root, name));
}

/// Renomme le répertoire (échec ignoré), puis `meta.name = new_name` et réécrit `world.json`.
pub fn rename_world(root: &Path, meta: &mut WorldMetadata, new_name: &str) -> Result<()> {
    let _ = fs::rename(world_dir(root, &meta.name), world_dir(root, new_name));
    meta.name = new_name.to_owned();
    save_metadata(root, meta)
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::io(dst, e))?;
    let entries = fs::read_dir(src).map_err(|e| Error::io(src, e))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| Error::io(&from, e))?;
        }
    }
    Ok(())
}

/// Clone un monde : même graine, mode et configuration ; **ni** `allow_cheats` **ni** l'état du
/// joueur (le clone repart du spawn), comme le Java. Copie les chunks.
pub fn clone_world(root: &Path, src: &WorldMetadata, new_name: &str) -> Result<WorldMetadata> {
    let mut meta = WorldMetadata::new(new_name, src.seed, src.game_mode.clone());
    meta.world_config = src.world_config.clone();
    let dir = create_world_directory(root, &mut meta)?;
    let src_chunks = world_dir(root, &src.name).join(CHUNKS_DIR);
    if src_chunks.is_dir() {
        copy_dir(&src_chunks, &dir.join(CHUNKS_DIR))?;
    }
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests::TempDir;

    #[test]
    fn sanitize_like_java() {
        assert_eq!(sanitize("Mon Monde!"), "Mon Monde_");
        assert_eq!(sanitize("  a/b\\c  "), "a_b_c");
        assert_eq!(sanitize("été"), "_t_");
        assert_eq!(sanitize("ok-name_1"), "ok-name_1");
        assert_eq!(sanitize("   "), "");
    }

    #[test]
    fn create_list_rename_clone_delete() {
        let root = TempDir::new("save");
        let r = &root.0;
        let mut a = WorldMetadata::new("Alpha", 42, "SURVIVAL");
        a.last_played_at = 10;
        a.allow_cheats = true;
        a.player = Some(PlayerState::default());
        let dir = create_world_directory(r, &mut a).unwrap();
        assert_eq!(dir, r.join("Alpha"));
        assert!(dir.join("chunks").is_dir());
        fs::write(dir.join("chunks").join("c_0_0.nbc"), b"x").unwrap();

        let mut dup = WorldMetadata::new("Alpha", 1, "CREATIVE");
        dup.last_played_at = 20;
        create_world_directory(r, &mut dup).unwrap();
        assert_eq!(dup.name, "Alpha_2");
        assert!(world_name_exists(r, "Alpha_2"));

        let list = list_worlds(r);
        assert_eq!(
            list.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            ["Alpha_2", "Alpha"]
        );
        assert_eq!(list[1], a);

        let c = clone_world(r, &a, "Beta").unwrap();
        assert_eq!((c.seed, c.game_mode.as_str()), (42, "SURVIVAL"));
        assert!(!c.allow_cheats && c.player.is_none());
        assert!(r.join("Beta").join("chunks").join("c_0_0.nbc").is_file());

        rename_world(r, &mut dup, "Gamma").unwrap();
        assert!(!r.join("Alpha_2").exists());
        assert_eq!(load_metadata(r, "Gamma").unwrap().name, "Gamma");

        delete_world(r, "Beta");
        assert!(!world_name_exists(r, "Beta"));
    }

    #[test]
    fn empty_name_goes_to_world_dir() {
        let root = TempDir::new("save-empty");
        let mut m = WorldMetadata::new("???", 0, "CREATIVE");
        // "???" → "___" (non vide) ; un nom d'espaces donne "world".
        create_world_directory(&root.0, &mut m).unwrap();
        assert!(root.0.join("___").join(WORLD_FILE).is_file());
        let mut e = WorldMetadata::new("  ", 0, "CREATIVE");
        let dir = create_world_directory(&root.0, &mut e).unwrap();
        assert_eq!(dir, root.0.join("world"));
        assert!(dir.join(WORLD_FILE).is_file());
        assert!(!root.0.join(WORLD_FILE).exists());
    }

    #[test]
    fn version_is_required_and_fields_default() {
        assert!(matches!(
            parse_metadata(r#"{"name":"old","seed":1,"playerY":100.0}"#),
            Err(Error::SaveVersion { found: 0, .. })
        ));
        assert!(matches!(
            parse_metadata(r#"{"formatVersion":2,"name":"x"}"#),
            Err(Error::SaveVersion { found: 2, .. })
        ));
        let m = parse_metadata(
            r#"{"formatVersion":1,"name":"x","seed":-987654321012345,
                "worldConfig":{"seaLevel":40,"treeDensityOverrides":[0.3]}}"#,
        )
        .unwrap();
        assert_eq!(m.seed, -987654321012345);
        assert_eq!(m.game_mode, "CREATIVE");
        assert!(m.never_played());
        assert_eq!(m.world_config.sea_level, 40);
        assert_eq!(m.world_config.tree_density_overrides[0], 0.3);
        assert_eq!(m.world_config.tree_density_overrides[7], -1.0);
    }
}
