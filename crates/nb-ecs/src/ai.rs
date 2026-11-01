//! IA des mobs : passive (errance, troupeau, fuite) §11 et agressive (poursuite, morsure) §12.

use std::f64::consts::PI;

use nb_core::glam::DVec3;
use nb_core::JavaRandom;
use nb_data::{MobConfig, MobMedium, MobRegistry};

use crate::components::{
    AggressiveAi, Collider, EntityId, GameMode, Health, MobType, PassiveAi, Player, Transform,
    Velocity,
};
use crate::jmath::{floor_i, jmax};
use crate::physics::BlockEnv;
use crate::schedule::{System, TickCtx};
use crate::world::EntityWorld;

/// Distance d'anticipation des poissons pour rester dans l'eau.
pub const LOOK_AHEAD: f32 = 1.5;

const TWO_PI_F: f32 = (PI * 2.0) as f32;
/// `(float) (Math.PI / 2.5f)` : ±36° pour la nage passive.
const PI_OVER_2_5_F: f32 = (PI / 2.5) as f32;
/// `(float) (Math.PI / 4)`.
const PI_OVER_4_F: f32 = (PI / 4.0) as f32;
/// `(float) (Math.PI / 2)`.
const PI_OVER_2_F: f32 = (PI / 2.0) as f32;
/// `(float) (Math.PI / 5)` : ±18° pour la nage agressive.
const PI_OVER_5_F: f32 = (PI / 5.0) as f32;

/// `(float) (Math.PI / 2 - h)`.
#[inline]
fn yaw_from_heading(h: f32) -> f32 {
    (PI / 2.0 - f64::from(h)) as f32
}

/// Consignes pour un cap `h` et une vitesse : terrestre en 2D, sinon en 3D avec l'angle `v`.
fn set_heading(v: &mut Velocity, t: &mut Transform, medium: MobMedium, h: f32, vang: f32, speed: f32) {
    if medium == MobMedium::Land {
        v.target.x = (f64::from(h).cos() as f32) * speed;
        v.target.z = (f64::from(h).sin() as f32) * speed;
        v.target.y = 0.0;
        t.yaw = yaw_from_heading(h);
    } else {
        let hs = (f64::from(vang).cos() * f64::from(speed)) as f32;
        v.target.x = (f64::from(h).cos() * f64::from(hs)) as f32;
        v.target.z = (f64::from(h).sin() * f64::from(hs)) as f32;
        v.target.y = (f64::from(vang).sin() * f64::from(speed)) as f32;
        t.yaw = yaw_from_heading(h);
        t.pitch = -vang;
    }
}

// ---------------------------------------------------------------------------------------------
// Passifs
// ---------------------------------------------------------------------------------------------

/// `PassiveMobAISystem`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassiveAiSystem;

/// Perception d'un passif calculée avant son tick.
#[derive(Clone, Copy, Debug)]
struct Perception {
    nearest_aggressor: f32,
    flee_x: f32,
    flee_z: f32,
    herdmates: i32,
    herd_cx: f32,
    herd_cz: f32,
}

impl System for PassiveAiSystem {
    fn name(&self) -> &'static str {
        "PassiveAI"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let passives =
            world.query_ids::<(&PassiveAi, &Velocity, &Transform, &MobType)>();
        if passives.is_empty() {
            return;
        }
        // Instantané des agresseurs (position, milieu). Les positions ne bougent pas pendant
        // ce système. Une config d'agresseur absente est ignorée (le Java plantait : écart).
        let aggressors: Vec<(DVec3, MobMedium)> = world
            .query_ids::<(&AggressiveAi, &Transform, &MobType)>()
            .into_iter()
            .filter_map(|id| {
                let medium = ctx.mobs.get(&world.get::<MobType>(id)?.mob_id)?.medium;
                Some((world.copy::<Transform>(id)?.pos, medium))
            })
            .collect();
        let snapshot: Vec<(EntityId, DVec3, String)> = passives
            .iter()
            .filter_map(|&id| {
                Some((
                    id,
                    world.copy::<Transform>(id)?.pos,
                    world.get::<MobType>(id)?.mob_id.clone(),
                ))
            })
            .collect();

        let env = BlockEnv::new(&*ctx.blocks, ctx.block_reg);
        let mobs: &MobRegistry = ctx.mobs;
        let rng = &mut ctx.rngs.passive;
        for (id, pos, mob_id) in &snapshot {
            let Some(cfg) = mobs.get(mob_id) else {
                continue;
            };
            let mut p = Perception {
                nearest_aggressor: f32::MAX,
                flee_x: 0.0,
                flee_z: 0.0,
                herdmates: 0,
                herd_cx: 0.0,
                herd_cz: 0.0,
            };
            if cfg.aggressor_detect_range > 0.0 {
                for (apos, amedium) in &aggressors {
                    if *amedium != cfg.medium {
                        continue;
                    }
                    let dx = (pos.x - apos.x) as f32;
                    let dz = (pos.z - apos.z) as f32;
                    let d = (dx * dx + dz * dz).sqrt();
                    if d < p.nearest_aggressor {
                        p.nearest_aggressor = d;
                        let inv = if d > 0.01 { 1.0 / d } else { 0.0 };
                        p.flee_x = dx * inv;
                        p.flee_z = dz * inv;
                    }
                }
            }
            if cfg.herding {
                for (other, opos, oid) in &snapshot {
                    if other == id || oid != mob_id {
                        continue;
                    }
                    let dx = (opos.x - pos.x) as f32;
                    let dz = (opos.z - pos.z) as f32;
                    if dx * dx + dz * dz <= cfg.herd_radius * cfg.herd_radius {
                        p.herd_cx += opos.x as f32;
                        p.herd_cz += opos.z as f32;
                        p.herdmates += 1;
                    }
                }
                if p.herdmates > 0 {
                    p.herd_cx /= p.herdmates as f32;
                    p.herd_cz /= p.herdmates as f32;
                }
            }
            let Some((ai, v, t)) =
                world.query_one_mut::<(&mut PassiveAi, &mut Velocity, &mut Transform)>(*id)
            else {
                continue;
            };
            passive_tick(ai, v, t, cfg, &p, &env, rng, dt);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn passive_tick(
    ai: &mut PassiveAi,
    v: &mut Velocity,
    t: &mut Transform,
    cfg: &MobConfig,
    p: &Perception,
    env: &BlockEnv<'_>,
    rng: &mut JavaRandom,
    dt: f32,
) {
    // 1. Fuite continue de l'agresseur le plus proche.
    if p.nearest_aggressor < cfg.aggressor_detect_range {
        v.target.x = p.flee_x * cfg.aggressor_flee_speed;
        v.target.y = 0.0;
        v.target.z = p.flee_z * cfg.aggressor_flee_speed;
        let h = f64::from(p.flee_z).atan2(f64::from(p.flee_x)) as f32;
        t.yaw = yaw_from_heading(h);
        t.pitch = 0.0;
        if ai.flee_timer > 0.0 {
            ai.flee_timer -= dt;
        }
        ai.moving = false;
        ai.move_timer = 0.0;
        return;
    }
    // 2. Fuite du joueur après un coup.
    if ai.flee_timer > 0.0 {
        ai.flee_timer -= dt;
        if ai.flee_timer <= 0.0 {
            v.target = Default::default();
            t.pitch = 0.0;
            ai.moving = false;
            ai.move_timer = 1.5;
        }
        return;
    }
    // 3. Cycle errance / pause.
    ai.move_timer -= dt;
    if ai.move_timer > 0.0 {
        return;
    }
    if ai.moving {
        ai.moving = false;
        v.target = Default::default();
        t.pitch = 0.0;
        ai.move_timer = cfg.pause_duration_min
            + rng.next_float() * (cfg.pause_duration_max - cfg.pause_duration_min);
    } else {
        let Some((h, vang)) = choose_idle_direction(cfg, t, p, env, rng) else {
            ai.move_timer = 1.0;
            return;
        };
        let speed =
            cfg.move_speed_min + rng.next_float() * (cfg.move_speed_max - cfg.move_speed_min);
        set_heading(v, t, cfg.medium, h, vang, speed);
        ai.moving = true;
        ai.move_timer = cfg.move_duration_min
            + rng.next_float() * (cfg.move_duration_max - cfg.move_duration_min);
    }
}

fn build_h_angle(cfg: &MobConfig, t: &Transform, p: &Perception, rng: &mut JavaRandom) -> f32 {
    if cfg.herding && p.herdmates > 0 {
        let to_x = p.herd_cx - t.pos.x as f32;
        let to_z = p.herd_cz - t.pos.z as f32;
        let d = (to_x * to_x + to_z * to_z).sqrt();
        if d > 1.5 && rng.next_float() < cfg.herd_bias {
            let base = f64::from(to_z).atan2(f64::from(to_x)) as f32;
            return base + (rng.next_float() - 0.5) * PI_OVER_2_5_F;
        }
    }
    rng.next_float() * TWO_PI_F
}

/// `chooseIdleDirection` : terrestre → `(h, 0)` ; aquatique → jusqu'à 8 caps validés par un
/// point d'anticipation dans l'eau, sinon `None`.
fn choose_idle_direction(
    cfg: &MobConfig,
    t: &Transform,
    p: &Perception,
    env: &BlockEnv<'_>,
    rng: &mut JavaRandom,
) -> Option<(f32, f32)> {
    let mut h = build_h_angle(cfg, t, p, rng);
    if cfg.medium == MobMedium::Land {
        return Some((h, 0.0));
    }
    let water = env.reg.ids().water;
    let la = f64::from(LOOK_AHEAD);
    let mut vang = (rng.next_float() - 0.5) * PI_OVER_2_5_F;
    for _ in 0..8 {
        let (hd, vd) = (f64::from(h), f64::from(vang));
        let cos_v = vd.cos();
        let look_x = t.pos.x + hd.cos() * la * cos_v;
        let look_y = t.pos.y + vd.sin() * la;
        let look_z = t.pos.z + hd.sin() * la * cos_v;
        if env.block(floor_i(look_x), floor_i(look_y), floor_i(look_z)) == water {
            return Some((h, vang));
        }
        if vang != 0.0 {
            // Le Java remet `vAngle` à 0 ici ; il est retiré au bas de la boucle de toute façon.
            let look_x = t.pos.x + hd.cos() * la;
            let look_z = t.pos.z + hd.sin() * la;
            if env.block(floor_i(look_x), floor_i(t.pos.y), floor_i(look_z)) == water {
                return Some((h, 0.0));
            }
        }
        h += PI_OVER_4_F;
        vang = (rng.next_float() - 0.5) * PI_OVER_2_5_F;
    }
    None
}

/// `triggerPlayerFlee` (§11.4) : direction **aléatoire** (B10), minuteur de fuite.
pub fn trigger_player_flee(
    world: &mut EntityWorld,
    id: EntityId,
    mobs: &MobRegistry,
    rng: &mut JavaRandom,
) {
    let Some(mob_id) = world.get::<MobType>(id).map(|m| m.mob_id.clone()) else {
        return;
    };
    let Some(cfg) = mobs.get(&mob_id) else {
        return;
    };
    let Some((ai, v, t)) =
        world.query_one_mut::<(&mut PassiveAi, &mut Velocity, &mut Transform)>(id)
    else {
        return;
    };
    if cfg.medium == MobMedium::Land {
        let a = rng.next_float() * TWO_PI_F;
        v.target.x = (f64::from(a).cos() as f32) * cfg.player_flee_speed;
        v.target.z = (f64::from(a).sin() as f32) * cfg.player_flee_speed;
        t.yaw = yaw_from_heading(a);
    } else {
        let h = rng.next_float() * TWO_PI_F;
        let vang = (rng.next_float() - 0.5) * PI_OVER_2_F;
        set_heading(v, t, cfg.medium, h, vang, cfg.player_flee_speed);
    }
    ai.moving = true;
    ai.flee_timer = cfg.player_flee_duration_min
        + rng.next_float() * (cfg.player_flee_duration_max - cfg.player_flee_duration_min);
}

// ---------------------------------------------------------------------------------------------
// Agressifs
// ---------------------------------------------------------------------------------------------

/// `AggressiveMobAISystem`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AggressiveAiSystem;

fn dist_sq(a: DVec3, b: DVec3) -> f64 {
    let (dx, dy, dz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    dx * dx + dy * dy + dz * dz
}

impl System for AggressiveAiSystem {
    fn name(&self) -> &'static str {
        "AggressiveAI"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let aggressors =
            world.query_ids::<(&AggressiveAi, &Velocity, &Transform, &MobType)>();
        if aggressors.is_empty() {
            return;
        }
        let preys = world.query_ids::<(&PassiveAi, &Transform, &Health)>();
        let players = world.query_ids::<(&Player, &Transform, &Velocity, &Collider)>();
        let water = ctx.block_reg.ids().water;
        let mobs: &MobRegistry = ctx.mobs;

        for id in aggressors {
            let Some(mob_id) = world.get::<MobType>(id).map(|m| m.mob_id.clone()) else {
                continue;
            };
            let Some(cfg) = mobs.get(&mob_id) else {
                continue;
            };
            let Some(ai) = world.get_mut::<AggressiveAi>(id) else {
                continue;
            };
            ai.attack_cooldown = jmax(0.0, ai.attack_cooldown - dt);
            let Some(self_pos) = world.copy::<Transform>(id).map(|t| t.pos) else {
                continue;
            };

            let mut target: Option<(EntityId, bool)> = None;
            let mut nearest_sq = f64::MAX;
            for &p in &preys {
                if !world.copy::<Health>(p).is_some_and(|h| h.alive) {
                    continue;
                }
                let Some(pmt) = world.get::<MobType>(p) else {
                    continue;
                };
                if mobs.get(&pmt.mob_id).map(|c| c.medium) != Some(cfg.medium) {
                    continue;
                }
                drop(pmt);
                let Some(pt) = world.copy::<Transform>(p) else {
                    continue;
                };
                let d = dist_sq(self_pos, pt.pos);
                if d < nearest_sq {
                    nearest_sq = d;
                    target = Some((p, false));
                }
            }
            if cfg.aggro_player_range > 0.0 {
                let range = f64::from(cfg.aggro_player_range);
                for &p in &players {
                    let (Some(pc), Some(pt), Some(cc)) = (
                        world.copy::<Player>(p),
                        world.copy::<Transform>(p),
                        world.copy::<Collider>(p),
                    ) else {
                        continue;
                    };
                    if pc.game_mode != GameMode::Survival {
                        continue;
                    }
                    if cfg.medium == MobMedium::Water {
                        let feet_water = ctx.blocks.block(
                            floor_i(pt.pos.x),
                            floor_i(pt.pos.y + f64::from(cc.height) * 0.4),
                            floor_i(pt.pos.z),
                        );
                        if feet_water != water {
                            continue;
                        }
                    }
                    let d = dist_sq(self_pos, pt.pos);
                    if d <= range * f64::from(cfg.aggro_player_range) {
                        target = Some((p, true));
                        nearest_sq = d;
                    }
                }
            }
            match target {
                Some((tid, _is_player)) => {
                    tick_chase(world, id, tid, nearest_sq.sqrt(), cfg);
                }
                None => {
                    let Some((ai, v, t)) = world
                        .query_one_mut::<(&mut AggressiveAi, &mut Velocity, &mut Transform)>(id)
                    else {
                        continue;
                    };
                    tick_idle(ai, v, t, cfg, &mut ctx.rngs.aggressive, dt);
                }
            }
        }
    }
}

/// `tickChase` (§12.2) : morsure si à portée et cooldown écoulé, puis poursuite. Les branches
/// joueur / proie du Java sont identiques.
fn tick_chase(world: &mut EntityWorld, id: EntityId, target: EntityId, dist: f64, cfg: &MobConfig) {
    let (Some(self_t), Some(target_t)) = (world.copy::<Transform>(id), world.copy::<Transform>(target))
    else {
        return;
    };
    let d = target_t.pos - self_t.pos;
    let cooldown_ready = world
        .copy::<AggressiveAi>(id)
        .is_some_and(|ai| ai.attack_cooldown <= 0.0);
    if dist < f64::from(cfg.attack_range) && cooldown_ready && world.has::<Health>(target) {
        if let Some(h) = world.get_mut::<Health>(target) {
            h.health -= cfg.attack_damage;
            if h.health <= 0.0 {
                h.alive = false;
            }
        }
        if dist > 0.01 {
            if let Some(tv) = world.get_mut::<Velocity>(target) {
                let inv = (1.0 / dist) as f32;
                tv.v.x += d.x as f32 * inv * cfg.knockback_force;
                tv.v.z += d.z as f32 * inv * cfg.knockback_force;
                tv.v.y = 5.0;
            }
        }
        if let Some(ai) = world.get_mut::<AggressiveAi>(id) {
            ai.attack_cooldown = cfg.attack_cooldown;
        }
    }
    if dist > 0.01 {
        let Some((v, t)) = world.query_one_mut::<(&mut Velocity, &mut Transform)>(id) else {
            return;
        };
        let inv = (1.0 / dist) as f32;
        let h = d.z.atan2(d.x) as f32;
        let h_dist = (d.x * d.x + d.z * d.z).sqrt() as f32;
        let vang = d.y.atan2(f64::from(h_dist)) as f32;
        t.yaw = yaw_from_heading(h);
        v.target.x = d.x as f32 * inv * cfg.chase_speed;
        v.target.z = d.z as f32 * inv * cfg.chase_speed;
        if cfg.medium != MobMedium::Land {
            v.target.y = d.y as f32 * inv * cfg.chase_speed;
            t.pitch = -vang;
        }
    }
}

/// `tickIdle` (§12.3) : errance sans troupeau ni validation d'eau ; tirages `h`, `speed`,
/// (`v`), puis `moveTimer`.
fn tick_idle(
    ai: &mut AggressiveAi,
    v: &mut Velocity,
    t: &mut Transform,
    cfg: &MobConfig,
    rng: &mut JavaRandom,
    dt: f32,
) {
    ai.move_timer -= dt;
    if ai.move_timer > 0.0 {
        return;
    }
    if ai.moving {
        ai.moving = false;
        v.target = Default::default();
        t.pitch = 0.0;
        ai.move_timer = cfg.pause_duration_min
            + rng.next_float() * (cfg.pause_duration_max - cfg.pause_duration_min);
    } else {
        let h = rng.next_float() * TWO_PI_F;
        let speed =
            cfg.move_speed_min + rng.next_float() * (cfg.move_speed_max - cfg.move_speed_min);
        let vang = if cfg.medium == MobMedium::Land {
            0.0
        } else {
            (rng.next_float() - 0.5) * PI_OVER_5_F
        };
        set_heading(v, t, cfg.medium, h, vang, speed);
        ai.moving = true;
        ai.move_timer = cfg.move_duration_min
            + rng.next_float() * (cfg.move_duration_max - cfg.move_duration_min);
    }
}
