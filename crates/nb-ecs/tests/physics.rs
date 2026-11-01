//! Tests de comportement : physique du joueur et des mobs (spec §6, §7).

mod common;

use common::Harness;
use nb_core::glam::{DVec3, Vec3};
use nb_ecs::physics::push_apart_entities;
use nb_ecs::{Collider, EcsEvent, GameAction, GameMode, Health, Transform, Velocity};

const FLOOR_TOP: f64 = 65.0;

fn on_floor(h: &mut Harness, mode: GameMode) {
    h.stone_floor();
    h.spawn_player(DVec3::new(0.5, FLOOR_TOP + 0.5, 0.5), mode);
    h.ticks(60);
}

#[test]
fn player_falls_and_rests_on_the_floor() {
    let mut h = Harness::new();
    on_floor(&mut h, GameMode::Survival);
    let id = h.player();
    let t = h.t(id);
    let v = h.v(id);
    assert!(v.grounded);
    assert!(
        t.pos.y >= FLOOR_TOP && t.pos.y < FLOOR_TOP + 0.05,
        "y = {}",
        t.pos.y
    );
    assert_eq!(h.h(id).health, 10.0, "petite chute : pas de dégâts");
    // Caméra aux yeux.
    let cam = h.p().camera;
    assert!((cam.position.y - (t.pos.y as f32 + 1.62)).abs() < 1e-4);
}

#[test]
fn jump_height_is_about_one_block_and_repeats_while_held() {
    let mut h = Harness::new();
    on_floor(&mut h, GameMode::Survival);
    let id = h.player();
    let y0 = h.t(id).pos.y;
    h.hold(GameAction::Jump);
    let mut max_y: f64 = y0;
    let mut landings = 0;
    let mut was_grounded = true;
    for _ in 0..180 {
        h.tick();
        max_y = max_y.max(h.t(id).pos.y);
        let g = h.v(id).grounded;
        if g && !was_grounded {
            landings += 1;
        }
        was_grounded = g;
    }
    let gain = max_y - y0;
    // Apex discret ≈ 1.03 ; sans recalage au sol, un saut peut partir jusqu'à un pas (≈ 0.13)
    // au-dessus du sol (B4), d'où la marge.
    assert!(gain > 1.0 && gain < 1.2, "hauteur de saut {gain}");
    assert!(
        landings >= 3,
        "sauts répétés tant que JUMP est maintenu (B17)"
    );
}

#[test]
fn walls_block_horizontal_movement() {
    let mut h = Harness::new();
    on_floor(&mut h, GameMode::Survival);
    let stone = h.ids().stone;
    // Mur haut de 3 en z = -3.
    for y in 65..68 {
        h.slab(-5, 5, y, -3, -3, stone);
    }
    h.hold(GameAction::Forward); // yaw 0 → −Z
    h.ticks(120);
    let z = h.t(h.player()).pos.z;
    // Contact quand floor(z − 0.3) = −3, soit z < −1.7 : approche par petits pas (B4).
    assert!((-1.7..-1.65).contains(&z), "z = {z}");
}

#[test]
fn walking_and_sprinting_speeds() {
    let mut h = Harness::new();
    on_floor(&mut h, GameMode::Survival);
    let id = h.player();
    h.hold(GameAction::Forward);
    h.ticks(60);
    let walk = h.v(id).v.z;
    assert!((walk + 4.317).abs() < 0.01, "marche {walk}");
    let fov_walk = h.p().camera.fov;
    h.hold(GameAction::Sprint);
    h.ticks(60);
    let sprint = h.v(id).v.z;
    assert!((sprint + 5.612).abs() < 0.01, "sprint {sprint}");
    assert!(h.p().camera.fov > fov_walk, "FOV élargi au sprint");
    // Pas de sprint en reculant.
    h.release(GameAction::Forward);
    h.hold(GameAction::Backward);
    h.ticks(60);
    assert!((h.v(id).v.z - 4.317).abs() < 0.01);
}

#[test]
fn fall_damage_depends_on_mode_and_setting() {
    let fall = |mode, enabled: bool| {
        let mut h = Harness::new();
        h.stone_floor();
        h.cfg.fall_damage_enabled = enabled;
        let id = h.spawn_player(DVec3::new(0.5, FLOOR_TOP + 10.0, 0.5), mode);
        h.ticks(120);
        h.h(id).health
    };
    let hp = fall(GameMode::Survival, true);
    // Vitesse d'impact ≈ √(2·32·10) ≈ 25.3 → (25.3 − 11.5)·0.4 ≈ 5.5.
    assert!(hp > 3.5 && hp < 5.5, "santé après chute de 10 : {hp}");
    assert_eq!(fall(GameMode::Creative, true), 10.0);
    assert_eq!(fall(GameMode::Survival, false), 10.0);

    // Chute de 2 blocs : sous le seuil de 11.5.
    let mut h = Harness::new();
    h.stone_floor();
    let id = h.spawn_player(DVec3::new(0.5, FLOOR_TOP + 2.0, 0.5), GameMode::Survival);
    h.ticks(60);
    assert_eq!(h.h(id).health, 10.0);
}

#[test]
fn fatal_fall_respawns_player_with_empty_inventory() {
    let mut h = Harness::new();
    h.stone_floor();
    h.cfg.spawn = Some([3.5, FLOOR_TOP, 3.5]);
    let id = h.spawn_player(DVec3::new(0.5, FLOOR_TOP + 40.0, 0.5), GameMode::Survival);
    let dirt = h.ids().dirt;
    h.inv_mut().add_block(dirt, 0);
    h.ticks(150);
    let ev = h.take_events();
    assert!(ev.contains(&EcsEvent::PlayerDied));
    assert!(ev.contains(&EcsEvent::PlayerRespawned));
    let hp = h.h(id);
    assert!(hp.alive);
    assert_eq!(hp.health, 10.0);
    assert!(h.inv().is_empty(), "/kill et la mort vident l'inventaire");
    let t = h.t(id);
    assert_eq!((t.pos.x, t.pos.z), (3.5, 3.5));
}

#[test]
fn double_tap_toggles_flight_in_creative_only() {
    let mut h = Harness::new();
    on_floor(&mut h, GameMode::Creative);
    h.press(GameAction::Jump);
    h.ticks(5);
    h.press(GameAction::Jump);
    assert!(h.p().flying);
    // En vol : pas de gravité, montée avec JUMP.
    let y0 = h.t(h.player()).pos.y;
    h.ticks(60);
    let y1 = h.t(h.player()).pos.y;
    assert!((y1 - y0).abs() < 0.2, "le vol ne tombe pas : {y0} → {y1}");
    h.hold(GameAction::Jump);
    h.ticks(30);
    assert!(h.t(h.player()).pos.y > y1 + 3.0);
    h.release(GameAction::Jump);

    // Trop lent : pas de bascule.
    let mut h = Harness::new();
    on_floor(&mut h, GameMode::Creative);
    h.press(GameAction::Jump);
    h.ticks(30);
    h.press(GameAction::Jump);
    assert!(!h.p().flying);

    // Survie : jamais.
    let mut h = Harness::new();
    on_floor(&mut h, GameMode::Survival);
    h.press(GameAction::Jump);
    h.press(GameAction::Jump);
    assert!(!h.p().flying);
}

#[test]
fn swimming_caps_sinking_speed_and_slows_movement() {
    let mut h = Harness::new();
    h.stone_floor();
    let water = h.ids().water;
    for y in 65..75 {
        h.slab(-10, 10, y, -10, 10, water);
    }
    let id = h.spawn_player(DVec3::new(0.5, 73.0, 0.5), GameMode::Survival);
    h.hold(GameAction::Forward);
    h.hold(GameAction::Sprint);
    for _ in 0..90 {
        h.tick();
        assert!(h.v(id).v.y >= -3.0);
    }
    let vz = h.v(id).v.z;
    assert!((vz + 4.317 * 0.4).abs() < 0.02, "nage sans sprint : {vz}");
    // Remonter en tenant JUMP, plafonné à jumpForce·0.4.
    h.hold(GameAction::Jump);
    h.ticks(60);
    assert!((h.v(id).v.y - 8.4 * 0.4).abs() < 1e-4);
    assert_eq!(h.p().fluid_at_eye_level, water);
}

#[test]
fn lava_damages_every_half_second_in_survival() {
    let mut h = Harness::new();
    h.stone_floor();
    let lava = h.ids().lava;
    h.slab(-3, 3, 65, -3, 3, lava);
    h.slab(-3, 3, 66, -3, 3, lava);
    let id = h.spawn_player(DVec3::new(0.5, FLOOR_TOP, 0.5), GameMode::Survival);
    h.ticks(29);
    assert_eq!(h.h(id).health, 10.0, "premier dégât après 0.5 s");
    h.ticks(3);
    assert_eq!(h.h(id).health, 9.0);
    h.ticks(60);
    assert_eq!(h.h(id).health, 7.0);

    let mut h = Harness::new();
    h.stone_floor();
    h.slab(-3, 3, 65, -3, 3, lava);
    let id = h.spawn_player(DVec3::new(0.5, FLOOR_TOP, 0.5), GameMode::Creative);
    h.ticks(120);
    assert_eq!(h.h(id).health, 10.0);
}

#[test]
fn lily_pad_is_a_thin_floor_for_the_player() {
    let mut h = Harness::new();
    let ids = h.ids();
    h.slab(-3, 3, 60, -3, 3, ids.stone);
    for y in 61..64 {
        h.slab(-3, 3, y, -3, 3, ids.water);
    }
    h.put(0, 64, 0, ids.lily_pad);
    let id = h.spawn_player(DVec3::new(0.5, 64.5, 0.5), GameMode::Survival);
    h.ticks(60);
    let y = h.t(id).pos.y;
    assert!((64.015..64.1).contains(&y), "y = {y}");
    assert!(h.v(id).grounded);
}

#[test]
fn unloaded_chunks_act_as_floor_for_the_player_only() {
    let mut h = Harness::new();
    h.blocks.set_loaded_chunks([]);
    let id = h.spawn_player(DVec3::new(0.5, 80.0, 0.5), GameMode::Survival);
    let mob = h.spawn_mob("reindeer", 5.5, 80.0, 5.5);
    h.ticks(60);
    assert_eq!(h.t(id).pos.y, 80.0, "sol virtuel (B18)");
    assert!(h.v(id).grounded);
    assert!(h.t(mob).pos.y < 70.0, "les mobs tombent dans le vide");
}

#[test]
fn fov_becomes_nan_with_zero_sprint_speed() {
    let mut h = Harness::new();
    on_floor(&mut h, GameMode::Survival);
    h.cfg.sprint_speed = 0.0;
    h.tick();
    assert!(h.p().camera.fov.is_nan(), "0/0 propagé comme en Java (B16)");
}

// ---------------------------------------------------------------------------------------------
// Mobs
// ---------------------------------------------------------------------------------------------

/// Entité terrestre sans IA (consignes fixées à la main).
fn dummy(h: &mut Harness, pos: DVec3, target: Vec3, health: Option<f32>) -> nb_ecs::EntityId {
    let w = &mut h.sim.entities;
    let id = w.create();
    w.insert(id, Transform::new(pos.x, pos.y, pos.z));
    w.insert(
        id,
        Velocity {
            target,
            ..Default::default()
        },
    );
    w.insert(id, Collider::new(0.6, 0.6, 0.9, 0.0));
    if let Some(hp) = health {
        w.insert(id, Health::new(hp));
    }
    id
}

#[test]
fn land_mob_gravity_and_asymptotic_landing() {
    let mut h = Harness::new();
    h.stone_floor();
    let id = dummy(&mut h, DVec3::new(0.5, 70.0, 0.5), Vec3::ZERO, None);
    h.ticks(120);
    let t = h.t(id);
    let v = h.v(id);
    assert!(v.grounded);
    assert!(
        t.pos.y >= FLOOR_TOP && t.pos.y < FLOOR_TOP + 0.01,
        "y = {}",
        t.pos.y
    );
}

#[test]
fn land_mob_auto_jumps_over_one_block() {
    let mut h = Harness::new();
    h.stone_floor();
    let stone = h.ids().stone;
    h.slab(3, 20, 65, -5, 5, stone); // marche d'un bloc
    let id = dummy(
        &mut h,
        DVec3::new(0.5, FLOOR_TOP, 0.5),
        Vec3::new(2.0, 0.0, 0.0),
        None,
    );
    h.ticks(180);
    let t = h.t(id);
    assert!(t.pos.x > 3.5, "x = {}", t.pos.x);
    assert!(
        t.pos.y >= 66.0 - 1e-9,
        "a sauté sur le bloc : y = {}",
        t.pos.y
    );
}

#[test]
fn land_mob_fall_damage_kills_and_health_system_destroys() {
    let mut h = Harness::new();
    h.stone_floor();
    let id = dummy(&mut h, DVec3::new(0.5, 80.0, 0.5), Vec3::ZERO, Some(5.0));
    let survivor = dummy(&mut h, DVec3::new(5.5, 66.0, 5.5), Vec3::ZERO, Some(5.0));
    h.ticks(120);
    assert!(!h.sim.entities.contains(id));
    assert!(h.take_events().contains(&EcsEvent::MobDied { entity: id }));
    assert_eq!(h.h(survivor).health, 5.0, "chute courte : |v| < 10");
}

#[test]
fn water_mobs_ignore_gravity_even_in_air() {
    let mut h = Harness::new();
    h.stone_floor();
    let fish = h.spawn_mob("fish", 0.5, 80.0, 0.5);
    h.ticks(120);
    assert_eq!(h.t(fish).pos.y, 80.0, "B6 : un poisson peut « voler »");
}

#[test]
fn push_apart_splits_overlap_and_player_is_immovable() {
    let mut h = Harness::new();
    h.stone_floor();
    let a = dummy(&mut h, DVec3::new(0.5, FLOOR_TOP, 0.5), Vec3::ZERO, None);
    let b = dummy(&mut h, DVec3::new(0.5, FLOOR_TOP, 0.5), Vec3::ZERO, None);
    push_apart_entities(&mut h.sim.entities, &h.mobs);
    let (ta, tb) = (h.t(a), h.t(b));
    assert_eq!(ta.pos.x, 0.5);
    assert!(
        ((ta.pos.z - tb.pos.z) - 0.6).abs() < 1e-6,
        "égalité → axe Z, 50/50"
    );
    assert!((ta.pos.z - 0.8).abs() < 1e-6);

    // Joueur immobile : le mob prend toute la séparation.
    let mut h = Harness::new();
    h.stone_floor();
    let pid = h.spawn_player(DVec3::new(0.5, FLOOR_TOP, 0.5), GameMode::Survival);
    let m = dummy(&mut h, DVec3::new(0.7, FLOOR_TOP, 0.5), Vec3::ZERO, None);
    push_apart_entities(&mut h.sim.entities, &h.mobs);
    assert_eq!(h.t(pid).pos.x, 0.5);
    assert!((h.t(m).pos.x - 1.1).abs() < 1e-6);

    // Hauteurs disjointes : pas de poussée.
    let mut h = Harness::new();
    let a = dummy(&mut h, DVec3::new(0.5, 65.0, 0.5), Vec3::ZERO, None);
    let b = dummy(&mut h, DVec3::new(0.5, 65.9, 0.5), Vec3::ZERO, None);
    push_apart_entities(&mut h.sim.entities, &h.mobs);
    assert_eq!(h.t(a).pos, h.t(b).pos.with_y(65.0));
}

#[test]
fn orca_and_fish_are_never_pushed_apart() {
    let mut h = Harness::new();
    let orca = h.spawn_mob("orca", 0.5, 50.0, 0.5);
    let fish = h.spawn_mob("fish", 0.5, 50.0, 0.5);
    let fish2 = h.spawn_mob("fish", 0.5, 50.0, 0.5);
    push_apart_entities(&mut h.sim.entities, &h.mobs);
    assert_eq!(h.t(orca).pos, DVec3::new(0.5, 50.0, 0.5));
    // Deux poissons, eux, se séparent : overlapX = 0.22 < overlapZ = 0.5 → axe X.
    let (t1, t2) = (h.t(fish), h.t(fish2));
    assert!(((t1.pos.x - t2.pos.x) - 0.22).abs() < 1e-6);
}
