//! Animaux domestiques de Norrcraft (session 5.7, doc 03 §3.6) : cohérence des 10 fiches.

use nb_data::behavior::in_window;
use nb_data::husbandry::{GaitName, Role};
use nb_data::{BehaviorBook, MobRegistry, Persistence, WorldKind};

const DOMESTIC: [(&str, f32, Role); 10] = [
    ("fjallko", 14.0, Role::Livestock),
    ("get", 8.0, Role::Livestock),
    ("gutefar", 10.0, Role::Livestock),
    ("linderodssvin", 12.0, Role::Livestock),
    ("hona", 3.0, Role::Poultry),
    ("tamgas", 5.0, Role::Poultry),
    ("fjordhast", 22.0, Role::Mount),
    ("islandshast", 20.0, Role::Mount),
    ("jamthund", 10.0, Role::Hound),
    ("skogskatt", 4.0, Role::Ratter),
];

fn registry() -> MobRegistry {
    MobRegistry::builtin_for(WorldKind::Norrcraft)
}

#[test]
fn the_roster_has_the_ten_domestic_species() {
    let reg = registry();
    for (id, hp, role) in DOMESTIC {
        let c = reg.get(id).unwrap_or_else(|| panic!("{id} manque"));
        assert_eq!(c.max_health, hp, "{id}");
        let h = c.husbandry.as_ref().unwrap_or_else(|| panic!("{id} sans fiche d'élevage"));
        assert_eq!(h.role, role, "{id}");
        assert!(!h.tame_with.is_empty(), "{id} : aucune friandise");
        assert!(c.rig.is_some() && c.spawn.is_some(), "{id}");
        assert_eq!(c.spawn.as_ref().unwrap().persistence, Persistence::Resident, "{id}");
    }
    assert_eq!(reg.all().iter().filter(|c| c.husbandry.is_some()).count(), 10);
}

#[test]
fn profiles_exist_and_companions_can_be_led_to_their_owner() {
    let reg = registry();
    let book = BehaviorBook::builtin();
    for (id, _, role) in DOMESTIC {
        let c = reg.get(id).unwrap();
        let p = book
            .get(c.behavior.as_deref().unwrap())
            .unwrap_or_else(|| panic!("{id} : profil inconnu"));
        // `Brain::goal` ne mène que par l'état Migrer.
        assert!(p.offers(nb_data::StateKind::Migrate), "{id} ne peut pas suivre son maître");
        if role == Role::Hound {
            // Le chien n'attaque pas le gibier qu'il tient.
            for s in [nb_data::StateKind::Attack, nb_data::StateKind::Pursue, nb_data::StateKind::Stalk] {
                assert!(!p.offers(s), "{id} offre {s:?}");
            }
            assert!(!p.hunts.is_empty());
        }
    }
}

#[test]
fn livestock_needs_winter_fodder_and_gives_its_product() {
    let reg = registry();
    for (id, _, role) in DOMESTIC {
        let h = reg.get(id).unwrap().husbandry.as_ref().unwrap();
        match role {
            Role::Livestock | Role::Poultry | Role::Mount => {
                let f = h.fodder.as_ref().unwrap_or_else(|| panic!("{id} sans fourrage"));
                assert!(f.per_day > 0.0 && f.ration > 0.0);
                // La pâture couvre l'été, pas l'hiver.
                assert!(in_window(6, f.pasture) && !in_window(0, f.pasture), "{id}");
            }
            Role::Hound | Role::Ratter => assert!(h.fodder.is_none(), "{id}"),
        }
    }
    let item = |id: &str| {
        let c = registry();
        c.get(id).unwrap().husbandry.as_ref().unwrap().produce.as_ref().map(|p| p.item.clone())
    };
    assert_eq!(item("fjallko").as_deref(), Some("mjolk"));
    assert_eq!(item("get").as_deref(), Some("mjolk"));
    assert_eq!(item("gutefar").as_deref(), Some("ull"));
    assert_eq!(item("hona").as_deref(), Some("agg"));
    assert_eq!(item("tamgas").as_deref(), Some("agg"));
    assert_eq!(item("linderodssvin"), None);
}

#[test]
fn the_icelandic_horse_tolts_smoothly() {
    let reg = registry();
    let g = |id: &str| reg.get(id).unwrap().husbandry.as_ref().unwrap().gait.unwrap();
    let tolt = g("islandshast");
    let fjord = g("fjordhast");
    assert_eq!(tolt.name, GaitName::Tolt);
    assert!(tolt.speed > fjord.speed && tolt.jolt < fjord.jolt);
    assert!(tolt.steady && !fjord.steady, "seul le tölt permet de manger ou viser en selle");
}

#[test]
fn the_elkhound_marks_game_and_alerts_on_big_predators() {
    let reg = registry();
    let h = reg.get("jamthund").unwrap().husbandry.as_ref().unwrap();
    let hound = h.hound.as_ref().unwrap();
    assert!(hound.alert_range > hound.point_range);
    for prey in ["varg", "bjorn"] {
        assert!(hound.alerts.iter().any(|a| a == prey), "{prey}");
        assert!(reg.get(prey).is_some(), "{prey}");
    }
}
