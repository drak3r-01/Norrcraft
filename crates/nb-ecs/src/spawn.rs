//! `MobSpawnSystem` (spec §10) : toutes les secondes, despawn par distance horizontale puis
//! tentatives de spawn par type de mob, dans l'ordre du registre.

use std::f64::consts::PI;

use nb_core::math::CHUNK_SIZE_Y;
use nb_core::JavaRandom;
use nb_data::{BlockRegistry, MobBehaviorType, MobConfig, MobMedium, Persistence};

use crate::blocks::BlockAccess;
use crate::components::{
    AggressiveAi, Collider, EntityId, Health, MobType, PassiveAi, Transform, Velocity,
};
use crate::density;
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
        // Règles de densité (session 5.2) : espèces hors saison, joueurs pour la visibilité.
        let moment = density::Moment::of(ctx.config, ctx.environment);
        let extinct = density::extinct(ctx, &moment);
        let viewers = density::viewers(world);
        for id in world.query_ids::<(&MobType, &Transform)>() {
            let Some(t) = world.copy::<Transform>(id) else {
                continue;
            };
            let rules = world
                .get::<MobType>(id)
                .and_then(|m| ctx.mobs.get(&m.mob_id))
                .and_then(|c| c.spawn.as_ref().map(|r| (c.id.as_str(), r.persistence)));
            if let Some((mob_id, persistence)) = rules {
                let gone = if persistence == Persistence::Ephemeral {
                    let far = {
                        let nearest_sq = origins
                            .iter()
                            .map(|o| {
                                let dx = t.pos.x as f32 - o.x;
                                let dz = t.pos.z as f32 - o.z;
                                dx * dx + dz * dz
                            })
                            .fold(f32::MAX, f32::min);
                        nearest_sq > max_dist_sq
                    };
                    far || (extinct.contains(mob_id)
                        && !density::in_view(&viewers, t.pos)
                        && ctx.rngs.spawn.next_float() < density::VANISH_CHANCE)
                } else if persistence == Persistence::Herd {
                    // Les hardes sont réabsorbées par `HerdSystem`, qui garde leur effectif.
                    false
                } else {
                    // Sédentaires, uniques : ils restent tant que leur lieu existe.
                    !ctx.blocks
                        .is_chunk_loaded(floor_f(t.pos.x as f32), floor_f(t.pos.z as f32))
                };
                if gone {
                    world.destroy(id);
                    ctx.events.push(EcsEvent::MobDespawned { entity: id });
                }
                continue;
            }
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
        for cfg in ctx.mobs.all().iter().filter(|c| c.spawn.is_none()) {
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
        let cap_players = players;
        density::run(
            world,
            ctx,
            (floor_f(origin.x), floor_f(origin.z)),
            cap_players,
        );
    }
}

pub(crate) struct Spawner<'a> {
    pub(crate) blocks: &'a dyn BlockAccess,
    pub(crate) reg: &'a BlockRegistry,
    pub(crate) origin_x: i32,
    pub(crate) origin_z: i32,
    pub(crate) radius: i32,
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
                let y = feet_y(cfg, self.blocks, self.reg, gx, gy, gz);
                let id = spawn_mob(world, cfg, f64::from(gx) + 0.5, y, f64::from(gz) + 0.5, rng);
                events.push(EcsEvent::MobSpawned { entity: id });
            }
            return;
        }
    }

    pub(crate) fn find_spawn_y(&self, cfg: &MobConfig, x: i32, z: i32) -> i32 {
        let y = match cfg.medium {
            MobMedium::Land => find_surface_y(self.blocks, self.reg, x, z),
            MobMedium::Water => {
                find_water_y(self.blocks, self.reg, x, z, cfg.spawn_min_water_depth)
            }
            // Sans règles de densité, un volatile n'apparaît pas (comportement d'origine).
            MobMedium::Air if cfg.spawn.is_none() => -1,
            MobMedium::Air => find_air_y(self.blocks, self.reg, cfg, x, z),
            MobMedium::Ice => find_ice_y(self.blocks, self.reg, x, z),
            MobMedium::Amphibian => find_shore_y(
                self.blocks,
                self.reg,
                x,
                z,
                cfg.spawn_min_water_depth.max(2),
            ),
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
                if !cfg.spawn_biomes.iter().any(|n| n == &b.name) {
                    return false;
                }
            }
        }
        let water = self.reg.ids().water;
        // Un volatile ne dépend d'aucun sol : seuls l'altitude et le biome comptent.
        if cfg.medium == MobMedium::Air {
            return true;
        }
        // En l'eau (aquatique, amphibie à la nage) on regarde le fond, sinon sous les pieds.
        let wet = self.blocks.block(x, y, z) == water;
        let mut floor_y = y - 1;
        if wet {
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
        if !wet && (floor.name.ends_with("leaves") || floor.name == "cactus" || !floor.solid) {
            return false;
        }
        cfg.spawn_surfaces.is_empty() || cfg.spawn_surfaces.contains(&floor.name)
    }
}

/// `findSurfaceY` : premier bloc solide (de 127 à 1) surmonté d'AIR → `y + 1`, sinon −1.
pub fn find_surface_y(blocks: &dyn BlockAccess, reg: &BlockRegistry, x: i32, z: i32) -> i32 {
    let air = reg.ids().air;
    let mut col = [air; CHUNK_SIZE_Y];
    blocks.read_column(x, z, &mut col);
    for y in (1..=127usize).rev() {
        if !reg.from_id(col[y]).solid {
            continue;
        }
        // `y + 1 = 128` est hors du monde : de l'air, comme `BlockAccess::block`.
        let above = col.get(y + 1).copied().unwrap_or(air);
        if above == air {
            return y as i32 + 1;
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
    let mut col = [reg.ids().air; CHUNK_SIZE_Y];
    blocks.read_column(x, z, &mut col);
    let at = |y: i32| {
        usize::try_from(y)
            .ok()
            .and_then(|i| col.get(i))
            .copied()
            .unwrap_or(reg.ids().air)
    };
    for y in (min_depth..=127).rev() {
        if at(y) != water {
            continue;
        }
        if (1..min_depth).all(|dy| at(y + dy) == water) {
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
    if cfg.behavior.is_some() {
        // Profil Norrcraft : moteur d'états (session 5.1), gîte au point d'apparition.
        world.insert(id, crate::brain::Brain::new(t.pos));
    } else if cfg.behavior_type == MobBehaviorType::Aggressive {
        world.insert(id, AggressiveAi::default());
    } else {
        world.insert(id, PassiveAi::default());
    }
    id
}

/// Hauteur des pieds pour une colonne `gy` renvoyée par `find_spawn_y` : sur le sol pour un
/// marcheur (ou un amphibie à terre), au milieu du bloc pour un nageur ou un volatile.
pub(crate) fn feet_y(
    cfg: &MobConfig,
    blocks: &dyn BlockAccess,
    reg: &BlockRegistry,
    x: i32,
    gy: i32,
    z: i32,
) -> f64 {
    let floating = match cfg.medium {
        MobMedium::Land | MobMedium::Ice => false,
        MobMedium::Water | MobMedium::Air => true,
        MobMedium::Amphibian => blocks.block(x, gy, z) == reg.ids().water,
    };
    f64::from(gy) + if floating { 0.5 } else { 0.0 }
}

/// Hauteur d'apparition d'un volatile : au-dessus du plus haut bloc (terre, arbre ou eau) de la
/// colonne, à l'altitude de croisière du profil (ou `spawn.altitude`), sous le plafond du monde.
pub fn find_air_y(
    blocks: &dyn BlockAccess,
    reg: &BlockRegistry,
    cfg: &MobConfig,
    x: i32,
    z: i32,
) -> i32 {
    let air = reg.ids().air;
    let mut col = [air; CHUNK_SIZE_Y];
    blocks.read_column(x, z, &mut col);
    let Some(top) = (0..CHUNK_SIZE_Y).rev().find(|&y| col[y] != air) else {
        return -1;
    };
    let altitude = cfg
        .spawn
        .as_ref()
        .and_then(|r| r.altitude)
        .or_else(|| {
            cfg.behavior.as_deref().and_then(|n| {
                nb_data::behavior::BehaviorBook::builtin()
                    .get(n)
                    .map(|p| p.cruise)
            })
        })
        .unwrap_or(10.0)
        .max(2.0);
    let y = (top as i32 + 1 + altitude as i32).min(CHUNK_SIZE_Y as i32 - 8);
    if y <= top as i32 {
        return -1;
    }
    y
}

/// Premier bloc plein de la colonne s'il est de la glace et à l'air libre : `y + 1`, sinon −1.
pub fn find_ice_y(blocks: &dyn BlockAccess, reg: &BlockRegistry, x: i32, z: i32) -> i32 {
    let air = reg.ids().air;
    let mut col = [air; CHUNK_SIZE_Y];
    blocks.read_column(x, z, &mut col);
    for y in (1..CHUNK_SIZE_Y).rev() {
        if !reg.from_id(col[y]).solid {
            continue;
        }
        let above = col.get(y + 1).copied().unwrap_or(air);
        let free = !reg.from_id(above).solid && above != reg.ids().water;
        return if reg.is_ice(col[y]) && free {
            y as i32 + 1
        } else {
            -1
        };
    }
    -1
}

/// Habitat d'un amphibie : la rive (sol à l'air libre avec de l'eau à 3 blocs ou moins) ou à
/// défaut une colonne d'eau d'au moins `min_depth` blocs.
pub fn find_shore_y(
    blocks: &dyn BlockAccess,
    reg: &BlockRegistry,
    x: i32,
    z: i32,
    min_depth: i32,
) -> i32 {
    let water = reg.ids().water;
    let land = find_surface_y(blocks, reg, x, z);
    if land > 0 {
        let wet_near = (-3..=3).any(|dx| {
            (-3..=3).any(|dz| (land - 2..land).any(|y| blocks.block(x + dx, y, z + dz) == water))
        });
        if wet_near {
            return land;
        }
    }
    find_water_y(blocks, reg, x, z, min_depth)
}
