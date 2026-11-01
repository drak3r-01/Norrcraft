//! Métadonnées de pack de ressources (`pack.mcmeta`).

use std::path::Path;

use nb_core::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackInfo {
    #[serde(default)]
    pub pack_format: i32,
    #[serde(default)]
    pub description: String,
}

/// Contenu de `pack.mcmeta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackMeta {
    pub pack: PackInfo,
}

impl PackMeta {
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| Error::data("pack.mcmeta", e))
    }

    pub fn load(pack_dir: &Path) -> Result<Self> {
        let path = pack_dir.join("pack.mcmeta");
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        Self::from_json(&text)
    }

    /// Dossier des textures de blocs d'un pack : `assets/minecraft/textures/block`.
    pub fn block_texture_dir(pack_dir: &Path) -> std::path::PathBuf {
        pack_dir
            .join("assets")
            .join("minecraft")
            .join("textures")
            .join("block")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_pack() {
        let meta = PackMeta::from_json(
            r#"{ "pack": { "pack_format": 15, "description": "Norrblock default textures" } }"#,
        )
        .unwrap();
        assert_eq!(meta.pack.pack_format, 15);
        assert_eq!(meta.pack.description, "Norrblock default textures");
    }
}
