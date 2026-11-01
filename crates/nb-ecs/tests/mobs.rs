//! Tests de comportement : IA passive / agressive (§11, §12), spawn et despawn (§10).

mod common;

use common::Harness;
use nb_core::glam::{DVec3, Vec3};
use nb_ecs::ai::trigger_player_flee;
use nb_ecs::{AggressiveAi, EcsEvent, GameMode, PassiveAi};

fn ocean(h: &mut Harness, y0: i32, y1: i32, r: i32) {
    let water = h.ids().water;
    for y in y0..=y1 {
        h.slab(-r, r, y, -r, r, water);
    }
}

fn horiz(v: Vec3) -> f32 {
    (v.x * v.x + v.z * v.z).sqrt()
}

#[test]
fn passive_land_mob_wanders_and_pauses() {
    let mut h = Harness::new();
    h.stone_floor();
    let m = h.spawn_mob("reindeer", 0.5, 65.0, 0.5);
    h.tick();
    let ai = h.sim.entities.copy::<PassiveAi>(m).unwrap();
    let v = h.v(m);
    assert!(
        ai.moving,
        "un mob neuf choisit une direction dès son premier tick"
    );
    let s = horiz(v.target);
    assert!((1.2..=1.8).contains(&s), "vitesse d'errance {s}");
    assert_eq!(v.target.y, 0.0);
    assert!(ai.move_timer > 0.5 - 1.0 / 60.0 && ai.move_timer <= 5.0);

    // Le cycle alterne marche et pause, toujours au sol.
    let mut saw_pause = false;
    let start = h.t(m).pos;
    for _ in 0..900 {
        h.tick();
        let ai = h.sim.entities.copy::<PassiveAi>(m).unwrap();
        if !ai.moving {
            saw_pause = true;
            assert_eq!(h.v(m).target, Vec3::ZERO);
        }
    }
    assert!(saw_pause);
    let end = h.t(m).pos;
    assert!((end - start).length() > 1.0);
    assert!(end.y >= 65.0 && end.y < 65.2, "reste au sol : {}", end.y);
}

#[test]
fn fish_without_water_ahead_stays_still() {
    let mut h = Harness::new();
    let water = h.ids().water;
    h.put(0, 50, 0, water);
    let fish = h.spawn_mob("fish", 0.5, 50.5, 0.5);
    h.tick();
    let ai = h.sim.entities.copy::<PassiveAi>(fish).unwrap();
    assert!(!ai.moving);
    assert_eq!(ai.move_timer, 1.0, "8 essais ratés → pause de 1 s");
    assert_eq!(h.v(fish).target, Vec3::ZERO);

    let mut h = Harness::new();
    ocean(&mut h, 40, 60, 16);
    let fish = h.spawn_mob("fish", 0.5, 50.5, 0.5);
    h.tick();
    let ai = h.sim.entities.copy::<PassiveAi>(fish).unwrap();
    assert!(ai.moving);
    let v = h.v(fish).target;
    assert!((0.4..=1.0 + 1e-5).contains(&v.length()));
    // Nage 3D : tangage = −angle vertical (±36°).
    let pitch = h.t(fish).pitch;
    assert!(pitch.abs() <= (std::f32::consts::PI / 5.0) + 1e-6);
}

#[test]
fn herding_fish_head_towards_the_herd() {
    let mut h = Harness::new();
    ocean(&mut h, 40, 60, 24);
    for c in h.mobs.all_mut() {
        if c.id == "fish" {
            c.herd_bias = 1.0;
        }
    }
    let a = h.spawn_mob("fish", 0.5, 50.5, 0.5);
    h.spawn_mob("fish", 6.5, 50.5, 0.5);
    h.spawn_mob("fish", 6.5, 50.5, 2.5);
    h.tick();
    let t = h.v(a).target;
    let heading = t.z.atan2(t.x);
    let to_herd = (1.0f32).atan2(6.0);
    assert!(
        (heading - to_herd).abs() <= std::f32::consts::PI / 5.0 + 1e-4,
        "cap {heading} vers le centroïde {to_herd}"
    );
}

#[test]
fn passive_flees_same_medium_aggressor_continuously() {
    let mut h = Harness::new();
    h.stone_floor();
    h.mobs
        .apply_overrides(r#"{"mobs": [{"id": "zombie", "behaviorType": "AGGRESSIVE"}]}"#)
        .unwrap();
    let deer = h.spawn_mob("reindeer", 0.5, 65.0, 0.5);
    h.spawn_mob("zombie", -4.5, 65.0, 0.5);
    for _ in 0..3 {
        h.tick();
        let v = h.v(deer);
        assert!((v.target.x - 3.0).abs() < 1e-4 && v.target.z.abs() < 1e-3);
        let ai = h.sim.entities.copy::<PassiveAi>(deer).unwrap();
        assert!(!ai.moving && ai.move_timer == 0.0);
    }
    assert!((h.t(deer).yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-3);

    // Hors de portée (12 > 10) : errance normale.
    let mut h = Harness::new();
    h.stone_floor();
    h.mobs
        .apply_overrides(r#"{"mobs": [{"id": "zombie", "behaviorType": "AGGRESSIVE"}]}"#)
        .unwrap();
    let deer = h.spawn_mob("reindeer", 0.5, 65.0, 0.5);
    h.spawn_mob("zombie", -11.5, 65.0, 0.5);
    h.tick();
    assert!(h.sim.entities.copy::<PassiveAi>(deer).unwrap().moving);

    // Un orque (milieu WATER) n'effraie pas un renne (LAND).
    let mut h = Harness::new();
    h.stone_floor();
    let deer = h.spawn_mob("reindeer", 0.5, 65.0, 0.5);
    h.spawn_mob("orca", -3.5, 65.0, 0.5);
    h.tick();
    assert!(h.sim.entities.copy::<PassiveAi>(deer).unwrap().moving);
}

#[test]
fn player_flee_lasts_then_pauses() {
    let mut h = Harness::new();
    h.stone_floor();
    let deer = h.spawn_mob("reindeer", 0.5, 65.0, 0.5);
    let mobs = h.mobs.clone();
    trigger_player_flee(&mut h.sim.entities, deer, &mobs, &mut h.rngs.passive);
    let ai = h.sim.entities.copy::<PassiveAi>(deer).unwrap();
    assert!((3.0..=5.0).contains(&ai.flee_timer));
    let s = horiz(h.v(deer).target);
    assert!((s - 4.317).abs() < 1e-3, "vitesse de fuite {s}");
    let mut ticks = 0;
    loop {
        h.tick();
        ticks += 1;
        let ai = h.sim.entities.copy::<PassiveAi>(deer).unwrap();
        if ai.flee_timer <= 0.0 {
            assert_eq!(ai.move_timer, 1.5);
            assert!(!ai.moving);
            assert_eq!(h.v(deer).target, Vec3::ZERO);
            break;
        }
        assert!(ticks < 400);
    }
    assert!((170..=310).contains(&ticks), "durée {ticks}");
}

#[test]
fn orca_chases_and_bites_fish_with_cooldown() {
    let mut h = Harness::new();
    ocean(&mut h, 40, 60, 32);
    let orca = h.spawn_mob("orca", 0.5, 50.0, 0.5);
    let far = h.spawn_mob("fish", 8.5, 50.0, 0.5);
    h.tick();
    let t = h.v(orca).target;
    assert!((t.x - 3.5).abs() < 1e-4 && t.y.abs() < 1e-4 && t.z.abs() < 1e-4);
    // Le poisson (détection 10) fuit l'orque à 2.5.
    let ft = h.v(far).target;
    assert!((ft.x - 2.5).abs() < 1e-4);

    let mut h = Harness::new();
    ocean(&mut h, 40, 60, 32);
    let orca = h.spawn_mob("orca", 0.5, 50.0, 0.5);
    let near = h.spawn_mob("fish", 2.5, 50.0, 0.5);
    let next = h.spawn_mob("fish", 0.5, 50.0, 3.0);
    h.take_events();
    h.tick();
    assert!(
        !h.sim.entities.contains(near),
        "mordu et détruit dans le même tick"
    );
    assert!(h
        .take_events()
        .contains(&EcsEvent::MobDied { entity: near }));
    let cd = h
        .sim
        .entities
        .copy::<AggressiveAi>(orca)
        .unwrap()
        .attack_cooldown;
    assert_eq!(cd, 2.5);
    h.ticks(60);
    assert!(h.sim.entities.contains(next), "recharge de 2.5 s");
    let mut caught = false;
    for _ in 0..1800 {
        h.tick();
        if !h.sim.entities.contains(next) {
            caught = true;
            break;
        }
    }
    assert!(caught, "l'orque (3.5) rattrape le poisson (2.5)");
}

#[test]
fn orca_targets_only_survival_players_in_water() {
    let setup = |mode: GameMode, feet_y: f64| {
        let mut h = Harness::new();
        ocean(&mut h, 40, 60, 32);
        let stone = h.ids().stone;
        h.slab(-32, 32, 39, -32, 32, stone);
        h.spawn_player(DVec3::new(20.5, feet_y, 0.5), mode);
        let orca = h.spawn_mob("orca", 0.5, 50.0, 0.5);
        h.tick();
        let speed = h.v(orca).target.length();
        (h, orca, speed)
    };
    let (_, _, s) = setup(GameMode::Survival, 50.0);
    assert!((s - 3.5).abs() < 1e-3, "poursuite du joueur : {s}");
    let (_, _, s) = setup(GameMode::Creative, 50.0);
    assert!(
        (0.8..=1.4 + 1e-5).contains(&s),
        "créatif ignoré → errance : {s}"
    );
    let (_, _, s) = setup(GameMode::Survival, 70.0);
    assert!(
        (0.8..=1.4 + 1e-5).contains(&s),
        "joueur hors de l'eau ignoré : {s}"
    );

    // Morsure du joueur : −5 PV, recul vertical 5.
    let mut h = Harness::new();
    ocean(&mut h, 40, 60, 32);
    let pid = h.spawn_player(DVec3::new(2.5, 50.0, 0.5), GameMode::Survival);
    h.spawn_mob("orca", 0.5, 50.0, 0.5);
    h.tick();
    assert_eq!(h.h(pid).health, 5.0);
    assert_eq!(h.v(pid).v.y, 5.0);
    assert!(h.v(pid).v.x > 4.0, "recul dans le sens orque → joueur");
}

// ---------------------------------------------------------------------------------------------
// Spawn
// ---------------------------------------------------------------------------------------------

#[test]
fn reindeer_spawn_on_surface_after_one_second_and_respect_max_count() {
    let mut h = Harness::new();
    let grass = h.ids().grass;
    h.slab(-80, 80, 64, -80, 80, grass);
    h.enable_spawn("reindeer");
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Creative);
    h.ticks(59);
    assert_eq!(h.count_mobs("reindeer"), 0, "premier cycle après 1 s");
    h.ticks(2);
    let n = h.count_mobs("reindeer");
    assert!((1..=4).contains(&n), "un groupe de 1 à 4 : {n}");
    for id in h.sim.entities.query_ids::<&nb_ecs::MobType>() {
        let t = h.t(id);
        assert_eq!(t.pos.y, 65.0);
        assert_eq!(t.pos.x.fract().abs(), 0.5);
        assert!(t.yaw >= 0.0 && t.yaw < std::f32::consts::TAU);
    }
    assert!(h
        .take_events()
        .iter()
        .any(|e| matches!(e, EcsEvent::MobSpawned { .. })));
    h.ticks(60 * 30);
    let n = h.count_mobs("reindeer");
    assert!(n > 4 && n <= 16, "plafond maxCount : {n}");
}

#[test]
fn water_mobs_spawn_mid_column() {
    let mut h = Harness::new();
    let ids = h.ids();
    for y in 40..=44 {
        h.slab(-80, 80, y, -80, 80, ids.water);
    }
    h.slab(-80, 80, 39, -80, 80, ids.sand);
    h.enable_spawn("fish");
    h.enable_spawn("orca");
    // Pas de joueur : origine du spawn en (0, 0, 0).
    h.ticks(61);
    assert!(h.count_mobs("fish") > 0 && h.count_mobs("orca") > 0);
    for id in h.sim.entities.query_ids::<&nb_ecs::MobType>() {
        let mt = h
            .sim
            .entities
            .get::<nb_ecs::MobType>(id)
            .unwrap()
            .mob_id
            .clone();
        let y = h.t(id).pos.y;
        match mt.as_str() {
            "fish" => assert_eq!(y, 43.5),
            "orca" => assert_eq!(y, 42.5),
            other => panic!("{other}"),
        }
    }
    assert!(h.count_mobs("orca") <= 2);
}

#[test]
fn far_mobs_despawn_and_degenerate_configs_do_not_panic() {
    let mut h = Harness::new();
    h.stone_floor();
    let far = h.spawn_mob("reindeer", 100.5, 65.0, 0.5);
    let near = h.spawn_mob("reindeer", 10.5, 65.0, 0.5);
    h.ticks(61);
    assert!(!h.sim.entities.contains(far));
    assert!(h.sim.entities.contains(near));
    assert!(h
        .take_events()
        .contains(&EcsEvent::MobDespawned { entity: far }));

    // Rayon de spawn nul et groupes incohérents : « aucun spawn » au lieu de l'exception Java.
    let mut h = Harness::new();
    h.stone_floor();
    h.enable_spawn("reindeer");
    h.cfg.mob_spawn_radius = 0;
    h.ticks(130);
    assert_eq!(h.count_mobs("reindeer"), 0);

    let mut h = Harness::new();
    let grass = h.ids().grass;
    h.slab(-80, 80, 64, -80, 80, grass);
    h.enable_spawn("reindeer");
    for c in h.mobs.all_mut() {
        c.group_size_min = 5;
        c.group_size_max = 2;
    }
    h.ticks(130);
    assert_eq!(h.count_mobs("reindeer"), 0);

    // Milieu AIR : jamais d'apparition.
    let mut h = Harness::new();
    h.slab(-80, 80, 64, -80, 80, grass);
    h.mobs
        .apply_overrides(r#"{"mobs": [{"id": "bird", "medium": "AIR", "maxCount": 5}]}"#)
        .unwrap();
    h.ticks(130);
    assert_eq!(h.count_mobs("bird"), 0);
}
