//! Scénarios du lot oiseaux de Norrcraft (session 5.5) avec les fiches réelles de
//! `assets/norrcraft/mobs.json` : le tétras en parade, la lagopède qui fuit, les oiseaux
//! volants qui quittent le sol, le corbeau qui charogne.

mod common;

use common::Harness;
use nb_core::glam::DVec3;
use nb_data::behavior::StateKind;
use nb_data::{MobRegistry, WorldKind};
use nb_ecs::{Brain, BrainSystem, EntityId, GameMode};
use nb_world::calendar::Calendar;

const NOON: f32 = 900.0;

fn harness() -> Harness {
    let mut h = Harness::new();
    h.stone_floor();
    h.mobs = MobRegistry::builtin_for(WorldKind::Norrcraft);
    for c in h.mobs.all_mut() {
        c.max_count = 0;
    }
    h.sim.schedule.push(Box::new(BrainSystem::default()));
    h.cfg.time_of_day = NOON;
    h.cfg.fall_damage_enabled = false;
    h.cfg.mob_despawn_radius = 200;
    h
}

fn set_month(h: &mut Harness, year: u64, month: usize) {
    let cal = Calendar::new(h.cfg.days_per_month);
    let doy = (0..cal.days_per_year())
        .find(|&d| cal.date(cal.day_for(year, d)).month == month)
        .unwrap();
    h.cfg.calendar_day = cal.day_for(year, doy);
}

fn brain(h: &Harness, id: EntityId) -> Brain {
    h.sim.entities.copy::<Brain>(id).unwrap()
}

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s * 60.0) as usize);
}

#[test]
fn flying_birds_leave_the_ground_and_keep_flying() {
    for id in ["korp", "havsorn", "mas", "trana"] {
        let mut h = harness();
        set_month(&mut h, 1, 5);
        let e = h.spawn_mob(id, 0.5, 66.0, 0.5);
        let start = h.t(e).pos;
        let (mut far, mut alive) = (0.0f64, true);
        for _ in 0..40 {
            secs(&mut h, 1.0);
            alive &= h.sim.entities.contains(e) && h.h(e).alive;
            if !alive {
                break;
            }
            let p = h.t(e).pos;
            far = far.max(((p.x - start.x).powi(2) + (p.z - start.z).powi(2)).sqrt());
        }
        assert!(alive, "{id} meurt");
        assert!(far > 3.0, "{id} ne bouge pas ({far})");
    }
}

#[test]
fn the_rutting_capercaillie_attacks_a_player_who_comes_close() {
    let mut h = harness();
    set_month(&mut h, 1, 3); // avril : parade
    let stone = h.ids().stone;
    h.slab(-90, 90, 64, -90, 90, stone);
    let bird = h.spawn_mob("tjader", 0.5, 65.0, 0.5);
    h.ticks(2);
    let player = h.spawn_player(DVec3::new(3.0, 65.0, 0.5), GameMode::Survival);
    let hp = h.h(player).health;
    let mut seen = std::collections::BTreeSet::new();
    let mut lowest = hp;
    for _ in 0..32 {
        secs(&mut h, 0.25);
        seen.insert(brain(&h, bird).state);
        lowest = lowest.min(h.h(player).health);
    }
    assert!(
        seen.contains(&StateKind::Charge) || seen.contains(&StateKind::Threaten),
        "états vus {seen:?}"
    );
    assert!(lowest < hp, "le tétras ne frappe pas : {seen:?}");
}

#[test]
fn a_ptarmigan_flees_the_player() {
    let mut h = harness();
    set_month(&mut h, 1, 6);
    let stone = h.ids().stone;
    h.slab(-90, 90, 64, -90, 90, stone);
    let bird = h.spawn_mob("ripa", 0.5, 65.0, 0.5);
    h.ticks(2);
    h.spawn_player(DVec3::new(5.5, 65.0, 0.5), GameMode::Survival);
    secs(&mut h, 0.6);
    let b = brain(&h, bird);
    assert!(
        matches!(b.state, StateKind::Flee | StateKind::Burrow),
        "ripa : {:?} {:?}",
        b.state,
        b.scores
    );
}
