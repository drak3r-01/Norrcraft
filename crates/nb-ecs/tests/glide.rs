//! Neige et glisse (session 6.2, doc 04 §4.3 et §4.4) : ralentissement, équipement, piétinement,
//! pente, avalanche et ensevelissement.

mod common;

use common::Harness;
use nb_core::glam::{DVec3, IVec3};
use nb_data::{BlockId, BlockRegistry, MobRegistry, WorldKind};
use nb_ecs::avalanche::{self, AvalancheSystem, Buried, SnowIds};
use nb_ecs::glide::{self, Gear, GearIds, GlideSystem, Surface, Vehicle};
use nb_ecs::ice::{IceKind, Load};
use nb_ecs::inventory::ItemStack;
use nb_ecs::{EcsEvent, GameAction, GameMode, MemoryBlocks};

const SKIS: u16 = 268;
const POLES: u16 = 269;
const SHOES: u16 = 271;
const SKATES: u16 = 272;
const SPARK: u16 = 273;

fn ids() -> GearIds {
    GearIds {
        skis: Some(SKIS),
        poles: Some(POLES),
        wax: None,
        snowshoes: Some(SHOES),
        skates: Some(SKATES),
        spark: Some(SPARK),
    }
}

fn b(h: &Harness, name: &str) -> BlockId {
    let id = h.reg.get(name).id;
    assert_ne!(id, h.reg.ids().air, "bloc {name} inconnu");
    id
}

/// Monde Norrcraft vide, sol de granit dont le dessus est en y = 64, systèmes de glisse posés.
fn field() -> Harness {
    let mut h = Harness::new();
    h.reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    h.blocks = MemoryBlocks::new(h.reg.ids().air);
    h.mobs = MobRegistry::builtin_for(WorldKind::Norrcraft);
    for c in h.mobs.all_mut() {
        c.max_count = 0;
        c.spawn = None;
    }
    h.cfg.fall_damage_enabled = false;
    let granite = b(&h, "granit");
    h.slab(-30, 30, 63, -30, 30, granite);
    let at = h
        .sim
        .schedule
        .names()
        .iter()
        .position(|n| *n == "Health")
        .unwrap();
    h.sim.schedule.insert(0, Box::new(GlideSystem::new(ids())));
    h.sim
        .schedule
        .insert(at + 1, Box::new(AvalancheSystem::ungated()));
    h
}

fn hold(h: &mut Harness, items: &[u16]) {
    for (i, it) in items.iter().enumerate() {
        h.inv_mut().set_slot(i as i32, Some(ItemStack::new(*it, 1)));
    }
    h.input.selected_slot = 0;
}

fn snow_floor(h: &mut Harness, name: &str) {
    let s = b(h, name);
    for x in -30..=30 {
        for z in -30..=30 {
            h.put(x, 64, z, s);
        }
    }
}

/// Distance parcourue en `s` secondes en tenant « avancer ».
fn walk(h: &mut Harness, s: f32, held: &[u16]) -> f64 {
    let id = h.spawn_player(DVec3::new(0.5, 64.0, 0.5), GameMode::Survival);
    hold(h, held);
    h.p_mut().camera.yaw_deg = 0.0;
    h.hold(GameAction::Forward);
    h.ticks((s * 60.0) as usize);
    let p = h.t(id).pos;
    ((p.x - 0.5).powi(2) + (p.z - 0.5).powi(2)).sqrt()
}

#[test]
fn snow_slows_after_three_layers() {
    assert_eq!(glide::snow_slow(0), 1.0);
    assert_eq!(glide::snow_slow(3), 1.0);
    assert!((glide::snow_slow(6) - 0.64).abs() < 1e-6);
    assert!((glide::snow_slow(8) - 0.4).abs() < 1e-6);
    // Jamais plus de −60 %.
    assert!(glide::snow_slow(40) >= 0.4 - 1e-6);
}

#[test]
fn gear_table_matches_the_design_doc() {
    use Surface::*;
    let ski = Gear::Skis {
        poles: true,
        waxed: false,
    };
    let snow = Snow { layers: 6 };
    // Skis : × 1,6 sur neige, sans s'enfoncer ; sans bâtons moins bien.
    assert_eq!(glide::modifier(ski, snow).speed, 1.6);
    assert!(glide::modifier(ski, snow).glide);
    assert!(
        glide::modifier(
            Gear::Skis {
                poles: false,
                waxed: false
            },
            snow
        )
        .speed
            < 1.6
    );
    // Le fart au goudron glisse mieux.
    let waxed = Gear::Skis {
        poles: true,
        waxed: true,
    };
    assert!(
        glide::modifier(waxed, snow).friction.unwrap() < glide::modifier(ski, snow).friction.unwrap()
    );
    // Raquettes : ni ralentissement ni glisse.
    let m = glide::modifier(Gear::Snowshoes, snow);
    assert_eq!((m.speed, m.glide), (1.0, false));
    // Patins : × 2,5 sur glace vive seulement.
    assert_eq!(glide::modifier(Gear::Skates, Ice(IceKind::Thick)).speed, 2.5);
    assert!(glide::modifier(Gear::Skates, snow).speed < 1.0);
    // Luge à pied : × 2 sur glace et neige tassée.
    assert_eq!(glide::modifier(Gear::Spark, Ice(IceKind::Bearing)).speed, 2.0);
    assert_eq!(glide::modifier(Gear::Spark, Packed).speed, 2.0);
    // À pied, la neige profonde ralentit.
    assert!(glide::modifier(Gear::None, snow).speed < 0.7);
    assert_eq!(glide::modifier(Gear::None, Bare).speed, 1.0);
}

#[test]
fn gear_is_chosen_from_the_held_item() {
    let g = ids();
    assert_eq!(g.gear(None, |_| true), Gear::None);
    assert_eq!(g.gear(Some(SHOES), |_| true), Gear::Snowshoes);
    assert_eq!(g.gear(Some(SKATES), |_| false), Gear::Skates);
    assert_eq!(g.gear(Some(SPARK), |_| false), Gear::Spark);
    assert_eq!(g.gear(Some(9), |_| true), Gear::None);
    // Les bâtons comptent dans l'inventaire.
    assert_eq!(
        g.gear(Some(SKIS), |i| i == POLES),
        Gear::Skis {
            poles: true,
            waxed: false
        }
    );
    assert_eq!(
        g.gear(Some(SKIS), |_| false),
        Gear::Skis {
            poles: false,
            waxed: false
        }
    );
}

#[test]
fn deep_snow_slows_the_walker_and_skis_do_not() {
    let mut bare = field();
    let d_bare = walk(&mut bare, 3.0, &[]);

    let mut deep = field();
    let s6 = b(&deep, "snotacke_6");
    for x in -30..=30 {
        for z in -30..=30 {
            deep.put(x, 64, z, s6);
        }
    }
    let d_deep = walk(&mut deep, 3.0, &[]);
    assert!(d_deep < d_bare * 0.8, "neige profonde {d_deep} vs sol {d_bare}");

    let mut skis = field();
    let s6 = b(&skis, "snotacke_6");
    for x in -30..=30 {
        for z in -30..=30 {
            skis.put(x, 64, z, s6);
        }
    }
    let d_ski = walk(&mut skis, 3.0, &[SKIS, POLES]);
    assert!(d_ski > d_bare * 0.9, "skis {d_ski} vs sol {d_bare}");
    assert!(d_ski > d_deep * 1.5);
}

#[test]
fn snowshoes_keep_the_pace_in_deep_snow() {
    let mut f = field();
    let s7 = b(&f, "snotacke_7");
    for x in -30..=30 {
        for z in -30..=30 {
            f.put(x, 64, z, s7);
        }
    }
    let d_shoes = walk(&mut f, 3.0, &[SHOES]);
    let mut bare = field();
    let d_bare = walk(&mut bare, 3.0, &[]);
    assert!(d_shoes > d_bare * 0.9, "raquettes {d_shoes} vs sol {d_bare}");
}

#[test]
fn skates_fly_on_ice_but_not_elsewhere() {
    let mut ice = field();
    snow_floor_ice(&mut ice);
    let d_ice = walk(&mut ice, 3.0, &[SKATES]);
    let mut plain = field();
    let d_plain = walk(&mut plain, 3.0, &[]);
    assert!(d_ice > d_plain * 1.5, "patins sur glace {d_ice} vs marche {d_plain}");
    let mut dry = field();
    assert!(walk(&mut dry, 3.0, &[SKATES]) < d_plain);
}

fn snow_floor_ice(h: &mut Harness) {
    let ice = b(h, "tjockis");
    h.slab(-30, 30, 63, -30, 30, ice);
}

#[test]
fn a_column_walked_three_times_becomes_packed_snow() {
    let mut f = field();
    snow_floor(&mut f, "snotacke_5");
    let id = f.spawn_player(DVec3::new(0.5, 64.0, 0.5), GameMode::Survival);
    let packed = b(&f, "packad_sno");
    let layer = b(&f, "snotacke_5");
    for i in 0..3 {
        // Entrer dans la colonne (5, 0) puis en sortir.
        f.sim.entities.get_mut::<nb_ecs::Transform>(id).unwrap().pos = DVec3::new(5.5, 64.0, 0.5);
        f.ticks(3);
        if i < 2 {
            assert_eq!(f.block(5, 64, 0), layer, "pas encore tassée au passage {i}");
        }
        f.sim.entities.get_mut::<nb_ecs::Transform>(id).unwrap().pos = DVec3::new(8.5, 64.0, 0.5);
        f.ticks(3);
    }
    assert_eq!(f.block(5, 64, 0), packed);
    // Les autres colonnes n'ont pas bougé.
    assert_eq!(f.block(6, 64, 0), layer);
}

/// Pente de skis : le sol descend d'un bloc par colonne vers +x, nappé de neige tassée.
fn ski_slope(h: &mut Harness) {
    let granite = b(h, "granit");
    let packed = b(h, "packad_sno");
    for x in 0..=24 {
        let top = 80 - x;
        for y in 40..top {
            for z in -3..=3 {
                h.put(x, y, z, granite);
            }
        }
        for z in -3..=3 {
            h.put(x, top, z, packed);
        }
    }
}

#[test]
fn skis_gain_speed_going_downhill() {
    let mut a = field();
    ski_slope(&mut a);

    let id = a.spawn_player(DVec3::new(4.5, 76.0, 0.5), GameMode::Survival);
    hold(&mut a, &[SKIS]);
    a.ticks(90);
    let skiing = a.t(id).pos.x - 4.5;

    let mut c = field();
    ski_slope(&mut c);
    let id = c.spawn_player(DVec3::new(4.5, 76.0, 0.5), GameMode::Survival);
    c.ticks(90);
    let walking = c.t(id).pos.x - 4.5;
    assert!(skiing > 3.0, "à skis la pente emporte : {skiing}");
    assert!(walking.abs() < 1.0, "à pied rien ne bouge : {walking}");
}

#[test]
fn vehicles_load_the_ice_as_the_design_says() {
    assert_eq!(Vehicle::Ackja.chests(), 2);
    assert_eq!(Vehicle::Sleigh.ice_load(false), Load::Sledge);
    assert_eq!(Vehicle::Sleigh.ice_load(true), Load::LoadedSledge);
    // Un traîneau chargé veut de la glace épaisse.
    assert!(!IceKind::Bearing.carries(Vehicle::Sleigh.ice_load(true), false));
    assert!(IceKind::Thick.carries(Vehicle::Sleigh.ice_load(true), false));
    assert!(IceKind::Bearing.carries(Vehicle::Ackja.ice_load(false), false));
}

#[test]
fn teams_are_faster_on_ice_and_slowed_by_load() {
    let ice = Surface::Ice(IceKind::Thick);
    let deep = Surface::Snow { layers: 7 };
    let full = glide::team_speed(Vehicle::Sleigh, 4.3, 2, 0.0, ice);
    assert!(full > 4.3);
    assert!(glide::team_speed(Vehicle::Sleigh, 4.3, 2, 1.0, ice) < full);
    assert!(glide::team_speed(Vehicle::Sleigh, 4.3, 1, 0.0, ice) < full);
    assert!(glide::team_speed(Vehicle::Sleigh, 4.3, 2, 0.0, deep) < full);
    let g = nb_data::husbandry::Gait {
        name: nb_data::husbandry::GaitName::Tolt,
        speed: 1.6,
        jolt: 0.0,
        steady: true,
    };
    assert!((glide::mount_speed(4.0, &g) - 6.4).abs() < 1e-5);
}

#[test]
fn the_ice_yacht_is_the_fastest_vehicle_and_needs_ice() {
    let ice = Surface::Ice(IceKind::Pack);
    assert_eq!(glide::ice_yacht_speed(10.0, 1.57, Surface::Bare), 0.0);
    // Pas de remontée de vent de face.
    assert_eq!(glide::ice_yacht_speed(10.0, 0.1, ice), 0.0);
    let beam = glide::ice_yacht_speed(10.0, 1.57, ice);
    assert!(beam > 25.0 && beam <= 40.0, "{beam}");
    assert!(beam > glide::team_speed(Vehicle::Ackja, 9.0, 2, 0.0, ice));
    assert_eq!(glide::ice_yacht_speed(50.0, 1.57, ice), 40.0);
}

/// Montagne : le sol descend de 2 blocs par colonne de x = 0 à 12, puis plat à 76 ; couche de
/// 6 huitièmes de neige sur toute la zone (largeur z = −3..=3).
fn mountain(h: &mut Harness) {
    let granite = b(h, "granit");
    let snow = b(h, "snotacke_6");
    // Sol de x = −12 à 28 sur z = −5..=5 ; neige seulement sur x = −6..=20, z = −3..=3, pour
    // qu'aucun bord de la maquette ne soit une falaise.
    for x in -12..=28 {
        let top = if x <= 0 { 100 } else if x <= 12 { 100 - 2 * x } else { 76 };
        for z in -5..=5 {
            for y in 40..top {
                h.put(x, y, z, granite);
            }
            if (-6..=20).contains(&x) && z.abs() <= 3 {
                h.put(x, top, z, snow);
            }
        }
    }
}

fn snow_ids(h: &Harness) -> SnowIds {
    SnowIds::new(&h.reg)
}

#[test]
fn a_steep_deep_slope_is_a_start() {
    let mut h = field();
    mountain(&mut h);
    let ids = snow_ids(&h);
    assert!(avalanche::can_start(&h.blocks, &h.reg, &ids, 3, 0));
    // Plat en bas : pas de départ.
    assert!(!avalanche::can_start(&h.blocks, &h.reg, &ids, 20, 0));
    // Trop peu de neige : pas de départ.
    let thin = b(&h, "snotacke_4");
    h.put(3, 94, 0, thin);
    assert!(!avalanche::can_start(&h.blocks, &h.reg, &ids, 3, 0));
}

#[test]
fn the_slab_slides_down_and_piles_up_at_the_foot() {
    let mut h = field();
    mountain(&mut h);
    let ids = snow_ids(&h);
    let run = avalanche::plan(&h.blocks, &h.reg, &ids, 0, 0).expect("coulée");
    assert!(run.swept.len() >= 9 && run.runout.len() >= avalanche::RUNOUT);
    let before = avalanche::column(&h.blocks, &h.reg, &ids, 11, 0).unwrap().snow;
    let dropped = avalanche::apply(&mut h.blocks, &h.reg, &ids, &run);
    assert!(dropped > 0);
    // La pente est nue, le pied s'est épaissi.
    assert_eq!(avalanche::column(&h.blocks, &h.reg, &ids, 3, 0).unwrap().snow, 0);
    let after = avalanche::column(&h.blocks, &h.reg, &ids, 11, 0).unwrap().snow;
    assert!(after > before + 8, "pied {before} -> {after}");
}

#[test]
fn caught_travellers_are_hurt_buried_and_dug_out() {
    let mut h = field();
    mountain(&mut h);
    let id = h.spawn_player(DVec3::new(3.5, 94.0, 0.5), GameMode::Survival);
    let hp = h.h(id).health;
    let ids = snow_ids(&h);
    let run = avalanche::plan(&h.blocks, &h.reg, &ids, 0, 0).unwrap();
    avalanche::apply(&mut h.blocks, &h.reg, &ids, &run);
    let caught = avalanche::sweep_entities(&mut h.sim.entities, &mut h.blocks, &h.reg, &ids, &run);
    assert_eq!(caught, vec![id]);
    assert!(h.h(id).health < hp);
    assert!(h.sim.entities.has::<Buried>(id));
    let foot = *run.runout.last().unwrap();
    let pos = h.t(id).pos;
    assert_eq!((pos.x.floor() as i32, pos.z.floor() as i32), foot);
    // Poche d'air sous un bouchon de neige pleine.
    let g = avalanche::column(&h.blocks, &h.reg, &ids, foot.0, foot.1)
        .map(|c| c.ground)
        .unwrap();
    let _ = g;
    let cap = IVec3::new(foot.0, pos.y.floor() as i32 + 2, foot.1);
    assert_eq!(h.block(cap.x, cap.y, cap.z), b(&h, "sno"));
    // Étouffement : après la réserve d'air, la santé baisse encore.
    let hurt = h.h(id).health;
    h.ticks(60 * 14);
    assert!(h.h(id).health < hurt, "étouffement");
    // On se dégage en cassant le bouchon.
    let air = h.reg.ids().air;
    h.put(cap.x, cap.y, cap.z, air);
    h.ticks(2);
    assert!(!h.sim.entities.has::<Buried>(id));
}

#[test]
fn a_loud_noise_warns_then_releases_the_slope() {
    let mut h = field();
    mountain(&mut h);
    let granite = b(&h, "granit");
    let mut warned = false;
    for _ in 0..40 {
        h.events.push(EcsEvent::BlockBroken {
            pos: IVec3::new(4, 90, 0),
            block: granite,
        });
        h.tick();
        if h
            .take_events()
            .iter()
            .any(|e| matches!(e, EcsEvent::Whoumf { .. }))
        {
            warned = true;
            break;
        }
    }
    assert!(warned, "un coup sourd aurait dû prévenir");
    // Rien n'est parti avant le délai.
    let ids = snow_ids(&h);
    assert!(avalanche::column(&h.blocks, &h.reg, &ids, 3, 0).unwrap().snow > 0);
    h.ticks(60 * 3);
    let ev = h.take_events();
    assert!(ev.iter().any(|e| matches!(e, EcsEvent::Avalanche { .. })));
    let snow_left: i32 = (0..=8)
        .flat_map(|x| (-3..=3).map(move |z| (x, z)))
        .map(|(x, z)| avalanche::column(&h.blocks, &h.reg, &ids, x, z).unwrap().snow)
        .sum();
    // 9 x 7 colonnes de 6 huitièmes : au moins une bande entière est partie.
    assert!(snow_left <= 378 - 100, "neige restante {snow_left}");
}

#[test]
fn quiet_slopes_and_snowless_noises_do_nothing() {
    let mut h = field();
    mountain(&mut h);
    h.ticks(60 * 5);
    assert!(!h
        .take_events()
        .iter()
        .any(|e| matches!(e, EcsEvent::Whoumf { .. } | EcsEvent::Avalanche { .. })));
    // Casser de la neige ne fait pas de bruit.
    let snow = b(&h, "snotacke_6");
    for _ in 0..40 {
        h.events.push(EcsEvent::BlockBroken {
            pos: IVec3::new(4, 90, 0),
            block: snow,
        });
        h.tick();
        assert!(!h
            .take_events()
            .iter()
            .any(|e| matches!(e, EcsEvent::Whoumf { .. })));
    }
}

#[test]
fn only_slope_biomes_know_avalanches() {
    assert!(avalanche::slope_biome("FJORD"));
    assert!(avalanche::slope_biome("KALFJALL"));
    assert!(avalanche::slope_biome("FJALLHED"));
    assert!(!avalanche::slope_biome("MYR"));
}

#[test]
fn packed_snow_melts_with_the_season_overlay() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    // Le registre connaît les objets de la glisse.
    let items = nb_data::ItemRegistry::for_kind(WorldKind::Norrcraft, &reg);
    for n in [
        "skidor",
        "skidstav",
        "skidtjara",
        "snoskor",
        "langfardsskridskor",
        "spark",
        "pulka",
        "ackja",
        "slade",
        "isjakt",
        "snoskyffel",
    ] {
        assert!(items.id_of(n).is_some(), "objet {n} absent");
    }
    let ids = GearIds::from_names(|n| items.id_of(n));
    assert!(ids.skis.is_some() && ids.spark.is_some());
    let packed = reg.get("packad_sno");
    assert_eq!(packed.name, "packad_sno");
    assert!(!packed.solid);
}

