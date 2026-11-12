//! Fiches d'écologie du bestiaire Norrcraft (session 5.8) : forme des données, noms de blocs
//! résolubles, cohérence avec les profils de comportement.

use nb_data::behavior::{BehaviorBook, StateKind};
use nb_data::ecology::TrapKind;
use nb_data::{BlockRegistry, MobRegistry, WorldKind};

fn reg() -> MobRegistry {
    MobRegistry::builtin_for(WorldKind::Norrcraft)
}

#[test]
fn thirty_odd_species_carry_an_ecology_sheet() {
    let mobs = reg();
    let n = mobs.all().iter().filter(|c| c.ecology.is_some()).count();
    assert!(n >= 30, "{n} fiches d'écologie");
    assert_eq!(mobs.all().len(), 67, "le bestiaire garde ses 67 espèces");
}

#[test]
fn every_block_a_sheet_names_exists() {
    let mobs = reg();
    let blocks = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let air = blocks.ids().air;
    let known = |n: &str| blocks.get(n).id != air;
    for c in mobs.all() {
        let Some(e) = &c.ecology else { continue };
        if let Some(b) = &e.browse {
            assert!(!b.eats.is_empty(), "{}", c.id);
            for n in b.eats.iter().chain(&b.fences) {
                assert!(known(n), "{} : bloc {n} inconnu", c.id);
            }
        }
        if let Some(f) = &e.fell {
            for n in &f.dam {
                assert!(known(n), "{} : bloc de digue {n} inconnu", c.id);
            }
            assert!(f.every_secs > 0.0 && f.cap > 0);
        }
    }
}

#[test]
fn the_fence_block_exists_and_is_solid() {
    let blocks = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let f = blocks.get("staket");
    assert_ne!(f.id, blocks.ids().air);
    assert!(f.solid);
}

#[test]
fn game_species_have_loot_and_a_naturfrid_sheet() {
    let mobs = reg();
    for id in ["alg", "ren", "radjur", "myskoxe", "snolopare", "orre", "bjorn", "lax", "val"] {
        let e = mobs.get(id).unwrap().ecology.as_ref().unwrap_or_else(|| panic!("{id}"));
        assert!(!e.loot.is_empty(), "{id} sans butin");
        assert!(e.naturfrid.is_some(), "{id} sans naturfrid");
        assert!(e.loot.iter().any(|c| c.item == "kott" || c.item == "fisk" || c.item == "krafta"), "{id}");
    }
    for id in ["bjorn", "val", "sangsvan", "trana"] {
        let n = mobs.get(id).unwrap().ecology.as_ref().unwrap().naturfrid.as_ref().unwrap();
        assert!(n.sacred, "{id} est protégé");
    }
    // La peau d'hiver et celle d'été ne se cumulent pas.
    let ren = &mobs.get("ren").unwrap().ecology.as_ref().unwrap().loot;
    let winter = ren.iter().find(|c| c.item == "vinterskinn").unwrap();
    let summer = ren.iter().find(|c| c.item == "skinn").unwrap();
    assert!(winter.needs_knife && summer.needs_knife);
    for m in 0..12 {
        let w = nb_data::behavior::in_window(m, winter.months.unwrap());
        let s = nb_data::behavior::in_window(m, summer.months.unwrap());
        assert!(w ^ s, "mois {m} : exactement une des deux peaux");
    }
}

#[test]
fn traps_match_the_prey() {
    let mobs = reg();
    let rate = |id: &str, k: TrapKind| mobs.get(id).unwrap().ecology.as_ref().unwrap().catch_rate(k);
    assert!(rate("snolopare", TrapKind::Snare) > 0.0);
    assert!(rate("orre", TrapKind::Snare) > 0.0);
    assert!(rate("ripa", TrapKind::Snare) > 0.0);
    assert!(rate("alg", TrapKind::Pit) > 0.0);
    assert_eq!(rate("alg", TrapKind::Snare), 0.0);
    assert!(rate("lax", TrapKind::FishTrap) > 0.0 && rate("lax", TrapKind::Net) > 0.0);
    assert!(rate("krafta", TrapKind::FishTrap) > 0.0);
    assert_eq!(rate("bjorn", TrapKind::Pit), 0.0);
}

#[test]
fn the_viper_is_venomous_and_nothing_else_is() {
    let mobs = reg();
    let venomous: Vec<&str> = mobs
        .all()
        .iter()
        .filter(|c| c.ecology.as_ref().is_some_and(|e| e.venom.is_some()))
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(venomous, ["huggorm"]);
    let v = mobs.get("huggorm").unwrap().ecology.as_ref().unwrap().venom.as_ref().unwrap();
    assert!(v.damage_per_sec * v.seconds > 3.0 && !v.cures.is_empty());
}

#[test]
fn the_beaver_fells_and_its_dam_block_is_a_log() {
    let e = reg().get("baver").unwrap().ecology.clone().unwrap();
    let f = e.fell.unwrap();
    assert_eq!(f.log_suffix, "_stam");
    assert_eq!(f.dam, ["stock"]);
    assert!(f.every_secs >= 600.0, "toutes les quelques journées de jeu");
}

#[test]
fn snow_trackers_have_prints_and_the_viper_a_slither() {
    let mobs = reg();
    let print = |id: &str| {
        mobs.get(id)
            .unwrap()
            .ecology
            .as_ref()
            .unwrap()
            .tracks
            .as_ref()
            .map(|t| t.print.clone())
    };
    assert_eq!(print("alg").as_deref(), Some("klov"));
    assert_eq!(print("varg").as_deref(), Some("tass"));
    assert_eq!(print("orre").as_deref(), Some("fagel"));
    assert_eq!(print("huggorm").as_deref(), Some("orm"));
    assert_eq!(print("lax"), None);
}

#[test]
fn the_musk_ox_profile_offers_the_circle() {
    let book = BehaviorBook::builtin();
    let p = book.get("myskdjur").unwrap();
    assert!(p.offers(StateKind::Circle));
    assert!(p.herd);
    // Le cercle bat la charge dès que la menace est un prédateur de meute.
    let w = |s| p.states.iter().find(|d| d.state == s).unwrap().weight;
    assert!(w(StateKind::Circle) > w(StateKind::Charge) * p.courage);
    assert!(p.fears("", "flockjagare") || p.fears.iter().any(|f| f == "flockjagare"));
    for name in book.names() {
        if name != "myskdjur" {
            assert!(!book.get(name).unwrap().offers(StateKind::Circle), "{name}");
        }
    }
}
