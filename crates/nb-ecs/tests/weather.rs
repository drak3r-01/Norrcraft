//! Survie dans un univers à météo (plan Norrcraft, session 3.3 ; doc 04 §3.3) : air de la saison,
//! vent, abri, mouillure, vêtements coupe-vent et imperméables, choc de l'eau glacée.

mod common;

use common::Harness;
use nb_core::glam::DVec3;
use nb_data::{ArmorSlot, ItemId, ItemRegistry};
use nb_ecs::{GameMode, ItemStack, Vitals, TICK_DT};
use nb_world::weather::{Environment, Wind};

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s / TICK_DT).round() as usize);
}

fn arena(env: Environment) -> (Harness, ItemRegistry) {
    let (mut h, items) = Harness::new().with_survival();
    h.environment = Some(env);
    h.stone_floor();
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    h.ticks(5);
    h.take_events();
    (h, items)
}

fn env(air: f32, wind: f32, rain: f32) -> Environment {
    Environment {
        air,
        wind: Wind {
            dir: [1.0, 0.0],
            speed: wind,
        },
        chill: 1.0,
        rain,
        snow: 0.0,
    }
}

fn vitals(h: &Harness) -> Vitals {
    h.sim.entities.copy::<Vitals>(h.player()).unwrap()
}

fn id(items: &ItemRegistry, n: &str) -> ItemId {
    items.id_of(n).unwrap_or_else(|| panic!("objet {n}"))
}

fn dress_in_fur(h: &mut Harness, items: &ItemRegistry) {
    for (slot, name) in [
        (ArmorSlot::Head, "fur_cap"),
        (ArmorSlot::Chest, "fur_tunic"),
        (ArmorSlot::Legs, "fur_trousers"),
        (ArmorSlot::Feet, "fur_boots"),
    ] {
        h.inv_mut()
            .set_armor(slot, Some(ItemStack::new(id(items, name), 1)));
    }
}

#[test]
fn comfortable_air_keeps_the_body_warm() {
    let (mut h, _) = arena(env(0.55, 0.0, 0.0));
    secs(&mut h, 30.0);
    assert!(vitals(&h).warmth >= 99.0, "{}", vitals(&h).warmth);
    assert!(vitals(&h).ambient > -0.1);
}

#[test]
fn wind_chills_and_a_blizzard_chills_three_times_more() {
    let still = {
        let (mut h, _) = arena(env(0.36, 0.0, 0.0));
        secs(&mut h, 20.0);
        vitals(&h)
    };
    let windy = {
        let (mut h, _) = arena(env(0.36, 0.8, 0.0));
        secs(&mut h, 20.0);
        vitals(&h)
    };
    let gale = {
        let mut e = env(0.36, 0.8, 0.0);
        e.chill = 3.0;
        let (mut h, _) = arena(e);
        secs(&mut h, 20.0);
        vitals(&h)
    };
    assert!(
        windy.ambient < still.ambient - 0.1,
        "{} vs {}",
        windy.ambient,
        still.ambient
    );
    let (d1, d3) = (still.ambient - windy.ambient, still.ambient - gale.ambient);
    assert!((d3 / d1 - 3.0).abs() < 0.05, "×{}", d3 / d1);
    assert!(gale.warmth < windy.warmth && windy.warmth <= still.warmth);
}

#[test]
fn a_wall_upwind_and_a_roof_shelter_from_the_wind() {
    let open = {
        let (mut h, _) = arena(env(0.36, 0.9, 0.0));
        secs(&mut h, 5.0);
        vitals(&h).ambient
    };
    // Mur à 2 blocs au vent (le vent souffle vers +x : l'amont est à x < 0).
    let walled = {
        let (mut h, _) = arena(env(0.36, 0.9, 0.0));
        let stone = h.ids().stone;
        for y in 65..=66 {
            for z in -2..=2 {
                h.put(-2, y, z, stone);
            }
        }
        secs(&mut h, 5.0);
        vitals(&h).ambient
    };
    let roofed = {
        let (mut h, _) = arena(env(0.36, 0.9, 0.0));
        let stone = h.ids().stone;
        h.put(0, 68, 0, stone);
        secs(&mut h, 5.0);
        vitals(&h).ambient
    };
    assert!(walled > open + 0.05, "mur {walled} vs dehors {open}");
    assert!(roofed >= walled, "toit {roofed} vs mur {walled}");
}

#[test]
fn rain_soaks_the_unprotected_and_a_roof_or_fur_helps() {
    let soaked = {
        let (mut h, _) = arena(env(0.5, 0.0, 1.0));
        secs(&mut h, 60.0);
        vitals(&h)
    };
    assert!(soaked.wetness > 0.95, "mouillure {}", soaked.wetness);
    let calm_dry = {
        let (mut h, _) = arena(env(0.5, 0.0, 0.0));
        secs(&mut h, 60.0);
        vitals(&h)
    };
    assert!(soaked.ambient < calm_dry.ambient - 0.2);

    let roofed = {
        let (mut h, _) = arena(env(0.5, 0.0, 1.0));
        let stone = h.ids().stone;
        h.put(0, 68, 0, stone);
        secs(&mut h, 60.0);
        vitals(&h).wetness
    };
    assert_eq!(roofed, 0.0, "à couvert");

    // Au bout de 20 s : la fourrure (30 % imperméable) mouille plus lentement que la peau nue.
    let after_20s = |fur: bool| {
        let (mut h, items) = arena(env(0.5, 0.0, 1.0));
        if fur {
            dress_in_fur(&mut h, &items);
        }
        secs(&mut h, 20.0);
        vitals(&h).wetness
    };
    let (bare, furred) = (after_20s(false), after_20s(true));
    assert!(furred < bare - 0.15, "fourrure {furred} vs nu {bare}");
}

#[test]
fn a_fire_dries_the_clothes_in_about_a_minute() {
    let (mut h, _) = arena(env(0.5, 0.0, 0.0));
    let camp = h.reg.get("campfire").id;
    h.put(1, 65, 0, camp);
    h.ticks(30);
    {
        let p = h.player();
        h.sim.entities.get_mut::<Vitals>(p).unwrap().wetness = 1.0;
    }
    secs(&mut h, 30.0);
    let half = vitals(&h).wetness;
    assert!(half > 0.3 && half < 0.7, "à mi-chemin {half}");
    secs(&mut h, 40.0);
    assert!(vitals(&h).wetness < 0.05, "{}", vitals(&h).wetness);

    // Loin du feu, au froid : bien plus long.
    let (mut cold, _) = arena(env(0.2, 0.0, 0.0));
    {
        let p = cold.player();
        cold.sim.entities.get_mut::<Vitals>(p).unwrap().wetness = 1.0;
    }
    secs(&mut cold, 60.0);
    assert!(vitals(&cold).wetness > 0.8);
}

#[test]
fn falling_into_icy_water_shocks() {
    let (mut h, _) = arena(env(0.2, 0.0, 0.0));
    let water = h.ids().water;
    for y in 65..=67 {
        h.put(0, y, 0, water);
    }
    h.ticks(10);
    let v = vitals(&h);
    assert!(v.wet && v.wetness > 0.99);
    assert!(v.warmth < 90.0, "chute de chaleur {}", v.warmth);
    assert!(v.stamina < 5.0, "endurance {}", v.stamina);
}

#[test]
fn without_weather_the_old_biome_rules_still_apply() {
    let (mut h, _) = Harness::new().with_survival();
    h.stone_floor();
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    secs(&mut h, 10.0);
    let v = vitals(&h);
    assert_eq!(v.wetness, 0.0);
    assert!(!v.was_in_water);
}
