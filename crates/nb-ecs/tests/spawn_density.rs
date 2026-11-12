//! Apparition par densité cible et milieux AIR, AMPHIBIE, GLACE (session 5.2, doc 03 §1 et §4).

mod common;

use common::Harness;
use nb_core::glam::DVec3;
use nb_data::behavior::{BehaviorBook, Stimulus};
use nb_ecs::density::{self, Viewer};
use nb_ecs::{EcsEvent, Emitter, GameMode, MobType, Transform};
use nb_world::calendar::Calendar;
use nb_world::weather::{Environment, Wind};
use nb_world::BiomeInfo;

const NOON: f32 = 900.0;
const MIDNIGHT: f32 = 300.0;

/// Banc : dalle de pierre de ±80, rayon d'apparition 60, joueur au centre, biome URSKOG partout.
fn world() -> Harness {
    let mut h = Harness::new();
    let stone = h.ids().stone;
    h.slab(-80, 80, 64, -80, 80, stone);
    h.cfg.mob_spawn_radius = 60;
    h.cfg.mob_despawn_radius = 200;
    h.cfg.fall_damage_enabled = false;
    h.cfg.time_of_day = NOON;
    h.blocks.set_biome(Some(BiomeInfo::named("URSKOG")));
    h
}

/// Déclare un mob d'essai : `rules` est l'objet JSON `spawn`, `extra` des champs de plus.
fn mob(h: &mut Harness, id: &str, rules: &str, extra: &str) {
    let json = format!(
        r#"{{"mobs": [{{"id": "{id}", "maxCount": 400, "groupSizeMin": 1, "groupSizeMax": 1,
            "spawn": {rules} {extra}}}]}}"#
    );
    h.mobs.apply_overrides(&json).unwrap();
    assert!(
        h.mobs.get(id).unwrap().spawn.is_some(),
        "règles de {id} refusées"
    );
}

fn player(h: &mut Harness) {
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
}

/// Jour de l'année `year` dont la fraction d'année est la plus proche de `frac` (0 = solstice
/// d'hiver, 0,5 = solstice d'été).
fn day_at(h: &Harness, year: u64, frac: f64) -> u64 {
    let cal = Calendar::new(h.cfg.days_per_month);
    let n = cal.days_per_year();
    let dist = |d: u32| {
        let f = cal.year_fraction(cal.day_for(year, d), 0.0);
        let x = (f - frac).abs();
        x.min(1.0 - x)
    };
    let d = (0..n).min_by(|&a, &b| dist(a).total_cmp(&dist(b))).unwrap();
    cal.day_for(year, d)
}

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s * 60.0) as usize);
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

fn calm_env(rain: f32) -> Environment {
    Environment {
        air: 0.5,
        wind: Wind::CALM,
        chill: 1.0,
        rain,
        snow: 0.0,
        fog: 0.0,
        sun: 1.0,
    }
}

const FOREST: &str = r#"{"densityPerBiome": {"URSKOG": 4.0}}"#;

#[test]
fn the_population_settles_on_the_density_target() {
    let mut h = world();
    mob(&mut h, "t_a", FOREST, "");
    player(&mut h);
    secs(&mut h, 40.0);
    let n = positions(&h, "t_a").len();
    // Disque de rayon 60 ≈ 11 300 blocs² → 4 × 11 300 / 4096 ≈ 11 individus.
    assert!(
        (8..=16).contains(&n),
        "{n} individus pour une cible d'environ 11"
    );
    // Une fois la cible atteinte, plus d'apparition.
    secs(&mut h, 20.0);
    assert!(positions(&h, "t_a").len() <= 17);
}

#[test]
fn species_follow_the_biome_densities() {
    let mut h = world();
    h.blocks
        .set_biome_zone((-80, -1), (-80, 80), BiomeInfo::named("MYR"));
    mob(&mut h, "t_b", FOREST, "");
    mob(
        &mut h,
        "t_c",
        r#"{"densityPerBiome": {"MYR": 4.0, "URSKOG": 0.5}}"#,
        "",
    );
    player(&mut h);
    secs(&mut h, 40.0);
    let b = positions(&h, "t_b");
    assert!(b.len() >= 3, "{} t_b", b.len());
    assert!(
        b.iter().all(|p| p.x >= -3.0),
        "t_b n'apparaît pas dans le Myr"
    );
    let c = positions(&h, "t_c");
    let west = c.iter().filter(|p| p.x < 0.0).count();
    let east = c.len() - west;
    assert!(
        west > east * 2,
        "t_c : {west} dans le Myr contre {east} en forêt"
    );
}

#[test]
fn spawns_stay_out_of_sight_and_out_of_reach() {
    let mut h = world();
    mob(&mut h, "t_a", FOREST, "");
    player(&mut h);
    let viewers = [Viewer {
        pos: DVec3::new(0.5, 65.0, 0.5),
        yaw: h.t(h.player()).yaw,
    }];
    let mut seen = 0;
    for _ in 0..60 * 30 {
        h.tick();
        for e in std::mem::take(&mut h.events) {
            if let EcsEvent::MobSpawned { entity } = e {
                let p = h.t(entity).pos;
                let d = (p.x - 0.5).hypot(p.z - 0.5);
                assert!(d >= 23.5, "apparu à {d:.1} blocs");
                assert!(!density::in_view(&viewers, p), "apparu en vue à {p:?}");
                seen += 1;
            }
        }
    }
    assert!(seen > 5, "{seen} apparitions");
}

#[test]
fn a_bear_is_absent_in_winter_and_present_in_summer() {
    let rules = r#"{"densityPerBiome": {"URSKOG": 4.0}, "seasonCurve": [0, 0, 1, 1]}"#;
    // Hiver (solstice) : la courbe vaut 0.
    let mut h = world();
    mob(&mut h, "t_bear", rules, "");
    h.cfg.calendar_day = day_at(&h, 2, 0.1);
    player(&mut h);
    secs(&mut h, 30.0);
    assert_eq!(positions(&h, "t_bear").len(), 0, "ours en plein hiver");
    // Été (mi-juillet) : le facteur monte à 1.
    let mut h = world();
    mob(&mut h, "t_bear", rules, "");
    h.cfg.calendar_day = day_at(&h, 2, 0.5);
    player(&mut h);
    secs(&mut h, 40.0);
    assert!(positions(&h, "t_bear").len() >= 6);
}

#[test]
fn out_of_season_individuals_leave_when_unseen() {
    let rules = r#"{"densityPerBiome": {"URSKOG": 4.0}, "seasonCurve": [0, 0, 1, 1]}"#;
    let mut h = world();
    mob(&mut h, "t_bear", rules, "");
    h.cfg.calendar_day = day_at(&h, 2, 0.5);
    player(&mut h);
    secs(&mut h, 30.0);
    let before = positions(&h, "t_bear").len();
    assert!(before >= 6);
    // Retour de l'hiver : ils disparaissent (hors de vue) en quelques secondes.
    h.cfg.calendar_day = day_at(&h, 2, 0.1);
    secs(&mut h, 40.0);
    let after = positions(&h, "t_bear").len();
    assert!(after <= before / 3, "{after} restent sur {before}");
}

#[test]
fn day_part_weather_and_cycle_modulate_the_target() {
    // Nocturne : rien à midi, des individus à minuit.
    let rules = r#"{"densityPerBiome": {"URSKOG": 4.0}, "timeOfDay": "nocturnal"}"#;
    let mut h = world();
    mob(&mut h, "t_owl", rules, "");
    player(&mut h);
    secs(&mut h, 20.0);
    assert_eq!(positions(&h, "t_owl").len(), 0, "chouette à midi");
    h.cfg.time_of_day = MIDNIGHT;
    secs(&mut h, 20.0);
    assert!(positions(&h, "t_owl").len() >= 5, "chouette à minuit");

    // Pluie : facteur 0 (les chauves-souris restent chez elles).
    let rules = r#"{"densityPerBiome": {"URSKOG": 4.0}, "weather": {"rain": 0.0}}"#;
    let mut h = world();
    mob(&mut h, "t_bat", rules, "");
    h.environment = Some(calm_env(0.9));
    player(&mut h);
    secs(&mut h, 20.0);
    assert_eq!(positions(&h, "t_bat").len(), 0, "sous la pluie");
    h.environment = Some(calm_env(0.0));
    secs(&mut h, 20.0);
    assert!(positions(&h, "t_bat").len() >= 5, "par temps sec");
}

#[test]
fn the_lemming_peak_year_quadruples_the_population() {
    let mut counts = Vec::new();
    // Cycle du profil `gnagare` : × 0,2 / 1 / 4 / 0,5 (années 1 à 4).
    for year in [1u64, 3] {
        let mut h = world();
        mob(
            &mut h,
            "t_lem",
            r#"{"densityPerBiome": {"URSKOG": 1.0}}"#,
            r#", "behavior": "gnagare", "moveSpeedMin": 0, "moveSpeedMax": 0"#,
        );
        let cal = Calendar::new(h.cfg.days_per_month);
        h.cfg.calendar_day = cal.day_for(year, 30);
        player(&mut h);
        secs(&mut h, 60.0);
        counts.push(positions(&h, "t_lem").len());
    }
    // Disque ≈ 11 300 blocs² × 1 / 4096 ≈ 2,8 : année 1 ≈ 0,55, année 3 ≈ 11.
    assert!(counts[0] <= 2, "année creuse : {}", counts[0]);
    assert!(counts[1] >= 8, "année de pic : {}", counts[1]);
}

#[test]
fn unique_and_resident_species() {
    let mut h = world();
    mob(
        &mut h,
        "t_troll",
        r#"{"densityPerBiome": {"URSKOG": 9.0}, "persistence": "unique"}"#,
        "",
    );
    mob(
        &mut h,
        "t_home",
        r#"{"densityPerBiome": {"URSKOG": 2.0}, "persistence": "resident"}"#,
        "",
    );
    // Rayon de disparition minuscule : un éphémère s'en irait, pas un sédentaire.
    h.cfg.mob_despawn_radius = 30;
    mob(
        &mut h,
        "t_eph",
        r#"{"densityPerBiome": {"URSKOG": 2.0}}"#,
        "",
    );
    player(&mut h);
    secs(&mut h, 30.0);
    assert_eq!(positions(&h, "t_troll").len(), 1, "un seul exemplaire");
    assert!(positions(&h, "t_home").len() >= 3);
    // Le joueur s'éloigne : l'éphémère part, le sédentaire reste (son chunk est chargé).
    let id = h.player();
    h.sim.entities.get_mut::<Transform>(id).unwrap().pos = DVec3::new(0.5, 65.0, 0.5);
    h.cfg.mob_spawn_radius = 60;
    let far = DVec3::new(70.0, 65.0, 70.0);
    for e in h.sim.entities.query_ids::<&MobType>() {
        let eph = h.sim.entities.get::<MobType>(e).unwrap().mob_id == "t_eph";
        if eph {
            h.sim.entities.get_mut::<Transform>(e).unwrap().pos = far;
        }
    }
    let home_before = positions(&h, "t_home").len();
    secs(&mut h, 3.0);
    let eph_far = positions(&h, "t_eph").iter().filter(|p| p.x > 60.0).count();
    assert_eq!(eph_far, 0, "les éphémères éloignés sont retirés");
    assert!(positions(&h, "t_home").len() >= home_before.saturating_sub(1));
}

#[test]
fn fire_and_features_gate_spawn_points() {
    // Un feu à 40 blocs à l'est tient les animaux à 20 blocs de lui.
    let mut h = world();
    mob(
        &mut h,
        "t_shy",
        r#"{"densityPerBiome": {"URSKOG": 4.0}, "avoid": {"fire": 20}}"#,
        "",
    );
    let fire = h.sim.entities.create();
    h.sim.entities.insert(fire, Transform::new(40.5, 65.0, 0.5));
    h.sim.entities.insert(
        fire,
        Emitter {
            kind: Stimulus::Fire,
            radius: 20.0,
        },
    );
    player(&mut h);
    let mut seen = 0;
    for _ in 0..60 * 30 {
        h.tick();
        for e in std::mem::take(&mut h.events) {
            if let EcsEvent::MobSpawned { entity } = e {
                let p = h.t(entity).pos;
                assert!(p.distance(DVec3::new(40.5, 65.0, 0.5)) >= 19.5);
                seen += 1;
            }
        }
    }
    assert!(seen > 4);

    // Trait de décor : seuls les points à 6 blocs d'un bloc de neige conviennent.
    let mut h = world();
    let snow = h.reg.get("snow").id;
    h.slab(20, 40, 64, 20, 40, snow);
    mob(
        &mut h,
        "t_pup",
        r#"{"densityPerBiome": {"URSKOG": 4.0}, "nearFeature": "snow"}"#,
        r#", "spawnScatterRadius": 0, "moveSpeedMin": 0, "moveSpeedMax": 0"#,
    );
    player(&mut h);
    secs(&mut h, 40.0);
    let near = positions(&h, "t_pup");
    assert!(!near.is_empty(), "aucune apparition près du décor");
    assert!(near
        .iter()
        .all(|p| (p.x - 30.5).abs() < 17.0 && (p.z - 30.5).abs() < 17.0));
}

#[test]
fn spawning_is_deterministic() {
    let run = || {
        let mut h = world();
        mob(&mut h, "t_a", FOREST, "");
        player(&mut h);
        secs(&mut h, 15.0);
        let mut p = positions(&h, "t_a");
        p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.z.total_cmp(&b.z)));
        p
    };
    assert_eq!(run(), run());
}

// ── Milieux ────────────────────────────────────────────────────────────────

#[test]
fn birds_spawn_in_the_air_and_stay_aloft() {
    let book = BehaviorBook::builtin();
    let cruise = book.get("rovfagel").unwrap().cruise;
    let mut h = world();
    mob(
        &mut h,
        "t_orn",
        r#"{"densityPerBiome": {"URSKOG": 1.5}, "persistence": "resident"}"#,
        r#", "medium": "AIR", "behavior": "rovfagel""#,
    );
    player(&mut h);
    secs(&mut h, 40.0);
    let birds = positions(&h, "t_orn");
    assert!(birds.len() >= 3, "{} oiseaux", birds.len());
    for p in &birds {
        // Sol à y = 65 : ni posés ni enfouis, à l'altitude de croisière à peu près.
        assert!(
            p.y > 65.0 + f64::from(cruise) * 0.4,
            "oiseau trop bas : {}",
            p.y
        );
    }
    // Sans gravité : après 10 s de plus, ils volent toujours.
    secs(&mut h, 10.0);
    let still = positions(&h, "t_orn");
    assert!(still.iter().all(|p| p.y > 68.0), "{still:?}");
}

#[test]
fn a_flying_mob_has_no_gravity_and_a_walker_has() {
    let mut h = world();
    mob(
        &mut h,
        "t_air",
        FOREST,
        r#", "medium": "AIR", "behavior": "rovfagel", "maxCount": 0"#,
    );
    mob(
        &mut h,
        "t_land",
        FOREST,
        r#", "behavior": "smavilt", "maxCount": 0"#,
    );
    player(&mut h);
    let bird = h.spawn_mob("t_air", 5.0, 90.0, 5.0);
    let hare = h.spawn_mob("t_land", -5.0, 72.0, -5.0);
    for s in 0..180 {
        h.tick();
        if h.sim.entities.copy::<Transform>(bird).is_none() {
            panic!("oiseau détruit au tick {s}");
        }
        if h.sim.entities.copy::<Transform>(hare).is_none() {
            panic!("lièvre détruit au tick {s}");
        }
    }
    assert!(
        h.t(bird).pos.y > 80.0,
        "l'oiseau tombe : {}",
        h.t(bird).pos.y
    );
    assert!(
        h.t(hare).pos.y < 70.0,
        "le lièvre vole : {}",
        h.t(hare).pos.y
    );
}

fn pond(h: &mut Harness) {
    let water = h.ids().water;
    let air = h.ids().air;
    // Étang de 41 × 41 de 4 blocs de profondeur au nord-est, creusé dans la dalle.
    for x in 20..=60 {
        for z in 20..=60 {
            for y in 61..=64 {
                h.put(x, y, z, water);
            }
            h.put(x, 65, z, air);
        }
    }
}

#[test]
fn amphibians_swim_in_water_and_walk_on_land() {
    let mut h = world();
    pond(&mut h);
    let water = h.ids().water;
    mob(
        &mut h,
        "t_seal",
        r#"{"densityPerBiome": {"URSKOG": 8.0}}"#,
        r#", "medium": "AMPHIBIAN", "behavior": "byggare", "spawnMinWaterDepth": 3"#,
    );
    player(&mut h);
    let pid = h.player();
    h.sim.entities.get_mut::<Transform>(pid).unwrap().yaw = std::f32::consts::PI;
    secs(&mut h, 40.0);
    let seals = positions(&h, "t_seal");
    assert!(seals.len() >= 3, "{} amphibies", seals.len());
    // Tous au bord de l'eau ou dedans (la rive est à moins de 3 blocs de l'étang).
    let near_water = |p: &DVec3| {
        (-4..=4).any(|dx| {
            (-4..=4).any(|dz| {
                (62..=65)
                    .any(|y| h.block(p.x.floor() as i32 + dx, y, p.z.floor() as i32 + dz) == water)
            })
        })
    };
    assert!(seals.iter().all(near_water), "{seals:?}");

    // Physique : à l'eau il flotte sans tomber, à terre il tombe au sol.
    let in_pond = h.spawn_mob("t_seal", 40.5, 63.5, 40.5);
    let on_land = h.spawn_mob("t_seal", -40.5, 90.0, -40.5);
    secs(&mut h, 2.0);
    let y = h.t(in_pond).pos.y;
    assert!((61.0..66.5).contains(&y), "dans l'étang : {y}");
    assert!(h.t(on_land).pos.y < 70.0, "à terre il doit tomber");
}

#[test]
fn ice_dwellers_spawn_on_ice_only_and_stay_on_it() {
    let mut h = world();
    let ice = h.reg.get("packed_ice").id;
    // Banquise : demi-plan est (x ≥ 20) ; ailleurs, pierre.
    h.slab(20, 80, 64, -80, 80, ice);
    mob(
        &mut h,
        "t_ice",
        r#"{"densityPerBiome": {"URSKOG": 3.0}}"#,
        r#", "medium": "ICE", "behavior": "smavilt""#,
    );
    player(&mut h);
    secs(&mut h, 40.0);
    let on_ice = positions(&h, "t_ice");
    assert!(on_ice.len() >= 3, "{} animaux de banquise", on_ice.len());
    assert!(on_ice.iter().all(|p| p.x > 18.0), "{on_ice:?}");
    // Ils errent sans quitter la glace.
    secs(&mut h, 40.0);
    assert!(positions(&h, "t_ice").iter().all(|p| p.x > 17.0));
}

#[test]
fn ice_is_slippery_for_ice_mobs_only() {
    let mut h = world();
    let ice = h.reg.get("packed_ice").id;
    h.slab(-10, 10, 64, -10, 10, ice);
    // Sans volonté propre : seule la friction freine.
    let still = r#", "maxCount": 0, "moveSpeedMin": 0, "moveSpeedMax": 0, "playerFleeSpeed": 0, "aggressorFleeSpeed": 0"#;
    mob(
        &mut h,
        "t_ice",
        FOREST,
        &format!(r#", "medium": "ICE"{still}"#),
    );
    mob(&mut h, "t_walk", FOREST, still);
    player(&mut h);
    let a = h.spawn_mob("t_ice", 0.5, 65.0, 0.5);
    let b = h.spawn_mob("t_walk", 5.5, 65.0, 5.5);
    secs(&mut h, 0.5);
    for id in [a, b] {
        let mut v = h.v(id);
        v.v.x = 4.0;
        v.target.x = 0.0;
        h.sim.entities.insert(id, v);
    }
    secs(&mut h, 0.5);
    let (sa, sb): (f32, f32) = (h.v(a).v.x, h.v(b).v.x);
    assert!(sa > sb + 0.3, "glisse sur la glace : {sa} contre {sb}");
}

#[test]
fn media_serialize_round_trip() {
    let mut h = world();
    mob(&mut h, "t_x", FOREST, r#", "medium": "AMPHIBIAN""#);
    let json = h.mobs.to_json();
    let mut again = nb_data::MobRegistry::builtin_for(nb_data::WorldKind::Norrcraft);
    again.apply_overrides(&json).unwrap();
    let c = again.get("t_x").unwrap();
    assert_eq!(c.medium, nb_data::MobMedium::Amphibian);
    assert_eq!(c.spawn, h.mobs.get("t_x").unwrap().spawn);
}
