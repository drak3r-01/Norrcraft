use glam::Vec3;
use nb_particles::*;

/// Sol plein pour y < 64, lave en (10, 63, 10)… ici : simple plancher.
struct Floor;
impl ParticleWorld for Floor {
    fn cell(&self, _: i32, y: i32, _: i32) -> Cell {
        if y < 64 {
            Cell::Solid
        } else {
            Cell::Air
        }
    }
}

fn view() -> View {
    View {
        eye: Vec3::new(0.0, 66.0, 0.0),
        forward: Vec3::Z,
        max_distance: 60.0,
        light: [1.0; 3],
    }
}

#[test]
fn pool_never_exceeds_capacity() {
    let mut s = ParticleSystem::new(100, 1);
    s.spawn(&presets::EMBER, &Emit::at(Vec3::new(0.0, 70.0, 5.0), 500));
    assert_eq!(s.len(), 100);
    assert_eq!(s.stats().dropped, 400);
}

#[test]
fn particles_expire() {
    let mut s = ParticleSystem::new(1000, 2);
    s.spawn(
        &presets::DUST_PUFF,
        &Emit::at(Vec3::new(0.0, 70.0, 5.0), 50),
    );
    for _ in 0..200 {
        s.update(0.02, &EmptyWorld);
    }
    assert!(s.is_empty());
}

#[test]
fn debris_bounces_on_floor_and_never_sinks() {
    let mut s = ParticleSystem::new(1000, 3);
    s.spawn(
        &presets::BLOCK_DEBRIS,
        &Emit::at(Vec3::new(0.5, 66.0, 5.5), 60).block(3),
    );
    for _ in 0..120 {
        s.update(1.0 / 60.0, &Floor);
        for p in s.particles() {
            assert!(p.pos.y >= 63.99, "traverse le sol: {}", p.pos.y);
        }
    }
}

#[test]
fn rain_dies_on_ground_and_splashes() {
    let mut s = ParticleSystem::new(5000, 4);
    s.spawn(
        &presets::RAIN,
        &Emit::at(Vec3::new(0.5, 70.0, 5.5), 200).dir(Vec3::NEG_Y),
    );
    for _ in 0..40 {
        s.update(1.0 / 60.0, &Floor);
    }
    // Plus aucune goutte de pluie sous le sol ; des éclaboussures sont apparues.
    assert!(s.particles().iter().all(|p| p.pos.y > 63.9));
    assert!(s.stats().spawned > 200);
}

#[test]
fn instances_are_culled_and_packed() {
    let mut s = ParticleSystem::new(1000, 5);
    s.spawn(&presets::FIREFLY, &Emit::at(Vec3::new(0.0, 66.0, 10.0), 10)); // devant
    s.spawn(
        &presets::FIREFLY,
        &Emit::at(Vec3::new(0.0, 66.0, -10.0), 10),
    ); // derrière
    s.spawn(
        &presets::FIREFLY,
        &Emit::at(Vec3::new(0.0, 66.0, 500.0), 10),
    ); // trop loin
    let mut out = Vec::new();
    s.write_instances(&view(), &mut out);
    assert_eq!(out.len(), 10);
    assert!(out.iter().all(|g| g.flags & FLAG_EMISSIVE != 0));
}

#[test]
fn block_debris_use_block_atlas() {
    let mut s = ParticleSystem::new(100, 6);
    s.spawn(
        &presets::BLOCK_DEBRIS,
        &Emit::at(Vec3::new(0.0, 66.0, 5.0), 5).block(42),
    );
    let mut out = Vec::new();
    s.write_instances(&view(), &mut out);
    assert!(out
        .iter()
        .all(|g| g.layer == 42 && g.flags & FLAG_BLOCK_ATLAS != 0));
    assert!(out.iter().all(|g| g.uv[2] == 16 && g.uv[3] == 16));
}

#[test]
fn simulation_is_deterministic() {
    let run = || {
        let mut s = ParticleSystem::new(1000, 9);
        s.spawn(
            &presets::BLOCK_DEBRIS,
            &Emit::at(Vec3::new(0.0, 66.0, 5.0), 40).block(1),
        );
        for _ in 0..30 {
            s.update(1.0 / 60.0, &Floor);
        }
        let mut out = Vec::new();
        s.write_instances(&view(), &mut out);
        out
    };
    assert_eq!(run(), run());
}

#[test]
fn ambient_rain_only_where_sky_is_open() {
    struct Roof;
    impl ParticleWorld for Roof {
        fn cell(&self, _: i32, y: i32, _: i32) -> Cell {
            if y == 80 {
                Cell::Solid
            } else {
                Cell::Air
            }
        }
    }
    let mut s = ParticleSystem::new(20000, 7);
    let mut a = Ambient::new();
    let ctx = AmbientCtx {
        eye: Vec3::new(0.0, 66.0, 0.0),
        forward: Vec3::Z,
        rain: 1.0,
        snow: 0.0,
        night: 0.0,
        eye_in_water: false,
    };
    for _ in 0..60 {
        a.update(&mut s, &Roof, &ctx, 1.0 / 60.0);
    }
    // Toit à 80 : le ciel est fermé partout dans la fenêtre de spawn (y ≤ 80).
    assert!(s.is_empty(), "{} particules sous un toit", s.len());
    let mut s2 = ParticleSystem::new(20000, 7);
    for _ in 0..60 {
        a.update(&mut s2, &EmptyWorld, &ctx, 1.0 / 60.0);
    }
    assert!(s2.len() > 100);
}

/// Budget : 30 000 particules mises à jour + exportées sans allocation. Seuil large (CI lente,
/// build debug optimisé) ; la mesure réelle s'affiche avec `--nocapture`.
#[test]
fn throughput_30k_particles() {
    let mut s = ParticleSystem::new(32768, 11);
    s.spawn(
        &presets::RAIN,
        &Emit::at(Vec3::new(0.0, 100.0, 5.0), 15000).dir(Vec3::NEG_Y),
    );
    s.spawn(
        &presets::SNOW,
        &Emit::at(Vec3::new(0.0, 90.0, 5.0), 15000).dir(Vec3::NEG_Y),
    );
    let mut out = Vec::new();
    let t = std::time::Instant::now();
    let frames = 60;
    for _ in 0..frames {
        s.update(1.0 / 60.0, &Floor);
        s.write_instances(&view(), &mut out);
    }
    let per = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    println!(
        "{} particules : {per:.3} ms par frame (update + export)",
        s.len()
    );
    assert!(per < 10.0, "{per} ms");
}
