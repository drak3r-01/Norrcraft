//! Scénarios du lot aquatique et insectes de Norrcraft (session 5.6) avec les fiches réelles de
//! `assets/norrcraft/mobs.json` : poissons qui nagent, brochet qui mord, phoque qui plonge,
//! morse qui charge, moustiques qui volent.

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
    for c in h.mobs.all_mut() {
        if let Some(s) = c.spawn.as_mut() {
            s.time_of_day = nb_data::behavior::Activity::Always;
        }
    }
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

/// Un bassin d'eau de 40 × 40, profond de 8, sur un sol de pierre.
fn pool(h: &mut Harness) {
    let (stone, water) = (h.ids().stone, h.ids().water);
    h.slab(-20, 20, 56, -20, 20, stone);
    for y in 57..=64 {
        h.slab(-20, 20, y, -20, 20, water);
    }
}

#[test]
fn fish_swim_around_and_stay_in_the_water() {
    for id in ["lax", "oring", "stromming", "torsk", "tumlare"] {
        let mut h = harness();
        pool(&mut h);
        set_month(&mut h, 1, 6);
        let e = h.spawn_mob(id, 0.5, 60.5, 0.5);
        let start = h.t(e).pos;
        let (mut far, mut alive) = (0.0f64, true);
        for _ in 0..30 {
            secs(&mut h, 1.0);
            alive &= h.sim.entities.contains(e) && h.h(e).alive;
            if !alive {
                break;
            }
            let p = h.t(e).pos;
            assert!(p.y < 65.6, "{id} sort de l'eau ({})", p.y);
            far = far.max(((p.x - start.x).powi(2) + (p.z - start.z).powi(2)).sqrt());
        }
        assert!(alive, "{id} meurt");
        assert!(far > 2.0, "{id} ne bouge pas ({far})");
    }
}

#[test]
fn the_pike_hunts_the_perch() {
    let mut h = harness();
    pool(&mut h);
    set_month(&mut h, 1, 6);
    let pike = h.spawn_mob("gadda", 0.5, 60.5, 0.5);
    let perch = h.spawn_mob("abborre", 3.5, 60.5, 0.5);
    let mut seen = std::collections::BTreeSet::new();
    let mut lowest = h.h(perch).health;
    let mut dead = false;
    for _ in 0..120 {
        secs(&mut h, 0.25);
        seen.insert(brain(&h, pike).state);
        if h.sim.entities.contains(perch) {
            lowest = lowest.min(h.h(perch).health);
        } else {
            dead = true;
            break;
        }
    }
    assert!(
        seen.contains(&StateKind::Attack) || seen.contains(&StateKind::Pursue) || seen.contains(&StateKind::Stalk),
        "états vus {seen:?}"
    );
    assert!(dead || lowest < 1.0, "la perche n'est pas mangée : {seen:?}");
}

#[test]
fn a_seal_flees_a_player_who_walks_up() {
    let mut h = harness();
    set_month(&mut h, 1, 6);
    let stone = h.ids().stone;
    h.slab(-90, 90, 64, -90, 90, stone);
    let seal = h.spawn_mob("grasal", 0.5, 65.0, 0.5);
    h.ticks(2);
    h.spawn_player(DVec3::new(5.5, 65.0, 0.5), GameMode::Survival);
    secs(&mut h, 0.6);
    let b = brain(&h, seal);
    assert!(
        matches!(b.state, StateKind::Flee | StateKind::Swim),
        "grasal : {:?} {:?}",
        b.state,
        b.scores
    );
}

#[test]
fn the_walrus_threatens_and_charges_an_intruder() {
    let mut h = harness();
    set_month(&mut h, 1, 6);
    let stone = h.ids().stone;
    h.slab(-90, 90, 64, -90, 90, stone);
    let walrus = h.spawn_mob("valross", 0.5, 65.0, 0.5);
    h.ticks(2);
    let player = h.spawn_player(DVec3::new(3.0, 65.0, 0.5), GameMode::Survival);
    h.h(player);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..40 {
        secs(&mut h, 0.25);
        seen.insert(brain(&h, walrus).state);
    }
    assert!(
        seen.contains(&StateKind::Threaten) || seen.contains(&StateKind::Charge),
        "états vus {seen:?}"
    );
}

#[test]
fn mosquitoes_and_dragonflies_fly() {
    for id in ["mygg", "trollslanda", "bi", "fjaril"] {
        let mut h = harness();
        set_month(&mut h, 1, 6);
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
        assert!(far > 1.0, "{id} ne bouge pas ({far})");
    }
}
