//! Lot oiseaux de Norrcraft (session 5.5, doc 03 §3.3) : cohérence des quinze fiches.

use nb_data::behavior::Locomotion;
use nb_data::{BehaviorBook, MobBehaviorType, MobMedium, MobRegistry, WorldKind};

const BIRDS: [(&str, f32, MobMedium); 15] = [
    ("tjader", 4.0, MobMedium::Land),
    ("orre", 3.0, MobMedium::Land),
    ("ripa", 2.0, MobMedium::Land),
    ("trana", 6.0, MobMedium::Air),
    ("sangsvan", 10.0, MobMedium::Amphibian),
    ("grasgas", 5.0, MobMedium::Air),
    ("ejder", 4.0, MobMedium::Amphibian),
    ("lunne", 2.0, MobMedium::Air),
    ("havsorn", 10.0, MobMedium::Air),
    ("fjalluggla", 6.0, MobMedium::Air),
    ("kattuggla", 3.0, MobMedium::Air),
    ("korp", 4.0, MobMedium::Air),
    ("hackspett", 2.0, MobMedium::Air),
    ("mas", 3.0, MobMedium::Air),
    ("goken", 2.0, MobMedium::Air),
];

fn registry() -> MobRegistry {
    MobRegistry::builtin_for(WorldKind::Norrcraft)
}

#[test]
fn the_fifteen_birds_follow_the_fiches() {
    let reg = registry();
    for (id, hp, medium) in BIRDS {
        let c = reg.get(id).unwrap_or_else(|| panic!("{id} manque"));
        assert_eq!(c.max_health, hp, "{id}");
        assert_eq!(c.medium, medium, "{id}");
        assert!(c.rig.is_some() && c.spawn.is_some(), "{id}");
    }
    assert!(reg.all().len() >= 34);
}

#[test]
fn flying_species_have_flying_profiles_and_an_altitude() {
    let reg = registry();
    let book = BehaviorBook::builtin();
    for (id, _, medium) in BIRDS {
        let c = reg.get(id).unwrap();
        let p = book.get(c.behavior.as_deref().unwrap()).unwrap();
        if medium == MobMedium::Air {
            assert_eq!(p.locomotion, Locomotion::Fly, "{id}");
            assert!(p.cruise > 0.0 || c.spawn.as_ref().unwrap().altitude.is_some(), "{id}");
        } else {
            assert_ne!(p.locomotion, Locomotion::Fly, "{id}");
        }
    }
}

#[test]
fn migrants_are_absent_in_winter_and_present_in_summer() {
    let reg = registry();
    for id in ["trana", "grasgas", "lunne", "goken"] {
        let r = reg.get(id).unwrap().spawn.as_ref().unwrap();
        assert_eq!(r.season_factor(0.0), 0.0, "{id} absent en plein hiver");
        assert!(r.season_factor(0.5) >= 1.0 - 1e-6, "{id} présent l'été");
    }
    // Le grand tétras, l'eider et le corbeau restent toute l'année.
    for id in ["tjader", "ejder", "korp", "mas"] {
        let r = reg.get(id).unwrap().spawn.as_ref().unwrap();
        assert!(r.season_factor(0.0) > 0.0, "{id}");
    }
}

#[test]
fn only_the_cock_of_the_woods_and_the_swan_attack() {
    let reg = registry();
    for (id, _, _) in BIRDS {
        let aggressive = reg.get(id).unwrap().behavior_type == MobBehaviorType::Aggressive;
        assert_eq!(aggressive, matches!(id, "tjader" | "sangsvan"), "{id}");
    }
}
