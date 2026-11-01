//! Tests de comportement : raycasts (§8), casser / poser / jeter (§6.13), coups sur les mobs
//! (§9.1).

mod common;

use common::Harness;
use nb_core::glam::{DVec3, IVec3, Vec2, Vec3};
use nb_ecs::physics::{ray_aabb, raycast_mob, BlockEnv};
use nb_ecs::{EcsEvent, GameAction, GameMode, ItemStack, PassiveAi, RaycastHit};

/// Sol en y = 64, mur de pierre en z = −3 (y 65..=67), joueur posé en (0.5, 65, 0.5).
fn arena(mode: GameMode) -> Harness {
    let mut h = Harness::new();
    h.stone_floor();
    let stone = h.ids().stone;
    for y in 65..68 {
        h.slab(-5, 5, y, -3, -3, stone);
    }
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), mode);
    h.ticks(5);
    h
}

#[test]
fn block_raycast_steps_and_neighbours() {
    let h = arena(GameMode::Survival);
    let ids = h.ids();
    let env = BlockEnv::new(&h.blocks, &h.reg);
    let o = Vec3::new(0.5, 66.5, 0.5);
    let fwd = Vec3::new(0.0, 0.0, -1.0);
    assert_eq!(
        env.raycast(o, fwd, 6.0),
        Some(RaycastHit {
            hit: IVec3::new(0, 66, -3),
            adjacent: IVec3::new(0, 66, -2),
        })
    );
    // Hors de portée.
    assert_eq!(env.raycast(o, Vec3::new(0.0, 0.0, 1.0), 6.0), None);
    // Origine dans un bloc : voisin = bloc touché.
    let inside = Vec3::new(0.5, 66.5, -2.5);
    let r = env.raycast(inside, fwd, 6.0).unwrap();
    assert_eq!(r.hit, r.adjacent);

    // Plantes touchées, fluides traversés.
    let mut h2 = Harness::new();
    h2.put(0, 66, -1, ids.water);
    h2.put(0, 66, -2, ids.lava);
    h2.put(0, 66, -3, ids.tall_grass);
    let env = BlockEnv::new(&h2.blocks, &h2.reg);
    assert_eq!(env.raycast(o, fwd, 6.0).unwrap().hit, IVec3::new(0, 66, -3));

    // Voisin diagonal (B8) : le rayon franchit un coin entre deux échantillons.
    let mut h3 = Harness::new();
    h3.put(1, 66, -1, ids.stone);
    let env = BlockEnv::new(&h3.blocks, &h3.reg);
    let diag = Vec3::new(1.0, 0.0, -1.0).normalize();
    let r = env.raycast(o, diag, 6.0).unwrap();
    assert_eq!(r.hit, IVec3::new(1, 66, -1));
    assert_eq!(r.adjacent, IVec3::new(0, 66, 0));
}

#[test]
fn slab_ray_aabb() {
    let o = Vec3::ZERO;
    let d = Vec3::new(1.0, 0.0, 0.0);
    let t = ray_aabb(o, d, Vec3::new(2.0, -1.0, -1.0), Vec3::new(3.0, 1.0, 1.0));
    assert_eq!(t, 2.0);
    assert_eq!(ray_aabb(o, -d, Vec3::new(2.0, -1.0, -1.0), Vec3::new(3.0, 1.0, 1.0)), -1.0);
    // Origine dans la boîte : t de sortie.
    let t = ray_aabb(o, d, Vec3::splat(-1.0), Vec3::splat(1.0));
    assert_eq!(t, 1.0);
    // Composante nulle hors de la tranche.
    assert_eq!(ray_aabb(o, d, Vec3::new(2.0, 1.0, -1.0), Vec3::new(3.0, 2.0, 1.0)), -1.0);
}

#[test]
fn targeted_block_follows_camera_and_breaking_fills_inventory() {
    let mut h = arena(GameMode::Survival);
    let stone = h.ids().stone;
    let p = h.p();
    assert_eq!(p.targeted_block, Some(IVec3::new(0, 66, -3)));
    assert_eq!(p.place_target, Some(IVec3::new(0, 66, -2)));
    h.take_events();

    h.press(GameAction::Primary);
    assert_eq!(h.block(0, 66, -3), h.ids().air);
    assert_eq!(h.inv().slot(27), Some(ItemStack::new(stone, 1)));
    assert!(h.take_events().contains(&EcsEvent::BlockBroken {
        pos: IVec3::new(0, 66, -3),
        block: stone,
    }));
    // Le rayon passe désormais par le trou jusqu'au néant : plus de cible.
    h.tick();
    assert_eq!(h.p().targeted_block, None);
}

#[test]
fn bedrock_breaks_and_creative_also_collects() {
    let mut h = arena(GameMode::Creative);
    let bedrock = h.ids().bedrock;
    h.put(0, 66, -3, bedrock);
    h.press(GameAction::Primary);
    assert_eq!(h.block(0, 66, -3), h.ids().air, "aucune dureté (B19)");
    assert_eq!(h.inv().slot(27), Some(ItemStack::new(bedrock, 1)));
}

#[test]
fn placing_consumes_held_block_even_in_creative() {
    for mode in [GameMode::Survival, GameMode::Creative] {
        let mut h = arena(mode);
        let dirt = h.ids().dirt;
        h.inv_mut().set_slot(27, Some(ItemStack::new(dirt, 2)));
        h.press(GameAction::Secondary);
        assert_eq!(h.block(0, 66, -2), dirt);
        assert_eq!(h.inv().slot(27), Some(ItemStack::new(dirt, 1)));
        assert!(h.take_events().contains(&EcsEvent::BlockPlaced {
            pos: IVec3::new(0, 66, -2),
            block: dirt,
        }));
        // Le bloc posé devient la cible ; le casser le rend à l'inventaire.
        h.tick();
        assert_eq!(h.p().targeted_block, Some(IVec3::new(0, 66, -2)));
        h.press(GameAction::Primary);
        assert_eq!(h.inv().slot(27), Some(ItemStack::new(dirt, 2)));
    }
}

#[test]
fn placing_into_own_body_or_with_empty_hand_is_refused() {
    let mut h = arena(GameMode::Survival);
    let dirt = h.ids().dirt;
    // Main vide.
    h.press(GameAction::Secondary);
    assert_eq!(h.block(0, 66, -2), h.ids().air);
    // Regard vers le sol : la case voisine est celle des pieds.
    h.inv_mut().set_slot(27, Some(ItemStack::new(dirt, 1)));
    h.p_mut().camera.pitch_deg = 89.0;
    h.tick();
    assert_eq!(h.p().place_target, Some(IVec3::new(0, 65, 0)));
    h.press(GameAction::Secondary);
    assert_eq!(h.block(0, 65, 0), h.ids().air);
    assert_eq!(h.inv().slot(27), Some(ItemStack::new(dirt, 1)));
}

#[test]
fn drop_destroys_one_item_of_selected_slot() {
    let mut h = arena(GameMode::Survival);
    let dirt = h.ids().dirt;
    h.inv_mut().set_slot(28, Some(ItemStack::new(dirt, 2)));
    h.input.selected_slot = 1;
    h.press(GameAction::Drop);
    assert_eq!(h.inv().slot(28), Some(ItemStack::new(dirt, 1)));
    h.press(GameAction::Drop);
    assert_eq!(h.inv().slot(28), None);
    assert_eq!(h.sim.entities.len(), 1, "aucune entité d'objet");
}

#[test]
fn unlocked_mouse_disables_rotation_and_actions() {
    let mut h = arena(GameMode::Survival);
    h.input.mouse_delta = Vec2::new(100.0, 50.0);
    h.tick();
    let cam = h.p().camera;
    assert!((cam.yaw_deg - 10.0).abs() < 1e-5, "sensibilité par défaut 0.1");
    assert!((cam.pitch_deg - 5.0).abs() < 1e-5);

    h.p_mut().camera.yaw_deg = 0.0;
    h.p_mut().camera.pitch_deg = 0.0;
    h.input.mouse_locked = false;
    h.input.mouse_delta = Vec2::new(100.0, 50.0);
    h.press(GameAction::Primary);
    assert_eq!(h.p().camera.yaw_deg, 0.0);
    assert_eq!(h.block(0, 66, -3), h.ids().stone);
    // La visée continue d'être calculée.
    assert_eq!(h.p().targeted_block, Some(IVec3::new(0, 66, -3)));
}

#[test]
fn hitting_a_passive_mob_damages_flees_and_knocks_back() {
    let mut h = arena(GameMode::Survival);
    // AABB en z ∈ [−1.69, −0.31] : hors des cellules du mur (sinon tout mouvement collisionne).
    let mob = h.spawn_mob("reindeer", 0.5, 65.0, -1.0);
    h.press(GameAction::Primary);
    assert_eq!(h.block(0, 66, -3), h.ids().stone, "le mob est prioritaire sur le bloc");
    assert_eq!(h.h(mob).health, 4.0);
    let ai = h.sim.entities.copy::<PassiveAi>(mob).unwrap();
    assert!(ai.flee_timer > 2.9 && ai.moving);
    let v = h.v(mob);
    assert!(v.v.y > 3.0, "recul vertical 4.0 puis gravité : {}", v.v.y);
    assert!(v.v.z < -2.0, "recul à l'opposé du joueur : {}", v.v.z);
    assert!(h.take_events().contains(&EcsEvent::MobHit { entity: mob }));
}

#[test]
fn mob_raycast_ignores_walls_and_range() {
    let mut h = arena(GameMode::Survival);
    // Derrière le mur, à 4.5 : touché quand même (B9).
    let behind = h.spawn_mob("reindeer", 0.5, 65.0, -4.0);
    h.press(GameAction::Primary);
    assert_eq!(h.h(behind).health, 4.0);
    assert_eq!(h.block(0, 66, -3), h.ids().stone);

    // Au-delà de 5 : le bloc est cassé.
    let mut h = arena(GameMode::Survival);
    let far = h.spawn_mob("reindeer", 0.5, 65.0, -6.0);
    h.press(GameAction::Primary);
    assert_eq!(h.h(far).health, 5.0);
    assert_eq!(h.block(0, 66, -3), h.ids().air);

    // Le plus proche des deux est retenu.
    let mut h = Harness::new();
    let a = h.spawn_mob("reindeer", 0.5, 0.0, -2.0);
    let b = h.spawn_mob("reindeer", 0.5, 0.0, -3.5);
    let o = Vec3::new(0.5, 1.0, 0.5);
    let d = Vec3::new(0.0, 0.0, -1.0);
    assert_eq!(raycast_mob(&h.sim.entities, o, d), Some(a));
    h.sim.entities.destroy(a);
    assert_eq!(raycast_mob(&h.sim.entities, o, d), Some(b));
}

#[test]
fn killing_blow_destroys_mob_in_same_tick_and_aggressors_do_not_flee() {
    let mut h = arena(GameMode::Survival);
    assert_eq!(h.cmd("/player damage 10").unwrap(), "Attack damage set to 10.0.");
    let mob = h.spawn_mob("reindeer", 0.5, 65.0, -1.5);
    h.take_events();
    h.press(GameAction::Primary);
    assert!(!h.sim.entities.contains(mob));
    let ev = h.take_events();
    assert!(ev.contains(&EcsEvent::MobHit { entity: mob }));
    assert!(ev.contains(&EcsEvent::MobDied { entity: mob }));

    let mut h = Harness::new();
    h.stone_floor();
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Creative);
    let orca = h.spawn_mob("orca", 0.5, 65.0, -3.0);
    h.press(GameAction::Primary);
    assert_eq!(h.h(orca).health, 39.0);
    assert!(h.v(orca).v.y > 0.0, "recul appliqué");
}
