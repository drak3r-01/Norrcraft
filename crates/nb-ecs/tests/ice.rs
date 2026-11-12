//! Glace jouable (session 6.1, doc 04 §4.1 et §4.2) : charge, craquement, chute, remontée,
//! forage, pêche, scie, glacière.

mod common;

use common::Harness;
use nb_core::glam::{DVec3, IVec3};
use nb_core::JavaRandom;
use nb_data::{BlockId, BlockRegistry, MobRegistry, WorldKind};
use nb_ecs::ice::{self, Climb, IceHandle, IceKind, IceSystem, Icehouse, LineKind, Load};
use nb_ecs::{EcsEvent, GameAction, GameMode, MemoryBlocks};

/// Lac de Norrcraft : eau de y = 58 à 63, glace `kind` en y = 64 sur ±8, berge de granit autour.
fn lake(kind: &str) -> (Harness, IceHandle) {
    let mut h = Harness::new();
    h.reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    h.blocks = MemoryBlocks::new(h.reg.ids().air);
    h.mobs = MobRegistry::builtin_for(WorldKind::Norrcraft);
    for c in h.mobs.all_mut() {
        c.max_count = 0;
        c.spawn = None;
    }
    h.cfg.fall_damage_enabled = false;
    h.cfg.mob_despawn_radius = 200;
    let water = h.reg.ids().water;
    let granite = b(&h, "granit");
    let ice = b(&h, kind);
    h.slab(-12, 12, 57, -12, 12, granite);
    for y in 58..=63 {
        h.slab(-8, 8, y, -8, 8, water);
    }
    h.slab(-8, 8, 64, -8, 8, ice);
    for y in 58..=64 {
        // Berge.
        for i in -12..=12 {
            for z in [-12, -11, -10, -9, 9, 10, 11, 12] {
                h.put(i, y, z, granite);
                h.put(z, y, i, granite);
            }
        }
    }
    let handle = IceHandle::default();
    let at = h
        .sim
        .schedule
        .names()
        .iter()
        .position(|n| *n == "Health")
        .unwrap();
    h.sim
        .schedule
        .insert(at, Box::new(IceSystem::new(handle.clone(), None)));
    (h, handle)
}

fn b(h: &Harness, name: &str) -> BlockId {
    let id = h.reg.get(name).id;
    assert_ne!(id, h.reg.ids().air, "bloc {name} inconnu");
    id
}

fn secs(h: &mut Harness, s: f32) {
    h.ticks((s * 60.0) as usize);
}

#[test]
fn kinds_follow_the_block_names() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let k = |n: &str| IceKind::of(&reg, reg.get(n).id);
    assert_eq!(k("nyis"), Some(IceKind::New));
    assert_eq!(k("is"), Some(IceKind::Bearing));
    assert_eq!(k("tjockis"), Some(IceKind::Thick));
    assert_eq!(k("havsis"), Some(IceKind::Sea));
    assert_eq!(k("blais"), Some(IceKind::Pack));
    assert_eq!(k("granit"), None);
}

#[test]
fn carrying_table_matches_the_design_doc() {
    use IceKind::*;
    use Load::*;
    // Neuve : à plat ventre et animal léger seulement.
    assert!(New.carries(Prone, false) && New.carries(Light, false));
    assert!(!New.carries(Walker, false) && !New.carries(Heavy, false) && !New.carries(Sledge, false));
    // Portante : joueur, renne, traîneau ; pas l'animal lourd ni le traîneau chargé.
    assert!(Bearing.carries(Walker, false) && Bearing.carries(Sledge, false));
    assert!(!Bearing.carries(Heavy, false) && !Bearing.carries(LoadedSledge, false));
    // Épaisse et banquise : tout.
    for l in [Prone, Walker, Light, Heavy, Sledge, LoadedSledge] {
        assert!(Thick.carries(l, false) && Pack.carries(l, true));
    }
    // Près de l'eau libre, un rang de moins ; la banquise y échappe.
    assert!(!Bearing.carries(Walker, true));
    assert!(!Thick.carries(Heavy, true) && Thick.carries(Walker, true));
    assert!(!Sea.carries(Walker, true) && Sea.carries(Walker, false));
}

#[test]
fn open_water_makes_the_ice_fragile() {
    let (mut h, _) = lake("is");
    let solid = IVec3::new(0, 64, 0);
    assert!(!ice::fragile(&h.blocks, &h.reg, solid));
    assert!(ice::bears(&h.blocks, &h.reg, solid, Load::Walker));
    let w = h.reg.ids().water;
    h.put(2, 64, 0, w);
    assert!(ice::fragile(&h.blocks, &h.reg, solid));
    assert!(!ice::bears(&h.blocks, &h.reg, solid, Load::Walker));
    assert!(ice::bears(&h.blocks, &h.reg, solid, Load::Prone));
    // À trois cases du trou, la glace tient.
    assert!(ice::bears(&h.blocks, &h.reg, IVec3::new(-3, 64, 0), Load::Walker));
}

#[test]
fn new_ice_cracks_then_gives_way_under_a_walker() {
    let (mut h, _) = lake("nyis");
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    h.tick();
    secs(&mut h, 0.3);
    let ev = h.take_events();
    assert!(ev.iter().any(|e| matches!(e, EcsEvent::IceCrack { .. })));
    // Pas encore rompue.
    assert_eq!(h.block(0, 64, 0), b(&h, "nyis"));
    secs(&mut h, 2.0);
    let ev = h.take_events();
    assert!(ev.iter().any(|e| matches!(e, EcsEvent::IceBroke { .. })));
    let w = h.reg.ids().water;
    assert_eq!(h.block(0, 64, 0), w);
    // Une croix de glace a cédé, pas tout le lac.
    assert_eq!(h.block(1, 64, 1), w);
    assert_eq!(h.block(5, 64, 5), b(&h, "nyis"));
    // Le joueur tombe à l'eau.
    secs(&mut h, 1.0);
    assert!(h.t(h.player()).pos.y < 64.5);
}

#[test]
fn lying_flat_spares_new_ice() {
    let (mut h, _) = lake("nyis");
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), GameMode::Survival);
    h.hold(GameAction::Crouch);
    secs(&mut h, 4.0);
    assert_eq!(h.block(0, 64, 0), b(&h, "nyis"));
    assert!(!h
        .take_events()
        .iter()
        .any(|e| matches!(e, EcsEvent::IceCrack { .. })));
}

#[test]
fn bearing_ice_carries_a_player_but_not_a_moose() {
    let (mut h, _) = lake("is");
    h.spawn_player(DVec3::new(-5.5, 65.0, -5.5), GameMode::Survival);
    let elk = h.spawn_mob("alg", 4.5, 65.0, 4.5);
    secs(&mut h, 3.0);
    assert_eq!(h.block(-5, 64, -5), b(&h, "is"), "le joueur est porté");
    let w = h.reg.ids().water;
    assert_eq!(h.block(4, 64, 4), w, "l'élan a traversé");
    assert!(h.t(elk).pos.y < 64.6);
}

#[test]
fn thick_ice_carries_the_moose() {
    let (mut h, _) = lake("tjockis");
    let elk = h.spawn_mob("alg", 4.5, 65.0, 4.5);
    secs(&mut h, 3.0);
    assert_eq!(h.block(4, 64, 4), b(&h, "tjockis"));
    assert!(h.t(elk).pos.y > 64.9);
}

#[test]
fn pack_ice_never_breaks() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let mut m = MemoryBlocks::new(reg.ids().air);
    let blais = reg.get("blais").id;
    m.put(0, 64, 0, blais);
    assert_eq!(ice::collapse(&mut m, &reg, IVec3::new(0, 64, 0)), 0);
    assert!(!ice::drill(&mut m, &reg, IVec3::new(0, 64, 0)));
}

#[test]
fn climbing_out_needs_picks_or_luck() {
    let (mut h, _) = lake("is");
    let w = h.reg.ids().water;
    let mut rng = JavaRandom::new(7);
    // Un trou à (0, 64, 0) ; on nage dedans, la poitrine à y = 64.
    h.put(0, 64, 0, w);
    let feet = IVec3::new(0, 64, 0);
    // Avec poinçons : toujours.
    for _ in 0..50 {
        let r = ice::climb_out(&mut h.blocks, &h.reg, feet, true, &mut rng);
        assert!(matches!(r, Climb::Out { to } if to.y == 65));
    }
    // Sans : environ un tiers, et le bord casse sinon.
    let mut ok = 0;
    let mut broke = 0;
    for _ in 0..300 {
        // Rétablit la glace autour.
        let ice_id = b(&h, "is");
        for d in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
            h.put(d.x, 64, d.z, ice_id);
        }
        match ice::climb_out(&mut h.blocks, &h.reg, feet, false, &mut rng) {
            Climb::Out { .. } => ok += 1,
            Climb::EdgeBroke { at } => {
                broke += 1;
                assert_eq!(h.block(at.x, at.y, at.z), w);
            }
            Climb::NoEdge => panic!("bord attendu"),
        }
    }
    assert_eq!(ok + broke, 300);
    let share = ok as f32 / 300.0;
    assert!((0.22..0.45).contains(&share), "part de réussite {share}");
}

#[test]
fn a_swimmer_climbs_out_with_the_jump_key() {
    let (mut h, _) = lake("is");
    let w = h.reg.ids().water;
    h.put(0, 64, 0, w);
    h.put(0, 64, 1, w);
    // Joueur dans le trou, une glace voisine à (1, 64, 0).
    let p = h.spawn_player(DVec3::new(0.5, 63.2, 0.5), GameMode::Survival);
    h.hold(GameAction::Jump);
    let mut out = false;
    for _ in 0..300 {
        h.tick();
        if h.t(p).pos.y >= 64.9 {
            out = true;
            break;
        }
    }
    h.release(GameAction::Jump);
    assert!(out, "le joueur remonte sans poinçons au bout de quelques essais");
}

#[test]
fn drilling_and_sawing() {
    let (mut h, _) = lake("is");
    let p = IVec3::new(2, 64, 2);
    assert!(ice::drill(&mut h.blocks, &h.reg, p));
    assert_eq!(h.block(2, 64, 2), h.reg.ids().water);
    assert!(ice::is_hole(&h.blocks, &h.reg, p));
    // On ne perce pas la berge, ni l'air.
    assert!(!ice::drill(&mut h.blocks, &h.reg, IVec3::new(0, 70, 0)));
    assert!(!ice::drill(&mut h.blocks, &h.reg, IVec3::new(-10, 64, 0)));
    // Scie : la neuve est refusée, la portante donne 1, l'épaisse 2.
    let q = IVec3::new(-3, 64, -3);
    assert_eq!(ice::saw(&mut h.blocks, &h.reg, q), Some(("isblock", 1)));
    assert_eq!(h.block(-3, 64, -3), h.reg.ids().water);
    let thick = b(&h, "tjockis");
    let nyis = b(&h, "nyis");
    h.put(5, 64, 5, thick);
    h.put(6, 64, 6, nyis);
    assert_eq!(ice::saw(&mut h.blocks, &h.reg, IVec3::new(5, 64, 5)), Some(("isblock", 2)));
    assert_eq!(ice::saw(&mut h.blocks, &h.reg, IVec3::new(6, 64, 6)), None);
}

#[test]
fn ice_fishing_with_a_jig_and_a_tip_up() {
    let (mut h, handle) = lake("is");
    let hole = IVec3::new(3, 64, 3);
    let hole2 = IVec3::new(-3, 64, -3);
    assert!(ice::drill(&mut h.blocks, &h.reg, hole));
    assert!(ice::drill(&mut h.blocks, &h.reg, hole2));
    let mut rng = JavaRandom::new(11);
    let (jig, tip) = {
        let mut st = handle.lock().unwrap();
        (
            st.set_line(LineKind::Jig, hole, &mut rng),
            st.set_line(LineKind::TipUp, hole2, &mut rng),
        )
    };
    // Ferrer trop tôt ne prend rien.
    assert_eq!(handle.lock().unwrap().reel(jig, 0, &mut rng), None);
    // La canne mord dans la minute ; ferrer dans la fenêtre prend un poisson.
    let mut caught = None;
    for _ in 0..(60 * 60) {
        h.tick();
        let bite = handle.lock().unwrap().line(jig).is_some_and(|l| l.bite);
        if bite {
            caught = handle.lock().unwrap().reel(jig, 0, &mut rng);
            break;
        }
    }
    let (name, n) = caught.expect("une touche à la canne");
    assert!(["abborre", "gadda", "sik", "roding", "harr"].contains(&name) && n >= 1);
    // La dormante lève son drapeau en quelques minutes et garde la prise.
    let mut flagged = false;
    for _ in 0..(60 * 200) {
        h.tick();
        if handle.lock().unwrap().line(tip).is_some_and(|l| l.bite) {
            flagged = true;
            break;
        }
    }
    assert!(flagged, "le drapeau se lève");
    assert!(h
        .take_events()
        .iter()
        .any(|e| matches!(e, EcsEvent::FishBite { .. })));
    secs(&mut h, 10.0);
    assert!(handle.lock().unwrap().line(tip).unwrap().bite, "la prise attend");
    assert!(handle.lock().unwrap().reel(tip, 0, &mut rng).is_some());
    // Le trou regèle : la ligne disparaît.
    let ice_id = b(&h, "is");
    h.put(hole2.x, hole2.y, hole2.z, ice_id);
    h.tick();
    assert!(handle.lock().unwrap().line(tip).is_none());
    assert!(handle.lock().unwrap().line(jig).is_some());
}

#[test]
fn winter_catch_includes_char_summer_does_not() {
    let mut rng = JavaRandom::new(3);
    let mut winter_char = 0;
    let mut summer_char = 0;
    for _ in 0..2000 {
        if ice::catch(0, &mut rng).0 == "roding" {
            winter_char += 1;
        }
        if ice::catch(6, &mut rng).0 == "roding" {
            summer_char += 1;
        }
    }
    assert!(winter_char > 100);
    assert_eq!(summer_char, 0);
}

#[test]
fn under_ice_seine_needs_two_holes_and_two_hands() {
    let (mut h, _) = lake("is");
    let a = IVec3::new(-4, 64, 0);
    let c = IVec3::new(5, 64, 0);
    ice::drill(&mut h.blocks, &h.reg, a);
    ice::drill(&mut h.blocks, &h.reg, c);
    let mut rng = JavaRandom::new(5);
    assert!(ice::seine(&h.blocks, &h.reg, a, c, 1, 1, &mut rng).is_none());
    let big = ice::seine(&h.blocks, &h.reg, a, c, 2, 1, &mut rng).unwrap();
    let total: u32 = big.iter().map(|x| x.1).sum();
    assert!(total >= 6, "grosse prise : {total}");
    // Trous trop proches, ou pas un trou.
    let near = IVec3::new(-3, 64, 0);
    ice::drill(&mut h.blocks, &h.reg, near);
    assert!(ice::seine(&h.blocks, &h.reg, a, near, 2, 1, &mut rng).is_none());
    assert!(ice::seine(&h.blocks, &h.reg, a, IVec3::new(0, 64, 4), 2, 1, &mut rng).is_none());
}

#[test]
fn sawdust_keeps_the_ice_through_the_summer() {
    let mut bare = Icehouse::new(64, false);
    let mut packed = Icehouse::new(64, true);
    bare.tick_days(120.0, 0.6);
    packed.tick_days(120.0, 0.6);
    assert!(bare.blocks < 1.0, "sans sciure : {}", bare.blocks);
    assert!(packed.blocks > 35.0, "sous la sciure : {}", packed.blocks);
    let before = packed.blocks;
    let got = packed.take(1000);
    assert_eq!(got, before.floor() as u32);
    assert!(packed.blocks < 1.0);
}

#[test]
fn tools_map_from_item_names() {
    use ice::IceTool::*;
    assert_eq!(ice::IceTool::from_item("isborr"), Some(Auger));
    assert_eq!(ice::IceTool::from_item("issag"), Some(Saw));
    assert_eq!(ice::IceTool::from_item("pimpel"), Some(Jig));
    assert_eq!(ice::IceTool::from_item("angeldon"), Some(TipUp));
    assert_eq!(ice::IceTool::from_item("isnot"), Some(Seine));
    assert_eq!(ice::IceTool::from_item("isdubbar"), Some(Picks));
    assert_eq!(ice::IceTool::from_item("stick"), None);
}

#[test]
fn norrcraft_items_exist() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let items = nb_data::ItemRegistry::for_kind(WorldKind::Norrcraft, &reg);
    for n in ["isborr", "pimpel", "angeldon", "issag", "isdubbar", "isnot", "isblock", "abborre"] {
        assert!(items.id_of(n).is_some(), "objet {n}");
    }
}
