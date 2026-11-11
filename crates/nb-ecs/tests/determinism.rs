//! Déterminisme (mêmes graines ⇒ même simulation), ordre des systèmes et intégration avec un
//! vrai `nb_world::World`.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::Harness;
use nb_core::glam::{DVec3, Vec2, Vec3};
use nb_data::{BlockRegistry, MobRegistry};
use nb_ecs::{
    CameraState, EcsRngs, GameAction, GameMode, Health, PlayerInput, Schedule, Simulation, TickCtx,
    Transform, Velocity, WorldBlocks, TICK_DT,
};
use nb_world::{InMemoryChunkStorage, World, WorldConfig, WorldType};

#[test]
fn systems_run_in_real_java_order() {
    assert_eq!(
        Schedule::java_default().names(),
        [
            "PlayerInput",
            "Physics",
            "PassiveAI",
            "AggressiveAI",
            "MobSpawn",
            "Health"
        ]
    );
}

/// Terrain varié : prairie, océan, arbres (feuilles), lave ; tous les mobs actifs.
fn scenario(seed: i64) -> Harness {
    let mut h = Harness::with_seed(seed);
    let ids = h.ids();
    h.slab(-80, 80, 60, -80, 80, ids.stone);
    h.slab(-80, 80, 64, -80, -1, ids.grass);
    for y in 61..=64 {
        h.slab(-80, 80, y, 0, 80, ids.water);
    }
    for (x, z) in [(-6, -6), (-12, -20), (5, -30)] {
        for y in 65..69 {
            h.put(x, y, z, ids.wood);
        }
        h.slab(x - 2, x + 2, 69, z - 2, z + 2, ids.leaves);
    }
    h.slab(-20, -18, 64, 10, 12, ids.lava);
    for id in ["reindeer", "fish", "orca"] {
        h.enable_spawn(id);
    }
    h.spawn_player(DVec3::new(0.5, 65.0, -3.5), GameMode::Survival);
    h
}

type Snapshot = Vec<(i32, [u64; 3], u32, u32, [u32; 3])>;

fn run(h: &mut Harness, ticks: usize) -> (Snapshot, usize) {
    for i in 0..ticks {
        // Entrées scriptées : marche, rotations, sauts, coups.
        h.input.pressed = Default::default();
        if (i / 120) % 2 == 0 {
            h.hold(GameAction::Forward);
        }
        if i % 90 == 0 {
            h.input.just_pressed.insert(GameAction::Jump);
            h.hold(GameAction::Jump);
        }
        if i % 45 == 0 {
            h.input.just_pressed.insert(GameAction::Primary);
        }
        h.input.mouse_delta = Vec2::new(((i % 37) as f32) - 18.0, ((i % 11) as f32) - 5.0);
        h.tick();
    }
    let w = &h.sim.entities;
    let snap = w
        .ids()
        .into_iter()
        .filter_map(|id| {
            let t = w.copy::<Transform>(id)?;
            let v = w.copy::<Velocity>(id)?;
            let hp = w.copy::<Health>(id).map_or(0, |x| x.health.to_bits());
            Some((
                id.0,
                [t.pos.x.to_bits(), t.pos.y.to_bits(), t.pos.z.to_bits()],
                t.yaw.to_bits(),
                hp,
                [v.v.x.to_bits(), v.v.y.to_bits(), v.v.z.to_bits()],
            ))
        })
        .collect();
    (snap, h.events.len())
}

#[test]
fn same_seeds_give_identical_simulations() {
    let mut a = scenario(99);
    let mut b = scenario(99);
    let ra = run(&mut a, 1500);
    let rb = run(&mut b, 1500);
    assert!(ra.0.len() > 5, "des mobs sont apparus : {}", ra.0.len());
    assert_eq!(ra, rb);
    assert_eq!(a.blocks.block_at_test(), b.blocks.block_at_test());

    let mut c = scenario(100);
    let rc = run(&mut c, 1500);
    assert_ne!(ra.0, rc.0, "une autre graine change la simulation");
}

/// Accès de test : petite empreinte du monde de blocs.
trait Fingerprint {
    fn block_at_test(&self) -> Vec<u8>;
}

impl Fingerprint for nb_ecs::MemoryBlocks {
    fn block_at_test(&self) -> Vec<u8> {
        use nb_ecs::BlockAccess;
        let mut v = Vec::new();
        for x in -10..10 {
            for z in -10..10 {
                for y in 60..70 {
                    v.push(self.block(x, y, z));
                }
            }
        }
        v
    }
}

#[test]
fn runs_on_a_real_nb_world() {
    let reg = Arc::new(BlockRegistry::builtin());
    let world = World::new(
        42,
        Arc::new(InMemoryChunkStorage::new()),
        WorldConfig::preset(WorldType::Normal),
        reg.clone(),
    );
    world.chunk_manager().set_render_distance(2);
    let [sx, sy, sz] = world.ensure_spawn();
    let eye = Vec3::new(sx as f32, sy as f32 + 1.62, sz as f32);
    let start = Instant::now();
    while !(-1..=1).all(|dx| {
        (-1..=1).all(|dz| world.is_chunk_loaded(sx as i32 + dx * 16, sz as i32 + dz * 16))
    }) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "chargement des chunks"
        );
        world.update(eye);
        std::thread::sleep(Duration::from_millis(5));
    }

    let mobs = MobRegistry::builtin();
    let mut rngs = EcsRngs::from_seed(5);
    let mut events = Vec::new();
    let input = PlayerInput::default();
    let mut sim = Simulation::new();
    let pid = sim.init_player(
        CameraState::new(eye, 1.2),
        GameMode::Survival,
        reg.ids().air,
    );
    sim.entities.get_mut::<Transform>(pid).unwrap().pos = DVec3::new(sx, sy, sz);
    for _ in 0..120 {
        world.update(sim.camera().unwrap().position);
        let mut blocks = WorldBlocks(&world);
        let mut cfg = world.config_mut();
        let mut ctx = TickCtx {
            blocks: &mut blocks,
            block_reg: &reg,
            mobs: &mobs,
            config: &mut cfg,
            input: &input,
            rngs: &mut rngs,
            events: &mut events,
    survival: None,
            environment: None,
        };
        sim.tick(&mut ctx, TICK_DT);
    }
    world.shutdown();
    let t = sim.entities.copy::<Transform>(pid).unwrap();
    let v = sim.entities.copy::<Velocity>(pid).unwrap();
    assert!(v.grounded, "le joueur repose sur le terrain généré");
    // Le spawn Java est en `hauteur + 1` : on peut retomber d'au plus un bloc.
    assert!(
        t.pos.y <= sy && t.pos.y > sy - 1.2,
        "y = {} (spawn {sy})",
        t.pos.y
    );
    let below = world.block(
        sx.floor() as i32,
        t.pos.y.floor() as i32 - 1,
        sz.floor() as i32,
    );
    assert!(reg.from_id(below).solid);
    assert_eq!(sim.entities.copy::<Health>(pid).unwrap().health, 10.0);
}
