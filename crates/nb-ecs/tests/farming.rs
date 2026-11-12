//! Cultures, croissance par la lumière, brûlis, fenaison, estive et kulning (session 6.4).

mod common;

use common::Harness;
use nb_core::glam::{DVec3, IVec3};
use nb_core::JavaRandom;
use nb_data::{BlockRegistry, MobRegistry, WorldKind};
use nb_ecs::farming::{
    self, burn, enclosed, fell, harvest, manure, sow, till, BurnError, Conditions, Crop,
    FarmBlocks, Soil, SowError, MANURE,
};
use nb_ecs::hay::{self, Barn, Hassja, Spell, Voice};
use nb_ecs::husbandry::{self, Domestic};
use nb_ecs::{
    BrainSystem, EcsEvent, EntityId, FarmSystem, FieldState, GameMode, HaySystem,
    HusbandrySystem, MemoryBlocks, SlashState,
};
use nb_world::calendar::Calendar;
use nb_world::weather::{freezing_point, Environment, Wind};

const NOON: f32 = 900.0;

struct Farm {
    h: Harness,
    fb: FarmBlocks,
    fields: FieldState,
    slash: SlashState,
}

fn warm() -> Environment {
    Environment {
        air: 0.95,
        wind: Wind {
            dir: [1.0, 0.0],
            speed: 0.1,
        },
        chill: 1.0,
        rain: 0.0,
        snow: 0.0,
        fog: 0.0,
        sun: 0.8,
    }
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

/// Champ de terre `jord` en y = 64 (`air` au-dessus), joueur au centre, journée de 10 s.
fn farm(month: usize) -> Farm {
    let mut h = Harness::new();
    h.reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    h.blocks = MemoryBlocks::new(h.reg.ids().air);
    h.mobs = MobRegistry::builtin_for(WorldKind::Norrcraft);
    for c in h.mobs.all_mut() {
        c.max_count = 0;
        c.spawn = None;
    }
    h.cfg.fall_damage_enabled = false;
    h.cfg.time_of_day = NOON;
    h.cfg.day_length_seconds = 10.0;
    h.cfg.mob_despawn_radius = 200;
    let soil = h.reg.get("jord").id;
    h.slab(-20, 20, 64, -20, 20, soil);
    h.environment = Some(warm());
    set_month(&mut h, 1, month);
    let fields: FieldState = Default::default();
    let slash: SlashState = Default::default();
    let fb = FarmBlocks::from_registry(&h.reg).expect("blocs de culture");
    let sys = FarmSystem::new(fields.clone(), slash.clone(), &h.reg);
    h.sim.schedule.push(Box::new(sys));
    Farm { h, fb, fields, slash }
}

fn conditions(hours: f32, warmth: f32) -> Conditions {
    Conditions {
        hours,
        air: freezing_point() as f32 + warmth,
        rain: 0.0,
    }
}

fn plot_at(f: &Farm, p: IVec3) -> farming::Plot {
    *f.fields.lock().unwrap().get(&p).expect("champ")
}

// ---------------------------------------------------------------------------------------------
// Lumière, chaleur, gel
// ---------------------------------------------------------------------------------------------

#[test]
fn nothing_grows_in_the_dark_and_midnight_sun_speeds_barley() {
    let s = Crop::Barley.spec();
    assert_eq!(farming::growth_rate(s, &conditions(4.0, 0.3), 0.6, 0.5), 0.0);
    let day = farming::growth_rate(s, &conditions(12.0, 0.3), 0.6, 0.5);
    let sun = farming::growth_rate(s, &conditions(24.0, 0.3), 0.6, 0.5);
    assert!(day > 0.0);
    assert!(sun > day * 1.3 && sun < day * 1.5, "{day} {sun}");
}

#[test]
fn daylight_is_longer_in_june_than_in_december_in_the_north() {
    let cal = Calendar::new(2);
    let a = |month: usize| {
        let doy = (0..cal.days_per_year())
            .find(|&d| cal.date(cal.day_for(1, d)).month == month)
            .unwrap();
        cal.year_fraction(cal.day_for(1, doy), 900.0)
    };
    let june = farming::daylight_hours(a(5), 0.8);
    let december = farming::daylight_hours(a(11), 0.8);
    assert!(june > 18.0 && december < 8.0, "{june} {december}");
}

#[test]
fn cold_stalls_growth_and_frost_kills_everything_but_rye() {
    let barley = Crop::Barley.spec();
    let rye = Crop::Rye.spec();
    assert_eq!(farming::growth_rate(barley, &conditions(16.0, 0.0), 0.6, 0.5), 0.0);
    assert!(farming::frost_kills(barley, &conditions(16.0, -0.05)));
    assert!(!farming::frost_kills(rye, &conditions(16.0, -0.05)));
    // Le seigle rustique avance un peu au froid.
    assert!(farming::growth_rate(rye, &conditions(10.0, -0.01), 0.6, 0.5) > 0.0);
}

#[test]
fn northern_limit_and_fertility_shape_the_rate() {
    let apple = Crop::Apple.spec();
    assert_eq!(farming::growth_rate(apple, &conditions(16.0, 0.3), 0.6, 0.9), 0.0);
    assert!(farming::growth_rate(apple, &conditions(16.0, 0.3), 0.6, 0.4) > 0.0);
    let b = Crop::Barley.spec();
    let poor = farming::growth_rate(b, &conditions(16.0, 0.3), 0.0, 0.5);
    let rich = farming::growth_rate(b, &conditions(16.0, 0.3), 1.2, 0.5);
    assert!(rich > poor * 1.9);
}

// ---------------------------------------------------------------------------------------------
// Champ : labour, semis, croissance, récolte
// ---------------------------------------------------------------------------------------------

#[test]
fn sowing_follows_the_season_and_the_latitude() {
    let mut f = farm(4);
    let pos = IVec3::new(0, 64, 0);
    assert!(till(&mut f.h.blocks, &f.fields, &f.fb, pos));
    assert_eq!(f.h.block(0, 64, 0), f.fb.tilled);
    // Pas de labour sur de la roche ni deux fois.
    assert!(!till(&mut f.h.blocks, &f.fields, &f.fb, pos));
    assert_eq!(
        sow(&mut f.h.blocks, &f.fields, &f.fb, IVec3::new(5, 64, 5), Crop::Barley, 4, 0.5),
        Err(SowError::NoField)
    );
    assert_eq!(
        sow(&mut f.h.blocks, &f.fields, &f.fb, pos, Crop::Barley, 0, 0.5),
        Err(SowError::OutOfSeason)
    );
    assert_eq!(
        sow(&mut f.h.blocks, &f.fields, &f.fb, pos, Crop::Apple, 3, 0.9),
        Err(SowError::TooNorth)
    );
    assert_eq!(sow(&mut f.h.blocks, &f.fields, &f.fb, pos, Crop::Barley, 4, 0.5), Ok(()));
    assert_eq!(f.h.block(0, 65, 0), f.fb.stage_block(farming::Form::Grain, 0));
    assert_eq!(
        sow(&mut f.h.blocks, &f.fields, &f.fb, pos, Crop::Oats, 4, 0.5),
        Err(SowError::Occupied)
    );
    // Le seigle d'hiver se sème en automne.
    assert!(Crop::Rye.spec().sow == [7, 8]);
}

#[test]
fn barley_grows_through_its_stages_and_yields_the_harvest() {
    let mut f = farm(5);
    f.h.spawn_player(DVec3::new(0.5, 65.0, 8.5), GameMode::Survival);
    let pos = IVec3::new(0, 64, 0);
    till(&mut f.h.blocks, &f.fields, &f.fb, pos);
    sow(&mut f.h.blocks, &f.fields, &f.fb, pos, Crop::Barley, 5, 0.5).unwrap();
    let mut seen = vec![f.h.block(0, 65, 0)];
    for _ in 0..60 {
        secs(&mut f.h, 10.0);
        let b = f.h.block(0, 65, 0);
        if seen.last() != Some(&b) {
            seen.push(b);
        }
        if plot_at(&f, pos).ripe() {
            break;
        }
    }
    assert!(plot_at(&f, pos).ripe(), "orge mûre : {:?}", plot_at(&f, pos));
    assert_eq!(
        seen,
        (0..3).map(|s| f.fb.stage_block(farming::Form::Grain, s)).collect::<Vec<_>>()
    );
    let mut rng = JavaRandom::new(7);
    let got = harvest(&mut f.h.blocks, &f.fields, &f.fb, pos, &mut rng).expect("récolte");
    assert_eq!(got.item, "korn");
    assert!(got.qty >= 1);
    assert_eq!(f.h.block(0, 65, 0), f.fb.air);
    // Un champ vide, moins fertile.
    let p = plot_at(&f, pos);
    assert!(p.crop.is_none() && p.fertility < farming::START_FERTILITY);
}

#[test]
fn manure_raises_fertility_and_the_yield() {
    let f = farm(5);
    let pos = IVec3::new(2, 64, 2);
    f.fields
        .lock()
        .unwrap()
        .insert(pos, farming::Plot::new(Soil::Tilled));
    assert!(manure(&f.fields, pos, MANURE));
    assert!(manure(&f.fields, pos, 5.0));
    assert!((plot_at(&f, pos).fertility - farming::MAX_FERTILITY).abs() < 1e-5);
    assert!(!manure(&f.fields, IVec3::new(9, 64, 9), MANURE));
}

#[test]
fn frost_withers_a_young_crop() {
    let mut f = farm(5);
    f.h.spawn_player(DVec3::new(0.5, 65.0, 8.5), GameMode::Survival);
    let pos = IVec3::new(0, 64, 0);
    till(&mut f.h.blocks, &f.fields, &f.fb, pos);
    sow(&mut f.h.blocks, &f.fields, &f.fb, pos, Crop::Barley, 5, 0.5).unwrap();
    secs(&mut f.h, 40.0);
    assert!(plot_at(&f, pos).growth > 0.0);
    let mut cold = warm();
    cold.air = freezing_point() as f32 - 0.1;
    f.h.environment = Some(cold);
    f.h.events.clear();
    secs(&mut f.h, 3.0);
    assert_eq!(f.h.block(0, 65, 0), f.fb.withered);
    assert!(plot_at(&f, pos).crop.is_none());
    assert!(f.h.events.iter().any(|e| matches!(e, EcsEvent::CropWithered { .. })));
}

#[test]
fn a_broken_field_is_forgotten() {
    let mut f = farm(5);
    f.h.spawn_player(DVec3::new(0.5, 65.0, 8.5), GameMode::Survival);
    let pos = IVec3::new(1, 64, 1);
    till(&mut f.h.blocks, &f.fields, &f.fb, pos);
    let jord = f.h.reg.get("jord").id;
    f.h.put(1, 64, 1, jord);
    secs(&mut f.h, 2.0);
    assert!(f.fields.lock().unwrap().get(&pos).is_none());
}

// ---------------------------------------------------------------------------------------------
// Brûlis
// ---------------------------------------------------------------------------------------------

#[test]
fn slash_and_burn_gives_a_tripled_yield_for_two_years_then_exhausts() {
    let mut f = farm(4);
    f.h.spawn_player(DVec3::new(0.5, 65.0, 12.5), GameMode::Survival);
    let site = IVec3::new(0, 64, 0);
    fell(&f.slash, site, 12);
    assert_eq!(farming::burn_radius(12), 3);
    let year_days = (12 * f.h.cfg.days_per_month) as f32;
    assert_eq!(
        burn(&mut f.h.blocks, &f.fields, &f.slash, &f.fb, IVec3::new(9, 64, 9), year_days),
        Err(BurnError::NoSlash)
    );
    assert_eq!(
        burn(&mut f.h.blocks, &f.fields, &f.slash, &f.fb, site, year_days),
        Err(BurnError::NotDry)
    );
    // Six jours de temps sec : l'abattis est prêt.
    secs(&mut f.h, 10.0 * farming::SLASH_DRY_DAYS + 2.0);
    let n = burn(&mut f.h.blocks, &f.fields, &f.slash, &f.fb, site, year_days).expect("brûlis");
    assert!(n >= 20, "{n}");
    assert_eq!(f.h.block(0, 64, 0), f.fb.ash);
    assert!(matches!(plot_at(&f, site).soil, Soil::Ash { .. }));

    // Rendement : cendre ×3 contre labour ordinaire.
    let plain = IVec3::new(8, 64, 8);
    till(&mut f.h.blocks, &f.fields, &f.fb, plain);
    for p in [site, plain] {
        sow(&mut f.h.blocks, &f.fields, &f.fb, p, Crop::Barley, 4, 0.5).unwrap();
        let mut g = f.fields.lock().unwrap();
        let e = g.get_mut(&p).unwrap();
        e.growth = 1.0;
    }
    let mut r1 = JavaRandom::new(3);
    let mut r2 = JavaRandom::new(3);
    let ash = harvest(&mut f.h.blocks, &f.fields, &f.fb, site, &mut r1).unwrap().qty;
    let ord = harvest(&mut f.h.blocks, &f.fields, &f.fb, plain, &mut r2).unwrap().qty;
    assert!(ash >= ord * 2, "cendre {ash} contre {ord}");

    // Deux ans plus tard : épuisé, puis reboisé.
    secs(&mut f.h, 10.0 * (farming::ASH_YEARS * year_days + 2.0));
    assert_eq!(f.h.block(0, 64, 0), f.fb.exhausted);
    assert_eq!(
        sow(&mut f.h.blocks, &f.fields, &f.fb, site, Crop::Barley, 4, 0.5),
        Err(SowError::Exhausted)
    );
    secs(&mut f.h, 10.0 * (farming::REFOREST_YEARS * year_days + 2.0));
    assert_eq!(f.h.block(0, 64, 0), f.fb.reforest);
    assert!(f.fields.lock().unwrap().get(&site).is_none());
}

#[test]
fn rain_stops_a_slash_from_drying() {
    let mut f = farm(5);
    f.h.spawn_player(DVec3::new(0.5, 65.0, 12.5), GameMode::Survival);
    let site = IVec3::new(0, 64, 0);
    fell(&f.slash, site, 6);
    let mut wet = warm();
    wet.rain = 0.8;
    f.h.environment = Some(wet);
    secs(&mut f.h, 30.0);
    assert_eq!(f.slash.lock().unwrap()[&site].dry_days, 0.0);
    // Les troncs frais abaissent le séchage moyen.
    f.h.environment = Some(warm());
    secs(&mut f.h, 20.0);
    let before = f.slash.lock().unwrap()[&site].dry_days;
    fell(&f.slash, site, 6);
    assert!((f.slash.lock().unwrap()[&site].dry_days - before / 2.0).abs() < 1e-4);
}

// ---------------------------------------------------------------------------------------------
// Clôtures et brouteurs
// ---------------------------------------------------------------------------------------------

fn ring_fence(f: &mut Farm, c: IVec3, r: i32) {
    let fence = f.fb.fence;
    for d in -r..=r {
        for (x, z) in [(c.x + d, c.z - r), (c.x + d, c.z + r), (c.x - r, c.z + d), (c.x + r, c.z + d)] {
            f.h.put(x, c.y + 1, z, fence);
        }
    }
}

#[test]
fn a_fence_ring_encloses_and_a_gap_does_not() {
    let mut f = farm(5);
    let c = IVec3::new(0, 64, 0);
    assert!(!enclosed(&f.h.blocks, f.fb.fence, c));
    ring_fence(&mut f, c, 3);
    assert!(enclosed(&f.h.blocks, f.fb.fence, c));
    f.h.put(3, 65, 0, f.fb.air);
    assert!(!enclosed(&f.h.blocks, f.fb.fence, c));
}

fn cow_next_to_crop(f: &mut Farm, fenced: bool) -> bool {
    let pos = IVec3::new(0, 64, 0);
    f.h.spawn_player(DVec3::new(0.5, 65.0, 15.5), GameMode::Survival);
    till(&mut f.h.blocks, &f.fields, &f.fb, pos);
    sow(&mut f.h.blocks, &f.fields, &f.fb, pos, Crop::Barley, 5, 0.5).unwrap();
    if fenced {
        ring_fence(f, pos, 4);
    }
    // La vache est dans la clôture (ou dans le champ ouvert), contre la plante.
    let cow = f.h.spawn_mob("fjallko", 0.5, 65.0, 1.0);
    for _ in 0..5 {
        f.h.sim
            .entities
            .get_mut::<nb_ecs::Transform>(cow)
            .unwrap()
            .pos = DVec3::new(0.5, 65.0, 1.0);
        secs(&mut f.h, 1.2);
    }
    plot_at(f, pos).crop.is_some()
}

#[test]
fn cattle_eat_an_open_field_but_not_a_fenced_one() {
    let mut open = farm(5);
    assert!(!cow_next_to_crop(&mut open, false), "champ ouvert broute");
    let mut shut = farm(5);
    assert!(cow_next_to_crop(&mut shut, true), "champ clos préservé");
}

// ---------------------------------------------------------------------------------------------
// Fenaison
// ---------------------------------------------------------------------------------------------

#[test]
fn mowing_is_for_summer_and_best_in_july() {
    assert_eq!(hay::mow_factor(6), 1.0);
    assert!(hay::mow_factor(5) < 1.0 && hay::mow_factor(7) < 1.0);
    assert_eq!(hay::mow_factor(1), 0.0);
    let mut f = farm(6);
    let tall = f.h.reg.get("hogt_gras").id;
    let air = f.h.reg.ids().air;
    f.h.put(3, 65, 3, tall);
    let mut rng = JavaRandom::new(1);
    assert_eq!(hay::mow(&mut f.h.blocks, &mut rng, IVec3::new(4, 65, 4), tall, air, 6), None);
    let n = hay::mow(&mut f.h.blocks, &mut rng, IVec3::new(3, 65, 3), tall, air, 6).unwrap();
    assert!((1..=2).contains(&n));
    assert_eq!(f.h.block(3, 65, 3), air);
    f.h.put(3, 65, 3, tall);
    assert_eq!(hay::mow(&mut f.h.blocks, &mut rng, IVec3::new(3, 65, 3), tall, air, 0), None);
}

#[test]
fn hay_dries_in_three_dry_days_and_rain_costs_a_day() {
    let mut r = Hassja::default();
    assert_eq!(r.load(5), 0);
    assert_eq!(r.load(5), 2, "huit brassées au plus");
    r.step(Spell::Dry, 2.0);
    assert!(!r.ready());
    // Une journée de pluie fait reculer d'un jour.
    r.step(Spell::Wet, 1.0);
    assert!((r.dry_days - 1.0).abs() < 1e-5);
    r.step(Spell::Idle, 5.0);
    assert!((r.dry_days - 1.0).abs() < 1e-5);
    r.step(Spell::Dry, 1.5);
    assert!(!r.ready());
    r.step(Spell::Dry, 1.0);
    assert!(r.ready());
    assert_eq!(r.take(), 8);
    assert_eq!(r.grass, 0);
    assert_eq!(r.take(), 0);
}

#[test]
fn hay_system_dries_racks_with_the_weather() {
    let mut h = Harness::new();
    h.reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    h.cfg.day_length_seconds = 10.0;
    h.environment = Some(warm());
    let state: nb_ecs::HayState = Default::default();
    let pos = IVec3::new(2, 65, 2);
    let mut rack = Hassja::default();
    rack.load(4);
    state.lock().unwrap().racks.insert(pos, rack);
    h.sim.schedule.push(Box::new(HaySystem::new(state.clone())));
    h.events.clear();
    secs(&mut h, 15.0);
    assert!(!state.lock().unwrap().racks[&pos].ready());
    secs(&mut h, 20.0);
    assert!(state.lock().unwrap().racks[&pos].ready());
    assert!(h.events.iter().any(|e| matches!(e, EcsEvent::HayReady { .. })));
    // La pluie défait le séchage.
    let mut wet = warm();
    wet.rain = 0.9;
    h.environment = Some(wet);
    secs(&mut h, 40.0);
    assert!(!state.lock().unwrap().racks[&pos].ready());
}

// ---------------------------------------------------------------------------------------------
// Grange, estive, kulning
// ---------------------------------------------------------------------------------------------

fn byre() -> Harness {
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
    h.cfg.day_length_seconds = 10.0;
    h
}

fn owned_cow(h: &mut Harness, at: DVec3) -> (EntityId, EntityId) {
    let player = h.spawn_player(at, GameMode::Survival);
    let e = h.spawn_mob("fjallko", at.x + 3.0, 65.0, at.z);
    assert!(husbandry::tame(&mut h.sim.entities, &h.mobs, e, player, "salt"));
    (e, player)
}

#[test]
fn herd_days_count_the_stored_hay() {
    assert_eq!(hay::herd_days(60, 4), 15);
    assert_eq!(hay::herd_days(3, 4), 0);
    assert_eq!(hay::herd_days(5, 0), u32::MAX);
}

#[test]
fn the_barn_feeds_winter_cattle_one_ration_a_day_and_runs_out() {
    let mut h = byre();
    set_month(&mut h, 1, 0);
    let (cow, _) = owned_cow(&mut h, DVec3::new(0.5, 65.0, 0.5));
    let state: nb_ecs::HayState = Default::default();
    let pos = IVec3::new(4, 65, 0);
    let mut barn = Barn::default();
    barn.store(3);
    state.lock().unwrap().barns.insert(pos, barn);
    h.sim.schedule.push(Box::new(HaySystem::new(state.clone())));
    // Quatre jours de jeu : trois bottes mangées, la quatrième journée est à sec.
    secs(&mut h, 10.0 * 4.0 + 1.0);
    assert_eq!(state.lock().unwrap().barns[&pos].hay, 0);
    let d = h.sim.entities.copy::<Domestic>(cow).unwrap();
    assert!(d.fodder > 0.0);
}

#[test]
fn a_barn_far_away_does_not_feed() {
    let mut h = byre();
    set_month(&mut h, 1, 0);
    let (_cow, _) = owned_cow(&mut h, DVec3::new(0.5, 65.0, 0.5));
    let state: nb_ecs::HayState = Default::default();
    let mut barn = Barn::default();
    barn.store(10);
    let far = IVec3::new(30, 65, 30);
    state.lock().unwrap().barns.insert(far, barn);
    h.sim.schedule.push(Box::new(HaySystem::new(state.clone())));
    secs(&mut h, 30.0);
    assert_eq!(state.lock().unwrap().barns[&far].hay, 10);
}

fn milk(h: &mut Harness, with_churn: bool) -> f32 {
    set_month(h, 1, 6); // juillet : estive
    if with_churn {
        // Une baratte `karna` dans la laiterie voisine.
        let karna = h.reg.get("karna").id;
        h.put(6, 65, 0, karna);
    }
    let (cow, _) = owned_cow(h, DVec3::new(0.5, 65.0, 0.5));
    h.sim.entities.get_mut::<Domestic>(cow).unwrap().fodder = 1.0;
    secs(h, 8.0);
    h.sim.entities.copy::<Domestic>(cow).unwrap().stock
}

#[test]
fn milk_doubles_at_a_fabod_in_summer() {
    // Registre Norrcraft (le bloc `karna` existe) dans les deux cas.
    let mut away = byre();
    away.reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    away.blocks = MemoryBlocks::new(away.reg.ids().air);
    away.stone_floor();
    let mut at = byre();
    at.reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    at.blocks = MemoryBlocks::new(at.reg.ids().air);
    at.stone_floor();
    let plain = milk(&mut away, false);
    let fabod = milk(&mut at, true);
    assert!(plain > 0.0);
    assert!(
        fabod > plain * 1.8 && fabod < plain * 2.2,
        "estive {fabod} contre {plain}"
    );
}

#[test]
fn kulning_needs_learning_and_scares_wolves_and_calls_cattle() {
    let mut h = byre();
    set_month(&mut h, 1, 6);
    let (cow, player) = owned_cow(&mut h, DVec3::new(0.5, 65.0, 0.5));
    let _ = cow;
    let mut events = Vec::new();
    // Sans apprentissage : pas de voix, mais la corne de vache marche.
    assert!(!hay::call(&mut h.sim.entities, &mut events, player, Voice::Kulning));
    assert!(!hay::learn_kulning(&mut h.sim.entities, player, false));
    assert!(hay::learn_kulning(&mut h.sim.entities, player, true));
    assert!(hay::call(&mut h.sim.entities, &mut events, player, Voice::Kulning));
    assert!(matches!(events[0], EcsEvent::Kulning { .. }));
    assert!(h.sim.entities.has::<nb_ecs::Emitter>(player));
    assert_eq!(Voice::Kulning.radius(), 96.0);
    assert!(Voice::CowHorn.radius() < 96.0 && Voice::BarkHorn.radius() < 96.0);

    // L'appel s'éteint après une minute.
    let state: nb_ecs::HayState = Default::default();
    h.sim.schedule.push(Box::new(HaySystem::new(state)));
    secs(&mut h, hay::WARD_SECS - 5.0);
    assert!(h.sim.entities.has::<hay::Calling>(player));
    secs(&mut h, 10.0);
    assert!(!h.sim.entities.has::<hay::Calling>(player));
    assert!(!h.sim.entities.has::<nb_ecs::Emitter>(player));
}

#[test]
fn the_cow_horn_works_without_the_voice() {
    let mut h = byre();
    let p = h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    let mut ev = Vec::new();
    assert!(hay::call(&mut h.sim.entities, &mut ev, p, Voice::CowHorn));
}

#[test]
fn every_crop_has_a_distinct_item_and_a_block_family() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    assert!(FarmBlocks::from_registry(&reg).is_some());
    let mut items: Vec<&str> = farming::SPECS.iter().map(|s| s.item).collect();
    items.sort_unstable();
    items.dedup();
    assert_eq!(items.len(), 13);
    for (i, s) in farming::SPECS.iter().enumerate() {
        assert_eq!(s.crop as usize, i);
        assert_eq!(Crop::of_item(s.item), Some(s.crop));
    }
    let _ = (Crop::Rye.spec().hardy, farming::MAX_FERTILITY);
}
