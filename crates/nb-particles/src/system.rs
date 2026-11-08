//! Pool de particules à capacité fixe, simulation et export des instances GPU.
//!
//! Choix de conception :
//! * **Aucune allocation** après `new` : le pool est un `Vec` réservé une fois ; un pool plein
//!   refuse simplement les nouvelles particules (les émetteurs ambiants se brident avant).
//! * Suppression par `swap_remove` : l'ordre n'a pas d'importance (pas de tri, mélange alpha
//!   sans écriture de profondeur) et la mise à jour reste un parcours linéaire.
//! * Le monde n'est interrogé **que quand une particule change de cellule** (et seulement pour
//!   celles qui ont un comportement lié aux blocs) : une goutte de pluie coûte ~1 requête/bloc.
//! * L'export vers le GPU remplit un `Vec<GpuParticle>` réutilisé, avec culling (distance et
//!   derrière la caméra) et fondu de distance.

use glam::Vec3;

use crate::effect::{behav, EffectDef};
use crate::gpu::{pack_rgba, unpack_rgba, GpuParticle, FLAG_BLOCK_ATLAS, FLAG_EMISSIVE};
use crate::rng::Rng;
use crate::world::{Cell, ParticleWorld};

/// Une particule vivante (champs chauds d'abord).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f32,
    pub life: f32,
    pub gravity: f32,
    pub drag: f32,
    pub sway: f32,
    pub phase: f32,
    pub size0: f32,
    pub size1: f32,
    pub rot: f32,
    pub spin: f32,
    pub col0: u32,
    pub col1: u32,
    pub gpu_flags: u32,
    pub behav: u32,
    pub bounce: f32,
    pub friction: f32,
    pub layer: u16,
    pub uv: [u8; 4],
}

/// Demande d'émission.
#[derive(Clone, Copy, Debug)]
pub struct Emit {
    pub pos: Vec3,
    /// Direction principale (normalisée ou nulle).
    pub dir: Vec3,
    pub count: u32,
    /// Vitesse héritée (par exemple celle du joueur).
    pub inherit: Vec3,
    /// Multiplie la couleur de départ et de fin.
    pub tint: [f32; 3],
    /// Couche de l'atlas des blocs : remplace le sprite par un éclat de cette texture.
    pub block_layer: Option<u16>,
}

impl Emit {
    pub fn at(pos: Vec3, count: u32) -> Self {
        Self {
            pos,
            dir: Vec3::Y,
            count,
            inherit: Vec3::ZERO,
            tint: [1.0; 3],
            block_layer: None,
        }
    }
    pub fn dir(mut self, d: Vec3) -> Self {
        self.dir = d;
        self
    }
    pub fn tint(mut self, t: [f32; 3]) -> Self {
        self.tint = t;
        self
    }
    pub fn inherit(mut self, v: Vec3) -> Self {
        self.inherit = v;
        self
    }
    pub fn block(mut self, layer: u16) -> Self {
        self.block_layer = Some(layer);
        self
    }
}

/// Point de vue pour l'export.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub eye: Vec3,
    pub forward: Vec3,
    /// Au-delà, les particules ne sont pas dessinées (fondu sur les derniers 20 %).
    pub max_distance: f32,
    /// Multiplicateur de lumière ambiante des particules non émissives.
    pub light: [f32; 3],
}

/// Compteurs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub alive: usize,
    pub spawned: u64,
    pub dropped: u64,
    /// Requêtes monde de la dernière mise à jour.
    pub world_queries: u32,
}

/// Impact différé (création d'éclaboussures après le parcours).
#[derive(Clone, Copy)]
struct Impact {
    pos: Vec3,
    in_water: bool,
}

pub struct ParticleSystem {
    p: Vec<Particle>,
    cap: usize,
    pub rng: Rng,
    impacts: Vec<Impact>,
    stats: Stats,
}

impl ParticleSystem {
    pub fn new(capacity: usize, seed: u64) -> Self {
        Self {
            p: Vec::with_capacity(capacity),
            cap: capacity,
            rng: Rng::new(seed),
            impacts: Vec::with_capacity(256),
            stats: Stats::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.p.len()
    }
    pub fn is_empty(&self) -> bool {
        self.p.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.cap
    }
    /// Taux de remplissage 0..1 (les émetteurs ambiants s'en servent pour se brider).
    pub fn load(&self) -> f32 {
        self.p.len() as f32 / self.cap as f32
    }
    pub fn stats(&self) -> Stats {
        Stats {
            alive: self.p.len(),
            ..self.stats
        }
    }
    pub fn particles(&self) -> &[Particle] {
        &self.p
    }
    pub fn clear(&mut self) {
        self.p.clear();
    }

    /// Crée `e.count` particules de l'effet `def`.
    pub fn spawn(&mut self, def: &EffectDef, e: &Emit) {
        let n = (e.count as usize).min(self.cap - self.p.len());
        self.stats.dropped += (e.count as usize - n) as u64;
        self.stats.spawned += n as u64;
        let dir = e.dir.normalize_or_zero();
        for _ in 0..n {
            let rng = &mut self.rng;
            let d = if def.spread >= 1.0 || dir == Vec3::ZERO {
                rng.unit_vec()
            } else {
                (dir + rng.unit_vec() * def.spread).normalize_or_zero()
            };
            let speed = rng.range(def.speed.min, def.speed.max);
            let jit = Vec3::from(def.jitter);
            let pos = e.pos + jit * Vec3::new(rng.signed(), rng.signed(), rng.signed());
            let life = rng.range(def.life.min, def.life.max).max(0.01);
            let size0 = rng.range(def.size.min, def.size.max);
            let k = 1.0 + def.color_jitter * rng.signed();
            let tint = |c: [f32; 4]| {
                [
                    c[0] * e.tint[0] * k,
                    c[1] * e.tint[1] * k,
                    c[2] * e.tint[2] * k,
                    c[3],
                ]
            };
            let (layer, uv, flags) = match e.block_layer {
                Some(l) => {
                    // Éclat de bloc : un quart de la texture, choisi au hasard.
                    let (u, v) = (rng.below(3) as u8 * 16, rng.below(3) as u8 * 16);
                    (
                        l,
                        [u, v, 16, 16],
                        (def.gpu_flags & !FLAG_EMISSIVE) | FLAG_BLOCK_ATLAS,
                    )
                }
                None => (def.sprite, [0; 4], def.gpu_flags),
            };
            self.p.push(Particle {
                pos,
                vel: d * speed + e.inherit,
                age: 0.0,
                life,
                gravity: def.gravity,
                drag: def.drag,
                sway: def.sway,
                phase: rng.f32() * std::f32::consts::TAU,
                size0,
                size1: size0 * def.size_end,
                rot: rng.f32() * std::f32::consts::TAU,
                spin: rng.range(def.spin.min, def.spin.max),
                col0: pack_rgba(tint(def.color)),
                col1: pack_rgba(tint(def.color_end)),
                gpu_flags: flags,
                behav: def.behav,
                bounce: def.bounce,
                friction: def.friction,
                layer,
                uv,
            });
        }
    }

    /// Fait avancer toutes les particules de `dt` secondes.
    pub fn update(&mut self, dt: f32, world: &dyn ParticleWorld) {
        const WORLD_MASK: u32 = behav::COLLIDE
            | behav::DIE_IN_WATER
            | behav::BUBBLE
            | behav::DIE_IN_LAVA
            | behav::DIE_ON_HIT;
        self.impacts.clear();
        let mut queries = 0u32;
        let mut i = 0;
        while i < self.p.len() {
            let q = &mut self.p[i];
            q.age += dt;
            if q.age >= q.life {
                self.p.swap_remove(i);
                continue;
            }
            // Euler semi-implicite.
            q.vel *= (1.0 - q.drag * dt).max(0.0);
            q.vel.y -= q.gravity * dt;
            if q.sway != 0.0 {
                let a = q.age * 2.5 + q.phase;
                q.vel.x += a.cos() * q.sway * dt;
                q.vel.z += a.sin() * q.sway * dt;
            }
            let old = q.pos;
            q.pos += q.vel * dt;
            q.rot += q.spin * dt;

            if q.behav & WORLD_MASK != 0 {
                let nc = cell_of(q.pos);
                if cell_of(old) != nc {
                    queries += 1;
                    if !resolve(q, old, nc, world, &mut self.impacts) {
                        self.p.swap_remove(i);
                        continue;
                    }
                }
            }
            i += 1;
        }
        self.stats.world_queries = queries;
        self.flush_impacts();
    }

    fn flush_impacts(&mut self) {
        // Éclaboussures bornées par frame : une averse ne doit pas saturer le pool.
        let budget = if self.load() > 0.85 { 0 } else { 48 };
        let list = std::mem::take(&mut self.impacts);
        for im in list.iter().take(budget) {
            if im.in_water {
                self.spawn(&crate::presets::RAIN_RIPPLE, &Emit::at(im.pos, 1));
            } else {
                self.spawn(&crate::presets::RAIN_SPLASH, &Emit::at(im.pos, 2));
            }
        }
        self.impacts = list;
    }

    /// Remplit `out` (vidé au préalable) avec les instances visibles.
    pub fn write_instances(&self, view: &View, out: &mut Vec<GpuParticle>) {
        out.clear();
        out.reserve(self.p.len());
        let max2 = view.max_distance * view.max_distance;
        let fade2 = max2 * 0.64;
        let inv_fade = 1.0 / (view.max_distance * 0.2).max(1e-3);
        for q in &self.p {
            let rel = q.pos - view.eye;
            let d2 = rel.length_squared();
            if d2 > max2 || rel.dot(view.forward) < -q.size0.max(q.size1) {
                continue;
            }
            let mut t = q.age / q.life;
            let size_t = t;
            if q.behav & behav::FADE_LATE != 0 {
                t *= t;
                t *= t;
            }
            let (a, b) = (unpack_rgba(q.col0), unpack_rgba(q.col1));
            let mut c = [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
                a[3] + (b[3] - a[3]) * t,
            ];
            if q.behav & behav::FADE_IN != 0 {
                c[3] *= (t * 8.0).min(1.0);
            }
            if d2 > fade2 {
                c[3] *= ((view.max_distance - d2.sqrt()) * inv_fade).clamp(0.0, 1.0);
            }
            if q.gpu_flags & FLAG_EMISSIVE == 0 {
                c[0] *= view.light[0];
                c[1] *= view.light[1];
                c[2] *= view.light[2];
            }
            out.push(GpuParticle {
                position: q.pos.to_array(),
                size: q.size0 + (q.size1 - q.size0) * size_t,
                color: pack_rgba(c),
                rotation: ((q.rot / std::f32::consts::TAU).rem_euclid(1.0) * 65535.0) as u16,
                layer: q.layer,
                uv: q.uv,
                flags: q.gpu_flags,
            });
        }
    }
}

#[inline]
fn cell_of(p: Vec3) -> (i32, i32, i32) {
    (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
}

/// Applique l'effet de la cellule d'arrivée `nc`. `false` = la particule meurt.
fn resolve(
    q: &mut Particle,
    old: Vec3,
    nc: (i32, i32, i32),
    world: &dyn ParticleWorld,
    impacts: &mut Vec<Impact>,
) -> bool {
    let cell = world.cell(nc.0, nc.1, nc.2);
    match cell {
        Cell::Water if q.behav & behav::DIE_IN_WATER != 0 => {
            if q.behav & behav::RAIN_SPLASH != 0 {
                impacts.push(Impact {
                    pos: Vec3::new(q.pos.x, nc.1 as f32 + 0.9, q.pos.z),
                    in_water: true,
                });
            }
            return false;
        }
        Cell::Lava if q.behav & behav::DIE_IN_LAVA != 0 => return false,
        Cell::Air if q.behav & behav::BUBBLE != 0 => return false,
        _ => {}
    }
    if cell.is_solid() && q.behav & (behav::COLLIDE | behav::DIE_ON_HIT) != 0 {
        if q.behav & (behav::DIE_ON_HIT | behav::RAIN_SPLASH) != 0 {
            if q.behav & behav::RAIN_SPLASH != 0 {
                impacts.push(Impact {
                    pos: Vec3::new(old.x, nc.1 as f32 + 1.02, old.z),
                    in_water: false,
                });
            }
            return false;
        }
        // Balayage par axe (y, x, z) : l'axe libre est conservé, l'axe bloqué est réfléchi.
        let solid = |p: Vec3| {
            let c = cell_of(p);
            world.cell(c.0, c.1, c.2).is_solid()
        };
        let mut np = q.pos;
        if solid(Vec3::new(old.x, np.y, old.z)) {
            np.y = old.y;
            q.vel.y = -q.vel.y * q.bounce;
            let f = 1.0 - q.friction;
            q.vel.x *= f;
            q.vel.z *= f;
        }
        if solid(Vec3::new(np.x, np.y, old.z)) {
            np.x = old.x;
            q.vel.x = -q.vel.x * q.bounce;
        }
        if solid(np) {
            np.z = old.z;
            q.vel.z = -q.vel.z * q.bounce;
        }
        q.pos = np;
    }
    true
}
