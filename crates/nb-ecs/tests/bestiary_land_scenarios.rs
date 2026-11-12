//! Scénarios du lot terrestre de Norrcraft (session 5.4) avec les fiches réelles de
//! `assets/norrcraft/mobs.json` (et non des mobs d'essai) : meute, lynx, vipère, ours blanc,
//! ours brun et saisons, cycle des lemmings et prédateurs en retard, pelage blanc.

mod common;

use common::Harness;
use nb_core::glam::DVec3;
use nb_data::behavior::{BehaviorBook, StateKind};
use nb_data::{MobRegistry, WorldKind};
use nb_ecs::brain::clock::{self, Clock};
use nb_ecs::{Brain, BrainSystem, EntityId, GameMode, MobType, Transform};
use nb_world::calendar::Calendar;
use nb_world::BiomeInfo;

const NOON: f32 = 900.0;

/// Sol de pierre ±32, le bestiaire Norrcraft, aucune apparition spontanée.
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

/// Premier jour de l'année `year` dont le mois réel est `month` (0 = januari).
fn set_month(h: &mut Harness, year: u64, month: usize) {
    let cal = Calendar::new(h.cfg.days_per_month);
    let doy = (0..cal.days_per_year())
        .find(|&d| cal.date(cal.day_for(year, d)).month == month)
        .unwrap();
    h.cfg.calendar_day = cal.day_for(year, doy);
}

/// Immobilise une espèce (proie d'essai).
fn freeze(h: &mut Harness, id: &str) {
    for c in h.mobs.all_mut().iter_mut().filter(|c| c.id == id) {
        c.move_speed_min = 0.0;
        c.move_speed_max = 0.0;
        c.player_flee_speed = 0.0;
        c.aggressor_flee_speed = 0.0;
        c.chase_speed = 0.0;
    }
}

fn brain(h: &Harness, id: EntityId) -> Brain {
    h.sim.entities.copy::<Brain>(id).unwrap()
}

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s * 60.0) as usize);
}

fn hdist(a: DVec3, b: DVec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

fn positions(h: &Harness, mob: &str) -> Vec<DVec3> {
    let w = &h.sim.entities;
    w.query_ids::<&MobType>()
        .into_iter()
        .filter(|&id| w.get::<MobType>(id).unwrap().mob_id == mob)
        .filter_map(|id| w.copy::<Transform>(id))
        .map(|t| t.pos)
        .collect()
}

#[test]
fn every_species_gets_a_brain_with_its_profile() {
    let mut h = harness();
    let book = BehaviorBook::builtin();
    let ids: Vec<String> = h.mobs.all().iter().map(|c| c.id.clone()).collect();
    for (i, id) in ids.iter().enumerate() {
        let e = h.spawn_mob(id, -20.0 + 2.0 * i as f64, 65.0, 10.5);
        h.tick();
        let b = brain(&h, e);
        let wanted = h.mobs.get(id).unwrap().behavior.clone().unwrap();
        assert_eq!(b.profile.map(usize::from), book.index_of(&wanted), "{id}");
    }
    secs(&mut h, 3.0);
}

#[test]
fn wolves_of_the_real_species_encircle_a_roe_deer() {
    let mut h = harness();
    freeze(&mut h, "radjur");
    let prey = h.spawn_mob("radjur", 0.5, 65.0, 0.5);
    let wolves: Vec<EntityId> = [(10.5, 0.5), (-5.5, 8.5), (-5.5, -7.5)]
        .iter()
        .map(|&(x, z)| h.spawn_mob("varg", x, 65.0, z))
        .collect();
    h.tick();
    for &w in &wolves {
        let b = h.sim.entities.get_mut::<Brain>(w).unwrap();
        b.needs.hunger = 1.0;
        b.needs.fatigue = 0.0;
    }
    secs(&mut h, 4.0);
    let p = h.t(prey).pos;
    for &w in &wolves {
        let b = brain(&h, w);
        assert_eq!(b.state, StateKind::Encircle, "scores {:?}", b.scores);
        assert_eq!(b.percepts.pack_size, 3);
        let d = hdist(h.t(w).pos, p);
        assert!((3.0..=8.0).contains(&d), "loup à {d} du chevreuil");
    }
    // Puis la meute finit par tuer le chevreuil.
    secs(&mut h, 14.0);
    assert!(
        !h.sim.entities.contains(prey) || !h.h(prey).alive,
        "le chevreuil est tué"
    );
}

#[test]
fn a_lynx_never_attacks_the_player_and_runs_when_seen() {
    let mut h = harness();
    let stone = h.ids().stone;
    h.slab(-90, 90, 64, -90, 90, stone);
    // Hors de son heure, une espèce crépusculaire hors de vue quitte le monde : on la garde.
    for c in h.mobs.all_mut().iter_mut().filter(|c| c.id == "lo") {
        c.spawn.as_mut().unwrap().time_of_day = nb_data::behavior::Activity::Always;
    }
    let lynx = h.spawn_mob("lo", 0.5, 65.0, 0.5);
    h.ticks(2);
    let player = h.spawn_player(DVec3::new(6.5, 65.0, 0.5), GameMode::Survival);
    let hp = h.h(player).health;
    secs(&mut h, 0.6);
    assert_eq!(brain(&h, lynx).state, StateKind::Flee, "lynx vu");
    secs(&mut h, 5.0);
    assert!(
        hdist(h.t(lynx).pos, h.t(player).pos) > 10.0,
        "le lynx s'éloigne"
    );
    assert_eq!(h.h(player).health, hp, "le lynx ne mord pas");
}

#[test]
fn a_viper_bites_whoever_steps_on_it() {
    let mut h = harness();
    freeze(&mut h, "huggorm");
    let snake = h.spawn_mob("huggorm", 0.5, 65.0, 0.5);
    h.ticks(2);
    // Juillet : la vipère se chauffe, elle n'hiberne pas.
    set_month(&mut h, 2, 6);
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.9), GameMode::Survival);
    let hp = h.h(player).health;
    secs(&mut h, 4.0);
    assert!(
        h.h(player).health < hp,
        "pas de morsure, état {:?}",
        brain(&h, snake).state
    );
}

#[test]
fn a_polar_bear_stalks_the_player_on_the_pack_ice() {
    let mut h = harness();
    let ice = h.reg.get("ice").id;
    h.slab(-32, 32, 64, -32, 32, ice);
    let bear = h.spawn_mob("isbjorn", 0.5, 65.0, 0.5);
    h.ticks(2);
    let player = h.spawn_player(DVec3::new(13.5, 65.0, 0.5), GameMode::Survival);
    h.sim.entities.get_mut::<Brain>(bear).unwrap().needs.hunger = 1.0;
    // Il traque, poursuit puis attaque : l'ours blanc est le seul animal qui chasse le joueur.
    let mut seen = Vec::new();
    for _ in 0..80 {
        secs(&mut h, 0.5);
        let state = brain(&h, bear).state;
        if seen.last() != Some(&state) {
            seen.push(state);
        }
        if state == StateKind::Attack {
            break;
        }
    }
    assert_eq!(
        &seen[..],
        &[StateKind::Stalk, StateKind::Pursue, StateKind::Attack],
        "l'ours blanc ne va pas jusqu'à l'attaque"
    );
    let _ = player;
}

#[test]
fn a_real_brown_bear_is_absent_in_winter_and_present_in_summer() {
    for (frac, present) in [(0.1, false), (0.5, true)] {
        let mut h = harness();
        let stone = h.ids().stone;
        h.slab(-80, 80, 64, -80, 80, stone);
        h.cfg.mob_spawn_radius = 60;
        h.cfg.mob_despawn_radius = 200;
        h.blocks.set_biome(Some(BiomeInfo::named("URSKOG")));
        for c in h.mobs.all_mut().iter_mut().filter(|c| c.id == "bjorn") {
            c.max_count = 400;
            // 0,04 par 4096 blocs est trop rare pour un disque d'essai de 11 000 blocs².
            for d in c.spawn.as_mut().unwrap().density_per_biome.values_mut() {
                *d *= 100.0;
            }
        }
        let cal = Calendar::new(h.cfg.days_per_month);
        let day = (0..cal.days_per_year())
            .min_by(|&a, &b| {
                let f = |d: u32| {
                    let x = (cal.year_fraction(cal.day_for(2, d), 0.0) - frac).abs();
                    x.min(1.0 - x)
                };
                f(a).total_cmp(&f(b))
            })
            .unwrap();
        h.cfg.calendar_day = cal.day_for(2, day);
        h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
        secs(&mut h, 40.0);
        let n = positions(&h, "bjorn").len();
        if present {
            assert!(n >= 3, "ours brun l'été : {n}");
        } else {
            assert_eq!(n, 0, "ours brun en plein hiver");
        }
    }
}

#[test]
fn lemming_predators_peak_one_year_after_the_lemmings() {
    let h = harness();
    let book = BehaviorBook::builtin();
    let factor = |species: &str, year: u64| {
        let name = h.mobs.get(species).unwrap().behavior.clone().unwrap();
        let p = book.get(&name).unwrap();
        clock::cycle_factor(
            &book,
            p,
            &Clock {
                month: 5,
                year,
                sun: 1.0,
                year_fraction: 0.4,
            },
        )
    };
    assert_eq!(factor("fjallammel", 3), 4.0, "lämmelår");
    assert_eq!(factor("fjallammel", 1), 0.2);
    for predator in ["fjallrav", "hermelin"] {
        assert_eq!(factor(predator, 3), 1.0, "{predator} l'année du pic");
        assert_eq!(factor(predator, 4), 4.0, "{predator} l'année d'après");
    }
}

#[test]
fn winter_coats_turn_white_at_the_solstice() {
    let mut h = harness();
    let cal = Calendar::new(h.cfg.days_per_month);
    h.cfg.calendar_day = cal.day_for(2, 0);
    let white: Vec<(&str, EntityId)> = ["snolopare", "hermelin", "fjallrav"]
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, h.spawn_mob(id, 0.5 + 3.0 * i as f64, 65.0, 0.5)))
        .collect();
    let brown = h.spawn_mob("alg", -6.5, 65.0, 0.5);
    h.tick();
    for (id, e) in white {
        assert!(brain(&h, e).winter_coat, "{id} blanc au solstice");
    }
    assert!(!brain(&h, brown).winter_coat, "l'élan ne blanchit pas");
}
