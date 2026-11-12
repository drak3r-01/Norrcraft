//! Bestiaire terrestre de Norrcraft (session 5.4, doc 03 §3.1 et §3.2) : cohérence des fiches.

use std::collections::BTreeSet;

use nb_data::{BehaviorBook, MobBehaviorType, MobMedium, MobRegistry, Persistence, WorldKind};

const BIOMES: &str = include_str!("../../../assets/norrcraft/biomes.json");

const HERBIVORES: [&str; 10] = [
    "alg", "ren", "radjur", "myskoxe", "snolopare", "fjallammel", "baver", "ekorre", "igelkott",
    "hermelin",
];
const PREDATORS: [&str; 9] = [
    "varg", "bjorn", "lo", "jarv", "fjallrav", "rodrav", "mard", "isbjorn", "huggorm",
];

fn biome_names() -> BTreeSet<String> {
    let v: serde_json::Value = serde_json::from_str(BIOMES).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap().to_owned())
        .collect()
}

fn registry() -> MobRegistry {
    MobRegistry::builtin_for(WorldKind::Norrcraft)
}

#[test]
fn the_roster_has_the_nineteen_land_species() {
    let reg = registry();
    for id in HERBIVORES.iter().chain(&PREDATORS) {
        assert!(reg.get(id).is_some(), "{id} manque");
    }
    assert!(reg.all().len() >= 19);
}

#[test]
fn every_species_has_a_profile_a_rig_and_valid_spawn_rules() {
    let reg = registry();
    let book = BehaviorBook::builtin();
    let biomes = biome_names();
    for c in reg.all() {
        let profile = c.behavior.as_deref().unwrap_or_else(|| panic!("{} : pas de profil", c.id));
        assert!(book.get(profile).is_some(), "{} : profil inconnu {profile}", c.id);
        assert!(c.rig.is_some(), "{} : pas de rig", c.id);
        let rules = c.spawn.as_ref().unwrap_or_else(|| panic!("{} : pas de bloc spawn", c.id));
        rules.validate().unwrap_or_else(|e| panic!("{} : {e}", c.id));
        let mut named: Vec<&String> = rules.density_per_biome.keys().collect();
        if let Some(h) = &rules.herd {
            named.extend(h.summer.iter().chain(&h.winter));
        }
        for b in named {
            assert!(biomes.contains(b), "{} : biome inconnu {b}", c.id);
        }
        // Les profils nomment des profils ou des mobs : ceux qui sont chassés ou craints existent.
        let p = book.get(profile).unwrap();
        for other in p.hunts.iter().chain(&p.fears) {
            assert!(
                other == "player" || book.get(other).is_some() || reg.get(other).is_some(),
                "{} : {other} inconnu",
                c.id
            );
        }
    }
}

#[test]
fn herds_and_residents_follow_the_document() {
    let reg = registry();
    let persistence = |id: &str| reg.get(id).unwrap().spawn.as_ref().unwrap().persistence;
    assert_eq!(persistence("ren"), Persistence::Herd);
    assert_eq!(persistence("varg"), Persistence::Herd);
    assert_eq!(persistence("baver"), Persistence::Resident);
    let ren = reg.get("ren").unwrap().spawn.as_ref().unwrap().herd.clone().unwrap();
    assert!(ren.summer.contains(&"FJALLHED".to_owned()) && ren.winter.contains(&"TALLHED".to_owned()));
    let varg = reg.get("varg").unwrap().spawn.as_ref().unwrap().herd.clone().unwrap();
    assert_eq!((varg.size_min, varg.size_max), (3, 7));
}

#[test]
fn media_and_health_match_the_fiches() {
    let reg = registry();
    for (id, hp) in [
        ("alg", 30.0),
        ("ren", 16.0),
        ("radjur", 8.0),
        ("myskoxe", 40.0),
        ("snolopare", 3.0),
        ("fjallammel", 1.0),
        ("baver", 10.0),
        ("ekorre", 2.0),
        ("igelkott", 2.0),
        ("hermelin", 3.0),
        ("varg", 14.0),
        ("bjorn", 45.0),
        ("lo", 12.0),
        ("jarv", 18.0),
        ("fjallrav", 5.0),
        ("rodrav", 6.0),
        ("mard", 4.0),
        ("isbjorn", 60.0),
        ("huggorm", 2.0),
    ] {
        assert_eq!(reg.get(id).unwrap().max_health, hp, "{id}");
    }
    assert_eq!(reg.get("baver").unwrap().medium, MobMedium::Amphibian);
    assert_eq!(reg.get("isbjorn").unwrap().medium, MobMedium::Ice);
    assert_eq!(reg.get("isbjorn").unwrap().behavior_type, MobBehaviorType::Aggressive);
    assert_eq!(reg.get("ren").unwrap().medium, MobMedium::Land);
}

#[test]
fn bears_hibernate_and_vipers_sleep_through_winter() {
    let reg = registry();
    for id in ["bjorn", "igelkott", "huggorm"] {
        let r = reg.get(id).unwrap().spawn.as_ref().unwrap();
        // Facteur de saison au solstice d'hiver (a = 0) et au solstice d'été (a = 0,5).
        assert_eq!(r.season_factor(0.0), 0.0, "{id} absent en plein hiver");
        assert!(r.season_factor(0.5) >= 1.0 - 1e-6, "{id} présent l'été");
    }
}

#[test]
fn lemming_predators_follow_the_cycle_with_a_lag() {
    let book = BehaviorBook::builtin();
    for name in ["fjallrav", "vessla"] {
        let p = book.get(name).unwrap();
        assert_eq!(p.cycle.as_ref().unwrap().lag, 1, "{name}");
    }
    assert_eq!(book.get("gnagare").unwrap().cycle.as_ref().unwrap().lag, 0);
}
