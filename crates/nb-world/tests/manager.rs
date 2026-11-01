//! Gestionnaire de chunks asynchrone, monde et sauvegardes de bout en bout.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_core::glam::Vec3;
use nb_core::ChunkPos;
use nb_data::BlockRegistry;
use nb_world::save::{self, PlayerState, WorldMetadata};
use nb_world::{
    fnv1a, ChunkManager, ChunkStorage, FileChunkStorage, InMemoryChunkStorage, World, WorldConfig,
    WorldType,
};

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "délai dépassé : {what}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Appelle `update` jusqu'à ce que le carré de rayon `r - 1` soit éclairé.
fn settle(m: &ChunkManager, player: Vec3) {
    let r = m.render_distance();
    let pc = ChunkPos::new(
        (player.x / 16.0).floor() as i32,
        (player.z / 16.0).floor() as i32,
    );
    wait_until("chargement et lumière", || {
        m.update(player);
        (-(r - 1)..r).all(|dx| {
            (-(r - 1)..r).all(|dz| {
                m.chunk(ChunkPos::new(pc.x + dx, pc.z + dz))
                    .is_some_and(|c| !c.flags.is_light_dirty())
            })
        })
    });
}

fn manager(storage: Arc<dyn ChunkStorage>, seed: i64, r: i32) -> ChunkManager {
    let reg = Arc::new(BlockRegistry::builtin());
    let m = ChunkManager::with_threads(
        storage,
        seed,
        Arc::new(WorldConfig::preset(WorldType::Normal)),
        reg,
        2,
        2,
    );
    m.set_render_distance(r);
    m
}

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("nb-world-it-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn loads_square_and_lights_like_reference() {
    let m = manager(Arc::new(InMemoryChunkStorage::new()), 12345, 2);
    settle(&m, Vec3::new(8.0, 60.0, 8.0));
    let mut active = m.active_positions();
    active.sort();
    assert_eq!(active.len(), 25);
    assert_eq!(active[0], ChunkPos::new(-2, -2));
    assert_eq!(active[24], ChunkPos::new(2, 2));
    // L'anneau extérieur n'a jamais tous ses voisins : jamais éclairé (comme le Java).
    assert!(m.chunk(ChunkPos::new(2, 0)).unwrap().flags.is_light_dirty());
    // Lumière identique au vecteur Java (chunk (0,0), NORMAL, graine 12345).
    let c = m.chunk(ChunkPos::new(0, 0)).unwrap();
    assert_eq!(c.read().fingerprint(), 0x3300759565bbadfe);
    assert_eq!(fnv1a(c.read().light_map().as_slice()), 0x3f00fe567734ed11);
    m.shutdown();
}

#[test]
fn set_block_flags_and_world_coordinates() {
    let m = manager(Arc::new(InMemoryChunkStorage::new()), 1, 2);
    settle(&m, Vec3::new(0.5, 60.0, 0.5));
    let stone = m.registry().ids().stone;
    let air = m.registry().ids().air;
    // Bloc monde (-1, 120, -16) : chunk (-1, -1), local (15, 120, 0).
    let target = m.chunk(ChunkPos::new(-1, -1)).unwrap();
    let west = m.chunk(ChunkPos::new(0, -1)).unwrap();
    let south = m.chunk(ChunkPos::new(-1, -2)).unwrap();
    target.flags.set_dirty(false);
    west.flags.set_mesh_dirty(false);
    south.flags.set_mesh_dirty(false);
    assert_eq!(m.block(-1, 120, -16), air);
    m.set_block(-1, 120, -16, stone);
    assert_eq!(m.block(-1, 120, -16), stone);
    assert_eq!(target.read().block(15, 120, 0), stone);
    assert!(target.flags.is_dirty() && target.flags.is_light_dirty());
    assert!(west.flags.is_mesh_dirty(), "voisin lx == 15 → x + 1");
    assert!(south.flags.is_mesh_dirty(), "voisin lz == 0 → z - 1");
    // Hors hauteur / chunk absent : air, écriture ignorée.
    assert_eq!(m.block(0, 128, 0), air);
    assert_eq!(m.block(0, -1, 0), air);
    m.set_block(10_000, 50, 0, stone);
    assert_eq!(m.block(10_000, 50, 0), air);
    assert!(!m.is_chunk_loaded(10_000, 0));
    assert!(m.is_chunk_loaded(-1, -16));
    // La lumière est recalculée après l'édition.
    settle(&m, Vec3::new(0.5, 60.0, 0.5));
    assert_eq!(target.read().light(15, 120, 0), 0);
    assert_eq!(target.read().light(15, 121, 0), 15);
}

#[test]
fn unload_saves_and_reload_restores_edits() {
    let storage = Arc::new(InMemoryChunkStorage::new());
    let m = manager(storage.clone(), 99, 1);
    settle(&m, Vec3::new(0.5, 60.0, 0.5));
    let wood = m.registry().ids().wood;
    m.set_block(3, 100, 4, wood);
    // Le joueur s'éloigne : les 9 chunks sont déchargés et sauvegardés (tous `dirty`).
    let far = Vec3::new(16.0 * 50.0, 60.0, 0.5);
    m.update(far);
    assert!(m.chunk(ChunkPos::new(0, 0)).is_none());
    assert!(storage.len() >= 9);
    assert!(storage.has_chunk(ChunkPos::new(0, 0)));
    settle(&m, far);
    // Retour : le chunk vient du stockage et garde la modification.
    settle(&m, Vec3::new(0.5, 60.0, 0.5));
    assert_eq!(m.block(3, 100, 4), wood);
}

#[test]
fn file_storage_flush_and_new_session() {
    let dir = TempDir::new("flush");
    let wood;
    {
        let storage = Arc::new(FileChunkStorage::new(&dir.0).unwrap());
        let m = manager(storage, 5, 1);
        settle(&m, Vec3::new(0.5, 60.0, 0.5));
        wood = m.registry().ids().wood;
        m.set_block(-5, 110, 7, wood);
        m.flush_dirty().unwrap();
        assert!(m.active_chunks().iter().all(|(_, c)| !c.flags.is_dirty()));
        m.shutdown();
    }
    assert!(dir.0.join("chunks").join("c_-1_0.nbc").is_file());
    let storage = Arc::new(FileChunkStorage::new(&dir.0).unwrap());
    let m = manager(storage, 5, 1);
    settle(&m, Vec3::new(0.5, 60.0, 0.5));
    assert_eq!(m.block(-5, 110, 7), wood);
}

#[test]
fn corrupt_chunk_file_is_regenerated() {
    let dir = TempDir::new("corrupt");
    let storage = Arc::new(FileChunkStorage::new(&dir.0).unwrap());
    std::fs::write(storage.chunk_path(ChunkPos::new(0, 0)), b"pas un chunk").unwrap();
    let m = manager(storage, 12345, 1);
    settle(&m, Vec3::new(0.5, 60.0, 0.5));
    assert_eq!(
        m.chunk(ChunkPos::new(0, 0)).unwrap().read().fingerprint(),
        0x3300759565bbadfe
    );
}

#[test]
fn world_spawn_and_save_roundtrip() {
    let root = TempDir::new("world");
    let reg = Arc::new(BlockRegistry::builtin());
    let cfg = WorldConfig::preset(WorldType::Normal);

    let mut meta = WorldMetadata::new("Mon monde", 12345, "SURVIVAL");
    meta.world_config = cfg.clone();
    let dir = save::create_world_directory(&root.0, &mut meta).unwrap();
    assert!(meta.never_played());

    let world = World::new(
        meta.seed,
        Arc::new(FileChunkStorage::new(&dir).unwrap()),
        meta.world_config.clone(),
        reg.clone(),
    );
    assert_eq!(world.respawn_position(), [0.5, 39.0, 0.5]);
    assert_eq!(world.ensure_spawn(), [-31.5, 49.0, -31.5]);
    assert_eq!(world.respawn_position(), [-31.5, 49.0, -31.5]);
    world.chunk_manager().set_render_distance(1);
    settle(world.chunk_manager(), Vec3::new(-31.5, 49.0, -31.5));
    world.set_block(-31, 60, -31, reg.ids().stone);
    world.chunk_manager().flush_dirty().unwrap();

    // Sauvegarde de l'état (≙ saveWorldState).
    meta.world_config = world.config().clone();
    meta.player = Some(PlayerState {
        x: -31.5,
        y: 61.0,
        z: -31.5,
        yaw: 1.25,
        pitch: -0.5,
        health: 7.5,
        flying: true,
    });
    meta.last_played_at += 1;
    save::save_metadata(&root.0, &meta).unwrap();
    world.shutdown();
    drop(world);

    let back = save::load_metadata(&root.0, "Mon monde").unwrap();
    assert_eq!(back, meta);
    assert_eq!(back.world_config.spawn, Some([-31.5, 49.0, -31.5]));
    let text = std::fs::read_to_string(dir.join("world.json")).unwrap();
    assert!(text.contains("\"formatVersion\": 1"));

    let world = World::new(
        back.seed,
        Arc::new(FileChunkStorage::new(&dir).unwrap()),
        back.world_config.clone(),
        reg.clone(),
    );
    world.chunk_manager().set_render_distance(1);
    settle(world.chunk_manager(), Vec3::new(-31.5, 49.0, -31.5));
    assert_eq!(world.block(-31, 60, -31), reg.ids().stone);
    assert_eq!(world.ensure_spawn(), [-31.5, 49.0, -31.5]);
}

#[test]
fn shutdown_is_idempotent_and_stops_work() {
    let m = manager(Arc::new(InMemoryChunkStorage::new()), 3, 4);
    m.update(Vec3::ZERO);
    m.shutdown();
    m.shutdown();
    // Après l'arrêt, plus aucune tâche n'est acceptée.
    m.update(Vec3::new(1000.0, 0.0, 1000.0));
    std::thread::sleep(Duration::from_millis(50));
    assert!(m.chunk(ChunkPos::new(62, 62)).is_none());
}
