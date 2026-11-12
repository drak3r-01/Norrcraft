//! Scénarios des animaux domestiques de Norrcraft (session 5.7) avec les fiches réelles de
//! `assets/norrcraft/mobs.json` : apprivoisement, fourrage d'hiver, lait, tonte, œufs, chien qui
//! suit son maître, marque l'arrêt et alerte.

mod common;

use common::Harness;
use nb_core::glam::DVec3;
use nb_ecs::husbandry::{self, Domestic};
use nb_ecs::{AnimalAct, BrainSystem, EcsEvent, EntityId, GameMode, HusbandrySystem};
use nb_data::{MobRegistry, WorldKind};
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
    h.sim.schedule.push(Box::new(HusbandrySystem::default()));
    h.cfg.time_of_day = NOON;
    h.cfg.fall_damage_enabled = false;
    h.cfg.mob_despawn_radius = 200;
    // Une journée de jeu dure 10 s : l'élevage se joue en secondes.
    h.cfg.day_length_seconds = 10.0;
    h
}

fn set_month(h: &mut Harness, year: u64, month: usize) {
    let cal = Calendar::new(h.cfg.days_per_month);
    let doy = (0..cal.days_per_year())
        .find(|&d| cal.date(cal.day_for(year, d)).month == month)
        .unwrap();
    h.cfg.calendar_day = cal.day_for(year, doy);
}

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s * 60.0) as usize);
}

fn dom(h: &Harness, id: EntityId) -> Domestic {
    h.sim.entities.copy::<Domestic>(id).unwrap()
}

/// Apprivoise `id` avec la friandise qui convient ; le maître est un joueur à `at`.
fn owned(h: &mut Harness, id: &str, at: DVec3, item: &str) -> (EntityId, EntityId) {
    let player = h.spawn_player(at, GameMode::Survival);
    let e = h.spawn_mob(id, at.x + 3.0, 65.0, at.z);
    assert!(husbandry::tame(&mut h.sim.entities, &h.mobs, e, player, item));
    (e, player)
}

#[test]
fn only_the_right_treat_tames() {
    let mut h = harness();
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    let cow = h.spawn_mob("fjallko", 3.5, 65.0, 0.5);
    assert!(!husbandry::tame(&mut h.sim.entities, &h.mobs, cow, player, "kott"));
    assert!(!h.sim.entities.has::<Domestic>(cow));
    assert!(husbandry::tame(&mut h.sim.entities, &h.mobs, cow, player, "salt"));
    assert!(dom(&h, cow).owner == Some(player));
    // Déjà domestique : pas de second apprivoisement.
    assert!(!husbandry::tame(&mut h.sim.entities, &h.mobs, cow, player, "salt"));
}

#[test]
fn winter_empties_the_fodder_and_a_dry_cow_wastes_away() {
    let mut h = harness();
    set_month(&mut h, 1, 0); // janvier : pas de pâture
    let (cow, _) = owned(&mut h, "fjallko", DVec3::new(0.5, 65.0, 0.5), "ho");
    let start = dom(&h, cow).fodder;
    secs(&mut h, 10.0);
    assert!(dom(&h, cow).fodder < start, "la réserve ne baisse pas");
    h.sim.entities.get_mut::<Domestic>(cow).unwrap().fodder = 0.0;
    let hp = h.h(cow).health;
    secs(&mut h, 20.0);
    assert!(h.h(cow).health < hp - 0.5, "elle ne dépérit pas ({})", h.h(cow).health);
    // Une ration de foin remonte la réserve et arrête la perte.
    assert!(husbandry::feed(&mut h.sim.entities, &h.mobs, cow, "ho"));
    assert!(dom(&h, cow).fodder >= 0.5);
    assert!(!husbandry::feed(&mut h.sim.entities, &h.mobs, cow, "sten"));
    let hp = h.h(cow).health;
    secs(&mut h, 3.0);
    assert_eq!(h.h(cow).health, hp);
}

#[test]
fn summer_pasture_refills_the_reserve() {
    let mut h = harness();
    set_month(&mut h, 1, 6); // juillet
    let (cow, _) = owned(&mut h, "fjallko", DVec3::new(0.5, 65.0, 0.5), "ho");
    h.sim.entities.get_mut::<Domestic>(cow).unwrap().fodder = 0.1;
    secs(&mut h, 10.0);
    assert!(dom(&h, cow).fodder > 0.3, "{}", dom(&h, cow).fodder);
}

#[test]
fn the_cow_gives_more_milk_at_the_summer_pasture() {
    let milk = |month: usize| {
        let mut h = harness();
        set_month(&mut h, 1, month);
        let (cow, _) = owned(&mut h, "fjallko", DVec3::new(0.5, 65.0, 0.5), "ho");
        h.sim.entities.get_mut::<Domestic>(cow).unwrap().fodder = 1.0;
        secs(&mut h, 4.0);
        let stock = dom(&h, cow).stock;
        let got = husbandry::collect(&mut h.sim.entities, &h.mobs, cow, month);
        (stock, got)
    };
    let (summer, got) = milk(6);
    let (autumn, _) = milk(9);
    assert!(summer > autumn * 1.2, "{summer} contre {autumn}");
    let (item, n) = got.expect("du lait prêt");
    assert_eq!(item, "mjolk");
    assert!(n >= 1);
}

#[test]
fn sheep_are_shorn_once_in_spring() {
    let mut h = harness();
    set_month(&mut h, 1, 1); // février
    let (sheep, _) = owned(&mut h, "gutefar", DVec3::new(0.5, 65.0, 0.5), "ho");
    {
        let d = h.sim.entities.get_mut::<Domestic>(sheep).unwrap();
        d.fodder = 1.0;
    }
    secs(&mut h, 20.0);
    // Elle pousse bien nourrie (0,25 par jour de 10 s)…
    let grown = dom(&h, sheep).stock;
    assert!((0.3..1.0).contains(&grown), "{grown}");
    // … et la tonte n'est permise qu'au printemps, toison pleine.
    h.sim.entities.get_mut::<Domestic>(sheep).unwrap().set_stock(3.0);
    assert!(husbandry::collect(&mut h.sim.entities, &h.mobs, sheep, 1).is_none());
    let (item, n) = husbandry::collect(&mut h.sim.entities, &h.mobs, sheep, 4).expect("tonte");
    assert_eq!((item.as_str(), n), ("ull", 3));
    // Rien à tondre juste après.
    assert!(husbandry::collect(&mut h.sim.entities, &h.mobs, sheep, 4).is_none());
}

#[test]
fn hens_lay_in_the_laying_season_only() {
    let eggs = |month: usize| {
        let mut h = harness();
        set_month(&mut h, 1, month);
        let (hen, _) = owned(&mut h, "hona", DVec3::new(0.5, 65.0, 0.5), "korn");
        h.sim.entities.get_mut::<Domestic>(hen).unwrap().fodder = 1.0;
        secs(&mut h, 12.0);
        dom(&h, hen).stock
    };
    assert!(eggs(5) >= 1.0);
    assert_eq!(eggs(0), 0.0);
}

#[test]
fn a_hound_follows_its_owner() {
    let mut h = harness();
    set_month(&mut h, 1, 6);
    let stone = h.ids().stone;
    h.slab(-60, 60, 64, -60, 60, stone);
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    let dog = h.spawn_mob("jamthund", 30.5, 65.0, 0.5);
    assert!(husbandry::tame(&mut h.sim.entities, &h.mobs, dog, player, "kott"));
    let before = (h.t(dog).pos - h.t(player).pos).length();
    secs(&mut h, 14.0);
    let after = (h.t(dog).pos - h.t(player).pos).length();
    assert!(after < before - 10.0, "le chien ne suit pas : {before} → {after}");
}

fn barks(events: &[EcsEvent], who: EntityId) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, EcsEvent::AnimalAct { entity, act: AnimalAct::Bark, .. } if *entity == who))
        .count()
}

#[test]
fn a_hound_points_at_game_and_barks() {
    let mut h = harness();
    set_month(&mut h, 1, 6);
    let stone = h.ids().stone;
    h.slab(-60, 60, 64, -60, 60, stone);
    let player = h.spawn_player(DVec3::new(0.5, 65.0, -4.5), GameMode::Survival);
    let dog = h.spawn_mob("jamthund", 0.5, 65.0, 0.5);
    assert!(husbandry::tame(&mut h.sim.entities, &h.mobs, dog, player, "kott"));
    h.spawn_mob("snolopare", 0.5, 65.0, 9.5);
    let mut pointed = false;
    let mut events = Vec::new();
    for _ in 0..30 {
        secs(&mut h, 0.1);
        events.extend(h.take_events());
        pointed |= dom(&h, dog).pointing.is_some();
    }
    assert!(pointed, "le chien ne marque pas l'arrêt");
    assert!(barks(&events, dog) >= 1, "le chien n'aboie pas");
    // Le chien ne tue pas le gibier : il le tient.
    assert_eq!(h.count_mobs("snolopare"), 1);
}

#[test]
fn a_hound_barks_at_a_wolf() {
    let mut h = harness();
    set_month(&mut h, 1, 6);
    let stone = h.ids().stone;
    h.slab(-60, 60, 64, -60, 60, stone);
    let player = h.spawn_player(DVec3::new(0.5, 65.0, -4.5), GameMode::Survival);
    let dog = h.spawn_mob("jamthund", 0.5, 65.0, 0.5);
    assert!(husbandry::tame(&mut h.sim.entities, &h.mobs, dog, player, "kott"));
    h.spawn_mob("varg", 0.5, 65.0, 22.5);
    let mut events = Vec::new();
    for _ in 0..30 {
        secs(&mut h, 0.1);
        events.extend(h.take_events());
    }
    assert!(barks(&events, dog) >= 1, "le chien n'alerte pas");
}

#[test]
fn a_wild_hound_is_not_a_domestic() {
    // Sans apprivoisement, aucun composant : l'élevage ne touche pas aux bêtes libres.
    let mut h = harness();
    let dog = h.spawn_mob("jamthund", 0.5, 65.0, 0.5);
    secs(&mut h, 3.0);
    assert!(!h.sim.entities.has::<Domestic>(dog));
}
