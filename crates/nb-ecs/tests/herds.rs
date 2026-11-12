//! Hardes persistantes et migrations (session 5.3, doc 03 §4.4).

mod common;

use std::sync::{Arc, Mutex};

use common::Harness;
use nb_core::glam::DVec3;
use nb_ecs::{BrainSystem, EcsEvent, GameMode, HerdMember, HerdSystem, MobType, Transform};
use nb_world::calendar::Calendar;
use nb_world::herds::{HerdBook, HerdRecord};
use nb_world::BiomeInfo;

const NOON: f32 = 900.0;

/// Fraction d'année du milieu de l'été et de l'hiver.
const SUMMER: f64 = 0.55;
const WINTER: f64 = 0.04;

const RENNE: &str = r#"{"persistence": "herd", "herd": {"motion": "seasonal",
    "cell": 256, "sizeMin": 8, "sizeMax": 8, "regrowPerDay": 0.5, "wander": 0,
    "materialize": 300, "summer": ["FJALL"], "winter": ["SKOG"]}}"#;

/// Cellule (0, 0) de 256 : fjäll au nord (z < 128), forêt au sud. Le joueur est au sud-est de la
/// cellule, tourné vers +z (il tourne le dos à la harde).
fn world() -> (Harness, Arc<Mutex<HerdBook>>) {
    let mut h = Harness::new();
    let stone = h.ids().stone;
    h.slab(0, 255, 64, 0, 255, stone);
    h.cfg.mob_despawn_radius = 4000;
    h.cfg.fall_damage_enabled = false;
    h.cfg.time_of_day = NOON;
    h.blocks
        .set_biome_zone((0, 255), (0, 127), BiomeInfo::named("FJALL"));
    h.blocks
        .set_biome_zone((0, 255), (128, 255), BiomeInfo::named("SKOG"));
    let book = Arc::new(Mutex::new(HerdBook::new()));
    h.sim
        .schedule
        .push(Box::new(HerdSystem::new(42, book.clone())));
    h.sim.schedule.push(Box::new(BrainSystem::default()));
    (h, book)
}

fn herd_mob(h: &mut Harness, id: &str, rules: &str) {
    let json = format!(
        r#"{{"mobs": [{{"id": "{id}", "maxCount": 400, "groupSizeMin": 1, "groupSizeMax": 1,
            "behavior": "hjord", "spawn": {rules}}}]}}"#
    );
    h.mobs.apply_overrides(&json).unwrap();
    assert!(h.mobs.get(id).unwrap().spawn.is_some(), "règles refusées");
}

fn player_at(h: &mut Harness, x: f64, z: f64) {
    if h.sim.player.is_none() {
        h.spawn_player(DVec3::new(x, 65.0, z), GameMode::Survival);
    } else {
        let p = h.player();
        h.sim.entities.get_mut::<Transform>(p).unwrap().pos = DVec3::new(x, 65.0, z);
    }
}

fn set_season(h: &mut Harness, year: u64, frac: f64) {
    let cal = Calendar::new(h.cfg.days_per_month);
    let dist = |d: u32| {
        let f = cal.year_fraction(cal.day_for(year, d), 0.0);
        let x = (f - frac).abs();
        x.min(1.0 - x)
    };
    let d = (0..cal.days_per_year())
        .min_by(|&a, &b| dist(a).total_cmp(&dist(b)))
        .unwrap();
    h.cfg.calendar_day = cal.day_for(year, d);
}

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s * 60.0) as usize);
}

fn members(h: &Harness) -> Vec<(nb_ecs::EntityId, DVec3)> {
    let w = &h.sim.entities;
    w.query_ids::<&HerdMember>()
        .into_iter()
        .filter_map(|id| Some((id, w.copy::<Transform>(id)?.pos)))
        .collect()
}

fn centroid(m: &[(nb_ecs::EntityId, DVec3)]) -> DVec3 {
    m.iter().fold(DVec3::ZERO, |a, p| a + p.1) / m.len() as f64
}

fn recorded(book: &Arc<Mutex<HerdBook>>) -> Option<f32> {
    book.lock().unwrap().count("ren", 0, 0)
}

#[test]
fn a_herd_materialises_near_the_player_with_its_full_headcount() {
    let (mut h, book) = world();
    herd_mob(&mut h, "ren", RENNE);
    set_season(&mut h, 2, SUMMER);
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 3.0);
    let m = members(&h);
    assert_eq!(m.len(), 8, "effectif plein de la harde");
    assert!(m.iter().all(|(id, _)| {
        h.sim.entities.get::<MobType>(*id).unwrap().mob_id == "ren"
            && *h.sim.entities.get::<HerdMember>(*id).unwrap()
                == HerdMember {
                    species: "ren".into(),
                    cx: 0,
                    cz: 0,
                }
    }));
    // En été la harde est sur le fjäll (z < 128), pas en forêt.
    assert!(centroid(&m).z < 128.0, "centre {:?}", centroid(&m));
    assert_eq!(recorded(&book), Some(8.0));
    // Ni la densité ni l'apparition d'origine ne rajoutent d'individus.
    secs(&mut h, 20.0);
    assert_eq!(members(&h).len(), 8);
}

#[test]
fn nothing_appears_when_the_player_is_far_away() {
    let (mut h, _book) = world();
    herd_mob(&mut h, "ren", RENNE);
    set_season(&mut h, 2, SUMMER);
    player_at(&mut h, 128.5, 2000.5);
    secs(&mut h, 5.0);
    assert!(members(&h).is_empty());
}

#[test]
fn a_cell_without_the_species_biomes_has_no_herd() {
    let (mut h, book) = world();
    let rules = RENNE.replace("FJALL", "LAVA").replace("SKOG", "MER");
    herd_mob(&mut h, "ren", &rules);
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 5.0);
    assert!(members(&h).is_empty());
    assert_eq!(recorded(&book), None);
}

#[test]
fn leaving_reabsorbs_the_herd_and_losses_are_kept() {
    let (mut h, book) = world();
    herd_mob(&mut h, "ren", RENNE);
    set_season(&mut h, 2, SUMMER);
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 3.0);
    let ids: Vec<_> = members(&h).into_iter().map(|m| m.0).collect();
    assert_eq!(ids.len(), 8);
    // Le chasseur tue trois rennes.
    for &id in &ids[..3] {
        h.sim.entities.destroy(id);
    }
    secs(&mut h, 2.0);
    assert_eq!(recorded(&book), Some(5.0), "les pertes sont enregistrées");
    assert_eq!(members(&h).len(), 5);

    // Il s'éloigne : les cinq survivants sont réabsorbés.
    h.events.clear();
    player_at(&mut h, 128.5, 1500.5);
    secs(&mut h, 3.0);
    assert!(members(&h).is_empty());
    assert!(h
        .events
        .iter()
        .any(|e| matches!(e, EcsEvent::MobDespawned { .. })));
    assert_eq!(recorded(&book), Some(5.0));

    // Il revient aussitôt : cinq individus, pas huit.
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 3.0);
    assert_eq!(members(&h).len(), 5);
}

#[test]
fn the_herd_regrows_while_nobody_looks() {
    let (mut h, book) = world();
    herd_mob(&mut h, "ren", RENNE);
    set_season(&mut h, 2, SUMMER);
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 3.0);
    let ids: Vec<_> = members(&h).into_iter().map(|m| m.0).collect();
    for &id in &ids[..4] {
        h.sim.entities.destroy(id);
    }
    secs(&mut h, 2.0);
    player_at(&mut h, 128.5, 1500.5);
    secs(&mut h, 3.0);
    assert_eq!(recorded(&book), Some(4.0));
    // Quatre jours de jeu plus tard : +2 (0,5 par jour).
    h.cfg.calendar_day += 4;
    secs(&mut h, 2.0);
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 3.0);
    let n = members(&h).len();
    assert!((5..=7).contains(&n), "{n} individus après la repousse");
}

#[test]
fn the_herd_moves_to_the_forest_for_the_winter() {
    let (mut h, _book) = world();
    herd_mob(&mut h, "ren", RENNE);
    set_season(&mut h, 2, SUMMER);
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 3.0);
    let summer = centroid(&members(&h));
    // L'automne passe sans témoin, puis l'hiver.
    player_at(&mut h, 128.5, 1500.5);
    secs(&mut h, 3.0);
    assert!(members(&h).is_empty());
    set_season(&mut h, 2, WINTER);
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 3.0);
    let winter = centroid(&members(&h));
    assert!(
        summer.z < 128.0 && winter.z >= 128.0,
        "{summer:?} → {winter:?}"
    );
}

#[test]
fn herd_members_get_their_calendar_destination_when_they_stray() {
    let (mut h, _book) = world();
    herd_mob(&mut h, "ren", RENNE);
    set_season(&mut h, 2, SUMMER);
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 3.0);
    // On éloigne un renne de plus de la laisse : il reçoit une destination.
    let (id, pos) = members(&h)[0];
    let far = DVec3::new(pos.x, pos.y, (pos.z + 60.0).min(250.0));
    h.sim.entities.get_mut::<Transform>(id).unwrap().pos = far;
    secs(&mut h, 1.2);
    let goal = h.sim.entities.get::<nb_ecs::Brain>(id).unwrap().goal;
    assert!(goal.is_some(), "pas de destination pour un égaré");
    assert!(goal.unwrap().z < 128.0, "la destination est le fjäll");
}

#[test]
fn the_roster_survives_a_save_and_a_new_session() {
    let (mut h, book) = world();
    herd_mob(&mut h, "ren", RENNE);
    set_season(&mut h, 2, SUMMER);
    player_at(&mut h, 128.5, 250.5);
    secs(&mut h, 3.0);
    let ids: Vec<_> = members(&h).into_iter().map(|m| m.0).collect();
    for &id in &ids[..2] {
        h.sim.entities.destroy(id);
    }
    secs(&mut h, 2.0);
    // Sauvegarde en cours de partie, puis rechargement dans un monde neuf.
    let rec: Vec<HerdRecord> = book.lock().unwrap().records();
    assert_eq!(rec.len(), 1);
    let (mut h2, book2) = world();
    *book2.lock().unwrap() = HerdBook::from_records(rec);
    herd_mob(&mut h2, "ren", RENNE);
    set_season(&mut h2, 2, SUMMER);
    h2.cfg.calendar_day = h.cfg.calendar_day;
    player_at(&mut h2, 128.5, 250.5);
    secs(&mut h2, 3.0);
    assert_eq!(
        members(&h2).len(),
        6,
        "les deux pertes survivent au rechargement"
    );
}

#[test]
fn herd_materialisation_is_deterministic() {
    let run = || {
        let (mut h, _b) = world();
        herd_mob(&mut h, "ren", RENNE);
        set_season(&mut h, 2, SUMMER);
        player_at(&mut h, 128.5, 250.5);
        secs(&mut h, 3.0);
        let mut v: Vec<(i64, i64)> = members(&h)
            .iter()
            .map(|m| ((m.1.x * 100.0) as i64, (m.1.z * 100.0) as i64))
            .collect();
        v.sort_unstable();
        v
    };
    assert_eq!(run(), run());
}
