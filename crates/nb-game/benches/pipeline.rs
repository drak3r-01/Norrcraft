//! Banc d'essai reproductible de la chaîne monde → lumière → maillage (phase 7, docs/PERF.md).
//!
//! `cargo bench -p nb-game --bench pipeline` (profil `bench` = `release`). Sans dépendance :
//! chronométrage `Instant`, médiane de `NB_BENCH_ITERS` répétitions (défaut 7). Chaque section
//! affiche aussi une empreinte des sorties : elle doit être identique avant/après toute
//! optimisation (contrôle bit à bit en plus des tests de référence).
//!
//! Filtre optionnel : `cargo bench -p nb-game --bench pipeline -- gen mesh` ne lance que les
//! sections dont le nom contient l'un des mots donnés.

use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_core::glam::Vec3;
use nb_core::ChunkPos;
use nb_data::BlockRegistry;
use nb_game::{WorldBiomes, WorldChunks};
use nb_mesh::{
    mesh_chunk, BiomeSource, BlockTable, ChunkSource, ChunkVoxels, MeshAction, MeshScheduler,
    Neighborhood, UniformBiome,
};
use nb_world::{
    compute_light, fnv1a, Chunk, ChunkManager, InMemoryChunkStorage, LightContext, LightTable,
    WorldConfig, WorldGenerator, WorldProfile, WorldType,
};

const SEED: i64 = 12345;
/// Grille générée : `[-G, G)²` chunks.
const G: i32 = 4;

fn iters() -> usize {
    std::env::var("NB_BENCH_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7)
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

/// Combine des empreintes (ordre significatif).
fn mix(h: u64, v: u64) -> u64 {
    fnv1a(&[h.to_le_bytes(), v.to_le_bytes()].concat())
}

fn report(name: &str, per: &str, units: usize, times: Vec<Duration>, check: u64) {
    let min = *times.iter().min().expect("au moins une mesure");
    let med = median(times);
    let per_unit = med.as_secs_f64() * 1e6 / units as f64;
    println!(
        "{name:<28} médiane {:>9.3} ms  min {:>9.3} ms  {per_unit:>10.1} µs/{per}  empreinte {check:016x}",
        med.as_secs_f64() * 1e3,
        min.as_secs_f64() * 1e3,
    );
}

fn bench<T>(n: usize, mut f: impl FnMut() -> T) -> (Vec<Duration>, T) {
    let mut out = f(); // échauffement
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        out = black_box(f());
        times.push(t.elapsed());
    }
    (times, out)
}

fn generator(reg: &BlockRegistry, t: WorldType) -> WorldGenerator {
    WorldGenerator::new(SEED, Arc::new(WorldConfig::preset(t)), reg.ids())
}

fn grid() -> Vec<ChunkPos> {
    let mut v = Vec::new();
    for z in -G..G {
        for x in -G..G {
            v.push(ChunkPos::new(x, z));
        }
    }
    v
}

fn gen_all(g: &WorldGenerator) -> Vec<Chunk> {
    grid()
        .into_iter()
        .map(|p| {
            let mut c = Chunk::new(p);
            g.generate_chunk(&mut c);
            c
        })
        .collect()
}

fn find(chunks: &[Chunk], p: ChunkPos) -> Option<&Chunk> {
    chunks.iter().find(|c| c.pos() == p)
}

/// Chunks intérieurs de la grille (voisinage 3×3 complet).
fn interior() -> Vec<ChunkPos> {
    grid()
        .into_iter()
        .filter(|p| p.x > -G && p.x < G - 1 && p.z > -G && p.z < G - 1)
        .collect()
}

fn neighbours(chunks: &[Chunk], p: ChunkPos) -> [Option<&Chunk>; 9] {
    std::array::from_fn(|i| {
        let (dx, dz) = (i as i32 % 3 - 1, i as i32 / 3 - 1);
        find(chunks, ChunkPos::new(p.x + dx, p.z + dz))
    })
}

fn wanted(filters: &[String], name: &str) -> bool {
    filters.is_empty() || filters.iter().any(|f| name.contains(f.as_str()))
}

fn main() {
    // `cargo bench` passe `--bench` : on ne garde que les mots libres.
    let filters: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .collect();
    let n = iters();
    let reg = BlockRegistry::builtin();
    println!("nb-game/benches/pipeline : {n} répétitions, médiane retenue, graine {SEED}");

    let normal = generator(&reg, WorldType::Normal);
    let chunks = gen_all(&normal);
    let units = chunks.len();

    for (name, t) in [
        ("gen/normal", WorldType::Normal),
        ("gen/flat", WorldType::Flat),
        ("gen/amplified", WorldType::Amplified),
    ] {
        if !wanted(&filters, name) {
            continue;
        }
        let g = generator(&reg, t);
        let (times, out) = bench(n, || gen_all(&g));
        let check = out.iter().fold(0, |h, c| mix(h, c.fingerprint()));
        report(name, "chunk", units, times, check);
    }

    if wanted(&filters, "light/world") {
        let table = LightTable::new(&reg);
        let inner = interior();
        let mut ctx = LightContext::default();
        let (times, out) = bench(n, || {
            inner
                .iter()
                .map(|&p| ctx.compute(&neighbours(&chunks, p), &table))
                .collect::<Vec<_>>()
        });
        let check = out.iter().fold(0, |h, l| mix(h, fnv1a(l.as_slice())));
        report("light/world", "chunk", inner.len(), times, check);
        // Contrôle : la version autonome donne le même résultat.
        let alone = inner.iter().fold(0, |h, &p| {
            mix(
                h,
                fnv1a(compute_light(&neighbours(&chunks, p), &table).as_slice()),
            )
        });
        assert_eq!(alone, check, "LightContext ≠ compute_light");
    }

    let table = BlockTable::from_registry(&reg, |_| 0);
    let world_biomes = WorldBiomes::new(Arc::new(WorldProfile::vanilla(
        SEED,
        Arc::new(WorldConfig::preset(WorldType::Normal)),
        reg.ids(),
    )));
    let voxels: Vec<(ChunkPos, ChunkVoxels)> = chunks
        .iter()
        .map(|c| (c.pos(), ChunkVoxels::from_vec(c.voxels().to_vec())))
        .collect();
    let vox = |p: ChunkPos| voxels.iter().find(|(q, _)| *q == p).map(|(_, v)| v);
    let hoods: Vec<(ChunkPos, [Option<&ChunkVoxels>; 9])> = interior()
        .into_iter()
        .map(|p| {
            (
                p,
                std::array::from_fn(|i| {
                    let (dx, dz) = (i as i32 % 3 - 1, i as i32 / 3 - 1);
                    vox(ChunkPos::new(p.x + dx, p.z + dz))
                }),
            )
        })
        .collect();

    for (name, biomes) in [
        ("mesh/world-biomes", &world_biomes as &dyn BiomeSource),
        ("mesh/uniform-biome", &UniformBiome(0) as &dyn BiomeSource),
    ] {
        if !wanted(&filters, name) {
            continue;
        }
        let (times, out) = bench(n, || {
            hoods
                .iter()
                .map(|(p, refs)| mesh_chunk(*p, &Neighborhood::from_voxels(*refs), &table, biomes))
                .collect::<Vec<_>>()
        });
        let check = out.iter().fold(0, |h, m| mix(h, m.fingerprint()));
        let verts: usize = out.iter().map(|m| m.vertex_count()).sum();
        report(name, "chunk", hoods.len(), times, check);
        println!("{:<28} {verts} sommets", "");
    }

    if wanted(&filters, "light/mesh") {
        let (times, out) = bench(n, || {
            hoods
                .iter()
                .map(|(_, refs)| {
                    nb_mesh::LightVolume::compute(&Neighborhood::from_voxels(*refs), &table)
                })
                .collect::<Vec<_>>()
        });
        let check = out.iter().fold(0, |h, l| mix(h, l.fingerprint()));
        report("light/mesh-volume", "chunk", hoods.len(), times, check);
    }

    if wanted(&filters, "manager") {
        manager_bench(&reg, &table, &world_biomes, n.min(3));
    }
    if wanted(&filters, "frame") {
        frame_bench(&reg, &table, &world_biomes);
    }
}

/// Charge entièrement un monde de rayon `r`, puis renvoie le gestionnaire et le planificateur.
fn load_world(
    reg: &Arc<BlockRegistry>,
    table: &Arc<BlockTable>,
    biomes: &Arc<dyn BiomeSource>,
    r: i32,
) -> (ChunkManager, MeshScheduler, usize) {
    let m = ChunkManager::new(
        Arc::new(InMemoryChunkStorage::new()),
        SEED,
        Arc::new(WorldConfig::preset(WorldType::Normal)),
        reg.clone(),
    );
    m.set_render_distance(r);
    let mut s = MeshScheduler::new(nb_mesh::default_threads());
    let side = (2 * r + 1) as usize;
    // Maillables : 9 voisins à lumière propre, soit le carré de rayon R − 2 (spec §11.7).
    let target = (side - 4) * (side - 4);
    let mut meshed = std::collections::HashSet::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        m.update(Vec3::new(8.0, 80.0, 8.0));
        let mut active = m.active_positions();
        active.sort_by_key(|p| (p.x, p.z));
        for a in s.update(&mut WorldChunks(&m), &active, table.clone(), biomes.clone()) {
            if let MeshAction::Upload(p, _) = a {
                meshed.insert(p);
            }
        }
        let all_light_clean = active
            .iter()
            .all(|p| !WorldChunks(&m).light_dirty(*p) || !m.has_neighbors(*p));
        if active.len() == side * side
            && m.pending() == (0, 0)
            && all_light_clean
            && meshed.len() >= target
            && s.in_flight_count() == 0
        {
            break;
        }
        assert!(Instant::now() < deadline, "chargement trop long");
        std::thread::sleep(Duration::from_micros(500));
    }
    (m, s, meshed.len())
}

/// Coût par frame, monde chargé et stable (rayon par défaut 12) : `ChunkManager::update` et
/// `MeshScheduler::update` (rien à faire : coût de pure planification).
fn frame_bench(reg: &BlockRegistry, table: &BlockTable, biomes: &WorldBiomes) {
    const FRAMES: usize = 200;
    let reg = Arc::new(reg.clone());
    let table = Arc::new(table.clone());
    let biomes: Arc<dyn BiomeSource> = Arc::new(biomes.clone());
    let (m, mut s, meshed) = load_world(&reg, &table, &biomes, nb_world::DEFAULT_RENDER_DISTANCE);
    let player = Vec3::new(8.0, 80.0, 8.0);
    let mut t_world = Vec::with_capacity(FRAMES);
    let mut t_mesh = Vec::with_capacity(FRAMES);
    let mut actions = 0;
    for _ in 0..FRAMES {
        let t = Instant::now();
        m.update(player);
        t_world.push(t.elapsed());
        let t = Instant::now();
        let mut active = m.active_positions();
        active.sort_by_key(|p| (p.x, p.z));
        actions += s
            .update(&mut WorldChunks(&m), &active, table.clone(), biomes.clone())
            .len();
        t_mesh.push(t.elapsed());
    }
    let n = m.active_positions().len();
    report("frame/chunk-manager", "frame", 1, t_world, n as u64);
    report(
        "frame/mesh-scheduler",
        "frame",
        1,
        t_mesh,
        (meshed as u64) << 32 | actions as u64,
    );
    s.shutdown();
    m.shutdown();
}

/// Chargement complet (génération + lumière + maillage) d'un rayon 6 autour de l'origine,
/// avec les pools par défaut (`max(1, cœurs/2)` threads chacun).
fn manager_bench(reg: &BlockRegistry, table: &BlockTable, biomes: &WorldBiomes, n: usize) {
    const R: i32 = 6;
    let reg = Arc::new(reg.clone());
    let table = Arc::new(table.clone());
    let biomes: Arc<dyn BiomeSource> = Arc::new(biomes.clone());
    let (times, (loaded, meshed)) = bench(n, || {
        let (m, s, meshed) = load_world(&reg, &table, &biomes, R);
        let loaded = m.active_positions().len();
        s.shutdown();
        m.shutdown();
        (loaded, meshed)
    });
    report(
        "manager/load-r6",
        "chunk",
        loaded,
        times,
        (loaded as u64) << 32 | meshed as u64,
    );
}
