//! Vérifie que les assets copiés du Java sont cohérents avec les registres.

use std::path::{Path, PathBuf};

use nb_data::{BlockRegistry, ItemRegistry, PackMeta, ParticleRegistry};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_block_texture_exists_in_default_pack() {
    let pack = repo_root().join("resource_packs/Default");
    let dir = PackMeta::block_texture_dir(&pack);
    let registry = BlockRegistry::builtin();
    for block in registry.iter().filter(|b| b.id != 0) {
        for name in block.texture_names.iter().filter(|n| !n.is_empty()) {
            let file = dir.join(format!("{name}.png"));
            assert!(
                file.is_file(),
                "texture manquante pour {}: {}",
                block.name,
                file.display()
            );
        }
    }
}

/// Blocs du craft (`blocks_craft.json`) et icônes des objets purs : tous présents dans le pack.
#[test]
fn every_craft_block_and_item_texture_exists_in_default_pack() {
    let pack = repo_root().join("resource_packs/Default");
    let dir = PackMeta::block_texture_dir(&pack);
    let blocks = BlockRegistry::builtin_with_craft();
    let mut names: Vec<(String, String)> = Vec::new();
    for b in blocks.iter().filter(|b| b.id != 0) {
        for n in b.texture_names.iter().filter(|n| !n.is_empty()) {
            names.push((b.name.clone(), n.clone()));
        }
    }
    for i in ItemRegistry::builtin(&blocks)
        .iter()
        .filter(|i| i.block.is_none())
    {
        names.push((i.name.clone(), i.texture.clone()));
    }
    for (owner, tex) in names {
        let file = dir.join(format!("{tex}.png"));
        assert!(
            file.is_file(),
            "texture manquante pour {owner}: {}",
            file.display()
        );
    }
}

/// Les fonds d'écran de l'établi et de la fonderie ont le même format que l'inventaire.
#[test]
fn station_backgrounds_match_inventory_size() {
    let hud = repo_root().join("assets/textures/hud");
    let size = |n: &str| image_size(&hud.join(n));
    assert_eq!(size("workbench.png"), size("inventory.png"));
    assert_eq!(size("forge.png"), size("inventory.png"));
}

/// Dimensions d'un PNG (en-tête IHDR, sans dépendance d'image).
fn image_size(path: &Path) -> (u32, u32) {
    let b = std::fs::read(path).unwrap();
    let be = |o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap());
    (be(16), be(20))
}

#[test]
fn default_pack_meta_loads() {
    let meta = PackMeta::load(&repo_root().join("resource_packs/Default")).unwrap();
    assert_eq!(meta.pack.pack_format, 15);
}

#[test]
fn every_particle_texture_exists() {
    let dir = repo_root().join("assets/textures/particle");
    for p in ParticleRegistry::builtin().iter() {
        let file = dir.join(format!("{}.png", p.texture));
        assert!(
            file.is_file(),
            "texture de particule manquante: {}",
            file.display()
        );
    }
}
