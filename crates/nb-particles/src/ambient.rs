//! Émetteurs d'ambiance autour du joueur : pluie, neige, lucioles, feuilles, lave, magma,
//! bulles et particules en suspension sous l'eau.
//!
//! Tout est piloté par des *taux* (événements par seconde) accumulés : le résultat ne dépend
//! pas de la fréquence d'images, et le coût par frame est borné (≈ 40 requêtes monde en tout).

use glam::Vec3;

use crate::presets as pre;
use crate::system::{Emit, ParticleSystem};
use crate::world::{Cell, ParticleWorld};

/// Rayon (blocs) de la zone d'ambiance autour du joueur.
const RADIUS: f32 = 22.0;
/// Échantillons de cellules par seconde pour les sources ponctuelles (feuillage, lave…).
const CELL_SAMPLES_PER_SEC: f32 = 900.0;

/// État du ciel et du joueur nécessaire aux émetteurs.
#[derive(Clone, Copy, Debug)]
pub struct AmbientCtx {
    pub eye: Vec3,
    pub forward: Vec3,
    /// 0..1 ; la pluie tombe là où le ciel est dégagé.
    pub rain: f32,
    /// 0..1 ; remplace la pluie en biome froid.
    pub snow: f32,
    /// 0 (jour) .. 1 (nuit noire).
    pub night: f32,
    pub eye_in_water: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Ambient {
    rain_acc: f32,
    snow_acc: f32,
    sample_acc: f32,
    firefly_acc: f32,
    bubble_acc: f32,
    mote_acc: f32,
}

impl Ambient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(
        &mut self,
        fx: &mut ParticleSystem,
        world: &dyn ParticleWorld,
        c: &AmbientCtx,
        dt: f32,
    ) {
        if c.eye_in_water {
            self.underwater(fx, c, dt);
        } else {
            self.weather(fx, world, c, dt);
            self.fireflies(fx, world, c, dt);
        }
        self.cells(fx, world, c, dt);
    }

    /// Pluie / neige : colonne autour du joueur, uniquement là où `sky_open`.
    fn weather(&mut self, fx: &mut ParticleSystem, world: &dyn ParticleWorld, c: &AmbientCtx, dt: f32) {
        const TOP: i32 = 20;
        let room = (0.9 - fx.load()).clamp(0.0, 1.0);
        self.rain_acc += 650.0 * c.rain * room * dt;
        self.snow_acc += 330.0 * c.snow * room * dt;
        let (rain, snow) = (self.rain_acc as u32, self.snow_acc as u32);
        self.rain_acc -= rain as f32;
        self.snow_acc -= snow as f32;
        for (def, n) in [(&pre::RAIN, rain), (&pre::SNOW, snow)] {
            for _ in 0..n {
                let a = fx.rng.f32() * std::f32::consts::TAU;
                let rad = fx.rng.f32().sqrt() * 17.0;
                let y = c.eye.y + fx.rng.range(-4.0, 14.0);
                let p = Vec3::new(c.eye.x + a.cos() * rad, y, c.eye.z + a.sin() * rad);
                let (x, yy, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
                if world.cell(x, yy, z) != Cell::Air || !world.sky_open(x, yy, z, TOP) {
                    continue;
                }
                let e = Emit::at(p, 1).dir(Vec3::NEG_Y);
                fx.spawn(def, &e);
            }
        }
    }

    /// Lucioles : de nuit, près du sol, en plein air.
    fn fireflies(&mut self, fx: &mut ParticleSystem, world: &dyn ParticleWorld, c: &AmbientCtx, dt: f32) {
        if c.night < 0.4 || c.rain > 0.2 || c.snow > 0.2 {
            return;
        }
        self.firefly_acc += 9.0 * c.night * dt;
        let n = self.firefly_acc as u32;
        self.firefly_acc -= n as f32;
        for _ in 0..n {
            let a = fx.rng.f32() * std::f32::consts::TAU;
            let rad = fx.rng.f32().sqrt() * 18.0;
            let p = Vec3::new(
                c.eye.x + a.cos() * rad,
                c.eye.y + fx.rng.range(-6.0, 4.0),
                c.eye.z + a.sin() * rad,
            );
            let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
            if world.cell(x, y, z) != Cell::Air {
                continue;
            }
            // Sol herbeux à moins de 4 blocs sous le point.
            if (1..=4).any(|d| world.cell(x, y - d, z) == Cell::Solid) {
                fx.spawn(&pre::FIREFLY, &Emit::at(p, 1));
            }
        }
    }

    /// Feuillage, lave, magma : cellules tirées au hasard (comme `generateRandomParticles`).
    fn cells(&mut self, fx: &mut ParticleSystem, world: &dyn ParticleWorld, c: &AmbientCtx, dt: f32) {
        self.sample_acc += CELL_SAMPLES_PER_SEC * dt;
        let n = self.sample_acc as u32;
        self.sample_acc -= n as f32;
        let r = RADIUS as i32;
        let (cx, cy, cz) = (c.eye.x.floor() as i32, c.eye.y.floor() as i32, c.eye.z.floor() as i32);
        for _ in 0..n {
            if fx.load() > 0.9 {
                break;
            }
            let x = cx + fx.rng.below(2 * r as u32) as i32 - r;
            let y = cy + fx.rng.below(2 * r as u32) as i32 - r;
            let z = cz + fx.rng.below(2 * r as u32) as i32 - r;
            let cell = world.cell(x, y, z);
            let base = Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5);
            match cell {
                Cell::Leaves => {
                    if world.cell(x, y - 1, z) == Cell::Air && fx.rng.f32() < 0.1 {
                        let t = world.leaf_tint(x, z);
                        let tint = [t[0] * 2.2, t[1] * 2.2, t[2] * 2.2];
                        let p = base + Vec3::new(0.0, -0.1, 0.0);
                        fx.spawn(&pre::LEAF, &Emit::at(p, 1).dir(Vec3::NEG_Y).tint(tint));
                    }
                }
                Cell::Lava => {
                    if world.cell(x, y + 1, z) == Cell::Air {
                        let top = base + Vec3::Y * 1.05;
                        let roll = fx.rng.f32();
                        if roll < 0.05 {
                            fx.spawn(&pre::LAVA_SPIT, &Emit::at(top, 1).dir(Vec3::Y));
                        } else if roll < 0.14 {
                            fx.spawn(&pre::LAVA_POP, &Emit::at(top, 1));
                        } else if roll < 0.24 {
                            fx.spawn(&pre::EMBER, &Emit::at(top, 1).dir(Vec3::Y));
                        } else if roll < 0.28 {
                            fx.spawn(&pre::SMOKE, &Emit::at(top, 1).dir(Vec3::Y));
                        }
                    }
                    if world.cell(x, y - 1, z) == Cell::Air && fx.rng.f32() < 0.03 {
                        let p = base + Vec3::new(0.0, -0.1, 0.0);
                        fx.spawn(&pre::LAVA_DRIP, &Emit::at(p, 1).dir(Vec3::NEG_Y));
                    }
                }
                Cell::Magma => {
                    if world.cell(x, y + 1, z) == Cell::Air && fx.rng.f32() < 0.07 {
                        fx.spawn(&pre::EMBER, &Emit::at(base + Vec3::Y * 1.05, 1).dir(Vec3::Y));
                    }
                }
                _ => {}
            }
        }
    }

    /// Bulles qui montent de la bouche et poussière en suspension autour de la caméra.
    fn underwater(&mut self, fx: &mut ParticleSystem, c: &AmbientCtx, dt: f32) {
        self.bubble_acc += 1.6 * dt;
        self.mote_acc += 26.0 * dt;
        let (b, m) = (self.bubble_acc as u32, self.mote_acc as u32);
        self.bubble_acc -= b as f32;
        self.mote_acc -= m as f32;
        for _ in 0..b {
            let p = c.eye + c.forward * 0.45 + Vec3::new(0.0, -0.25, 0.0);
            fx.spawn(&pre::BUBBLE, &Emit::at(p, 1).dir(Vec3::Y));
        }
        for _ in 0..m {
            let p = c.eye + fx.rng.unit_vec() * fx.rng.range(0.8, 6.0);
            fx.spawn(&pre::WATER_MOTE, &Emit::at(p, 1));
        }
    }
}
