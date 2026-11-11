//! Changement de saison sur un monde Norrcraft vivant : les chunks actifs sont réécrits en
//! arrière-plan, ceux que le joueur a modifiés gardent leur colonne (session 3.2).

use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_core::glam::Vec3;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::{Chunk, InMemoryChunkStorage, World, WorldConfig};

const AUG: u8 = 4;
const JAN: u8 = 0;

fn wait_loaded(w: &World, n: usize) {
    let t = Instant::now();
    loop {
        w.update(Vec3::new(8.0, 80.0, 8.0));
        let m = w.chunk_manager();
        if m.active_positions().len() >= n && m.pending() == (0, 0) {
            return;
        }
        assert!(
            t.elapsed() < Duration::from_secs(90),
            "chargement trop long"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_season_change_rewrites_the_loaded_chunks() {
    let reg = Arc::new(BlockRegistry::for_kind(WorldKind::Norrcraft));
    let w = World::for_kind(
        WorldKind::Norrcraft,
        12345,
        Arc::new(InMemoryChunkStorage::new()),
        WorldConfig::default(),
        reg.clone(),
    );
    let m = w.chunk_manager();
    m.set_render_distance(3);
    m.set_season(Some(AUG));
    wait_loaded(&w, 49);
    let snow: Vec<u8> = (1..=7)
        .map(|i| reg.get(&format!("snotacke_{i}")).id)
        .collect();
    let summer: Vec<_> = m.active_chunks();
    let count_snow = |chunks: &[(nb_core::ChunkPos, nb_world::chunk::SharedChunk)]| {
        chunks
            .iter()
            .map(|(_, c)| {
                c.read()
                    .voxels()
                    .iter()
                    .filter(|b| snow.contains(b))
                    .count()
            })
            .sum::<usize>()
    };
    assert_eq!(count_snow(&summer), 0, "pas de neige en août");

    // Le joueur pose un bloc dans un chunk avant l'hiver.
    let stone = reg.get("granit").id;
    let (px, pz) = (3, 3);
    let top = (1..126)
        .rev()
        .find(|&y| m.block(px, y, pz) != 0)
        .expect("sol");
    m.set_block(px, top + 1, pz, stone);

    m.set_season(Some(JAN));
    let t = Instant::now();
    let generator = w.profile().new_generator();
    loop {
        w.update(Vec3::new(8.0, 80.0, 8.0));
        let now: Vec<_> = m.active_chunks();
        if count_snow(&now) > 200 {
            // Le chunk modifié garde sa colonne ; les autres égalent la génération d'hiver.
            let mut checked = 0;
            for (pos, c) in &now {
                if pos.x == 0 && pos.z == 0 {
                    assert_eq!(c.read().block(px, top + 1, pz), stone);
                    continue;
                }
                let mut fresh = Chunk::new(*pos);
                generator.generate_chunk(&mut fresh);
                if c.read().voxels() == fresh.voxels() {
                    checked += 1;
                }
            }
            if checked + 1 >= now.len() {
                return;
            }
        }
        assert!(
            t.elapsed() < Duration::from_secs(40),
            "surcouche trop lente : {} couches de neige, {} chunks, pas {:?}",
            count_snow(&now),
            now.len(),
            m.season()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
