//! Lot aquatique, amphibie et insectes de Norrcraft (session 5.6, doc 03 §3.4 et §3.5) :
//! cohérence des 23 fiches.

use nb_data::behavior::Locomotion;
use nb_data::{BehaviorBook, MobBehaviorType, MobMedium, MobRegistry, WorldKind};

const AQUATIC: [(&str, f32, MobMedium); 17] = [
    ("lax", 3.0, MobMedium::Water),
    ("oring", 2.0, MobMedium::Water),
    ("roding", 2.0, MobMedium::Water),
    ("gadda", 6.0, MobMedium::Water),
    ("abborre", 1.0, MobMedium::Water),
    ("stromming", 1.0, MobMedium::Water),
    ("torsk", 5.0, MobMedium::Water),
    ("al_fisk", 2.0, MobMedium::Water),
    ("krafta", 2.0, MobMedium::Amphibian),
    ("brannmanet", 2.0, MobMedium::Water),
    ("grasal", 12.0, MobMedium::Amphibian),
    ("vikare", 8.0, MobMedium::Ice),
    ("valross", 40.0, MobMedium::Ice),
    ("tumlare", 8.0, MobMedium::Water),
    ("utter", 6.0, MobMedium::Amphibian),
    ("hakarl", 30.0, MobMedium::Water),
    ("val", 80.0, MobMedium::Water),
];
const INSECTS: [&str; 6] = ["mygg", "knott", "bi", "fjaril", "trollslanda", "lysmask"];

fn registry() -> MobRegistry {
    MobRegistry::builtin_for(WorldKind::Norrcraft)
}

#[test]
fn the_roster_has_the_aquatic_and_insect_species() {
    let reg = registry();
    for (id, hp, medium) in AQUATIC {
        let c = reg.get(id).unwrap_or_else(|| panic!("{id} manque"));
        assert_eq!(c.max_health, hp, "{id}");
        assert_eq!(c.medium, medium, "{id}");
        assert!(c.rig.is_some() && c.spawn.is_some(), "{id}");
    }
    for id in INSECTS {
        let c = reg.get(id).unwrap_or_else(|| panic!("{id} manque"));
        assert_eq!(c.max_health, 1.0, "{id}");
        assert!(c.rig.is_some() && c.spawn.is_some(), "{id}");
    }
    assert!(reg.all().len() >= 57);
}

#[test]
fn swimmers_have_swimming_profiles_and_flyers_flying_ones() {
    let reg = registry();
    let book = BehaviorBook::builtin();
    for (id, _, medium) in AQUATIC {
        let c = reg.get(id).unwrap();
        let p = book.get(c.behavior.as_deref().unwrap()).unwrap();
        if medium == MobMedium::Water && id != "brannmanet" {
            assert_ne!(p.locomotion, Locomotion::Walk, "{id}");
        }
    }
    for id in ["mygg", "knott", "bi", "fjaril", "trollslanda"] {
        let c = reg.get(id).unwrap();
        assert_eq!(c.medium, MobMedium::Air, "{id}");
        let p = book.get(c.behavior.as_deref().unwrap()).unwrap();
        assert_eq!(p.locomotion, Locomotion::Fly, "{id}");
    }
}

#[test]
fn salmon_run_in_summer_and_cod_come_in_winter() {
    let reg = registry();
    let rules = |id: &str| reg.get(id).unwrap().spawn.clone().unwrap();
    let lax = rules("lax");
    assert!(lax.season_factor(0.0) < 0.05, "pas de saumon en plein hiver");
    assert!(lax.season_factor(0.55) >= 0.9, "saumons en juillet");
    let torsk = rules("torsk");
    assert!(torsk.season_factor(0.0) > torsk.season_factor(0.55) * 2.0, "skrei d'hiver");
    let manet = rules("brannmanet");
    assert!(manet.season_factor(0.65) > 0.9 && manet.season_factor(0.0) == 0.0, "méduses en août");
}

#[test]
fn insect_swarms_are_summer_only_and_harmless() {
    let reg = registry();
    for id in ["mygg", "knott", "bi", "fjaril", "trollslanda", "lysmask"] {
        let c = reg.get(id).unwrap();
        let r = c.spawn.as_ref().unwrap();
        assert!(r.season_factor(0.0) == 0.0, "{id} absent l'hiver");
        assert!(r.season_factor(0.55) > 0.3, "{id} présent l'été");
        assert_eq!(c.behavior_type, MobBehaviorType::Passive, "{id}");
        assert_eq!(c.attack_damage, 0.0, "{id}");
    }
    // Les moustiques fuient le vent et la fumée.
    let mygg = reg.get("mygg").unwrap().spawn.clone().unwrap();
    assert!(mygg.weather["wind"] < 0.2);
    let book = BehaviorBook::builtin();
    let p = book.get("skymning").unwrap();
    assert!(p.reaction(nb_data::behavior::Stimulus::Smoke).fear >= 1.0);
}

#[test]
fn only_the_documented_species_bite() {
    let reg = registry();
    for (id, _, _) in AQUATIC {
        let aggressive = reg.get(id).unwrap().behavior_type == MobBehaviorType::Aggressive;
        assert_eq!(aggressive, matches!(id, "gadda" | "brannmanet" | "valross"), "{id}");
    }
}

#[test]
fn deep_species_need_deep_water() {
    let reg = registry();
    for (id, depth) in [("hakarl", 8), ("val", 10), ("tumlare", 3)] {
        assert!(reg.get(id).unwrap().spawn_min_water_depth >= depth, "{id}");
    }
    let r = reg.get("val").unwrap().spawn.clone().unwrap();
    assert!(r.density("DJUPHAV") > 0.0 && r.density("URSKOG") == 0.0);
}
