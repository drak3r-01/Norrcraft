//! Joueur : création (`World.initPlayer`), réapparition (`respawnPlayer`) et
//! `PlayerInputSystem` (spec §6). L'ordre des étapes du tick est normatif.

use nb_core::glam::{IVec3, Vec3};
use nb_data::BlockId;
use nb_world::WorldConfig;

use crate::components::{
    CameraState, Collider, EntityId, GameMode, Health, InventoryComp, Player, Transform, Velocity,
};
use crate::health::hit_mob;
use crate::input::GameAction;
use crate::inventory::HOTBAR_START;
use crate::jmath::{jmax, jmin, lerp, to_radians_f};
use crate::physics::{aabb_intersects_block, raycast_mob, BlockEnv};
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;

pub const EYE_OFFSET: f32 = 1.62;
pub const HEAD_CLEARANCE: f32 = 0.18;
pub const PLAYER_WIDTH: f32 = 0.6;
pub const PLAYER_MAX_HEALTH: f32 = 10.0;
pub const DEFAULT_SENSITIVITY: f32 = 0.1;
pub const DEFAULT_FOV_DEG: f32 = 70.0;
pub const FOV_SPRINT_OFFSET_DEG: f32 = 25.0;
pub const LAVA_DAMAGE_INTERVAL: f32 = 0.5;
pub const LAVA_DAMAGE_AMOUNT: f32 = 1.0;
pub const PLAYER_FALL_DAMAGE_THRESHOLD: f32 = -11.5;
pub const PLAYER_FALL_DAMAGE_SCALE: f32 = 0.4;
pub const DOUBLE_TAP_WINDOW: f32 = 0.35;
pub const BLOCK_REACH: f32 = 6.0;
pub const FLY_FRICTION: f32 = 8.0;

/// `(float) Math.toRadians(70.0)`.
pub fn default_fov() -> f32 {
    to_radians_f(DEFAULT_FOV_DEG)
}

/// `World.initPlayer` : entité joueur à partir de la caméra (yeux) ; pieds = `cam.y − 1.62`.
pub fn init_player(
    world: &mut EntityWorld,
    camera: CameraState,
    mode: GameMode,
    air: BlockId,
) -> EntityId {
    let id = world.create();
    let p = camera.position;
    world.insert(
        id,
        Transform::new(f64::from(p.x), f64::from(p.y - EYE_OFFSET), f64::from(p.z)),
    );
    world.insert(id, Velocity::default());
    world.insert(
        id,
        Collider::new(
            PLAYER_WIDTH,
            PLAYER_WIDTH,
            EYE_OFFSET + HEAD_CLEARANCE,
            EYE_OFFSET,
        ),
    );
    world.insert(id, Health::new(PLAYER_MAX_HEALTH));
    world.insert(id, InventoryComp::default());
    world.insert(id, Player::new(camera, mode, default_fov(), air));
    id
}

/// `World.respawnPlayer` (mort ou `/kill`) : spawn du monde (ou `(0.5, seaLevel + 5, 0.5)`),
/// vitesse nulle (`grounded` inchangé), santé pleine, **inventaire vidé**, caméra recalée.
pub fn respawn_player(world: &mut EntityWorld, id: EntityId, cfg: &WorldConfig) {
    let [rx, ry, rz] = cfg.respawn_position();
    if let Some(t) = world.get_mut::<Transform>(id) {
        t.pos = nb_core::glam::DVec3::new(rx, ry, rz);
    }
    if let Some(v) = world.get_mut::<Velocity>(id) {
        v.v = Vec3::ZERO;
    }
    if let Some(h) = world.get_mut::<Health>(id) {
        h.health = h.max_health;
        h.alive = true;
    }
    if let Some(inv) = world.get_mut::<InventoryComp>(id) {
        inv.0.clear();
    }
    if let Some(p) = world.get_mut::<Player>(id) {
        p.camera.position = Vec3::new(
            rx as f32,
            (ry + f64::from(EYE_OFFSET)) as f32,
            rz as f32,
        );
    }
}

/// Dégâts au joueur clampés à 0 (chute, lave : B15).
fn damage_clamped(h: &mut Health, amount: f32) {
    h.health -= amount;
    if h.health < 0.0 {
        h.health = 0.0;
    }
    if h.health <= 0.0 {
        h.alive = false;
    }
}

/// `PlayerInputSystem` : orientation, déplacement (sol, nage, vol), dégâts de chute et de lave,
/// FOV de sprint, visée, casser / poser / jeter.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlayerInputSystem;

impl System for PlayerInputSystem {
    fn name(&self) -> &'static str {
        "PlayerInput"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let Some(&id) = world
            .query_ids::<(&Player, &Transform, &Velocity, &Collider, &InventoryComp)>()
            .first()
        else {
            return;
        };
        let input = *ctx.input;
        let ids = *ctx.block_reg.ids();
        let pressed = |a| input.is_pressed(a);
        let just = |a| input.is_just_pressed(a);

        // --- Étapes 1 à 12 : mouvement, caméra, dégâts, visée ------------------------------
        let (origin, dir, targeted, place_target, feet, collider) = {
            let env = BlockEnv::new(&*ctx.blocks, ctx.block_reg);
            let wc: &WorldConfig = &*ctx.config;
            let Some((p, t, v, c, mut health)) = world.query_one_mut::<(
                &mut Player,
                &mut Transform,
                &mut Velocity,
                &Collider,
                Option<&mut Health>,
            )>(id) else {
                return;
            };
            let cam = &mut p.camera;

            // 1. Rotation.
            if input.mouse_locked {
                let sens = input.sensitivity.unwrap_or(DEFAULT_SENSITIVITY);
                cam.rotate(input.mouse_delta.y * sens, input.mouse_delta.x * sens);
            }

            // 2. Entrée locale normalisée.
            let mut local_dx = 0.0f32;
            let mut local_dz = 0.0f32;
            if pressed(GameAction::Forward) {
                local_dz -= 1.0;
            }
            if pressed(GameAction::Backward) {
                local_dz += 1.0;
            }
            if pressed(GameAction::Left) {
                local_dx -= 1.0;
            }
            if pressed(GameAction::Right) {
                local_dx += 1.0;
            }
            let len = (local_dx * local_dx + local_dz * local_dz).sqrt();
            if len > 0.0 {
                local_dx /= len;
                local_dz /= len;
            }

            // 3. Double appui sur saut : vol (créatif uniquement).
            p.double_tap_timer += dt;
            if just(GameAction::Jump) {
                if p.double_tap_timer < DOUBLE_TAP_WINDOW && p.game_mode == GameMode::Creative {
                    p.flying = !p.flying;
                    if p.flying {
                        v.v.y = 0.0;
                    }
                }
                p.double_tap_timer = 0.0;
            }

            // 4. Direction monde.
            let cam = &mut p.camera;
            let yaw = to_radians_f(cam.yaw_deg);
            let (cos_yaw, sin_yaw) = (f64::from(yaw).cos() as f32, f64::from(yaw).sin() as f32);
            let target_dx = local_dx * cos_yaw - local_dz * sin_yaw;
            let target_dz = local_dx * sin_yaw + local_dz * cos_yaw;

            // 5. Configuration.
            let walk_speed = wc.walk_speed;
            let sprint_speed = wc.sprint_speed;
            let fly_speed = wc.fly_speed;
            let fly_vertical = wc.fly_speed * 0.6;
            let jump_force = wc.jump_force;
            let gravity = wc.gravity;

            if p.flying {
                // 6. Vol.
                v.v.x = lerp(v.v.x, target_dx * fly_speed, FLY_FRICTION * dt);
                v.v.z = lerp(v.v.z, target_dz * fly_speed, FLY_FRICTION * dt);
                let d = v.v.x * dt;
                env.move_player_axis(t, v, c, d, 0.0, 0.0);
                let d = v.v.z * dt;
                env.move_player_axis(t, v, c, 0.0, 0.0, d);
                let mut target_vy = 0.0;
                if pressed(GameAction::Jump) {
                    target_vy += fly_vertical;
                }
                if pressed(GameAction::Crouch) {
                    target_vy -= fly_vertical;
                }
                v.v.y = lerp(v.v.y, target_vy, FLY_FRICTION * dt);
                let d = v.v.y * dt;
                env.move_player_axis(t, v, c, 0.0, d, 0.0);
            } else {
                // 7. Sol / nage.
                let in_fluid = env.is_in_fluid(t.pos.x, t.pos.y, t.pos.z, c);
                let sprint = pressed(GameAction::Sprint) && local_dz < 0.0 && !in_fluid;
                let mut speed = if sprint { sprint_speed } else { walk_speed };
                if in_fluid {
                    speed *= 0.4;
                }
                let friction = if in_fluid {
                    8.0
                } else if v.grounded {
                    15.0
                } else {
                    2.5
                };
                v.v.x = lerp(v.v.x, target_dx * speed, friction * dt);
                v.v.z = lerp(v.v.z, target_dz * speed, friction * dt);
                let d = v.v.x * dt;
                env.move_player_axis(t, v, c, d, 0.0, 0.0);
                let d = v.v.z * dt;
                env.move_player_axis(t, v, c, 0.0, 0.0, d);

                let was_grounded = v.grounded;
                if in_fluid {
                    if pressed(GameAction::Jump) {
                        v.v.y += jump_force * 1.5 * dt;
                        if v.v.y > jump_force * 0.4 {
                            v.v.y = jump_force * 0.4;
                        }
                    } else if !v.grounded {
                        v.v.y += gravity * 0.2 * dt;
                    }
                    if v.v.y < -3.0 {
                        v.v.y = -3.0;
                    }
                } else {
                    if pressed(GameAction::Jump) && v.grounded {
                        v.v.y = jump_force;
                        v.grounded = false;
                    }
                    v.v.y += gravity * dt;
                }
                let pre_y = v.v.y;
                env.move_player_axis(t, v, c, 0.0, pre_y * dt, 0.0);

                if !was_grounded
                    && v.grounded
                    && pre_y < PLAYER_FALL_DAMAGE_THRESHOLD
                    && p.game_mode == GameMode::Survival
                    && wc.fall_damage_enabled
                {
                    if let Some(h) = health.as_deref_mut() {
                        damage_clamped(h, (-pre_y - 11.5) * PLAYER_FALL_DAMAGE_SCALE);
                    }
                }
            }

            // 8. Caméra aux yeux.
            let cam = &mut p.camera;
            cam.position = Vec3::new(
                t.pos.x as f32,
                (t.pos.y + f64::from(c.eye_offset)) as f32,
                t.pos.z as f32,
            );

            // 9. FOV de sprint (NaN propagé si la vitesse max est nulle : B16).
            let base_fov = input.fov_deg.map_or_else(default_fov, to_radians_f);
            let sprint_fov = base_fov + to_radians_f(FOV_SPRINT_OFFSET_DEG);
            let h_speed = (v.v.x * v.v.x + v.v.z * v.v.z).sqrt();
            let max_speed = if p.flying { fly_speed } else { sprint_speed };
            let fov_target = lerp(base_fov, sprint_fov, jmax(0.0, jmin(h_speed / max_speed, 1.0)));
            p.current_fov = lerp(
                p.current_fov,
                fov_target,
                1.0 - (-10.0 * f64::from(dt)).exp() as f32,
            );
            p.camera.fov = p.current_fov;

            // 10. Lave : un dégât par 0.5 s d'immersion, au plus un par tick.
            if p.game_mode == GameMode::Survival
                && health.is_some()
                && env.is_in_lava(t.pos.x, t.pos.y, t.pos.z, c)
            {
                p.lava_timer += dt;
                if p.lava_timer >= LAVA_DAMAGE_INTERVAL {
                    if let Some(h) = health.as_deref_mut() {
                        damage_clamped(h, LAVA_DAMAGE_AMOUNT);
                    }
                    p.lava_timer -= LAVA_DAMAGE_INTERVAL;
                }
            } else {
                p.lava_timer = 0.0;
            }

            // 11. Visée des blocs.
            let origin = p.camera.position;
            let dir = p.camera.look_dir();
            let hit = env.raycast(origin, dir, BLOCK_REACH);
            p.targeted_block = hit.map(|h| h.hit);
            p.place_target = hit.map(|h| h.adjacent);

            // 12. Fluide aux yeux.
            p.fluid_at_eye_level =
                env.fluid_at_eye_level(t.pos.x, t.pos.y + f64::from(c.eye_offset), t.pos.z);

            (origin, dir, p.targeted_block, p.place_target, *t, *c)
        };

        // --- Étape 13 : actions -------------------------------------------------------------
        if !input.mouse_locked {
            return;
        }
        let slot = i32::from(input.selected_slot);
        if just(GameAction::Primary) {
            if let Some(mob) = raycast_mob(world, origin, dir) {
                hit_mob(
                    world,
                    mob,
                    Some(id),
                    ctx.config,
                    ctx.mobs,
                    &mut ctx.rngs.passive,
                );
                ctx.events.push(EcsEvent::MobHit { entity: mob });
            } else if let Some(tb) = targeted {
                // Aucune vérification de mode ni de dureté (B19).
                let broken = ctx.blocks.block_at(tb);
                ctx.blocks.set_block(tb.x, tb.y, tb.z, ids.air);
                if let Some(inv) = world.get_mut::<InventoryComp>(id) {
                    inv.0.add_block(broken, ids.air);
                }
                ctx.events.push(EcsEvent::BlockBroken {
                    pos: tb,
                    block: broken,
                });
            }
        }
        if just(GameAction::Drop) {
            if let Some(inv) = world.get_mut::<InventoryComp>(id) {
                inv.0.remove_one(HOTBAR_START + slot);
            }
        }
        if just(GameAction::Secondary) {
            if let Some(pt) = place_target {
                try_place(world, ctx, id, pt, slot, &feet, &collider);
            }
        }
    }
}

/// Pose du bloc tenu si le cube cible n'intersecte pas l'AABB du joueur (consommé même en
/// créatif ; les mobs ne bloquent pas ; un fluide est remplacé).
fn try_place(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    id: EntityId,
    pt: IVec3,
    slot: i32,
    feet: &Transform,
    collider: &Collider,
) {
    let Some(inv) = world.get_mut::<InventoryComp>(id) else {
        return;
    };
    let Some(held) = inv.0.hotbar_slot(slot) else {
        return;
    };
    if aabb_intersects_block(pt, feet.pos.x, feet.pos.y, feet.pos.z, collider) {
        return;
    }
    ctx.blocks.set_block(pt.x, pt.y, pt.z, held.block);
    inv.0.remove_one(HOTBAR_START + slot);
    ctx.events.push(EcsEvent::BlockPlaced {
        pos: pt,
        block: held.block,
    });
}
