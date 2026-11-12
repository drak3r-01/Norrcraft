//! Bateaux, voile et vent, houle, coque, échouage, morue et stockfisch (session 6.3).

mod common;

use common::Harness;
use nb_core::glam::{DVec3, IVec3};
use nb_data::{BlockRegistry, MobRegistry, WorldKind};
use nb_ecs::boat::{
    self, Aboard, Boat, BoatKind, BoatSystem, Catch, DryingWeather, Rack, RackSystem, Sail,
};
use nb_ecs::{EcsEvent, GameAction, GameMode, MemoryBlocks};
use nb_world::weather::{Environment, Wind};

/// Lac `[-20, 20]²` profond de 3 blocs (surface en y = 64), entouré de granit.
fn lake() -> Harness {
    let mut h = Harness::new();
    h.reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    h.blocks = MemoryBlocks::new(h.reg.ids().air);
    h.mobs = MobRegistry::builtin_for(WorldKind::Norrcraft);
    for c in h.mobs.all_mut() {
        c.max_count = 0;
        c.spawn = None;
    }
    h.cfg.fall_damage_enabled = false;
    let granite = h.reg.get("granit").id;
    let water = h.reg.ids().water;
    h.slab(-40, 40, 60, -40, 40, granite);
    for y in 61..64 {
        h.slab(-20, 20, y, -20, 20, water);
    }
    // Rives : terre à la hauteur de l'eau.
    for y in 61..64 {
        for x in -40i32..=40 {
            for z in -40i32..=40 {
                if x.abs() > 20 || z.abs() > 20 {
                    h.put(x, y, z, granite);
                }
            }
        }
    }
    let at = h
        .sim
        .schedule
        .names()
        .iter()
        .position(|n| *n == "Health")
        .unwrap();
    h.sim.schedule.insert(at + 1, Box::new(BoatSystem));
    h
}

fn env(wind_dir: [f32; 2], speed: f32) -> Environment {
    Environment {
        air: 0.9,
        wind: Wind {
            dir: wind_dir,
            speed,
        },
        chill: 1.0,
        rain: 0.0,
        snow: 0.0,
        fog: 0.0,
        sun: 0.5,
    }
}

/// Bateau au centre du lac, joueur à bord (cap nord : yaw 0 = vers −z).
fn afloat(h: &mut Harness, kind: BoatKind, tarred: bool) -> (nb_ecs::EntityId, nb_ecs::EntityId) {
    let p = h.spawn_player(DVec3::new(0.5, 64.0, 0.5), GameMode::Survival);
    let b = boat::spawn_boat(&mut h.sim.entities, kind, DVec3::new(0.5, 64.0, 0.5), 0.0, tarred);
    assert!(boat::board(&mut h.sim.entities, p, b));
    (p, b)
}

fn pos_of(h: &Harness, id: nb_ecs::EntityId) -> DVec3 {
    h.sim.entities.get::<nb_ecs::Transform>(id).unwrap().pos
}

#[test]
fn sail_has_a_dead_zone_and_peaks_at_the_reach() {
    let g = Sail::Gaff;
    assert_eq!(boat::sail_efficiency(g, 0.0), 0.0);
    assert_eq!(boat::sail_efficiency(g, 44f32.to_radians()), 0.0);
    assert!(boat::sail_efficiency(g, 60f32.to_radians()) > 0.0);
    let reach = boat::sail_efficiency(g, 120f32.to_radians());
    assert!((reach - 1.0).abs() < 1e-5);
    // Vent arrière : trois quarts.
    let run = boat::sail_efficiency(g, std::f32::consts::PI);
    assert!((run - 0.75).abs() < 1e-5);
    // La voile carrée remonte moins le vent : rien à 60°, mais plus de puissance au portant.
    let s = Sail::Square;
    assert_eq!(boat::sail_efficiency(s, 60f32.to_radians()), 0.0);
    assert!(boat::sail_efficiency(s, 150f32.to_radians()) >= 1.0 - 1e-5);
}

#[test]
fn heading_into_the_wind_gives_nothing() {
    // Vent soufflant vers +x ; cap vers −x (yaw 270°) : vent debout.
    let dir = [1.0, 0.0];
    assert!((boat::wind_angle(270.0, dir)).abs() < 1e-4);
    assert_eq!(boat::sail_speed(BoatKind::Snipa, 270.0, dir, 0.8), 0.0);
    // Vent arrière (cap +x, yaw 90°).
    assert!((boat::wind_angle(90.0, dir) - std::f32::consts::PI).abs() < 1e-4);
    assert!(boat::sail_speed(BoatKind::Snipa, 90.0, dir, 0.8) > 4.0);
    // Largue : cap sud (yaw 180°) est perpendiculaire (90°), cap sud-est plus encore.
    let beam = boat::sail_speed(BoatKind::Snipa, 180.0, dir, 0.8);
    assert!(beam > 0.0);
    // Pas de voile sur une eka ni sur un radeau.
    assert_eq!(boat::sail_speed(BoatKind::Eka, 90.0, dir, 1.0), 0.0);
    assert_eq!(boat::sail_speed(BoatKind::Raft, 90.0, dir, 1.0), 0.0);
    // Plafond de vitesse du gréement.
    assert!(boat::sail_speed(BoatKind::Longship, 90.0, dir, 1.0) <= BoatKind::Longship.top_speed());
}

#[test]
fn boats_follow_the_design_table() {
    use BoatKind::*;
    assert_eq!((Raft.crew(), Raft.chests()), (1, 0));
    assert_eq!((Eka.crew(), Eka.chests()), (2, 1));
    assert_eq!((Snipa.crew(), Snipa.chests()), (3, 3));
    assert_eq!((Knarr.crew(), Knarr.chests()), (6, 8));
    assert!(Knarr.livestock() && !Snipa.livestock());
    assert_eq!(Longship.crew(), 8);
    assert_eq!(Eka.sail(), None);
    assert_eq!(Snipa.sail(), Some(Sail::Gaff));
    assert_eq!(Knarr.sail(), Some(Sail::Square));
    assert!(Longship.top_speed() > Knarr.top_speed());
    // Mêmes noms que les objets de Norrcraft.
    let items = nb_data::ItemRegistry::for_kind(WorldKind::Norrcraft, &BlockRegistry::for_kind(WorldKind::Norrcraft));
    for k in BoatKind::ALL {
        assert!(items.id_of(k.item()).is_some(), "{}", k.item());
    }
    for n in ["arar", "ostkar", "ankare", "linsegel", "hjell", "skrei", "stockfisk", "djuplina"] {
        assert!(items.id_of(n).is_some(), "{n}");
    }
}

#[test]
fn swell_needs_open_water() {
    assert_eq!(boat::swell(1.0, 0.0), 0.0);
    assert!((boat::swell(1.0, 48.0) - 1.0).abs() < 1e-6);
    assert!(boat::swell(0.5, 12.0) < boat::swell(0.5, 40.0));
    let h = lake();
    let water = h.reg.ids().water;
    // Vent vers +x : l'eau libre se compte vers −x ; depuis la rive ouest, presque rien.
    let near_west = boat::fetch(&h.blocks, water, DVec3::new(-19.5, 64.0, 0.5), [1.0, 0.0]);
    let near_east = boat::fetch(&h.blocks, water, DVec3::new(19.5, 64.0, 0.5), [1.0, 0.0]);
    assert!(near_west <= 1.0, "{near_west}");
    assert!(near_east >= 38.0, "{near_east}");
}

#[test]
fn rowing_carries_the_pilot_and_stops_at_the_shore() {
    let mut h = lake();
    let (p, b) = afloat(&mut h, BoatKind::Eka, true);
    h.hold(GameAction::Forward);
    h.ticks(60 * 4);
    let t = pos_of(&h, b);
    // Cap nord : vers −z, à peu près 3,2 blocs/s.
    assert!(t.z < -8.0 && t.z > -16.0, "{t:?}");
    assert!((t.x - 0.5).abs() < 0.5, "{t:?}");
    // Le pilote est sur le pont.
    let pp = pos_of(&h, p);
    assert!((pp.x - t.x).abs() < 1e-9 && (pp.z - t.z).abs() < 1e-9);
    assert!(pp.y > 64.0 && pp.y < 65.0);
    // Une eka accoste et se tire au sec ; un långskepp, trop lourd, bute sur la rive (z = −20,5).
    let mut g = lake();
    let (_p, ship) = afloat(&mut g, BoatKind::Longship, true);
    g.hold(GameAction::Forward);
    g.ticks(60 * 12);
    let t = pos_of(&g, ship);
    assert!(t.z > -21.0, "{t:?}");
    assert!(g.sim.entities.get::<Boat>(ship).unwrap().speed.abs() < 1.5);
    assert!(!g.sim.entities.get::<Boat>(ship).unwrap().ashore);
}

#[test]
fn rudder_turns_and_sail_moves_the_snipa() {
    let mut h = lake();
    h.environment = Some(env([0.0, -1.0], 0.7));
    // Vent soufflant vers le nord (−z) ; cap sud (yaw 180°) : vent debout.. non, vent arrière
    // pour un cap nord. Cap nord (yaw 0), voile établie : vent arrière.
    let (_p, b) = afloat(&mut h, BoatKind::Snipa, true);
    h.press(GameAction::Sprint);
    assert!(h.sim.entities.get::<Boat>(b).unwrap().sail);
    h.ticks(60 * 4);
    let v = h.sim.entities.get::<Boat>(b).unwrap().speed;
    assert!(v > 3.0, "{v}");
    assert!(pos_of(&h, b).z < -8.0);
    // Barre à droite : le cap augmente.
    h.hold(GameAction::Right);
    h.ticks(30);
    h.release(GameAction::Right);
    let yaw = h.sim.entities.get::<nb_ecs::Transform>(b).unwrap().yaw;
    assert!(yaw > 20.0, "{yaw}");
}

#[test]
fn sailing_into_the_wind_goes_nowhere() {
    let mut h = lake();
    // Vent soufflant vers le sud (+z) ; cap nord (yaw 0) : vent debout.
    h.environment = Some(env([0.0, 1.0], 0.9));
    let (_p, b) = afloat(&mut h, BoatKind::Snipa, true);
    h.press(GameAction::Sprint);
    h.ticks(60 * 5);
    assert!(pos_of(&h, b).z > -0.6, "{:?}", pos_of(&h, b));
}

#[test]
fn bare_hull_leaks_until_bailed_and_tarred_does_not() {
    let mut h = lake();
    let (_p, b) = afloat(&mut h, BoatKind::Eka, false);
    h.ticks(60 * 30);
    let w = h.sim.entities.get::<Boat>(b).unwrap().water;
    assert!(w > 0.25 && w < 0.5, "{w}");
    // En écopant, le niveau baisse.
    h.hold(GameAction::Jump);
    h.ticks(60 * 5);
    h.release(GameAction::Jump);
    assert!(h.sim.entities.get::<Boat>(b).unwrap().water < w);
    // Sans écoper, il coule et l'alerte vient avant.
    h.ticks(60 * 120);
    let ev = h.take_events();
    assert!(ev.iter().any(|e| matches!(e, EcsEvent::BoatSwamped { .. })));
    assert!(ev.iter().any(|e| matches!(e, EcsEvent::BoatSank { .. })));
    assert!(!h.sim.entities.contains(b));
    assert!(!h.sim.entities.has::<Aboard>(h.player()));

    let mut h = lake();
    let (_p, b) = afloat(&mut h, BoatKind::Eka, true);
    h.ticks(60 * 120);
    assert!(h.sim.entities.get::<Boat>(b).unwrap().water < 0.2);
    // Un radeau ne prend jamais l'eau.
    let mut h = lake();
    let (_p, b) = afloat(&mut h, BoatKind::Raft, false);
    h.ticks(60 * 120);
    assert_eq!(h.sim.entities.get::<Boat>(b).unwrap().water, 0.0);
}

#[test]
fn storm_swell_swamps_in_open_water_but_not_in_a_sheltered_cove() {
    // Au large (est du lac, vent vers +x) contre à l'abri (rive ouest).
    let mut open = lake();
    open.environment = Some(env([1.0, 0.0], 1.0));
    let p = open.spawn_player(DVec3::new(19.5, 64.0, 0.5), GameMode::Survival);
    let b = boat::spawn_boat(&mut open.sim.entities, BoatKind::Eka, DVec3::new(19.5, 64.0, 0.5), 0.0, true);
    boat::board(&mut open.sim.entities, p, b);
    open.ticks(60 * 60);
    let wet = open.sim.entities.get::<Boat>(b).unwrap().water;

    let mut cove = lake();
    cove.environment = Some(env([1.0, 0.0], 1.0));
    let p = cove.spawn_player(DVec3::new(-19.5, 64.0, 0.5), GameMode::Survival);
    let b = boat::spawn_boat(&mut cove.sim.entities, BoatKind::Eka, DVec3::new(-19.5, 64.0, 0.5), 0.0, true);
    boat::board(&mut cove.sim.entities, p, b);
    cove.ticks(60 * 60);
    let dry = cove.sim.entities.get::<Boat>(b).unwrap().water;
    assert!(wet > dry * 5.0 && wet > 0.3, "{wet} {dry}");
}

#[test]
fn anchor_holds_and_an_unattended_boat_drifts_downwind() {
    let mut h = lake();
    h.environment = Some(env([1.0, 0.0], 0.8));
    let (p, b) = afloat(&mut h, BoatKind::Snipa, true);
    h.press(GameAction::Secondary); // mouillage
    assert!(h.sim.entities.get::<Boat>(b).unwrap().anchored);
    h.press(GameAction::Crouch); // débarque
    assert!(!h.sim.entities.has::<Aboard>(p));
    h.ticks(60 * 5);
    assert!((pos_of(&h, b).x - 0.5).abs() < 0.2, "ancré : {:?}", pos_of(&h, b));
    // Sans ancre, dérive sous le vent.
    h.sim.entities.get_mut::<Boat>(b).unwrap().anchored = false;
    h.ticks(60 * 5);
    assert!(pos_of(&h, b).x > 3.0, "{:?}", pos_of(&h, b));
}

#[test]
fn boarding_needs_a_free_boat_within_reach() {
    let mut h = lake();
    let p = h.spawn_player(DVec3::new(0.5, 64.0, 0.5), GameMode::Survival);
    let far = boat::spawn_boat(&mut h.sim.entities, BoatKind::Eka, DVec3::new(10.5, 64.0, 0.5), 0.0, true);
    let near = boat::spawn_boat(&mut h.sim.entities, BoatKind::Eka, DVec3::new(2.0, 64.0, 0.5), 0.0, true);
    h.press(GameAction::Secondary);
    assert_eq!(h.sim.entities.get::<Aboard>(p).map(|a| a.0), Some(near));
    assert!(h.sim.entities.get::<Boat>(far).unwrap().pilot.is_none());
    // Place prise : un second joueur ne peut pas la prendre.
    assert!(!boat::board(&mut h.sim.entities, nb_ecs::EntityId(999), near));
}

#[test]
fn small_boats_beach_gently_and_big_ones_do_not() {
    for (kind, beaches) in [(BoatKind::Eka, true), (BoatKind::Knarr, false)] {
        let mut h = lake();
        let (_p, b) = afloat(&mut h, kind, true);
        // Cap est : rive à x = 20,5, terre à 64 (granit en y = 63, dessus à 64).
        h.sim.entities.get_mut::<nb_ecs::Transform>(b).unwrap().yaw = 90.0;
        if kind == BoatKind::Knarr {
            h.sim.entities.get_mut::<Boat>(b).unwrap().sail = false;
        }
        h.hold(GameAction::Forward);
        h.ticks(60 * 14);
        let boat_state = Boat::clone(&h.sim.entities.get::<Boat>(b).unwrap());
        assert_eq!(boat_state.ashore, beaches, "{kind:?} {:?} {:?}", pos_of(&h, b), boat_state);
        if beaches {
            assert!(pos_of(&h, b).x > 20.4);
            // Au sec, plus de fuite.
            let w = boat_state.water;
            h.ticks(60 * 30);
            assert_eq!(h.sim.entities.get::<Boat>(b).unwrap().water, w);
        } else {
            assert!(pos_of(&h, b).x < 20.0);
        }
    }
}

#[test]
fn ice_crushes_a_hull() {
    let mut h = lake();
    let ice = h.reg.get("tjockis").id;
    let (_p, b) = afloat(&mut h, BoatKind::Eka, true);
    // Quatre voisins pris par la glace à hauteur de l'eau (y = 63).
    for (dx, dz) in [(2, 0), (-2, 0), (0, 2), (0, -2)] {
        for i in 0..2 {
            let _ = i;
            h.put(dx, 63, dz, ice);
        }
    }
    h.ticks(60 * 20);
    let w = h.sim.entities.get::<Boat>(b).unwrap().water;
    assert!(w > 0.2, "{w}");
}

#[test]
fn cold_dry_windy_weather_makes_stockfish_and_warm_weather_rots_it() {
    let good = env([1.0, 0.0], 0.5);
    let mut cold = good;
    cold.air = 0.1;
    assert_eq!(boat::drying_weather(&cold), DryingWeather::Good);
    let mut wet = cold;
    wet.rain = 0.5;
    assert_eq!(boat::drying_weather(&wet), DryingWeather::Idle);
    let mut calm = cold;
    calm.wind.speed = 0.0;
    assert_eq!(boat::drying_weather(&calm), DryingWeather::Idle);
    let mut warm = good;
    warm.air = 0.95;
    assert_eq!(boat::drying_weather(&warm), DryingWeather::Warm);

    let day = 1200.0;
    let mut rack = Rack::default();
    assert_eq!(rack.hang(20), 8);
    assert_eq!(rack.fish, 12);
    // Trop tôt : rien à décrocher.
    rack.step(DryingWeather::Good, 2.0 * day, day);
    assert_eq!(rack.take(), (0, 0));
    // La pluie met le séchage en pause.
    let d = rack.dried;
    rack.step(DryingWeather::Idle, day, day);
    assert_eq!(rack.dried, d);
    rack.step(DryingWeather::Good, 2.5 * day, day);
    assert_eq!(rack.take(), (12, 0));
    assert_eq!(rack.fish, 0);

    // Le temps doux et humide fait pourrir avant le séchage.
    let mut rack = Rack::default();
    rack.hang(4);
    rack.step(DryingWeather::Warm, 2.0 * day, day);
    assert_eq!(rack.take(), (0, 4));
    // Du poisson sec ne pourrit plus.
    let mut rack = Rack::default();
    rack.hang(3);
    rack.step(DryingWeather::Good, 5.0 * day, day);
    rack.step(DryingWeather::Warm, 5.0 * day, day);
    assert_eq!(rack.take(), (3, 0));
    assert!(boat::NEVER_SPOILS.contains(&"stockfisk"));
}

#[test]
fn racks_dry_in_the_simulation() {
    let mut h = lake();
    let racks: nb_ecs::RackState = Default::default();
    h.sim.schedule.insert(0, Box::new(RackSystem::new(racks.clone())));
    let mut cold = env([1.0, 0.0], 0.5);
    cold.air = 0.1;
    h.environment = Some(cold);
    h.cfg.day_length_seconds = 10.0;
    racks.lock().unwrap().entry(IVec3::new(3, 64, 3)).or_default().hang(5);
    h.ticks(60 * 45);
    let took = racks.lock().unwrap().get_mut(&IVec3::new(3, 64, 3)).unwrap().take();
    assert_eq!(took, (5, 0));
}

#[test]
fn deep_bottom_fishing_follows_the_season() {
    // Skrei de février à avril, à 6 blocs de fond au moins.
    assert_eq!(boat::bottom_fishing(10, 3, 0.1), Some(Catch::Skrei));
    assert_eq!(boat::bottom_fishing(10, 7, 0.1), None);
    assert_eq!(boat::bottom_fishing(3, 3, 0.1), None);
    // Plus profond : flétan toute l'année, requin du Groenland rarement.
    assert_eq!(boat::bottom_fishing(16, 8, 0.2), Some(Catch::Halibut));
    assert_eq!(boat::bottom_fishing(30, 8, 0.02), Some(Catch::GreenlandShark));
    assert!(boat::skrei_season(2) && boat::skrei_season(4) && !boat::skrei_season(5));
}
