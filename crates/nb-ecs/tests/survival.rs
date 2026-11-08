//! Mode survie : minage progressif, outils, faim, froid, nourriture, armure, cultures, pêche,
//! butin des mobs.

mod common;

use common::Harness;
use nb_core::glam::{DVec3, IVec3};
use nb_data::{ArmorSlot, ItemRegistry, ItemId};
use nb_ecs::{EcsEvent, GameAction, GameMode, ItemStack, Vitals, TICK_DT};

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s / TICK_DT).round() as usize);
}

/// Sol de pierre en y = 64, joueur en (0.5, 65, 0.5) face à −Z, survie activée.
fn arena() -> (Harness, ItemRegistry) {
    let (mut h, items) = Harness::new().with_survival();
    h.stone_floor();
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    h.ticks(5);
    h.take_events();
    (h, items)
}

fn id(items: &ItemRegistry, n: &str) -> ItemId {
    items.id_of(n).unwrap_or_else(|| panic!("objet {n}"))
}

fn vitals(h: &Harness) -> Vitals {
    h.sim.entities.copy::<Vitals>(h.player()).unwrap()
}

fn set_vitals(h: &mut Harness, f: impl FnOnce(&mut Vitals)) {
    let p = h.player();
    f(h.sim.entities.get_mut::<Vitals>(p).unwrap());
}

/// Cible : un bloc en (0, 65, −2) à hauteur de visée (les yeux sont en 66.62, on vise vers le bas).
fn aim_at_block(h: &mut Harness, block: u8) {
    h.put(0, 66, -3, block);
    // Les yeux sont à y = 66,62 : le bloc (0,66,−3) est pile devant.
    let _ = h;
}

#[test]
fn bare_hands_cannot_harvest_stone_but_a_pickaxe_can() {
    let (mut h, items) = arena();
    let stone = h.ids().stone;
    aim_at_block(&mut h, stone);
    h.hold(GameAction::Primary);
    // Mains nues : 2 s ne suffisent pas, et sans pioche rien ne tomberait.
    secs(&mut h, 2.0);
    assert_eq!(h.block(0, 66, -3), stone, "le bloc résiste encore à 2 s");
    h.release(GameAction::Primary);

    let pick = id(&items, "iron_pickaxe");
    h.inv_mut().set_slot(27, Some(ItemStack::new(pick, 1)));
    h.hold(GameAction::Primary);
    secs(&mut h, 1.0);
    assert_eq!(h.block(0, 66, -3), h.ids().air, "pioche de fer : < 1 s");
    let cobble = id(&items, "cobblestone");
    assert_eq!(h.inv().count_of(cobble), 1, "la pierre donne du pavé");
    assert_eq!(h.inv().slot(27).unwrap().wear, 1, "la pioche s'use");
}

#[test]
fn wooden_pickaxe_cannot_mine_iron_ore() {
    let (mut h, items) = arena();
    let ore = h.reg.get("iron_ore").id;
    aim_at_block(&mut h, ore);
    h.inv_mut()
        .set_slot(27, Some(ItemStack::new(id(&items, "wood_pickaxe"), 1)));
    h.hold(GameAction::Primary);
    secs(&mut h, 10.0);
    assert_eq!(h.block(0, 66, -3), h.ids().air, "le bloc finit par céder");
    assert_eq!(h.inv().count_of(id(&items, "raw_iron")), 0, "mais sans butin");
    assert_eq!(h.inv().count_of(ore.into()), 0);
}

#[test]
fn creative_mode_still_breaks_instantly() {
    let (mut h, _items) = Harness::new().with_survival();
    h.stone_floor();
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Creative);
    h.ticks(3);
    let stone = h.ids().stone;
    h.put(0, 66, -3, stone);
    h.press(GameAction::Primary);
    assert_eq!(h.block(0, 66, -3), h.ids().air);
}

#[test]
fn bedrock_is_unbreakable() {
    let (mut h, _) = arena();
    let bedrock = h.reg.get("bedrock").id;
    aim_at_block(&mut h, bedrock);
    h.hold(GameAction::Primary);
    h.ticks(400);
    assert_eq!(h.block(0, 66, -3), bedrock);
}

#[test]
fn tool_breaks_at_the_end_of_its_durability() {
    let (mut h, items) = arena();
    let pick = id(&items, "wood_pickaxe");
    h.inv_mut()
        .set_slot(27, Some(ItemStack::new(pick, 1).with_wear(58)));
    let dirt = h.ids().dirt;
    aim_at_block(&mut h, dirt);
    h.hold(GameAction::Primary);
    secs(&mut h, 3.0);
    // La pioche (usure 58/59) casse au premier bloc de terre cassé.
    assert!(h.inv().slot(27).is_none(), "outil cassé");
}

#[test]
fn hunger_drains_and_food_restores_it() {
    let (mut h, items) = arena();
    set_vitals(&mut h, |v| {
        v.hunger = 8.0;
        v.saturation = 0.0;
    });
    h.inv_mut()
        .set_slot(27, Some(ItemStack::new(id(&items, "cooked_meat"), 2)));
    let before = vitals(&h).hunger;
    h.hold(GameAction::Secondary);
    secs(&mut h, 2.0); // une portion mangée
    let v = vitals(&h);
    assert!(v.hunger > before + 6.0, "faim {} → {}", before, v.hunger);
    assert_eq!(h.inv().slot(27), Some(ItemStack::new(id(&items, "cooked_meat"), 1)));
    assert!(h
        .take_events()
        .iter()
        .any(|e| matches!(e, EcsEvent::Ate { .. })));
}

#[test]
fn hunger_slowly_decreases_with_time() {
    let (mut h, _) = arena();
    set_vitals(&mut h, |v| v.saturation = 0.0);
    secs(&mut h, 60.0);
    let v = vitals(&h);
    assert!(v.hunger < 19.5 && v.hunger > 17.0, "{}", v.hunger);
}

#[test]
fn starvation_hurts_but_never_kills() {
    let (mut h, _) = arena();
    set_vitals(&mut h, |v| {
        v.hunger = 0.0;
        v.saturation = 0.0;
    });
    secs(&mut h, 120.0);
    let hp = h.h(h.player()).health;
    assert!(hp >= 1.0 && hp < 10.0, "santé {hp}");
}

#[test]
fn cold_biome_chills_and_fur_and_fire_keep_warm() {
    let snowy = nb_world::Biome::SnowyPeaks;
    // Sans protection.
    let (mut h, _) = arena();
    h.blocks.set_biome(Some(snowy));
    secs(&mut h, 30.0);
    let bare = vitals(&h).warmth;
    assert!(bare < 75.0, "chaleur nue {bare}");

    // Avec un équipement de fourrure complet.
    let (mut h, items) = arena();
    h.blocks.set_biome(Some(snowy));
    for (slot, name) in [
        (ArmorSlot::Head, "fur_cap"),
        (ArmorSlot::Chest, "fur_tunic"),
        (ArmorSlot::Legs, "fur_trousers"),
        (ArmorSlot::Feet, "fur_boots"),
    ] {
        h.inv_mut()
            .set_armor(slot, Some(ItemStack::new(id(&items, name), 1)));
    }
    secs(&mut h, 30.0);
    let furred = vitals(&h).warmth;
    assert!(furred > bare + 15.0, "fourrure {furred} vs nu {bare}");

    // Un feu de camp à côté.
    let (mut h, _) = arena();
    h.blocks.set_biome(Some(snowy));
    let camp = h.reg.get("campfire").id;
    h.put(1, 65, 0, camp);
    secs(&mut h, 30.0);
    let fire = vitals(&h).warmth;
    assert!(fire > bare + 15.0, "feu {fire} vs nu {bare}");
}

#[test]
fn freezing_damages_the_player() {
    let (mut h, _) = arena();
    h.blocks.set_biome(Some(nb_world::Biome::IceSpikes));
    set_vitals(&mut h, |v| v.warmth = 0.0);
    secs(&mut h, 20.0);
    assert!(h.h(h.player()).health < 10.0);
}

#[test]
fn armor_reduces_mob_damage_value() {
    let (mut h, items) = arena();
    for (slot, name) in [
        (ArmorSlot::Head, "iron_helmet"),
        (ArmorSlot::Chest, "iron_chestplate"),
        (ArmorSlot::Legs, "iron_leggings"),
        (ArmorSlot::Feet, "iron_boots"),
    ] {
        h.inv_mut()
            .set_armor(slot, Some(ItemStack::new(id(&items, name), 1)));
    }
    h.ticks(2);
    let v = vitals(&h);
    assert_eq!(v.defense, 15.0);
    assert!((v.reduction - 0.45).abs() < 1e-3);
}

#[test]
fn right_click_equips_armor_and_swaps() {
    let (mut h, items) = arena();
    let cap = id(&items, "fur_cap");
    let helm = id(&items, "iron_helmet");
    h.inv_mut().set_slot(27, Some(ItemStack::new(cap, 1)));
    h.press(GameAction::Secondary);
    assert_eq!(h.inv().armor(ArmorSlot::Head).map(|s| s.item), Some(cap));
    assert!(h.inv().slot(27).is_none());
    h.release(GameAction::Secondary);
    h.inv_mut().set_slot(27, Some(ItemStack::new(helm, 1)));
    h.press(GameAction::Secondary);
    assert_eq!(h.inv().armor(ArmorSlot::Head).map(|s| s.item), Some(helm));
    assert_eq!(h.inv().slot(27).map(|s| s.item), Some(cap), "échange");
}

#[test]
fn hoe_tills_and_seeds_grow_into_wheat() {
    let (mut h, items) = arena();
    // On vise la dalle de pierre ? Non : de la terre en (0, 66, −3), air au-dessus.
    let dirt = h.ids().dirt;
    h.put(0, 66, -3, dirt);
    h.inv_mut()
        .set_slot(27, Some(ItemStack::new(id(&items, "wood_hoe"), 1)));
    h.press(GameAction::Secondary);
    let farm = h.reg.get("farmland").id;
    assert_eq!(h.block(0, 66, -3), farm, "terre labourée");
    h.release(GameAction::Secondary);
    h.inv_mut()
        .set_slot(27, Some(ItemStack::new(id(&items, "wheat_seeds"), 3)));
    h.press(GameAction::Secondary);
    let w0 = h.reg.get("wheat_0").id;
    assert_eq!(h.block(0, 67, -3), w0, "graine plantée");
    assert_eq!(h.inv().count_of(id(&items, "wheat_seeds")), 2);
    // Après un long moment, la culture est mûre (stade final = `wheat`).
    let wheat = h.reg.get("wheat").id;
    h.release(GameAction::Secondary);
    let mut grown = false;
    for _ in 0..400 {
        h.ticks(100);
        if h.block(0, 67, -3) == wheat {
            grown = true;
            break;
        }
    }
    assert!(grown, "le blé finit par mûrir");
    assert!(h
        .take_events()
        .iter()
        .any(|e| matches!(e, EcsEvent::CropGrew { .. })));
}

#[test]
fn harvesting_ripe_wheat_gives_grain_and_seeds() {
    let (mut h, items) = arena();
    let wheat = h.reg.get("wheat").id;
    h.put(0, 66, -3, wheat);
    h.hold(GameAction::Primary);
    h.ticks(10);
    assert_eq!(h.block(0, 66, -3), h.ids().air);
    assert!(h.inv().count_of(id(&items, "wheat_grain")) >= 1);
}

#[test]
fn fishing_catches_something_from_water() {
    let (mut h, items) = arena();
    let water = h.ids().water;
    // Un bassin devant le joueur, à hauteur des yeux vers le bas.
    h.slab(-3, 3, 65, -8, -2, water);
    h.p_mut().camera.pitch_deg = 20.0; // vers le bas
    h.inv_mut()
        .set_slot(27, Some(ItemStack::new(id(&items, "fishing_rod"), 1)));
    h.press(GameAction::Secondary);
    let events = h.take_events();
    assert!(
        events.iter().any(|e| matches!(e, EcsEvent::Cast { .. })),
        "la ligne est lancée : {events:?}"
    );
    h.release(GameAction::Secondary);
    // Attendre la touche.
    let mut bit = false;
    for _ in 0..(60 * 15) {
        h.ticks(1);
        if h.take_events()
            .iter()
            .any(|e| matches!(e, EcsEvent::FishBite { .. }))
        {
            bit = true;
            break;
        }
    }
    assert!(bit, "un poisson mord");
    h.press(GameAction::Secondary);
    assert!(h
        .take_events()
        .iter()
        .any(|e| matches!(e, EcsEvent::Caught { .. })));
    assert!(vitals(&h).fishing.is_none());
}

#[test]
fn killing_a_mob_with_a_sword_drops_loot() {
    let (mut h, items) = arena();
    h.mobs.get("hare").unwrap();
    h.p_mut().camera.pitch_deg = 25.0;
    let mob = h.spawn_mob("hare", 0.5, 65.0, -2.0);
    h.inv_mut()
        .set_slot(27, Some(ItemStack::new(id(&items, "iron_sword"), 1)));
    let mut killed = false;
    for _ in 0..12 {
        h.press(GameAction::Primary);
        h.ticks(10);
        if !h.sim.entities.contains(mob) {
            killed = true;
            break;
        }
    }
    assert!(killed, "le lièvre meurt");
    let inv = h.inv();
    let total: i32 = ["raw_meat", "pelt", "bone"]
        .iter()
        .map(|n| inv.count_of(id(&items, n)))
        .sum();
    assert!(total >= 1, "butin attendu");
}

#[test]
fn sprint_needs_stamina() {
    let (mut h, _) = arena();
    set_vitals(&mut h, |v| {
        v.stamina = 0.0;
        v.exhausted = true;
    });
    h.hold(GameAction::Forward);
    h.hold(GameAction::Sprint);
    h.ticks(10);
    assert!(!vitals(&h).sprinting, "épuisé : pas de sprint");
    set_vitals(&mut h, |v| {
        v.stamina = 100.0;
        v.exhausted = false;
    });
    h.ticks(15);
    assert!(vitals(&h).sprinting);
    assert!(vitals(&h).stamina < 100.0);
}

#[test]
fn tools_and_armor_do_not_stack() {
    let (_h, items) = arena();
    let surv = nb_data::SurvivalData::builtin(
        &nb_data::BlockRegistry::builtin_with_craft(),
        &items,
    );
    let unstackable: Vec<_> = items
        .iter()
        .filter(|i| surv.max_durability(i.id) > 0)
        .map(|i| i.id)
        .collect();
    nb_ecs::inventory::set_unstackable(unstackable);
    let mut inv = nb_ecs::Inventory::new();
    let pick = id(&items, "iron_pickaxe");
    inv.add_stack(ItemStack::new(pick, 1));
    inv.add_stack(ItemStack::new(pick, 1));
    let n = inv.slots().iter().flatten().filter(|s| s.item == pick).count();
    assert_eq!(n, 2, "deux piles distinctes");
    nb_ecs::inventory::set_unstackable(std::iter::empty());
    let _ = IVec3::ZERO;
}
