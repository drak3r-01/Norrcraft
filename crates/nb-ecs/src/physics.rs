//! Physique des entités (spec §7) et raycasts (§8).
//!
//! Les fonctions de collision sont libres (sans état) pour être partagées avec
//! `PlayerInputSystem`. Les types numériques suivent le Java (positions `f64`, vitesses `f32`,
//! accumulations du raycast en `f32`).

use nb_core::glam::{IVec3, Vec3};
use nb_data::{BlockId, BlockRegistry, MobBehaviorType, MobMedium, MobRegistry};

use crate::blocks::BlockAccess;
use crate::components::{Collider, EntityId, Health, MobType, Player, Transform, Velocity};
use crate::jmath::{floor_f, floor_i, lerp, signum};
use crate::schedule::{System, TickCtx};
use crate::world::EntityWorld;

pub const FRICTION_GROUND: f32 = 6.0;
pub const FRICTION_AIR: f32 = 2.0;
pub const FRICTION_WATER: f32 = 4.0;
pub const FALL_DAMAGE_THRESHOLD: f32 = 10.0;
pub const FALL_DAMAGE_SCALE: f32 = 0.5;
pub const RAYCAST_STEP: f32 = 0.05;
/// Saut automatique sans `WorldConfig` (jamais le cas en pratique).
pub const DEFAULT_MOB_JUMP_FORCE: f32 = 9.0;

/// Vue en lecture sur les blocs + registre (solidité, ids connus).
#[derive(Clone, Copy)]
pub struct BlockEnv<'a> {
    pub blocks: &'a dyn BlockAccess,
    pub reg: &'a BlockRegistry,
}

/// Cellules entières balayées par une AABB (bornes **inclusives**, B3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cells {
    min: IVec3,
    max: IVec3,
}

impl Cells {
    fn of(cx: f64, cy: f64, cz: f64, c: &Collider) -> Self {
        let (hx, hz) = (f64::from(c.half_x()), f64::from(c.half_z()));
        Self {
            min: IVec3::new(floor_i(cx - hx), floor_i(cy), floor_i(cz - hz)),
            max: IVec3::new(
                floor_i(cx + hx),
                floor_i(cy + f64::from(c.height)),
                floor_i(cz + hz),
            ),
        }
    }

    fn any(&self, mut f: impl FnMut(i32, i32, i32) -> bool) -> bool {
        for x in self.min.x..=self.max.x {
            for y in self.min.y..=self.max.y {
                for z in self.min.z..=self.max.z {
                    if f(x, y, z) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl<'a> BlockEnv<'a> {
    pub fn new(blocks: &'a dyn BlockAccess, reg: &'a BlockRegistry) -> Self {
        Self { blocks, reg }
    }

    #[inline]
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId {
        self.blocks.block(x, y, z)
    }

    #[inline]
    pub fn is_solid(&self, id: BlockId) -> bool {
        self.reg.from_id(id).solid
    }

    /// `checkCollision` (mobs) : un bloc solide dans les cellules de l'AABB.
    pub fn check_collision(&self, cx: f64, cy: f64, cz: f64, c: &Collider) -> bool {
        Cells::of(cx, cy, cz, c).any(|x, y, z| self.is_solid(self.block(x, y, z)))
    }

    /// `checkPlayerCollision` : comme `checkCollision`, mais le nénuphar n'est qu'une tranche
    /// de 0.015 au bas de sa cellule.
    pub fn check_player_collision(&self, cx: f64, cy: f64, cz: f64, c: &Collider) -> bool {
        let ids = self.reg.ids();
        let (hx, hz) = (f64::from(c.half_x()), f64::from(c.half_z()));
        let p_min_y = cy as f32;
        let p_max_y = (cy + f64::from(c.height)) as f32;
        let cells = Cells {
            min: IVec3::new(floor_i(cx - hx), floor_f(p_min_y), floor_i(cz - hz)),
            max: IVec3::new(floor_i(cx + hx), floor_f(p_max_y), floor_i(cz + hz)),
        };
        cells.any(|x, y, z| {
            let b = self.block(x, y, z);
            if b == ids.air {
                false
            } else if b == ids.lily_pad {
                let pad_max_y = y as f32 + 0.015;
                p_min_y < pad_max_y && p_max_y > y as f32
            } else {
                self.is_solid(b)
            }
        })
    }

    /// Eau **ou** lave dans les cellules de l'AABB.
    pub fn is_in_fluid(&self, cx: f64, cy: f64, cz: f64, c: &Collider) -> bool {
        let ids = self.reg.ids();
        Cells::of(cx, cy, cz, c).any(|x, y, z| {
            let b = self.block(x, y, z);
            b == ids.water || b == ids.lava
        })
    }

    /// Lave dans les cellules de l'AABB.
    pub fn is_in_lava(&self, cx: f64, cy: f64, cz: f64, c: &Collider) -> bool {
        let lava = self.reg.ids().lava;
        Cells::of(cx, cy, cz, c).any(|x, y, z| self.block(x, y, z) == lava)
    }

    /// Bloc à hauteur des yeux si WATER/LAVA, sinon AIR.
    pub fn fluid_at_eye_level(&self, ex: f64, ey: f64, ez: f64) -> BlockId {
        let ids = self.reg.ids();
        let b = self.block(floor_i(ex), floor_i(ey), floor_i(ez));
        if b == ids.water || b == ids.lava {
            b
        } else {
            ids.air
        }
    }

    /// Raycast blocs à pas fixe (§8.1, B8).
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<RaycastHit> {
        let ids = self.reg.ids();
        let step = RAYCAST_STEP;
        let mut distance = 0.0f32;
        let mut current = origin;
        let mut last = IVec3::new(floor_f(origin.x), floor_f(origin.y), floor_f(origin.z));
        while distance < max_distance {
            let c = IVec3::new(floor_f(current.x), floor_f(current.y), floor_f(current.z));
            let b = self.blocks.block_at(c);
            if b != ids.air && b != ids.water && b != ids.lava {
                return Some(RaycastHit {
                    hit: c,
                    adjacent: last,
                });
            }
            last = c;
            current.x += dir.x * step;
            current.y += dir.y * step;
            current.z += dir.z * step;
            distance += step;
        }
        None
    }

    /// `moveAxis` (mobs terrestres, §7.2) : collision, saut automatique, dégâts de chute.
    #[allow(clippy::too_many_arguments)]
    pub fn move_axis(
        &self,
        t: &mut Transform,
        v: &mut Velocity,
        c: &Collider,
        health: Option<&mut Health>,
        dx: f32,
        dy: f32,
        dz: f32,
        jump_force: f32,
        fall_damage_enabled: bool,
    ) {
        if dx != 0.0 {
            let nx = t.pos.x + f64::from(dx);
            if !self.check_collision(nx, t.pos.y, t.pos.z, c) {
                t.pos.x = nx;
            } else {
                v.v.x = 0.0;
                if v.grounded {
                    v.v.y = jump_force;
                }
            }
        }
        if dz != 0.0 {
            let nz = t.pos.z + f64::from(dz);
            if !self.check_collision(t.pos.x, t.pos.y, nz, c) {
                t.pos.z = nz;
            } else {
                v.v.z = 0.0;
                if v.grounded {
                    v.v.y = jump_force;
                }
            }
        }
        if dy != 0.0 {
            let ny = t.pos.y + f64::from(dy);
            if !self.check_collision(t.pos.x, ny, t.pos.z, c) {
                t.pos.y = ny;
                v.grounded = false;
            } else {
                if dy < 0.0 {
                    if let (false, Some(h), true) = (v.grounded, health, fall_damage_enabled) {
                        let speed = v.v.y.abs();
                        if speed > FALL_DAMAGE_THRESHOLD {
                            h.health -= (speed - FALL_DAMAGE_THRESHOLD) * FALL_DAMAGE_SCALE;
                            if h.health <= 0.0 {
                                h.alive = false;
                            }
                        }
                    }
                    v.grounded = true;
                }
                v.v.y = 0.0;
            }
        }
    }

    /// `moveAxisSwim` (§7.3) : sans saut, sans `grounded`, sans dégâts.
    pub fn move_axis_swim(
        &self,
        t: &mut Transform,
        v: &mut Velocity,
        c: &Collider,
        dx: f32,
        dy: f32,
        dz: f32,
    ) {
        if dx != 0.0 {
            let nx = t.pos.x + f64::from(dx);
            if !self.check_collision(nx, t.pos.y, t.pos.z, c) {
                t.pos.x = nx;
            } else {
                v.v.x = 0.0;
            }
        }
        if dz != 0.0 {
            let nz = t.pos.z + f64::from(dz);
            if !self.check_collision(t.pos.x, t.pos.y, nz, c) {
                t.pos.z = nz;
            } else {
                v.v.z = 0.0;
            }
        }
        if dy != 0.0 {
            let ny = t.pos.y + f64::from(dy);
            if !self.check_collision(t.pos.x, ny, t.pos.z, c) {
                t.pos.y = ny;
            } else {
                v.v.y = 0.0;
            }
        }
    }

    /// `movePlayerAxis` (§7.4) : collision joueur et sol virtuel au-dessus des chunks non
    /// chargés (B18).
    pub fn move_player_axis(
        &self,
        t: &mut Transform,
        v: &mut Velocity,
        c: &Collider,
        dx: f32,
        dy: f32,
        dz: f32,
    ) {
        if dx != 0.0 {
            let nx = t.pos.x + f64::from(dx);
            if !self.check_player_collision(nx, t.pos.y, t.pos.z, c) {
                t.pos.x = nx;
            } else {
                v.v.x = 0.0;
            }
        }
        if dz != 0.0 {
            let nz = t.pos.z + f64::from(dz);
            if !self.check_player_collision(t.pos.x, t.pos.y, nz, c) {
                t.pos.z = nz;
            } else {
                v.v.z = 0.0;
            }
        }
        if dy != 0.0 {
            if dy < 0.0
                && !self
                    .blocks
                    .is_chunk_loaded(floor_i(t.pos.x), floor_i(t.pos.z))
            {
                v.grounded = true;
                v.v.y = 0.0;
            } else {
                let ny = t.pos.y + f64::from(dy);
                if !self.check_player_collision(t.pos.x, ny, t.pos.z, c) {
                    t.pos.y = ny;
                    v.grounded = false;
                } else {
                    if dy < 0.0 {
                        v.grounded = true;
                    }
                    v.v.y = 0.0;
                }
            }
        }
    }
}

/// `isAABBIntersectingBlock` (inégalités strictes).
pub fn aabb_intersects_block(block: IVec3, cx: f64, cy: f64, cz: f64, c: &Collider) -> bool {
    let (bx, by, bz) = (f64::from(block.x), f64::from(block.y), f64::from(block.z));
    let (hx, hz) = (f64::from(c.half_x()), f64::from(c.half_z()));
    cx - hx < bx + 1.0
        && cx + hx > bx
        && cy < by + 1.0
        && cy + f64::from(c.height) > by
        && cz - hz < bz + 1.0
        && cz + hz > bz
}

/// Résultat du raycast blocs : bloc touché et cellule de l'échantillon précédent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RaycastHit {
    pub hit: IVec3,
    pub adjacent: IVec3,
}

/// Portée du coup du joueur sur un mob.
pub const MOB_HIT_RANGE: f32 = 5.0;

/// Intersection rayon / AABB par slabs ; renvoie le `t` d'entrée (ou de sortie si l'origine
/// est dans la boîte), `-1` si pas de touche.
pub fn ray_aabb(o: Vec3, d: Vec3, min: Vec3, max: Vec3) -> f32 {
    let mut t_near = f32::NEG_INFINITY;
    let mut t_far = f32::INFINITY;
    for a in 0..3 {
        if d[a] != 0.0 {
            let mut t1 = (min[a] - o[a]) / d[a];
            let mut t2 = (max[a] - o[a]) / d[a];
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            t_near = crate::jmath::jmax(t_near, t1);
            t_far = crate::jmath::jmin(t_far, t2);
        } else if o[a] < min[a] || o[a] > max[a] {
            return -1.0;
        }
    }
    if t_near > t_far || t_far < 0.0 {
        return -1.0;
    }
    if t_near >= 0.0 {
        t_near
    } else {
        t_far
    }
}

/// `raycastMobEntity` (§8.2, B9) : mob le plus proche à `0 ≤ t < 5`, sans occlusion par les
/// blocs ; à égalité, le premier rencontré.
pub fn raycast_mob(world: &EntityWorld, origin: Vec3, dir: Vec3) -> Option<EntityId> {
    let mut nearest = None;
    let mut nearest_t = MOB_HIT_RANGE;
    for id in world.query_ids::<(&Transform, &MobType, &Collider)>() {
        let (Some(t), Some(c)) = (world.copy::<Transform>(id), world.copy::<Collider>(id)) else {
            continue;
        };
        let (hx, hz) = (c.half_x(), c.half_z());
        let (x, y, z) = (t.pos.x as f32, t.pos.y as f32, t.pos.z as f32);
        let tv = ray_aabb(
            origin,
            dir,
            Vec3::new(x - hx, y, z - hz),
            Vec3::new(x + hx, y + c.height, z + hz),
        );
        if tv >= 0.0 && tv < nearest_t {
            nearest_t = tv;
            nearest = Some(id);
        }
    }
    nearest
}

fn mob_kind(
    world: &EntityWorld,
    mobs: &MobRegistry,
    id: EntityId,
) -> Option<(MobMedium, MobBehaviorType)> {
    let mt = world.get::<MobType>(id)?;
    mobs.get(&mt.mob_id).map(|c| (c.medium, c.behavior_type))
}

/// `PhysicsSystem` : intègre les mobs (le joueur est géré par `PlayerInputSystem`) puis sépare
/// les entités qui se chevauchent.
#[derive(Debug, Default, Clone, Copy)]
pub struct PhysicsSystem;

impl System for PhysicsSystem {
    fn name(&self) -> &'static str {
        "Physics"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let env = BlockEnv::new(&*ctx.blocks, ctx.block_reg);
        let gravity = ctx.config.gravity;
        let jump_force = ctx.config.jump_force;
        let fall_damage = ctx.config.fall_damage_enabled;
        for id in world.query_ids::<(&Transform, &Velocity, &Collider)>() {
            if world.has::<Player>(id) {
                continue;
            }
            let water = matches!(mob_kind(world, ctx.mobs, id), Some((MobMedium::Water, _)));
            let Some((t, v, c, mut h)) = world.query_one_mut::<(
                &mut Transform,
                &mut Velocity,
                &Collider,
                Option<&mut Health>,
            )>(id) else {
                continue;
            };
            if water {
                v.v.x = lerp(v.v.x, v.target.x, FRICTION_WATER * dt);
                v.v.z = lerp(v.v.z, v.target.z, FRICTION_WATER * dt);
                v.v.y = lerp(v.v.y, v.target.y, FRICTION_WATER * dt);
                // Chaque déplacement est évalué au moment de l'appel (comme en Java).
                let d = v.v.x * dt;
                env.move_axis_swim(t, v, c, d, 0.0, 0.0);
                let d = v.v.z * dt;
                env.move_axis_swim(t, v, c, 0.0, 0.0, d);
                let d = v.v.y * dt;
                env.move_axis_swim(t, v, c, 0.0, d, 0.0);
            } else {
                let friction = if v.grounded {
                    FRICTION_GROUND
                } else {
                    FRICTION_AIR
                };
                v.v.x = lerp(v.v.x, v.target.x, friction * dt);
                v.v.z = lerp(v.v.z, v.target.z, friction * dt);
                v.v.y += gravity * dt;
                let (jf, fd) = (jump_force, fall_damage);
                // Un saut automatique en X/Z modifie `vy` et donc le déplacement Y du même tick.
                let d = v.v.x * dt;
                env.move_axis(t, v, c, h.as_deref_mut(), d, 0.0, 0.0, jf, fd);
                let d = v.v.z * dt;
                env.move_axis(t, v, c, h.as_deref_mut(), 0.0, 0.0, d, jf, fd);
                let d = v.v.y * dt;
                env.move_axis(t, v, c, h, 0.0, d, 0.0, jf, fd);
            }
        }
        push_apart_entities(world, ctx.mobs);
    }
}

/// `pushApartEntities` (§7.5, B7) : séparation horizontale des AABB qui se chevauchent ; le
/// joueur est immobile ; les paires orque/poisson (agressif/passif aquatiques) sont ignorées.
pub fn push_apart_entities(world: &mut EntityWorld, mobs: &MobRegistry) {
    struct Info {
        id: EntityId,
        col: Collider,
        player: bool,
        aggressive_water: bool,
        passive_water: bool,
    }
    let list: Vec<Info> = world
        .query_ids::<(&Transform, &Collider)>()
        .into_iter()
        .filter_map(|id| {
            let kind = mob_kind(world, mobs, id);
            Some(Info {
                id,
                col: world.copy::<Collider>(id)?,
                player: world.has::<Player>(id),
                aggressive_water: kind == Some((MobMedium::Water, MobBehaviorType::Aggressive)),
                passive_water: kind == Some((MobMedium::Water, MobBehaviorType::Passive)),
            })
        })
        .collect();
    for i in 0..list.len() {
        let a = &list[i];
        for b in &list[i + 1..] {
            if (a.aggressive_water && b.passive_water) || (a.passive_water && b.aggressive_water) {
                continue;
            }
            let Some((ta, tb)) = world.query_pair_mut::<&mut Transform>(a.id, b.id) else {
                continue;
            };
            let (ca, cb) = (&a.col, &b.col);
            let dx = ta.pos.x - tb.pos.x;
            let dz = ta.pos.z - tb.pos.z;
            let overlap_x = f64::from(ca.half_x() + cb.half_x()) - dx.abs();
            let overlap_z = f64::from(ca.half_z() + cb.half_z()) - dz.abs();
            if overlap_x <= 0.0 || overlap_z <= 0.0 {
                continue;
            }
            if ta.pos.y + f64::from(ca.height) <= tb.pos.y
                || tb.pos.y + f64::from(cb.height) <= ta.pos.y
            {
                continue;
            }
            let push_a = if a.player {
                0.0
            } else if b.player {
                1.0
            } else {
                0.5
            };
            let push_b = if b.player {
                0.0
            } else if a.player {
                1.0
            } else {
                0.5
            };
            if overlap_x < overlap_z {
                let sep = overlap_x * if dx == 0.0 { 1.0 } else { signum(dx) };
                ta.pos.x += sep * push_a;
                tb.pos.x -= sep * push_b;
            } else {
                let sep = overlap_z * if dz == 0.0 { 1.0 } else { signum(dz) };
                ta.pos.z += sep * push_a;
                tb.pos.z -= sep * push_b;
            }
        }
    }
}
