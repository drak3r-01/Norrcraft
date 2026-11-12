//! Moteur d'IA du bestiaire Norrcraft (session 5.1) : besoins, utilité, perception, horloges,
//! scénarios du doc 03 §6.

mod common;

use common::Harness;
use nb_core::glam::DVec3;
use nb_data::behavior::{BehaviorBook, StateKind, Stimulus};
use nb_ecs::brain::clock::{self, Clock};
use nb_ecs::{
    AnimalAct, Brain, BrainSystem, Carrying, EcsEvent, EntityId, GameMode, Health, PassiveAi,
    Schedule, Transform,
};
use nb_world::calendar::Calendar;
use nb_world::weather::{Environment, Wind};

const NOON: f32 = 900.0;
const MIDNIGHT: f32 = 300.0;

/// Banc : sol de pierre, mobs d'essai menés par les profils intégrés, moteur d'états en fin
/// d'ordonnancement.
fn harness() -> Harness {
    let mut h = Harness::new();
    h.stone_floor();
    let mob = |id: &str, profile: &str, extra: &str| {
        format!(
            r#"{{"id": "{id}", "behavior": "{profile}", "maxCount": 0, "attackRange": 1.6,
                "attackDamage": 2.0, "attackCooldown": 0.8, "chaseSpeed": 5.0 {extra}}}"#
        )
    };
    let still = r#", "moveSpeedMin": 0, "moveSpeedMax": 0, "playerFleeSpeed": 0, "aggressorFleeSpeed": 0, "chaseSpeed": 0"#;
    let list = [
        mob("t_ren", "hjord", ""),
        mob("t_alg", "betare", ""),
        mob("t_hare", "smavilt", ""),
        mob("t_byte", "smavilt", still),
        mob("t_varg", "flockjagare", ""),
        mob("t_bjorn", "allatare", ""),
        mob("t_orn", "rovfagel", ""),
        mob("t_baver", "byggare", still),
        mob("t_lammel", "gnagare", ""),
    ];
    h.mobs
        .apply_overrides(&format!(r#"{{"mobs": [{}]}}"#, list.join(",")))
        .unwrap();
    h.sim.schedule.push(Box::new(BrainSystem::default()));
    h.cfg.time_of_day = NOON;
    h
}

/// Premier jour de l'année `year` dont le mois réel est `month`.
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

fn edit(h: &mut Harness, id: EntityId, f: impl FnOnce(&mut Brain)) {
    f(h.sim.entities.get_mut::<Brain>(id).unwrap());
}

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s * 60.0) as usize);
}

fn hdist(a: DVec3, b: DVec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

fn calm_env(wind: Wind) -> Environment {
    Environment {
        air: 0.5,
        wind,
        chill: 1.0,
        rain: 0.0,
        snow: 0.0,
        fog: 0.0,
        sun: 1.0,
    }
}

#[test]
fn profiled_mobs_get_a_brain_and_vanilla_mobs_keep_their_ai() {
    let mut h = harness();
    let ren = h.spawn_mob("t_ren", 0.5, 65.0, 0.5);
    let deer = h.spawn_mob("reindeer", 4.5, 65.0, 0.5);
    assert!(h.sim.entities.has::<Brain>(ren));
    assert!(!h.sim.entities.has::<PassiveAi>(ren));
    assert!(h.sim.entities.has::<PassiveAi>(deer));
    assert!(!h.sim.entities.has::<Brain>(deer));
    h.tick();
    let b = brain(&h, ren);
    let book = BehaviorBook::builtin();
    assert_eq!(b.profile.map(usize::from), book.index_of("hjord"));
    // L'ordonnancement d'origine n'a pas changé (le moteur s'ajoute côté jeu).
    assert!(!Schedule::java_default().names().contains(&"Brain"));
}

#[test]
fn a_reindeer_flees_a_player_it_notices() {
    let mut h = harness();
    let ren = h.spawn_mob("t_ren", 0.5, 65.0, 0.5);
    h.ticks(2);
    h.spawn_player(DVec3::new(6.5, 65.0, 0.5), GameMode::Survival);
    secs(&mut h, 0.5);
    let b = brain(&h, ren);
    assert_eq!(b.state, StateKind::Flee, "scores {:?}", b.scores);
    assert!(b.needs.fear > 0.5);
    secs(&mut h, 2.0);
    let t = h.t(ren);
    assert!(t.pos.x < -2.0, "le renne s'éloigne : {:?}", t.pos);
}

#[test]
fn hunger_leads_to_grazing_and_grazing_feeds() {
    let mut h = harness();
    // Soleil à l'horizon : l'élan, crépusculaire, est actif.
    h.environment = Some(calm_env(Wind::CALM));
    h.environment.as_mut().unwrap().sun = 0.0;
    let alg = h.spawn_mob("t_alg", 0.5, 65.0, 0.5);
    h.ticks(30);
    edit(&mut h, alg, |b| {
        b.needs.hunger = 0.9;
        b.needs.fatigue = 0.0;
    });
    secs(&mut h, 2.5);
    let b = brain(&h, alg);
    assert_eq!(b.state, StateKind::Graze, "scores {:?}", b.scores);
    assert!(b.needs.hunger < 0.8, "brouter nourrit : {}", b.needs.hunger);
    assert!(h.take_events().iter().any(|e| matches!(
        e,
        EcsEvent::AnimalAct {
            act: AnimalAct::Graze,
            ..
        }
    )));
}

#[test]
fn a_diurnal_animal_sleeps_at_night() {
    let mut h = harness();
    set_month(&mut h, 1, 1); // vraie nuit d'hiver (les nuits de mai sont claires)
    h.cfg.time_of_day = MIDNIGHT;
    let ren = h.spawn_mob("t_ren", 0.5, 65.0, 0.5);
    secs(&mut h, 3.0);
    let b = brain(&h, ren);
    assert_eq!(b.state, StateKind::Sleep, "scores {:?}", b.scores);
    assert!(b.activity < 0.05);
}

#[test]
fn hysteresis_holds_a_state_for_two_seconds() {
    let mut h = harness();
    let alg = h.spawn_mob("t_alg", 0.5, 65.0, 0.5);
    h.ticks(5);
    // Brouter de justesse devant dormir : l'écart (< 0,35) ne suffit pas à changer tout de suite.
    edit(&mut h, alg, |b| {
        b.state = StateKind::Graze;
        b.since = 0.0;
        b.needs.hunger = 0.55;
        b.needs.fatigue = 0.9;
    });
    h.tick();
    let b = brain(&h, alg);
    assert!(
        b.score(StateKind::Sleep) > b.score(StateKind::Graze),
        "{:?}",
        b.scores
    );
    secs(&mut h, 1.5);
    assert_eq!(brain(&h, alg).state, StateKind::Graze);
    secs(&mut h, 1.0);
    assert_eq!(brain(&h, alg).state, StateKind::Sleep);
}

#[test]
fn smell_carries_downwind_only() {
    // Un loup derrière un mur : ni vue ni ouïe à 50 blocs, seulement l'odorat (64).
    let run = |wind_dir: [f32; 2]| {
        let mut h = harness();
        let stone = h.ids().stone;
        h.blocks.fill(
            nb_core::glam::IVec3::new(-20, 65, -10),
            nb_core::glam::IVec3::new(-20, 75, 10),
            stone,
        );
        h.environment = Some(calm_env(Wind {
            dir: wind_dir,
            speed: 0.6,
        }));
        let varg = h.spawn_mob("t_varg", 0.5, 65.0, 0.5);
        h.spawn_player(DVec3::new(-49.5, 65.0, 0.5), GameMode::Survival);
        h.ticks(12);
        brain(&h, varg).percepts.threat
    };
    // Vent vers +x : le joueur (en −x) est au vent du loup, son odeur porte à 128 blocs.
    let upwind = run([1.0, 0.0]).expect("odeur portée par le vent");
    assert_eq!(upwind.sense, nb_ecs::brain::senses::Sense::Smell);
    // Vent vers −x : sous le vent, la portée tombe à 32 blocs.
    assert!(run([-1.0, 0.0]).is_none());
}

#[test]
fn a_crouching_player_is_heard_three_times_closer() {
    let run = |crouch: bool| {
        let mut h = harness();
        h.cfg.time_of_day = MIDNIGHT;
        let hare = h.spawn_mob("t_byte", 0.5, 65.0, 0.5);
        // De nuit à 26 blocs : la vue (14) et l'odorat (24) ne portent pas, l'ouïe (28) si.
        h.tick();
        let p = h.spawn_player(DVec3::new(0.5, 65.0, -25.5), GameMode::Survival);
        if crouch {
            h.sim.entities.insert(p, nb_ecs::Crouching);
        }
        h.ticks(12);
        brain(&h, hare).percepts.threat.map(|t| t.sense)
    };
    assert_eq!(run(false), Some(nb_ecs::brain::senses::Sense::Hearing));
    assert_eq!(run(true), None);
}

#[test]
fn white_coat_at_the_winter_solstice() {
    let mut h = harness();
    let cal = Calendar::new(h.cfg.days_per_month);
    h.cfg.calendar_day = cal.day_for(2, 0);
    let hare = h.spawn_mob("t_hare", 0.5, 65.0, 0.5);
    let ren = h.spawn_mob("t_ren", 4.5, 65.0, 0.5);
    h.tick();
    assert!(brain(&h, hare).winter_coat, "lièvre blanc au solstice");
    assert!(
        !brain(&h, ren).winter_coat,
        "le renne n'a pas de mue blanche"
    );
    set_month(&mut h, 2, 6);
    h.tick();
    assert!(!brain(&h, hare).winter_coat, "lièvre brun en juillet");
}

#[test]
fn lemming_peak_year_and_lagging_predators() {
    let book = BehaviorBook::builtin();
    let lemming = book.get("gnagare").unwrap();
    let owl = book.get("nattrovfagel").unwrap();
    let at = |year| Clock {
        month: 5,
        year,
        sun: 1.0,
        year_fraction: 0.4,
    };
    let lem: Vec<f32> = (1..=4)
        .map(|y| clock::cycle_factor(&book, lemming, &at(y)))
        .collect();
    assert_eq!(lem, [0.2, 1.0, 4.0, 0.5], "lämmelår la troisième année");
    assert_eq!(clock::cycle_factor(&book, owl, &at(3)), 1.0);
    assert_eq!(
        clock::cycle_factor(&book, owl, &at(4)),
        4.0,
        "harfangs partout l'année d'après"
    );
    let ren = book.get("hjord").unwrap();
    assert_eq!(clock::cycle_factor(&book, ren, &at(3)), 1.0);
}

#[test]
fn a_pack_encircles_its_prey() {
    let mut h = harness();
    let prey = h.spawn_mob("t_byte", 0.5, 65.0, 0.5);
    let wolves: Vec<EntityId> = [(-9.5, -3.5), (-10.5, 0.5), (-9.5, 4.5)]
        .iter()
        .map(|&(x, z)| h.spawn_mob("t_varg", x, 65.0, z))
        .collect();
    h.tick();
    for &w in &wolves {
        edit(&mut h, w, |b| {
            b.needs.hunger = 1.0;
            b.needs.fatigue = 0.0;
        });
    }
    secs(&mut h, 4.0);
    let p = h.t(prey).pos;
    let mut angles: Vec<f64> = wolves
        .iter()
        .map(|&w| {
            let b = brain(&h, w);
            assert_eq!(b.state, StateKind::Encircle, "scores {:?}", b.scores);
            assert_eq!(b.percepts.pack_size, 3);
            let t = h.t(w).pos;
            let d = hdist(t, p);
            assert!((3.5..=7.0).contains(&d), "loup à {d} de la proie");
            (t.z - p.z).atan2(t.x - p.x)
        })
        .collect();
    angles.sort_by(f64::total_cmp);
    let gaps: Vec<f64> = (0..3)
        .map(|i| {
            let next = if i == 2 {
                angles[0] + std::f64::consts::TAU
            } else {
                angles[i + 1]
            };
            next - angles[i]
        })
        .collect();
    assert!(
        gaps.iter().all(|&g| g < 2.6),
        "la meute entoure la proie : écarts {gaps:?}"
    );
}

#[test]
fn hungry_wolves_end_up_killing() {
    let mut h = harness();
    let prey = h.spawn_mob("t_byte", 0.5, 65.0, 0.5);
    let w = h.spawn_mob("t_varg", -6.5, 65.0, 0.5);
    h.tick();
    edit(&mut h, w, |b| b.needs.hunger = 1.0);
    secs(&mut h, 15.0);
    assert!(
        !h.sim.entities.contains(prey) || !h.h(prey).alive,
        "la proie est tuée"
    );
    assert!(brain(&h, w).needs.hunger < 0.5, "le loup a mangé");
}

#[test]
fn bears_hibernate_in_their_den() {
    let mut h = harness();
    set_month(&mut h, 3, 0);
    let bear = h.spawn_mob("t_bjorn", 0.5, 65.0, 0.5);
    h.tick();
    h.sim.entities.get_mut::<Transform>(bear).unwrap().pos = DVec3::new(12.5, 65.0, 0.5);
    secs(&mut h, 12.0);
    let b = brain(&h, bear);
    assert_eq!(b.state, StateKind::Hibernate);
    assert!(b.hidden, "tapi dans la tanière");
    assert!(hdist(h.t(bear).pos, DVec3::new(0.5, 65.0, 0.5)) <= 2.0);
    // Au printemps, l'ours sort.
    set_month(&mut h, 3, 5);
    secs(&mut h, 3.0);
    let b = brain(&h, bear);
    assert_ne!(b.state, StateKind::Hibernate);
    assert!(!b.hidden);
}

#[test]
fn rutting_moose_charge_instead_of_fleeing() {
    let run = |month: usize| {
        let mut h = harness();
        set_month(&mut h, 1, month);
        let alg = h.spawn_mob("t_alg", 0.5, 65.0, 0.5);
        h.tick();
        let p = h.spawn_player(DVec3::new(3.5, 65.0, 0.5), GameMode::Survival);
        secs(&mut h, 0.5);
        let s = brain(&h, alg).state;
        secs(&mut h, 2.0);
        (s, h.h(p).health < h.h(p).max_health)
    };
    let (feb, hurt_feb) = run(1);
    assert_eq!(feb, StateKind::Flee);
    assert!(!hurt_feb);
    let (sep, hurt_sep) = run(8);
    assert_eq!(sep, StateKind::Charge);
    assert!(hurt_sep, "l'élan en rut charge le joueur");
}

#[test]
fn being_hit_makes_a_brave_animal_charge() {
    let mut h = harness();
    set_month(&mut h, 1, 5);
    let bear = h.spawn_mob("t_bjorn", 0.5, 65.0, 0.5);
    let p = h.spawn_player(DVec3::new(8.5, 65.0, 0.5), GameMode::Survival);
    h.tick();
    nb_ecs::health::hit_mob_damage(
        &mut h.sim.entities,
        bear,
        Some(p),
        1.0,
        &h.mobs,
        &mut h.rngs.passive,
    );
    secs(&mut h, 0.5);
    let b = brain(&h, bear);
    assert_eq!(b.state, StateKind::Charge, "scores {:?}", b.scores);
    assert_eq!(b.provoker, Some(p));
}

#[test]
fn fire_keeps_wolves_at_bay() {
    let mut h = harness();
    let varg = h.spawn_mob("t_varg", 4.5, 65.0, 0.5);
    let fire = h.sim.entities.create();
    h.sim.entities.insert(fire, Transform::new(0.5, 65.0, 0.5));
    h.sim.entities.insert(
        fire,
        nb_ecs::Emitter {
            kind: Stimulus::Fire,
            radius: 4.0,
        },
    );
    secs(&mut h, 0.3);
    assert_eq!(brain(&h, varg).state, StateKind::Flee);
    secs(&mut h, 3.0);
    assert!(hdist(h.t(varg).pos, DVec3::new(0.5, 65.0, 0.5)) > 8.0);
}

#[test]
fn an_eagle_steals_a_fish() {
    let mut h = harness();
    let baver = h.spawn_mob("t_baver", 0.5, 65.0, 0.5);
    h.sim.entities.insert(baver, Carrying { food: 0.5 });
    let orn = h.spawn_mob("t_orn", -8.5, 65.0, 0.5);
    h.tick();
    edit(&mut h, orn, |b| {
        b.needs.hunger = 0.9;
        b.needs.fatigue = 0.0;
    });
    for _ in 0..360 {
        h.tick();
        if h.sim.entities.has::<Carrying>(orn) {
            break;
        }
    }
    assert!(
        h.sim.entities.has::<Carrying>(orn),
        "le pygargue a le poisson"
    );
    assert!(!h.sim.entities.has::<Carrying>(baver));
    assert!(h.take_events().iter().any(|e| matches!(
        e,
        EcsEvent::AnimalAct {
            act: AnimalAct::Steal,
            ..
        }
    )));
    assert!(brain(&h, baver).needs.fear > 0.5);
}

#[test]
fn reindeer_migrate_in_their_window_and_follow_their_leader() {
    let mut h = harness();
    set_month(&mut h, 1, 4);
    let lead = h.spawn_mob("t_ren", 0.5, 65.0, 0.5);
    let z0 = h.t(lead).pos.z;
    secs(&mut h, 5.0);
    assert_eq!(brain(&h, lead).state, StateKind::Migrate);
    assert!(h.t(lead).pos.z < z0 - 3.0, "printemps : cap au nord (−z)");

    let mut h = harness();
    set_month(&mut h, 1, 6);
    let lead = h.spawn_mob("t_ren", 0.5, 65.0, 0.5);
    let calf = h.spawn_mob("t_ren", 20.5, 65.0, 0.5);
    h.tick();
    let b = brain(&h, calf);
    assert_eq!(b.percepts.leader.and_then(|l| l.id), Some(lead));
    assert!(brain(&h, lead).percepts.leader.is_none());
    secs(&mut h, 6.0);
    assert_eq!(brain(&h, calf).state, StateKind::Follow);
    assert!(hdist(h.t(calf).pos, h.t(lead).pos) < 16.0);
}

#[test]
fn leaves_hide_a_player_from_sight() {
    let run = |leaves: bool| {
        let mut h = harness();
        if leaves {
            // Hors de l'ouïe et de l'odorat (vent contraire) : la vue seule, à travers un fourré.
            let l = h.ids().leaves;
            h.blocks.fill(
                nb_core::glam::IVec3::new(-8, 65, -6),
                nb_core::glam::IVec3::new(-3, 69, 6),
                l,
            );
        }
        h.environment = Some(calm_env(Wind {
            dir: [-1.0, 0.0],
            speed: 0.6,
        }));
        let ren = h.spawn_mob("t_ren", 0.5, 65.0, 0.5);
        h.tick();
        h.spawn_player(DVec3::new(-25.5, 65.0, 0.5), GameMode::Survival);
        for _ in 0..12 {
            // Le renne regarde vers −x.
            h.sim.entities.get_mut::<Transform>(ren).unwrap().yaw = -std::f32::consts::FRAC_PI_2;
            h.tick();
        }
        brain(&h, ren).percepts.threat.map(|t| t.sense)
    };
    assert_eq!(run(false), Some(nb_ecs::brain::senses::Sense::Sight));
    assert_eq!(run(true), None);
}

#[test]
fn hundred_twenty_animals_fit_the_budget() {
    let mut h = harness();
    h.stone_floor();
    h.sim.schedule = Schedule::new();
    h.sim.schedule.push(Box::new(BrainSystem::default()));
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    let kinds = [
        "t_ren", "t_alg", "t_hare", "t_varg", "t_bjorn", "t_orn", "t_lammel",
    ];
    for i in 0..120 {
        let (x, z) = ((i % 12) as f64 * 5.0 - 28.0, (i / 12) as f64 * 5.0 - 24.0);
        h.spawn_mob(kinds[i % kinds.len()], x + 0.5, 65.0, z + 0.5);
    }
    h.ticks(60);
    let start = std::time::Instant::now();
    let n = 600;
    h.ticks(n);
    let per_tick = start.elapsed().as_secs_f64() * 1000.0 / n as f64;
    eprintln!("moteur d'états : {per_tick:.3} ms par tick pour 120 animaux");
    let budget = if cfg!(debug_assertions) { 3.0 } else { 1.0 };
    assert!(per_tick < budget, "{per_tick:.3} ms > {budget} ms");
    assert!(h.sim.entities.query_ids::<&Brain>().len() >= 100);
    let _ = h.sim.entities.query_ids::<&Health>();
}
