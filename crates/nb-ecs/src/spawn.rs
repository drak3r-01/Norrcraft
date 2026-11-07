//! `MobSpawnSystem` (spec §10) : toutes les secondes, despawn par distance horizontale puis
//! tentatives de spawn par type de mob, dans l'ordre du registre.

use std::f64::consts::PI;

use nb_core::JavaRandom;
use nb_data::{BlockRegistry, MobBehaviorType, MobConfig, MobMedium};

use crate::blocks::BlockAccess;
use crate::components::{
    AggressiveAi, Collider, EntityId, Health, MobType, PassiveAi, Transform, Velocity,
};
use crate::jmath::floor_f;
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;

pub const SPAWN_INTERVAL: f32 = 1.0;
pub const MIN_SPAWN_RADIUS: i32 = 16;

#[derive(Debug, Default, Clone, Copy)]
pub struct MobSpawnSystem {
    pub timer: f32,
    /// Tour de rôle entre les joueurs (partie LAN) : un joueur par passe d'apparition.
    pub round: usize,
}

/// `nextInt(bound)` qui renvoie `None` là où le Java lèverait une exception (`bound ≤ 0`).
fn next_int(rng: &mut JavaRandom, bound: i32) -> Option<i32> {
    (bound > 0).then(|| rng.next_int_bound(bound))
}

impl System for MobSpawnSystem {
    fn name(&self) -> &'static str {
        "MobSpawn"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        self.timer += dt;
        if self.timer < SPAWN_INTERVAL {
            return;
        }
        self.timer = 0.0;
        // Un seul joueur en solo ; en partie LAN, l'hôte puis les joueurs distants.
        let mut origins = vec![world.spawn_origin];
        origins.extend(world.extra_origins.iter().copied());
        let origin = origins[self.round % origins.len()];
        self.round = self.round.wrapping_add(1);
        let players = origins.len() as i32;

        // Despawn : distance horizontale en f32 (B21), au joueur le plus proche.
        let r = ctx.config.mob_despawn_radius as f32;
        let max_dist_sq = r * ctx.config.mob_despawn_radius as f32;
        for id in world.query_ids::<(&MobType, &Transform)>() {
            let Some(t) = world.copy::<Transform>(id) else {
                continue;
            };
            let nearest_sq = origins
                .iter()
                .map(|o| {
                    let dx = t.pos.x as f32 - o.x;
                    let dz = t.pos.z as f32 - o.z;
                    dx * dx + dz * dz
                })
                .fold(f32::MAX, f32::min);
            if nearest_sq > max_dist_sq {
                world.destroy(id);
                ctx.events.push(EcsEvent::MobDespawned { entity: id });
            }
        }

        let radius = ctx.config.mob_spawn_radius;
        for cfg in ctx.mobs.all() {
            let count = world
                .query_ids::<&MobType>()
                .into_iter()
                .filter(|&id| world.get::<MobType>(id).is_some_and(|m| m.mob_id == cfg.id))
                .count() as i32;
            // Le plafond de chaque espèce grandit avec le nombre de joueurs.
            if count < cfg.max_count.saturating_mul(players) {
                let spawner = Spawner {
                    blocks: &*ctx.blocks,
                    reg: ctx.block_reg,
                    origin_x: floor_f(origin.x),
                    origin_z: floor_f(origin.z),
                    radius,
                };
                spawner.try_spawn(
                    world,
                    cfg,
                    count,
                    cfg.max_count.saturating_mul(players),
                    &mut ctx.rngs.spawn,
                    ctx.events,
                );
            }
        }
    }
}

struct Spawner<'a> {
    blocks: &'a dyn BlockAccess,
    reg: &'a BlockRegistry,
    origin_x: i32,
    origin_z: i32,
    radius: i32,
}

impl Spawner<'_> {
    /// `trySpawn` : jusqu'à 15 (groupe de 1) ou 10 origines tirées dans un carré à trou central ;
    /// la première origine valide produit un groupe puis on s'arrête. Les cas où le Java lèverait
    /// une exception (`mobSpawnRadius ≤ 0`, `groupSizeMax < groupSizeMin`) ne font rien (B11).
    fn try_spawn(
        &self,
        world: &mut EntityWorld,
        cfg: &MobConfig,
        current: i32,
        cap: i32,
        rng: &mut JavaRandom,
        events: &mut Vec<EcsEvent>,
    ) {
        // Les mobs à conditions d'apparition (phase 9) tentent plus d'origines : beaucoup de
        // points tirés ne sont pas dans leur habitat. Les mobs d'origine gardent 15 / 10.
        let attempts = if cfg.has_habitat() {
            40
        } else if cfg.group_size_max == 1 {
            15
        } else {
            10
        };
        for _ in 0..attempts {
            let Some(ox) = next_int(rng, self.radius.saturating_mul(2)) else {
                return;
            };
            let ox = ox - self.radius;
            let oz = rng.next_int_bound(self.radius.saturating_mul(2)) - self.radius;
            if ox.abs() < MIN_SPAWN_RADIUS && oz.abs() < MIN_SPAWN_RADIUS {
                continue;
            }
            let cx = self.origin_x.wrapping_add(ox);
            let cz = self.origin_z.wrapping_add(oz);
            if self.find_spawn_y(cfg, cx, cz) < 0 {
                continue;
            }
            let room = cap - current;
            let range = cfg.group_size_max - cfg.group_size_min;
            let Some(extra) = next_int(rng, (range + 1).min(room)) else {
                return;
            };
            let group = cfg.group_size_min + extra;
            let scatter = cfg.spawn_scatter_radius;
            for _ in 0..group {
                let gx = cx
                    + if scatter > 0 {
                        rng.next_int_bound(scatter * 2 + 1) - scatter
                    } else {
                        0
                    };
                let gz = cz
                    + if scatter > 0 {
                        rng.next_int_bound(scatter * 2 + 1) - scatter
                    } else {
                        0
                    };
                let gy = self.find_spawn_y(cfg, gx, gz);
                if gy < 0 {
                    continue;
                }
                let y = if cfg.medium == MobMedium::Land {
                    f64::from(gy)
                } else {
                    f64::from(gy) + 0.5
                };
                let id = spawn_mob(world, cfg, f64::from(gx) + 0.5, y, f64::from(gz) + 0.5, rng);
                events.push(EcsEvent::MobSpawned { entity: id });
            }
            return;
        }
    }

    fn find_spawn_y(&self, cfg: &MobConfig, x: i32, z: i32) -> i32 {
        let y = match cfg.medium {
            MobMedium::Land => find_surface_y(self.blocks, self.reg, x, z),
            MobMedium::Water => {
                find_water_y(self.blocks, self.reg, x, z, cfg.spawn_min_water_depth)
            }
            MobMedium::Air => -1,
        };
        if y < 0 || !cfg.has_habitat() || self.habitat_ok(cfg, x, y, z) {
            y
        } else {
            -1
        }
    }

    /// Conditions d'apparition de la phase 9 : biome, altitude, sol (ou fond), profondeur.
    fn habitat_ok(&self, cfg: &MobConfig, x: i32, y: i32, z: i32) -> bool {
        if y < cfg.spawn_min_y || (cfg.spawn_max_y > 0 && y > cfg.spawn_max_y) {
            return false;
        }
        if !cfg.spawn_biomes.is_empty() {
            if let Some(b) = self.blocks.biome_at(x, z) {
                if !cfg.spawn_biomes.iter().any(|n| n == b.name()) {
                    return false;
                }
            }
        }
        let water = self.reg.ids().water;
        // Sol : sous les pieds d'un terrestre, fond de la colonne d'eau pour un aquatique.
        let mut floor_y = y - 1;
        if cfg.medium == MobMedium::Water {
            while floor_y > 0 && self.blocks.block(x, floor_y, z) == water {
                floor_y -= 1;
            }
            if cfg.spawn_max_water_depth > 0 {
                let mut top = y;
                while top < 127 && self.blocks.block(x, top + 1, z) == water {
                    top += 1;
                }
                if top - floor_y > cfg.spawn_max_water_depth {
                    return false;
                }
            }
        }
        let floor = self.reg.from_id(self.blocks.block(x, floor_y, z));
        if cfg.medium == MobMedium::Land
            && (floor.name.ends_with("leaves") || floor.name == "cactus" || !floor.solid)
        {
            return false;
        }
        cfg.spawn_surfaces.is_empty() || cfg.spawn_surfaces.contains(&floor.name)
    }
}

/// `findSurfaceY` : premier bloc solide (de 127 à 1) surmonté d'AIR → `y + 1`, sinon −1.
pub fn find_surface_y(blocks: &dyn BlockAccess, reg: &BlockRegistry, x: i32, z: i32) -> i32 {
    let air = reg.ids().air;
    for y in (1..=127).rev() {
        if !reg.from_id(blocks.block(x, y, z)).solid {
            continue;
        }
        if blocks.block(x, y + 1, z) == air {
            return y + 1;
        }
    }
    -1
}

/// `findWaterY` : colonne d'au moins `min_depth` blocs d'eau → `y + (min_depth − 1) / 2`.
pub fn find_water_y(
    blocks: &dyn BlockAccess,
    reg: &BlockRegistry,
    x: i32,
    z: i32,
    min_depth: i32,
) -> i32 {
    let water = reg.ids().water;
    let min_depth = min_depth.max(1);
    for y in (min_depth..=127).rev() {
        if blocks.block(x, y, z) != water {
            continue;
        }
        if (1..min_depth).all(|dy| blocks.block(x, y + dy, z) == water) {
            return y + (min_depth - 1) / 2;
        }
    }
    -1
}

/// `spawnMob` : composants du mob, lacet aléatoire.
pub fn spawn_mob(
    world: &mut EntityWorld,
    cfg: &MobConfig,
    x: f64,
    y: f64,
    z: f64,
    rng: &mut JavaRandom,
) -> EntityId {
    let id = world.create();
    let mut t = Transform::new(x, y, z);
    t.yaw = rng.next_float() * (PI * 2.0) as f32;
    world.insert(id, t);
    world.insert(id, Velocity::default());
    world.insert(id, Collider::new(cfg.width_x, cfg.width_z, cfg.height, 0.0));
    world.insert(id, Health::new(cfg.max_health));
    world.insert(id, MobType::new(cfg.id.clone()));
    if cfg.behavior_type == MobBehaviorType::Aggressive {
        world.insert(id, AggressiveAi::default());
    } else {
        world.insert(id, PassiveAi::default());
    }
    id
}
