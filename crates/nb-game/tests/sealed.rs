//! Étanchéité des univers (plan Norrcraft, session 0.4 ; GDD doc 01 §10) : hors socle commun,
//! Norrcraft ne contient aucun nom de bloc, d'objet, de mob ni de biome de vanilla, et
//! réciproquement.

use std::collections::BTreeSet;
use std::sync::Arc;

use nb_data::blocks::SOCLE;
use nb_data::{BlockRegistry, ItemRegistry, WorldKind};
use nb_game::Registries;
use nb_world::norrcraft::NorrConfig;
use nb_world::{WorldConfig, WorldProfile};

fn items_of(kind: WorldKind) -> BTreeSet<String> {
    let blocks = BlockRegistry::for_kind(kind);
    ItemRegistry::for_kind(kind, &blocks)
        .iter()
        .map(|i| i.name.clone())
        .filter(|n| !SOCLE.contains(&n.as_str()))
        .collect()
}

fn mobs_of(kind: WorldKind) -> BTreeSet<String> {
    Registries::for_world(kind, &Default::default())
        .unwrap()
        .0
        .mobs
        .all()
        .iter()
        .map(|m| m.id.to_lowercase())
        .collect()
}

fn biomes_of(kind: WorldKind) -> BTreeSet<String> {
    let reg = BlockRegistry::for_kind(kind);
    let profile = match kind {
        WorldKind::Vanilla => WorldProfile::vanilla(1, Arc::new(WorldConfig::default()), reg.ids()),
        WorldKind::Norrcraft => WorldProfile::norrcraft(1, NorrConfig::default(), &reg),
    };
    profile.biomes.iter().map(|b| b.name.to_lowercase()).collect()
}

fn blocks_of(kind: WorldKind) -> BTreeSet<String> {
    BlockRegistry::for_kind(kind)
        .iter()
        .map(|b| b.name.clone())
        .filter(|n| !SOCLE.contains(&n.as_str()))
        .collect()
}

fn assert_disjoint(what: &str, a: &BTreeSet<String>, b: &BTreeSet<String>) {
    let shared: Vec<_> = a.intersection(b).collect();
    assert!(shared.is_empty(), "{what} partagés entre univers : {shared:?}");
}

#[test]
fn universes_share_no_content_beyond_the_socle() {
    let (v, n) = (WorldKind::Vanilla, WorldKind::Norrcraft);
    assert_disjoint("blocs", &blocks_of(v), &blocks_of(n));
    assert_disjoint("objets", &items_of(v), &items_of(n));
    assert_disjoint("mobs", &mobs_of(v), &mobs_of(n));
    assert_disjoint("biomes", &biomes_of(v), &biomes_of(n));
}

#[test]
fn no_norrcraft_name_contains_a_vanilla_biome_or_mob() {
    let vanilla: BTreeSet<String> = biomes_of(WorldKind::Vanilla)
        .into_iter()
        .chain(mobs_of(WorldKind::Vanilla))
        .filter(|s| s.len() >= 4)
        .collect();
    let norr: BTreeSet<String> = blocks_of(WorldKind::Norrcraft)
        .into_iter()
        .chain(items_of(WorldKind::Norrcraft))
        .chain(mobs_of(WorldKind::Norrcraft))
        .chain(biomes_of(WorldKind::Norrcraft))
        .collect();
    for name in &norr {
        for v in &vanilla {
            assert!(
                !name.to_lowercase().contains(v.as_str()),
                "« {name} » (Norrcraft) contient « {v} » (vanilla)"
            );
        }
    }
}

#[test]
fn norrcraft_world_uses_granite_from_the_registry() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    assert_ne!(reg.get("granit").id, 0);
    assert_eq!(biomes_of(WorldKind::Norrcraft).len(), 25);
}
