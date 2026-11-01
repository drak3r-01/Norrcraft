//! Packs de ressources (spec §5.2) : racine des packs, liste, `pack.mcmeta` (parseur minimal
//! reproduit tel quel, pas `serde_json`) et résolution du dossier de textures de blocs.

use std::path::{Path, PathBuf};

/// Nom du pack intégré.
pub const DEFAULT_PACK: &str = "Default";
/// Description du pack intégré.
pub const DEFAULT_DESCRIPTION: &str = "Built-in textures";

/// Métadonnées affichées d'un pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourcePackMeta {
    /// Toujours le nom du dossier (jamais lu du JSON).
    pub name: String,
    pub description: String,
}

impl ResourcePackMeta {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_owned(),
            description: description.to_owned(),
        }
    }

    /// Méta par défaut `("Default", "Built-in textures")`.
    pub fn builtin() -> Self {
        Self::new(DEFAULT_PACK, DEFAULT_DESCRIPTION)
    }

    /// `ResourcePackMeta.load(dossier)` : `None` si `pack.mcmeta` est absent ;
    /// `(nomDossier, "")` sur erreur d'E/S.
    pub fn load(dir: &Path) -> Option<Self> {
        let file = dir.join("pack.mcmeta");
        if !file.is_file() {
            return None;
        }
        let name = folder_name(dir);
        Some(match std::fs::read_to_string(&file) {
            Ok(text) => Self {
                name,
                description: parse_description(&text),
            },
            Err(_) => Self {
                name,
                description: String::new(),
            },
        })
    }
}

fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Parseur minimal de la description : première occurrence de `"description"`, `:`, espaces,
/// puis une chaîne entre guillemets où `\x` devient `x`. Toute autre forme → `""`.
pub fn parse_description(text: &str) -> String {
    let Some(pos) = text.find("\"description\"") else {
        return String::new();
    };
    let rest = text[pos + "\"description\"".len()..].trim_start();
    let Some(rest) = rest.strip_prefix(':') else {
        return String::new();
    };
    let Some(rest) = rest.trim_start().strip_prefix('"') else {
        return String::new();
    };
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// `packsRoot()` : premier dossier existant parmi `./resource_packs`, `../resource_packs`,
/// `<dossier_du_binaire>/../resource_packs` ; sinon `resource_packs` (relatif).
pub fn find_packs_root() -> PathBuf {
    let mut candidates = vec![
        PathBuf::from("resource_packs"),
        PathBuf::from("../resource_packs"),
    ];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join("../resource_packs"));
    }
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("resource_packs"))
}

/// `listResourcePacks()` : « Default » d'abord, puis les autres triés sans casse.
pub fn list_resource_packs(root: &Path) -> Vec<ResourcePackMeta> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    if dirs.is_empty() {
        return vec![ResourcePackMeta::builtin()];
    }
    dirs.sort_by_key(|p| {
        let n = folder_name(p);
        (n != DEFAULT_PACK, n.to_lowercase())
    });
    dirs.iter()
        .map(|d| {
            ResourcePackMeta::load(d).unwrap_or_else(|| ResourcePackMeta::new(&folder_name(d), ""))
        })
        .collect()
}

/// Résolution du dossier des textures de blocs d'un pack (`reloadBlockTextures`, §5.2) :
/// `(dossier, méta)`. Nom vide → « Default ». Repli sur le Default (méta intégrée).
pub fn resolve_block_textures(root: &Path, name: &str) -> (PathBuf, ResourcePackMeta) {
    let name = if name.is_empty() { DEFAULT_PACK } else { name };
    let pack = root.join(name);
    for sub in [
        Path::new("assets/minecraft/textures/block"),
        Path::new("textures/block"),
    ] {
        let dir = pack.join(sub);
        if dir.is_dir() {
            let meta =
                ResourcePackMeta::load(&pack).unwrap_or_else(|| ResourcePackMeta::new(name, ""));
            return (dir, meta);
        }
    }
    (default_block_dir(root), ResourcePackMeta::builtin())
}

/// Dossier des textures de blocs du pack Default.
pub fn default_block_dir(root: &Path) -> PathBuf {
    root.join(DEFAULT_PACK)
        .join("assets/minecraft/textures/block")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_parser_quirks() {
        assert_eq!(
            parse_description(r#"{"pack":{"description": "a\"b\nc"}}"#),
            "a\"bnc"
        );
        assert_eq!(
            parse_description(r#"{"pack":{"description":{"text":"x"}}}"#),
            ""
        );
        assert_eq!(
            parse_description(r#"{"pack":{"description" :  "ok"}}"#),
            "ok"
        );
        assert_eq!(parse_description("{}"), "");
        assert_eq!(parse_description("{\"description\": \"\\u00e9\"}"), "u00e9");
    }

    #[test]
    fn repo_packs() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resource_packs");
        let list = list_resource_packs(&root);
        assert_eq!(list[0].name, "Default");
        let (dir, meta) = resolve_block_textures(&root, "");
        assert!(dir.ends_with("block") && dir.is_dir());
        assert_eq!(meta.name, "Default");
        let (dir2, meta2) = resolve_block_textures(&root, "Inexistant");
        assert_eq!(dir2, default_block_dir(&root));
        assert_eq!(meta2, ResourcePackMeta::builtin());
        assert_eq!(
            list_resource_packs(Path::new("/nope")),
            vec![ResourcePackMeta::builtin()]
        );
    }
}
