//! Établi et fonderie dans le monde : clic droit = ouvrir, Accroupi = poser, objets purs non
//! posables, casser rend le bloc-station.

mod common;

use common::Harness;
use nb_core::glam::{DVec3, IVec3};
use nb_data::{BlockRegistry, ItemRegistry, Station};
use nb_ecs::{EcsEvent, GameAction, GameMode, ItemStack};

/// Sol en y = 64, joueur en (0.5, 65, 0.5) face à −Z, bloc `station` en (0, 66, −3).
fn arena(station: &str) -> (Harness, u8) {
    let mut h = Harness::new();
    h.reg = BlockRegistry::builtin_with_craft();
    h.stone_floor();
    let id = h.reg.get(station).id;
    h.put(0, 66, -3, id);
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    h.ticks(5);
    h.take_events();
    (h, id)
}

#[test]
fn right_click_opens_the_station_instead_of_placing() {
    for (name, station) in [("workbench", Station::Workbench), ("forge", Station::Forge)] {
        let (mut h, _) = arena(name);
        let dirt = h.ids().dirt;
        h.inv_mut().set_slot(27, Some(ItemStack::new(dirt, 3)));
        h.press(GameAction::Secondary);
        assert!(
            h.take_events().contains(&EcsEvent::StationUsed {
                pos: IVec3::new(0, 66, -3),
                station
            }),
            "{name}"
        );
        assert_eq!(h.block(0, 66, -2), h.ids().air, "rien n'est posé");
        assert_eq!(h.inv().slot(27), Some(ItemStack::new(dirt, 3)));
    }
}

#[test]
fn crouching_places_a_block_against_the_station() {
    let (mut h, _) = arena("workbench");
    let dirt = h.ids().dirt;
    h.inv_mut().set_slot(27, Some(ItemStack::new(dirt, 3)));
    h.hold(GameAction::Crouch);
    h.press(GameAction::Secondary);
    assert_eq!(h.block(0, 66, -2), dirt);
    assert!(!h
        .take_events()
        .iter()
        .any(|e| matches!(e, EcsEvent::StationUsed { .. })));
}

#[test]
fn pure_items_cannot_be_placed() {
    let (mut h, _) = arena("workbench");
    // Cible : un mur de pierre, pas une station.
    h.put(0, 66, -3, h.ids().stone);
    h.tick();
    let items = ItemRegistry::builtin(&h.reg);
    let stick = items.id_of("stick").unwrap();
    h.inv_mut().set_slot(27, Some(ItemStack::new(stick, 4)));
    h.press(GameAction::Secondary);
    assert_eq!(h.block(0, 66, -2), h.ids().air);
    assert_eq!(h.inv().slot(27), Some(ItemStack::new(stick, 4)));
    assert!(h.take_events().is_empty());
}

#[test]
fn breaking_a_station_gives_the_station_back() {
    let (mut h, id) = arena("forge");
    h.press(GameAction::Primary);
    assert_eq!(h.block(0, 66, -3), h.ids().air);
    assert_eq!(h.inv().slot(27), Some(ItemStack::new(id, 1)));
}
