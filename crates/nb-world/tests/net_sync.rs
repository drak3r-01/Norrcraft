//! Primitives de synchronisation réseau du gestionnaire de chunks : journal d'éditions,
//! chunk « modifié », installation d'un chunk reçu, écriture silencieuse.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_core::glam::Vec3;
use nb_core::ChunkPos;
use nb_data::BlockRegistry;
use nb_world::{BlockEdit, ChunkManager, ChunkStorage, InMemoryChunkStorage, WorldConfig};

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < Duration::from_secs(60), "délai : {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn manager(seed: i64) -> (ChunkManager, Arc<InMemoryChunkStorage>) {
    let storage = Arc::new(InMemoryChunkStorage::new());
    let reg = Arc::new(BlockRegistry::builtin());
    let m = ChunkManager::with_threads(
        storage.clone() as Arc<dyn ChunkStorage>,
        seed,
        Arc::new(WorldConfig::default()),
        reg,
        2,
        2,
    );
    m.set_render_distance(2);
    (m, storage)
}

fn load(m: &ChunkManager) {
    wait_until("chargement", || {
        m.update(Vec3::new(8.0, 80.0, 8.0));
        m.chunk(ChunkPos::new(0, 0)).is_some()
    });
}

#[test]
fn edit_log_records_only_when_enabled_and_quiet_writes_are_skipped() {
    let (m, _) = manager(7);
    load(&m);
    m.set_block(1, 70, 1, 3);
    assert!(m.take_edits().is_empty(), "journal désactivé");

    m.enable_edit_log(true);
    m.set_block(1, 70, 1, 4);
    assert!(m.set_block_quiet(2, 70, 1, 5));
    m.set_block(100_000, 70, 1, 6); // chunk absent : ignoré, non consigné
    assert_eq!(
        m.take_edits(),
        vec![BlockEdit {
            x: 1,
            y: 70,
            z: 1,
            id: 4
        }]
    );
    assert!(m.take_edits().is_empty(), "journal vidé par take_edits");
    assert_eq!(
        m.block(2, 70, 1),
        5,
        "l'écriture silencieuse a bien eu lieu"
    );
    m.enable_edit_log(false);
    m.set_block(1, 70, 1, 9);
    assert!(m.take_edits().is_empty());
}

#[test]
fn chunks_are_exported_from_active_table() {
    let (m, _) = manager(11);
    load(&m);
    let pos = ChunkPos::new(0, 0);
    let v = m.voxels_of(pos).expect("chunk actif lisible");
    m.set_block(3, 70, 3, 7);
    let v2 = m.voxels_of(pos).unwrap();
    assert_eq!(v2[3 + 16 * 70 + 2048 * 3], 7);
    assert_ne!(v[3 + 16 * 70 + 2048 * 3], 7);
    assert!(m.voxels_of(ChunkPos::new(50, 50)).is_none(), "jamais écrit");
}

#[test]
fn extra_centers_keep_remote_players_chunks_loaded() {
    let (m, _) = manager(21);
    load(&m);
    let far = Vec3::new(40.0 * 16.0 + 8.0, 80.0, 8.0);
    wait_until("chunks du joueur distant", || {
        m.update_with_extras(Vec3::new(8.0, 80.0, 8.0), &[(far, 1)]);
        m.chunk(ChunkPos::new(40, 0)).is_some() && m.chunk(ChunkPos::new(0, 0)).is_some()
    });
    // Le joueur distant part : ses chunks sont déchargés.
    wait_until("déchargement", || {
        m.update(Vec3::new(8.0, 80.0, 8.0));
        m.chunk(ChunkPos::new(40, 0)).is_none()
    });
}

#[test]
fn remote_mode_loads_only_received_chunks_and_never_generates() {
    let (m, storage) = manager(31);
    m.set_remote(true);
    for _ in 0..50 {
        m.update(Vec3::new(8.0, 80.0, 8.0));
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        m.active_positions().is_empty(),
        "rien n'est généré en mode client"
    );

    let mut voxels = vec![0u8; nb_core::math::CHUNK_VOLUME];
    voxels[5 + 16 * 66 + 2048 * 5] = 9;
    let voxels: Box<[u8; nb_core::math::CHUNK_VOLUME]> =
        voxels.into_boxed_slice().try_into().unwrap();
    m.install_chunk(ChunkPos::new(0, 0), &voxels);
    assert!(storage.has_chunk(ChunkPos::new(0, 0)));
    wait_until("chargement du chunk reçu", || {
        m.update(Vec3::new(8.0, 80.0, 8.0));
        m.chunk(ChunkPos::new(0, 0)).is_some()
    });
    assert_eq!(m.block(5, 66, 5), 9);
    // Identique : pas de remaillage ; différent : remplacé.
    let c = m.chunk(ChunkPos::new(0, 0)).unwrap();
    c.flags.set_mesh_dirty(false);
    m.install_chunk(ChunkPos::new(0, 0), &voxels);
    assert!(!c.flags.is_mesh_dirty());
    let mut other = voxels.clone();
    other[0] = 3;
    m.install_chunk(ChunkPos::new(0, 0), &other);
    assert!(c.flags.is_mesh_dirty());
    // Déchargé : rien n'est écrit en retour.
    m.unload_chunk(ChunkPos::new(0, 0));
    assert_eq!(
        storage.load_chunk(ChunkPos::new(0, 0)).unwrap().unwrap()[0],
        0,
        "le stockage garde la version installée, pas la copie locale"
    );
}

#[test]
fn installed_chunk_replaces_active_and_feeds_storage_for_unloaded() {
    let (m, storage) = manager(13);
    load(&m);
    let mut voxels = m.voxels_of(ChunkPos::new(0, 0)).unwrap();
    voxels[5 + 16 * 66 + 2048 * 5] = 9;
    m.install_chunk(ChunkPos::new(0, 0), &voxels);
    assert_eq!(m.block(5, 66, 5), 9);

    // Chunk hors zone : écrit dans le stockage, relu au chargement.
    let far = ChunkPos::new(30, 30);
    assert!(m.chunk(far).is_none());
    m.install_chunk(far, &voxels);
    assert!(storage.has_chunk(far));
    wait_until("chargement lointain", || {
        m.update(Vec3::new(30.0 * 16.0 + 8.0, 80.0, 30.0 * 16.0 + 8.0));
        m.chunk(far).is_some()
    });
    assert_eq!(m.block(30 * 16 + 5, 66, 30 * 16 + 5), 9);
}
