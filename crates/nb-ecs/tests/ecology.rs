//! Interactions de la faune avec le monde, traces, piégeage, dépeçage, venin et naturfrid
//! (session 5.8, doc 03 §4.5 et §5) avec les fiches réelles de `assets/norrcraft/mobs.json`.

mod common;

use common::Harness;
use nb_core::glam::{DVec3, IVec3};
use nb_core::JavaRandom;
use nb_data::behavior::{BehaviorBook, StateKind};
use nb_data::ecology::{Tool, TrapKind};
use nb_data::{BlockId, BlockRegistry, MobRegistry, WorldKind};
use nb_ecs::density::{self, Moment};
use nb_ecs::ecology::{self, Carcass, EcologyHandle, EcologySystem, Poisoned, Standing};
use nb_ecs::health::hit_mob_damage;
use nb_ecs::{Brain, BrainSystem, EntityId, GameMode, Health, MemoryBlocks, MobType, Transform, Velocity};
use nb_world::calendar::Calendar;

const NOON: f32 = 900.0;

/// Monde de Norrcraft : sol de granit ±32, écologie avant la santé, aucune apparition spontanée.
fn world() -> (Harness, EcologyHandle) {
    let mut h = Harness::new();
    h.reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    h.blocks = MemoryBlocks::new(h.reg.ids().air);
    h.mobs = MobRegistry::builtin_for(WorldKind::Norrcraft);
    for c in h.mobs.all_mut() {
        c.max_count = 0;
        // Sans règles d'apparition, l'animal d'essai ne disparaît pas hors saison.
        c.spawn = None;
    }
    let granite = b(&h, "granit");
    h.slab(-32, 32, 64, -32, 32, granite);
    h.cfg.time_of_day = NOON;
    h.cfg.fall_damage_enabled = false;
    h.cfg.mob_despawn_radius = 200;
    let handle = EcologyHandle::default();
    let at = h
        .sim
        .schedule
        .names()
        .iter()
        .position(|n| *n == "Health")
        .unwrap();
    h.sim
        .schedule
        .insert(at, Box::new(EcologySystem::new(handle.clone())));
    (h, handle)
}

fn b(h: &Harness, name: &str) -> BlockId {
    let id = h.reg.get(name).id;
    assert_ne!(id, h.reg.ids().air, "bloc {name} inconnu");
    id
}

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s * 60.0) as usize);
}

fn set_month(h: &mut Harness, month: usize) {
    let cal = Calendar::new(h.cfg.days_per_month);
    let doy = (0..cal.days_per_year())
        .find(|&d| cal.date(cal.day_for(1, d)).month == month)
        .unwrap();
    h.cfg.calendar_day = cal.day_for(1, doy);
}

fn brain_mut(h: &mut Harness, id: EntityId) -> &mut Brain {
    h.sim.entities.get_mut::<Brain>(id).unwrap()
}

fn sheet_mut<'a>(h: &'a mut Harness, id: &str) -> &'a mut nb_data::Ecology {
    h.mobs
        .all_mut()
        .iter_mut()
        .find(|c| c.id == id)
        .and_then(|c| c.ecology.as_mut())
        .unwrap()
}

fn carcasses(h: &Harness) -> Vec<(EntityId, String)> {
    let w = &h.sim.entities;
    w.query_ids::<&Carcass>()
        .into_iter()
        .map(|id| (id, w.get::<Carcass>(id).unwrap().mob.clone()))
        .collect()
}

fn standing(h: &Harness, p: EntityId) -> f32 {
    h.sim.entities.get::<Standing>(p).map_or(0.0, |s| s.score)
}

// ---------------------------------------------------------------------------------------------
// Broutage
// ---------------------------------------------------------------------------------------------

#[test]
fn moose_browse_the_bilberries_but_not_behind_a_fence() {
    let (mut h, _) = world();
    let bilberry = b(&h, "blabar");
    let fence = b(&h, "staket");
    let air = h.reg.ids().air;
    // Un buisson libre à droite, un autre derrière une clôture à gauche.
    h.put(1, 65, 0, bilberry);
    h.put(-8, 65, 0, bilberry);
    h.put(-10, 65, 0, fence);
    let free = h.spawn_mob("alg", 0.5, 65.0, 0.5);
    let fenced = h.spawn_mob("alg", -7.5, 65.0, 0.5);
    for e in [free, fenced] {
        brain_mut(&mut h, e).state = StateKind::Graze;
    }
    secs(&mut h, 40.0);
    assert_eq!(h.block(1, 65, 0), air, "l'élan a mangé le buisson libre");
    assert_eq!(h.block(-8, 65, 0), bilberry, "la clôture protège la pousse");
    assert_eq!(h.block(-10, 65, 0), fence);
}

#[test]
fn an_elk_leaves_droppings_and_debarks_trees() {
    let (mut h, st) = world();
    let bilberry = b(&h, "blabar");
    let spruce = b(&h, "gran_stam");
    for y in 65..70 {
        h.put(2, y, 0, spruce);
    }
    for x in -6..=6 {
        h.put(x, 65, 1, bilberry);
    }
    let e = h.spawn_mob("alg", 0.5, 65.0, 0.5);
    brain_mut(&mut h, e).state = StateKind::Graze;
    sheet_mut(&mut h, "alg").tracks.as_mut().unwrap().droppings = 1.0;
    secs(&mut h, 120.0);
    let kinds: Vec<_> = st.lock().unwrap().signs().map(|s| s.kind).collect();
    assert!(kinds.contains(&ecology::SignKind::Dropping), "crottes : {kinds:?}");
    assert!(kinds.contains(&ecology::SignKind::Debarked), "arbre écorcé : {kinds:?}");
}

// ---------------------------------------------------------------------------------------------
// Castor et barrage
// ---------------------------------------------------------------------------------------------

#[test]
fn a_beaver_colony_fells_the_nearest_living_tree_up_to_its_cap() {
    let (mut h, st) = world();
    {
        let f = sheet_mut(&mut h, "baver").fell.as_mut().unwrap();
        f.every_secs = 5.0;
        f.cap = 2;
    }
    let spruce = b(&h, "gran_stam");
    let dead = b(&h, "dod_stam");
    let birch = b(&h, "bjork_stam");
    // Un arbre mort tout près (épargné), puis trois arbres vivants.
    for y in 65..69 {
        h.put(2, y, 3, dead);
    }
    for (x, kind) in [(6, spruce), (-9, birch), (12, spruce)] {
        for y in 65..72 {
            h.put(x, y, 0, kind);
        }
    }
    let beaver = h.spawn_mob("baver", 0.5, 65.0, 0.5);
    brain_mut(&mut h, beaver).state = StateKind::Build;
    let air = h.reg.ids().air;

    secs(&mut h, 9.0);
    assert!((65..72).all(|y| h.block(6, y, 0) == air), "le plus proche tombe");
    assert_eq!(h.block(-9, 65, 0), birch, "un seul arbre à la fois");
    assert_eq!(h.block(2, 65, 3), dead, "l'arbre mort est épargné");

    secs(&mut h, 30.0);
    assert!((65..72).all(|y| h.block(-9, y, 0) == air), "le deuxième tombe");
    assert_eq!(h.block(12, 65, 0), spruce, "plafond de la colonie : 2 arbres");
    let colonies = st.lock().unwrap().colonies.clone();
    assert_eq!(colonies.len(), 1);
    assert_eq!(colonies.values().next().unwrap().felled, 2);
}

#[test]
fn breaking_the_dam_drains_the_pond_down_to_the_downstream_level() {
    let (mut h, _) = world();
    let water = h.reg.ids().water;
    let log = b(&h, "stock");
    let air = h.reg.ids().air;
    // Étang amont de 3 blocs de profondeur, digue de rondins, aval d'un bloc.
    for x in -5..=-1 {
        for z in -2..=2 {
            for y in 65..=67 {
                h.put(x, y, z, water);
            }
        }
    }
    for z in -2..=2 {
        for y in 65..=68 {
            h.put(0, y, z, log);
        }
        for x in 1..=3 {
            h.put(x, 65, z, water);
        }
    }
    // Casser une pierre ne fait rien.
    let granite = b(&h, "granit");
    assert_eq!(
        ecology::dam_broken(&mut h.blocks, &h.reg, &h.mobs, IVec3::new(0, 66, 0), granite),
        0
    );
    // Casser un rondin de la digue.
    h.put(0, 66, 0, air);
    let n = ecology::dam_broken(&mut h.blocks, &h.reg, &h.mobs, IVec3::new(0, 66, 0), log);
    assert_eq!(n, 5 * 5 * 2, "deux couches d'eau au-dessus de l'aval");
    for x in -5..=-1 {
        for z in -2..=2 {
            assert_eq!(h.block(x, 65, z), water, "le fond reste en eau");
            assert_eq!(h.block(x, 66, z), air);
            assert_eq!(h.block(x, 67, z), air);
        }
    }
    assert_eq!(h.block(2, 65, 0), water, "l'aval est intact");
}

// ---------------------------------------------------------------------------------------------
// Traces
// ---------------------------------------------------------------------------------------------

/// Fait marcher `id` le long de +x de `from` à `to` (pas de 0,5 par tick).
fn walk(h: &mut Harness, id: EntityId, from: f64, to: f64, z: f64) {
    let mut x = from;
    while x < to {
        x += 0.5;
        h.sim.entities.get_mut::<Transform>(id).unwrap().pos = DVec3::new(x, 65.0, z);
        h.sim.entities.get_mut::<Velocity>(id).unwrap().grounded = true;
        h.tick();
    }
}

#[test]
fn a_wolf_leaves_readable_prints_in_the_snow_and_none_on_bare_ground() {
    let (mut h, st) = world();
    let snow = b(&h, "snotacke_1");
    for x in 0..=20 {
        h.put(x, 65, 0, snow);
    }
    let wolf = h.spawn_mob("varg", -3.5, 65.0, 0.5);
    // Sur le sol nu d'abord : aucune trace.
    walk(&mut h, wolf, -3.5, -0.5, 0.5);
    assert_eq!(st.lock().unwrap().sign_count(), 0);
    // Puis sur la neige.
    walk(&mut h, wolf, -0.5, 18.0, 0.5);
    let guard = st.lock().unwrap();
    assert!(guard.sign_count() >= 10, "{} empreintes", guard.sign_count());
    let r = guard.read_sign(DVec3::new(9.0, 65.0, 0.5), 3.0).expect("piste lisible");
    assert_eq!(r.species, "varg");
    assert_eq!(r.print, "tass");
    assert!(r.heading.abs() < 0.2, "cap {} : le loup allait vers +x", r.heading);
    assert!(guard.read_sign(DVec3::new(9.0, 65.0, 30.0), 3.0).is_none());
}

#[test]
fn prints_fade_with_time() {
    let (mut h, st) = world();
    h.mobs
        .all_mut()
        .iter_mut()
        .find(|c| c.id == "varg")
        .and_then(|c| c.ecology.as_mut())
        .and_then(|e| e.tracks.as_mut())
        .unwrap()
        .ttl = 5.0;
    let snow = b(&h, "snotacke_1");
    for x in 0..=10 {
        h.put(x, 65, 0, snow);
    }
    let wolf = h.spawn_mob("varg", -0.5, 65.0, 0.5);
    walk(&mut h, wolf, -0.5, 8.0, 0.5);
    let here = DVec3::new(4.0, 65.0, 0.5);
    assert!(st.lock().unwrap().read_sign(here, 4.0).is_some());
    secs(&mut h, 8.0);
    assert!(st.lock().unwrap().read_sign(here, 4.0).is_none(), "la trace s'est effacée");
}

// ---------------------------------------------------------------------------------------------
// Pièges
// ---------------------------------------------------------------------------------------------

#[test]
fn snares_catch_hares_and_grouse_and_not_elk_or_far_prey() {
    let (mut h, st) = world();
    for id in ["snolopare", "alg"] {
        for c in sheet_mut(&mut h, id).catch.iter_mut() {
            c.chance = 600.0; // prise certaine à chaque passe
        }
    }
    let hare = h.spawn_mob("snolopare", 3.5, 65.0, 0.5);
    let far = h.spawn_mob("snolopare", 20.5, 65.0, 0.5);
    let elk = h.spawn_mob("alg", 3.5, 65.0, 1.5);
    let snare = st.lock().unwrap().place_trap(TrapKind::Snare, IVec3::new(3, 65, 0), true);
    secs(&mut h, 3.0);
    let mut g = st.lock().unwrap();
    assert_eq!(g.trap(snare).unwrap().catch.as_deref(), Some("snolopare"));
    drop(g);
    assert!(!h.sim.entities.contains(hare), "le lièvre est pris");
    assert!(h.sim.entities.contains(far), "trop loin du collet");
    assert!(h.sim.entities.contains(elk), "un élan ne se prend pas au collet");
    g = st.lock().unwrap();
    assert!(!g.trap(snare).unwrap().armed);
    assert_eq!(g.take_catch(snare).as_deref(), Some("snolopare"));
    assert!(g.trap(snare).unwrap().armed, "relevé, le collet est réarmé");
    assert!(g.take_catch(snare).is_none());
}

#[test]
fn pits_take_elk_and_nets_take_fish() {
    let (mut h, st) = world();
    for (id, kind) in [("alg", TrapKind::Pit), ("lax", TrapKind::Net)] {
        for c in sheet_mut(&mut h, id).catch.iter_mut().filter(|c| c.trap == kind) {
            c.chance = 600.0;
        }
    }
    let elk = h.spawn_mob("alg", 5.5, 65.0, 5.5);
    let salmon = h.spawn_mob("lax", -5.5, 65.0, -5.5);
    let (pit, net, wrong) = {
        let mut g = st.lock().unwrap();
        (
            g.place_trap(TrapKind::Pit, IVec3::new(5, 65, 5), false),
            g.place_trap(TrapKind::Net, IVec3::new(-5, 65, -5), false),
            // Un collet sous le saumon ne prend rien.
            g.place_trap(TrapKind::Snare, IVec3::new(-6, 65, -5), false),
        )
    };
    secs(&mut h, 3.0);
    assert!(!h.sim.entities.contains(elk) && !h.sim.entities.contains(salmon));
    let mut g = st.lock().unwrap();
    assert_eq!(g.take_catch(pit).as_deref(), Some("alg"));
    assert_eq!(g.take_catch(net).as_deref(), Some("lax"));
    assert!(g.trap(wrong).unwrap().catch.is_none());
}

// ---------------------------------------------------------------------------------------------
// Venin
// ---------------------------------------------------------------------------------------------

#[test]
fn a_viper_bite_poisons_over_time_and_the_right_remedy_cures_it() {
    let (mut h, _) = world();
    h.sim.schedule.push(Box::new(BrainSystem::default()));
    let venom = {
        let v = sheet_mut(&mut h, "huggorm").venom.as_mut().unwrap();
        v.chance = 1.0;
        v.clone()
    };
    assert_eq!(venom.cures, ["groblad", "serum"]);
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    let snake = h.spawn_mob("huggorm", 1.5, 65.0, 0.5);
    {
        let b = brain_mut(&mut h, snake);
        b.provoked = 30.0;
        b.provoker = Some(player);
        b.needs.fear = 1.0;
    }
    secs(&mut h, 6.0);
    let p = h.sim.entities.copy::<Poisoned>(player);
    assert!(p.is_some(), "la morsure inocule du venin");
    let health_a = h.h(player).health;
    secs(&mut h, 4.0);
    let health_b = h.h(player).health;
    assert!(health_b < health_a, "le venin agit dans le temps : {health_a} -> {health_b}");
    // Mauvais remède : rien. Bon remède : guéri.
    assert!(!ecology::cure(&mut h.sim.entities, player, &venom, "kott"));
    assert!(ecology::cure(&mut h.sim.entities, player, &venom, "groblad"));
    assert!(!h.sim.entities.has::<Poisoned>(player));
    let kept = h.h(player).health;
    secs(&mut h, 3.0);
    assert!(h.h(player).health >= kept - 2.5, "plus de venin : seules les morsures comptent");
}

#[test]
fn venom_wears_off_by_itself() {
    let (mut h, _) = world();
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    h.sim
        .entities
        .insert(player, Poisoned { remaining: 4.0, per_sec: 0.5 });
    let before = h.h(player).health;
    secs(&mut h, 8.0);
    let after = h.h(player).health;
    assert!(!h.sim.entities.has::<Poisoned>(player));
    assert!((before - after - 2.0).abs() < 0.2, "{before} -> {after} : 4 s × 0,5");
}

// ---------------------------------------------------------------------------------------------
// Carcasse, dépeçage et naturfrid
// ---------------------------------------------------------------------------------------------

fn kill(h: &mut Harness, mob: &str, player: EntityId, x: f64) -> EntityId {
    let e = h.spawn_mob(mob, x, 65.0, 0.5);
    let mut rng = JavaRandom::new(1);
    hit_mob_damage(&mut h.sim.entities, e, Some(player), 1000.0, &h.mobs, &mut rng);
    h.tick();
    e
}

#[test]
fn a_killed_elk_leaves_a_carcass_that_the_knife_skins_better() {
    let (mut h, _) = world();
    set_month(&mut h, 0); // januari : pelage d'hiver, aucune mise bas
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    let e = kill(&mut h, "alg", player, 4.5);
    assert!(!h.sim.entities.contains(e), "l'élan est retiré");
    let cs = carcasses(&h);
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].1, "alg");
    assert!(h.sim.entities.has::<nb_ecs::Emitter>(cs[0].0), "la charogne attire les charognards");
    assert!(standing(&h, player).abs() < 1e-3, "chasse hors saison de mise bas : pas de faute");

    // À mains nues : seulement la viande, en moindre quantité.
    let mut rng = JavaRandom::new(7);
    let hand = ecology::skin(&mut h.sim.entities, &h.mobs, cs[0].0, Tool::Hands, 0, &mut rng).unwrap();
    assert!(hand.iter().all(|(i, _)| i == "kott"), "{hand:?}");
    let meat_hand: u32 = hand.iter().map(|(_, n)| n).sum();

    // Au couteau en hiver : viande, os, bois, pelage d'hiver (et pas celui d'été).
    let e2 = kill(&mut h, "alg", player, 5.5);
    let _ = e2;
    let c2 = carcasses(&h)[0].0;
    let knife = ecology::skin(&mut h.sim.entities, &h.mobs, c2, Tool::Knife, 0, &mut JavaRandom::new(7)).unwrap();
    let items: Vec<&str> = knife.iter().map(|(i, _)| i.as_str()).collect();
    assert!(items.contains(&"kott") && items.contains(&"vinterskinn"), "{items:?}");
    assert!(!items.contains(&"skinn"), "pas de peau d'été en hiver : {items:?}");
    let meat_knife: u32 = knife.iter().filter(|(i, _)| i == "kott").map(|(_, n)| n).sum();
    assert!(meat_knife > meat_hand, "{meat_knife} > {meat_hand}");

    // En été, c'est la peau d'été.
    kill(&mut h, "alg", player, 6.5);
    let c3 = carcasses(&h)[0].0;
    let summer = ecology::skin(&mut h.sim.entities, &h.mobs, c3, Tool::Knife, 7, &mut JavaRandom::new(7)).unwrap();
    let items: Vec<&str> = summer.iter().map(|(i, _)| i.as_str()).collect();
    assert!(items.contains(&"skinn") && !items.contains(&"vinterskinn"), "{items:?}");
    assert!(carcasses(&h).is_empty(), "dépecée, la carcasse disparaît");
    assert!(standing(&h, player) >= -1e-3);
}

#[test]
fn wasting_a_carcass_costs_reputation_but_skinning_it_does_not() {
    let (mut h, _) = world();
    set_month(&mut h, 0);
    sheet_mut(&mut h, "alg").carcass = 5.0;
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    kill(&mut h, "alg", player, 4.5);
    kill(&mut h, "alg", player, 5.5);
    let mine = carcasses(&h);
    assert_eq!(mine.len(), 2);
    // L'une est dépecée à temps.
    ecology::skin(&mut h.sim.entities, &h.mobs, mine[0].0, Tool::Knife, 0, &mut JavaRandom::new(1)).unwrap();
    secs(&mut h, 8.0);
    assert!(carcasses(&h).is_empty());
    let s = standing(&h, player);
    assert!((s + ecology::WASTE).abs() < 0.01, "une seule carcasse perdue : {s}");
}

#[test]
fn killing_a_mother_in_the_calving_season_or_a_sacred_animal_is_a_grave_fault() {
    let (mut h, _) = world();
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    set_month(&mut h, 4); // mai : mise bas
    kill(&mut h, "alg", player, 4.5);
    let s1 = standing(&h, player);
    assert!((s1 + ecology::WITH_YOUNG).abs() < 0.01, "femelle suitée : {s1}");
    set_month(&mut h, 0);
    kill(&mut h, "bjorn", player, 6.5);
    let s2 = standing(&h, player);
    assert!(s2 < s1 - 0.3, "animal sacré : {s1} -> {s2}");
}

#[test]
fn killing_more_than_the_daily_allowance_adds_up() {
    let (mut h, _) = world();
    set_month(&mut h, 0);
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    for i in 0..ecology::DAILY_ALLOWANCE {
        kill(&mut h, "snolopare", player, 2.0 + i as f64);
    }
    assert!(standing(&h, player).abs() < 1e-3, "la ration du jour est permise");
    kill(&mut h, "snolopare", player, 8.0);
    kill(&mut h, "snolopare", player, 9.0);
    let s = standing(&h, player);
    assert!((s + 2.0 * ecology::OVERKILL * 0.6).abs() < 0.01, "{s}");
}

#[test]
fn a_hunted_down_predator_kill_by_wolves_is_nobody_s_fault() {
    let (mut h, _) = world();
    set_month(&mut h, 0);
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    let e = h.spawn_mob("alg", 4.5, 65.0, 0.5);
    h.sim.entities.get_mut::<Health>(e).unwrap().alive = false; // tué par autre chose
    h.tick();
    assert_eq!(carcasses(&h).len(), 1);
    assert!(standing(&h, player).abs() < 1e-3);
}

#[test]
fn reputation_recovers_with_the_days_and_thins_the_game() {
    let (mut h, _) = world();
    h.cfg.day_length_seconds = 10.0;
    let player = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    h.sim.entities.insert(player, Standing::with_score(-0.5));
    assert!((nature_density(&h) - 0.65).abs() < 1e-3);
    // 10 jours de jeu (100 s) rendent 10 × 0,03.
    secs(&mut h, 100.0);
    let s = standing(&h, player);
    assert!((s + 0.2).abs() < 0.03, "{s}");
    assert!(nature_density(&h) > 0.8);
}

fn nature_density(h: &Harness) -> f32 {
    ecology::nature_factor(&h.sim.entities)
}

#[test]
fn the_density_target_follows_the_reputation_for_wild_game_only() {
    let (h, _) = world();
    let mobs = MobRegistry::builtin_for(WorldKind::Norrcraft);
    let book = BehaviorBook::builtin();
    let at = |m: &Moment, id: &str| {
        let c = mobs.get(id).unwrap();
        density::climate_factor(&book, c, c.spawn.as_ref().unwrap(), m)
    };
    let mut good = Moment::of(&h.cfg, None);
    good.nature = 1.0;
    let mut bad = good;
    bad.nature = 0.3;
    let (elk_good, elk_bad) = (at(&good, "myskoxe"), at(&bad, "myskoxe"));
    assert!(elk_good > 0.0);
    assert!((elk_bad / elk_good - 0.3).abs() < 1e-3, "{elk_bad} / {elk_good}");
    // Les hardes de rennes (déjà matérialisées par leur registre) n'en dépendent pas.
    assert!((at(&bad, "ren") - at(&good, "ren")).abs() < 1e-6);
}

// ---------------------------------------------------------------------------------------------
// Cercle du bœuf musqué
// ---------------------------------------------------------------------------------------------

fn circle_world() -> Harness {
    let (mut h, _) = world();
    h.sim.schedule.push(Box::new(BrainSystem::default()));
    set_month(&mut h, 8); // septembre : ni mise bas ni rut
    for id in ["varg"] {
        for c in h.mobs.all_mut().iter_mut().filter(|c| c.id == id) {
            c.move_speed_min = 0.0;
            c.move_speed_max = 0.0;
            c.chase_speed = 0.0;
        }
    }
    h
}

#[test]
fn musk_oxen_form_a_ring_facing_the_wolf() {
    let mut h = circle_world();
    let oxen: Vec<EntityId> = [(0.5, 0.5), (3.5, 1.5), (-3.5, 0.5), (1.5, -3.5), (-1.5, 3.5)]
        .iter()
        .map(|&(x, z)| h.spawn_mob("myskoxe", x, 65.0, z))
        .collect();
    let wolf = h.spawn_mob("varg", 12.5, 65.0, 0.5);
    {
        // Le loup ne s'approche pas : seule sa présence compte.
        let b = brain_mut(&mut h, wolf);
        b.needs.hunger = 0.0;
    }
    secs(&mut h, 12.0);
    let chief = *oxen.iter().min().unwrap();
    let centre = h.t(chief).pos;
    for &o in &oxen {
        let b = h.sim.entities.copy::<Brain>(o).unwrap();
        assert_eq!(b.state, StateKind::Circle, "scores {:?}", b.scores);
        if o == chief {
            continue;
        }
        let p = h.t(o).pos;
        let d = ((p.x - centre.x).powi(2) + (p.z - centre.z).powi(2)).sqrt();
        assert!((1.6..=3.6).contains(&d), "bœuf à {d} du doyen");
    }
    // Les bœufs de la couronne sont répartis tout autour (pas tous du même côté).
    let angles: Vec<f64> = oxen
        .iter()
        .filter(|&&o| o != chief)
        .map(|&o| {
            let p = h.t(o).pos;
            (p.z - centre.z).atan2(p.x - centre.x)
        })
        .collect();
    let spread = angles.iter().cloned().fold(f64::MIN, f64::max)
        - angles.iter().cloned().fold(f64::MAX, f64::min);
    assert!(spread > 3.0, "couronne ouverte sur {spread} rad");
}

#[test]
fn a_lone_musk_ox_does_not_form_a_circle() {
    let mut h = circle_world();
    let ox = h.spawn_mob("myskoxe", 0.5, 65.0, 0.5);
    h.spawn_mob("varg", 10.5, 65.0, 0.5);
    secs(&mut h, 8.0);
    let b = h.sim.entities.copy::<Brain>(ox).unwrap();
    assert_ne!(b.state, StateKind::Circle);
    assert_eq!(b.percepts.pack_size, 1);
}

#[test]
fn the_ecology_system_is_inert_without_ecology_sheets() {
    let (mut h, st) = world();
    let sheep = h.spawn_mob("fjallko", 0.5, 65.0, 0.5);
    secs(&mut h, 5.0);
    assert!(h.sim.entities.contains(sheep));
    assert_eq!(st.lock().unwrap().sign_count(), 0);
    let _ = MobType::new("x");
}
