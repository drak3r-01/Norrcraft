//! `HealthSystem` (spec §13) et coup du joueur (`World.hitMobEntity`, §9.1).

use nb_core::JavaRandom;
use nb_data::MobRegistry;
use nb_world::WorldConfig;

use crate::ai::trigger_player_flee;
use crate::components::{EntityId, Health, PassiveAi, Player, RemotePlayer, Transform, Velocity};
use crate::player::respawn_player;
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;

/// Détruit les entités mortes ; la mort du joueur déclenche immédiatement sa réapparition.
#[derive(Debug, Default, Clone, Copy)]
pub struct HealthSystem;

impl System for HealthSystem {
    fn name(&self) -> &'static str {
        "Health"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, _dt: f32) {
        let mut dead = Vec::new();
        for id in world.query_ids::<&Health>() {
            if world.copy::<Health>(id).is_some_and(|h| h.alive) {
                continue;
            }
            // Un joueur distant (partie LAN) meurt chez lui : l'hôte ne le détruit pas.
            if world.has::<RemotePlayer>(id) {
                continue;
            }
            if world.has::<Player>(id) {
                ctx.events.push(EcsEvent::PlayerDied);
                respawn_player(world, id, ctx.config);
                ctx.events.push(EcsEvent::PlayerRespawned);
            } else {
                dead.push(id);
            }
        }
        for id in dead {
            if let Some(mob_id) = crate::survival::mob_type_of(world, id) {
                crate::survival::loot_for_kill(world, ctx, id, &mob_id);
            }
            world.destroy(id);
            ctx.events.push(EcsEvent::MobDied { entity: id });
        }
    }
}

/// `HealthSystem` d'un client LAN : seule la mort du joueur local est gérée (réapparition). Les
/// mobs répliqués ne sont jamais détruits ici : l'hôte décide de leur mort.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlayerHealthSystem;

impl System for PlayerHealthSystem {
    fn name(&self) -> &'static str {
        "PlayerHealth"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, _dt: f32) {
        for id in world.query_ids::<(&Health, &Player)>() {
            if world.copy::<Health>(id).is_some_and(|h| h.alive) {
                continue;
            }
            ctx.events.push(EcsEvent::PlayerDied);
            respawn_player(world, id, ctx.config);
            ctx.events.push(EcsEvent::PlayerRespawned);
        }
    }
}

/// Recul horizontal appliqué par le coup du joueur.
pub const HIT_KNOCKBACK: f32 = 5.0;
/// Vitesse verticale imposée par le coup du joueur.
pub const HIT_KNOCKBACK_UP: f32 = 4.0;

/// `World.hitMobEntity(entity)` : dégâts `attackDamage`, fuite des passifs survivants, recul
/// (même sur un coup mortel). `rng` = RNG du système passif (comme en Java).
pub fn hit_mob(
    world: &mut EntityWorld,
    mob: EntityId,
    player: Option<EntityId>,
    cfg: &WorldConfig,
    mobs: &MobRegistry,
    rng: &mut JavaRandom,
) {
    hit_mob_damage(world, mob, player, cfg.attack_damage, mobs, rng);
}

/// Comme [`hit_mob`] avec des dégâts explicites (arme tenue en survie).
pub fn hit_mob_damage(
    world: &mut EntityWorld,
    mob: EntityId,
    player: Option<EntityId>,
    damage: f32,
    mobs: &MobRegistry,
    rng: &mut JavaRandom,
) {
    let Some(h) = world.get_mut::<Health>(mob) else {
        return;
    };
    h.health -= damage;
    if h.health <= 0.0 {
        h.alive = false;
    } else if world.has::<PassiveAi>(mob) {
        trigger_player_flee(world, mob, mobs, rng);
    } else if world.has::<crate::brain::Brain>(mob) {
        crate::brain::provoke(world, mob, player);
    }
    let player_pos = player
        .and_then(|p| world.copy::<Transform>(p))
        .map(|t| t.pos);
    let mob_pos = world.copy::<Transform>(mob).map(|t| t.pos);
    let (Some(pp), Some(mp)) = (player_pos, mob_pos) else {
        return;
    };
    let Some(v) = world.get_mut::<Velocity>(mob) else {
        return;
    };
    let dx = mp.x - pp.x;
    let dz = mp.z - pp.z;
    let dist = (dx * dx + dz * dz).sqrt();
    if dist > 0.01 {
        let inv = (1.0 / dist) as f32;
        v.v.x += dx as f32 * inv * HIT_KNOCKBACK;
        v.v.z += dz as f32 * inv * HIT_KNOCKBACK;
        v.v.y = HIT_KNOCKBACK_UP;
    }
}
