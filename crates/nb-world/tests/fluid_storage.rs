//! Couche fluide : format de chunk version 2, assainissement, gestionnaire (doc 07 §2, §11).

use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_core::glam::{IVec3, Vec3};
use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::fluid::{Dir, FluidBlocks, FluidCode};
use nb_world::storage::{
    decode_chunk, decode_chunk_blob, encode_chunk, encode_chunk_fluid, ChunkBlob,
    CHUNK_FORMAT_VERSION, CHUNK_FORMAT_VERSION_FLUID,
};
use nb_world::{
    Chunk, ChunkManager, ChunkStorage, EmptyTerrain, FileChunkStorage, InMemoryChunkStorage,
    CHUNK_VOLUME,
};

fn voxels() -> Box<[u8; CHUNK_VOLUME]> {
    let mut v = Box::new([0u8; CHUNK_VOLUME]);
    for (i, b) in v.iter_mut().enumerate() {
        *b = (i % 7) as u8;
    }
    v
}

fn layer() -> Box<[u8; CHUNK_VOLUME]> {
    let mut l = Box::new([0u8; CHUNK_VOLUME]);
    l[5] = FluidCode::explicit(3, Dir::NegX, false).0;
    l[900] = FluidCode::explicit(4, Dir::Down, true).0;
    l
}

fn version(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[4], bytes[5]])
}

#[test]
fn version_2_roundtrip() {
    let (v, l) = (voxels(), layer());
    let bytes = encode_chunk_fluid(&v, Some(&l));
    assert_eq!(version(&bytes), CHUNK_FORMAT_VERSION_FLUID);
    let blob = decode_chunk_blob(&bytes).unwrap();
    assert_eq!(blob.voxels, v);
    assert_eq!(blob.fluid.as_deref(), Some(&*l));
    // La lecture des seuls voxels accepte aussi la version 2.
    assert_eq!(decode_chunk(&bytes).unwrap(), v);
}

#[test]
fn version_1_files_still_load_and_stay_version_1() {
    let v = voxels();
    let old = encode_chunk(&v);
    assert_eq!(version(&old), CHUNK_FORMAT_VERSION);
    let blob = decode_chunk_blob(&old).unwrap();
    assert_eq!(blob.voxels, v);
    assert!(blob.fluid.is_none());
    // Sans couche, ou avec une couche nulle : octet pour octet la version 1.
    assert_eq!(encode_chunk_fluid(&v, None), old);
    let zero = Box::new([0u8; CHUNK_VOLUME]);
    assert_eq!(encode_chunk_fluid(&v, Some(&zero)), old);
}

#[test]
fn truncated_fluid_layers_are_rejected() {
    let bytes = encode_chunk_fluid(&voxels(), Some(&layer()));
    assert!(decode_chunk_blob(&bytes[..bytes.len() - 3]).is_err());
}

#[test]
fn storages_round_trip_the_layer() {
    let (v, l) = (voxels(), layer());
    let p = ChunkPos::new(-2, 9);
    let mem = InMemoryChunkStorage::new();
    mem.save_chunk_blob(p, &v, Some(&l)).unwrap();
    let blob = mem.load_chunk_blob(p).unwrap().unwrap();
    assert_eq!(blob.fluid.as_deref(), Some(&*l));
    // `load_chunk` ne rend que les voxels.
    assert_eq!(mem.load_chunk(p).unwrap().unwrap(), v);

    let dir = std::env::temp_dir().join(format!("nb-fluid-storage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = FileChunkStorage::new(&dir).unwrap();
    file.save_chunk_blob(p, &v, Some(&l)).unwrap();
    let blob = file.load_chunk_blob(p).unwrap().unwrap();
    assert_eq!(
        blob,
        ChunkBlob {
            voxels: v.clone(),
            fluid: Some(l)
        }
    );
    // Réécrit sans couche : version 1.
    file.save_chunk_blob(p, &v, None).unwrap();
    let bytes = std::fs::read(file.chunk_path(p)).unwrap();
    assert_eq!(version(&bytes), CHUNK_FORMAT_VERSION);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replacing_voxels_sanitizes_the_layer() {
    let registry = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let blocks = FluidBlocks::from_registry(&registry);
    let keep = blocks.keep_table();
    let granite = registry.get("granit").id;
    let mut c = Chunk::new(ChunkPos::new(0, 0));
    c.set_block(1, 1, 1, blocks.water);
    c.set_fluid_code(1, 1, 1, FluidCode::explicit(3, Dir::None, false));
    c.set_block(2, 1, 1, blocks.water);
    c.set_fluid_code(2, 1, 1, FluidCode::explicit(5, Dir::None, false));
    assert!(c.fluid_layer().is_some());
    // (1, 1, 1) devient de la roche ; (2, 1, 1) reste de l'eau.
    let mut next = Box::new(*c.voxels());
    next[1 + 16 + 2048] = granite;
    c.replace_voxels_keep(&next, &keep);
    assert!(c.fluid_code(1, 1, 1).is_default());
    assert_eq!(c.fluid_code(2, 1, 1).level(8), 5);
    // Plus rien à porter : la couche est libérée.
    let mut stone_only = Box::new(*c.voxels());
    stone_only[2 + 16 + 2048] = granite;
    c.replace_voxels_keep(&stone_only, &keep);
    assert!(c.fluid_layer().is_none());
    // Écrire le défaut sans couche n'en alloue pas.
    c.set_fluid_code(3, 3, 3, FluidCode::DEFAULT);
    assert!(c.fluid_layer().is_none());
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < Duration::from_secs(60), "délai : {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn manager_events_persistence_and_clearing() {
    let registry = Arc::new(BlockRegistry::for_kind(WorldKind::Norrcraft));
    let storage = Arc::new(InMemoryChunkStorage::new());
    let m = ChunkManager::with_terrain(
        storage.clone() as Arc<dyn ChunkStorage>,
        Arc::new(EmptyTerrain),
        registry.clone(),
        1,
        1,
    );
    m.set_render_distance(1);
    m.enable_fluids();
    let blocks = m.fluid_blocks().unwrap().clone();
    let p0 = ChunkPos::new(0, 0);
    wait_until("chargement", || {
        m.update(Vec3::new(8.0, 5.0, 8.0));
        m.chunk(p0).is_some()
    });
    let ev = m.take_fluid_events();
    assert!(ev.loaded.contains(&p0), "chunk chargé non signalé");

    // Écriture de la simulation : voxel et octet, sans événement d'édition.
    let code = FluidCode::explicit(3, Dir::PosX, false);
    assert!(m.write_fluid(3, 4, 5, blocks.water, code));
    assert_eq!(m.fluid_cell(3, 4, 5), Some((blocks.water, code)));
    assert_eq!(m.water_level(3, 4, 5), 3);
    assert!(m.take_fluid_events().edits.is_empty());
    assert!(!m.write_fluid(10_000, 4, 5, blocks.water, code));

    // Une édition manuelle efface l'état fluide et se signale.
    m.set_block(3, 4, 5, blocks.water);
    let ev = m.take_fluid_events();
    assert_eq!(ev.edits, vec![IVec3::new(3, 4, 5)]);
    assert_eq!(m.fluid_cell(3, 4, 5).unwrap().1, FluidCode::DEFAULT);
    assert_eq!(m.water_level(3, 4, 5), 8);

    // Persistance : la couche suit le chunk jusqu'au stockage et revient.
    assert!(m.write_fluid(6, 7, 8, blocks.water, code));
    m.unload_chunk(p0);
    m.flush_pending_saves();
    let blob = storage.load_chunk_blob(p0).unwrap().unwrap();
    assert!(blob.fluid.is_some(), "couche non enregistrée");
    wait_until("rechargement", || {
        m.update(Vec3::new(8.0, 5.0, 8.0));
        m.chunk(p0).is_some()
    });
    assert_eq!(m.fluid_cell(6, 7, 8), Some((blocks.water, code)));
    assert!(m.take_fluid_events().loaded.contains(&p0));
    // blob_of rend aussi la couche d'un chunk actif.
    assert!(m.blob_of(p0).unwrap().fluid.is_some());
}

#[test]
fn a_manager_without_fluids_pushes_nothing() {
    let registry = Arc::new(BlockRegistry::for_kind(WorldKind::Norrcraft));
    let m = ChunkManager::with_terrain(
        Arc::new(InMemoryChunkStorage::new()) as Arc<dyn ChunkStorage>,
        Arc::new(EmptyTerrain),
        registry,
        1,
        1,
    );
    m.set_render_distance(1);
    wait_until("chargement", || {
        m.update(Vec3::new(8.0, 5.0, 8.0));
        m.chunk(ChunkPos::new(0, 0)).is_some()
    });
    m.set_block(1, 1, 1, 3);
    assert!(m.take_fluid_events().is_empty());
}

/// La simulation sur de vrais chunks : l'eau tombe, traverse une frontière de chunk, se conserve
/// et finit au repos ; les deltas du pas sont collectés pour le réseau.
#[test]
fn simulation_on_real_chunks_crosses_borders() {
    use nb_world::fluid::managed::ManagerFluid;
    use nb_world::fluid::{FluidEnv, FluidSim};

    let registry = Arc::new(BlockRegistry::for_kind(WorldKind::Norrcraft));
    let m = ChunkManager::with_terrain(
        Arc::new(InMemoryChunkStorage::new()) as Arc<dyn ChunkStorage>,
        Arc::new(EmptyTerrain),
        registry.clone(),
        1,
        1,
    );
    m.set_render_distance(1);
    m.enable_fluids();
    wait_until("chargement", || {
        m.update(Vec3::new(8.0, 5.0, 8.0));
        m.chunk(ChunkPos::new(0, 0)).is_some()
            && m.chunk(ChunkPos::new(1, 0)).is_some()
            && m.chunk(ChunkPos::new(-1, 0)).is_some()
    });
    let blocks = m.fluid_blocks().unwrap().clone();
    let mut rules = (*nb_data::WaterRules::builtin()).clone();
    rules.time_cap_micros = 0;
    let mut sim = FluidSim::new(Arc::new(rules), blocks.clone(), 5, false);
    sim.set_collect_deltas(true);
    // Granit au sol sur toute la zone, eau posée à la main de part et d'autre de x = 16.
    let granite = registry.get("granit").id;
    for x in 0..32 {
        for z in 0..16 {
            m.set_block(x, 0, z, granite);
        }
    }
    m.set_block(15, 9, 8, blocks.water);
    m.set_block(16, 9, 8, blocks.water);
    let env = FluidEnv::default();
    for _ in 0..(12 * 400) {
        let ev = m.take_fluid_events();
        let mut w = ManagerFluid::new(&m);
        sim.tick(&mut w, ev, &env);
    }
    assert!(sim.is_idle(), "l'eau doit finir au repos");
    let total: i32 = (0..32)
        .flat_map(|x| (0..16).map(move |z| (x, z)))
        .flat_map(|(x, z)| (0..10).map(move |y| (x, y, z)))
        .map(|(x, y, z)| i32::from(m.water_level(x, y, z)))
        .sum();
    assert_eq!(total, 16, "le volume doit se conserver (2 cellules pleines = 16 h)");
    // L'eau s'est étalée sur le sol, des deux côtés de la frontière de chunk.
    let left: i32 = (0..16).map(|x| i32::from(m.water_level(x, 1, 8))).sum();
    let right: i32 = (16..32).map(|x| i32::from(m.water_level(x, 1, 8))).sum();
    assert!(left > 0 && right > 0, "gauche {left} droite {right}");
    assert!(!sim.take_deltas().is_empty());
}
