//! Vérifie que les assets copiés du Java sont cohérents avec les registres.

use std::path::{Path, PathBuf};

use nb_data::{BlockRegistry, PackMeta, ParticleRegistry};

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
