//! Catalogue d'effets du jeu et raccourcis d'émission.
//!
//! Chaque effet est une constante [`EffectDef`] : ajuster un rendu = changer des nombres ici.

use glam::Vec3;

use crate::effect::{behav, r, sprites, EffectDef};
use crate::system::{Emit, ParticleSystem};

const D: EffectDef = EffectDef::DEFAULT;

// ---------------------------------------------------------------------------- blocs

/// Éclats d'un bloc cassé (texture du bloc, quart de tuile).
pub const BLOCK_DEBRIS: EffectDef = EffectDef {
    life: r(0.6, 1.3),
    speed: r(1.2, 3.6),
    gravity: 14.0,
    drag: 0.4,
    size: r(0.06, 0.13),
    size_end: 0.8,
    color: [1.0, 1.0, 1.0, 1.0],
    color_end: [0.8, 0.8, 0.8, 0.0],
    color_jitter: 0.12,
    spin: r(-6.0, 6.0),
    bounce: 0.35,
    friction: 0.45,
    jitter: [0.4, 0.4, 0.4],
    ..D
}
.with_behav(behav::COLLIDE | behav::FADE_LATE);

/// Petits éclats sous les pieds (pas, atterrissage).
pub const FOOT_CHIP: EffectDef = EffectDef {
    life: r(0.3, 0.6),
    speed: r(0.6, 1.6),
    spread: 0.6,
    gravity: 12.0,
    size: r(0.035, 0.07),
    color_end: [0.8, 0.8, 0.8, 0.0],
    color_jitter: 0.1,
    bounce: 0.2,
    friction: 0.5,
    ..D
}
.with_behav(behav::COLLIDE | behav::FADE_LATE);

/// Nuage de poussière teinté (casse, pas, atterrissage, pose).
pub const DUST_PUFF: EffectDef = EffectDef {
    sprite: sprites::SMOKE,
    life: r(0.45, 0.9),
    speed: r(0.2, 0.9),
    gravity: -0.15,
    drag: 3.0,
    size: r(0.12, 0.22),
    size_end: 2.6,
    color: [0.78, 0.74, 0.68, 0.55],
    color_end: [0.78, 0.74, 0.68, 0.0],
    color_jitter: 0.08,
    spin: r(-1.5, 1.5),
    jitter: [0.3, 0.1, 0.3],
    ..D
}
.with_behav(behav::FADE_IN);

// ---------------------------------------------------------------------------- eau

pub const SPLASH_DROP: EffectDef = EffectDef {
    sprite: sprites::DROP,
    life: r(0.5, 0.95),
    speed: r(2.0, 5.0),
    spread: 0.55,
    gravity: 14.0,
    size: r(0.05, 0.1),
    color: [0.75, 0.88, 1.0, 0.9],
    color_end: [0.75, 0.88, 1.0, 0.0],
    jitter: [0.25, 0.0, 0.25],
    ..D
}
.with_behav(behav::DIE_IN_WATER | behav::COLLIDE);

pub const SPLASH_FOAM: EffectDef = EffectDef {
    sprite: sprites::SMOKE,
    life: r(0.4, 0.8),
    speed: r(0.4, 1.4),
    spread: 0.8,
    gravity: -0.3,
    drag: 2.5,
    size: r(0.15, 0.28),
    size_end: 2.0,
    color: [0.92, 0.97, 1.0, 0.6],
    color_end: [0.92, 0.97, 1.0, 0.0],
    jitter: [0.3, 0.0, 0.3],
    ..D
};

pub const BUBBLE: EffectDef = EffectDef {
    sprite: sprites::BUBBLE,
    life: r(1.5, 4.0),
    speed: r(0.05, 0.3),
    gravity: -1.6,
    drag: 1.2,
    sway: 0.9,
    size: r(0.04, 0.1),
    size_end: 1.1,
    color: [0.85, 0.95, 1.0, 0.85],
    color_end: [0.85, 0.95, 1.0, 0.0],
    jitter: [0.2, 0.2, 0.2],
    ..D
}
.with_behav(behav::BUBBLE | behav::COLLIDE | behav::FADE_LATE);

/// Particules en suspension sous l'eau (ambiance).
pub const WATER_MOTE: EffectDef = EffectDef {
    life: r(3.0, 6.0),
    speed: r(0.02, 0.12),
    drag: 0.5,
    sway: 0.15,
    size: r(0.015, 0.035),
    color: [0.85, 0.95, 1.0, 0.35],
    color_end: [0.85, 0.95, 1.0, 0.0],
    ..D
}
.with_behav(behav::FADE_IN);

// ---------------------------------------------------------------------------- météo

pub const RAIN: EffectDef = EffectDef {
    sprite: sprites::STREAK,
    life: r(1.1, 1.5),
    speed: r(21.0, 27.0),
    spread: 0.015,
    size: r(0.035, 0.05),
    color: [0.72, 0.8, 0.95, 0.5],
    color_end: [0.72, 0.8, 0.95, 0.5],
    ..D
}
.vertical(14.0)
.with_behav(behav::COLLIDE | behav::DIE_IN_WATER | behav::RAIN_SPLASH);

pub const RAIN_SPLASH: EffectDef = EffectDef {
    sprite: sprites::DROP,
    life: r(0.18, 0.38),
    speed: r(0.9, 1.9),
    spread: 0.7,
    gravity: 11.0,
    size: r(0.03, 0.05),
    color: [0.8, 0.9, 1.0, 0.75],
    color_end: [0.8, 0.9, 1.0, 0.0],
    jitter: [0.04, 0.0, 0.04],
    ..D
};

pub const RAIN_RIPPLE: EffectDef = EffectDef {
    sprite: sprites::BUBBLE,
    life: r(0.3, 0.45),
    size: r(0.08, 0.1),
    size_end: 4.0,
    color: [0.9, 0.96, 1.0, 0.55],
    color_end: [0.9, 0.96, 1.0, 0.0],
    jitter: [0.04, 0.0, 0.04],
    ..D
};

pub const SNOW: EffectDef = EffectDef {
    sprite: sprites::SNOW,
    life: r(9.0, 12.0),
    speed: r(0.9, 1.7),
    spread: 0.25,
    drag: 0.35,
    sway: 0.55,
    size: r(0.04, 0.085),
    color: [1.0, 1.0, 1.0, 0.9],
    color_end: [1.0, 1.0, 1.0, 0.9],
    spin: r(-1.5, 1.5),
    ..D
}
.with_behav(behav::DIE_ON_HIT | behav::DIE_IN_WATER | behav::FADE_IN);

// ---------------------------------------------------------------------------- ambiance

/// Feuille qui se détache d'un feuillage (anciennement `leaf`).
pub const LEAF: EffectDef = EffectDef {
    sprite: sprites::LEAF,
    life: r(4.0, 6.5),
    speed: r(0.05, 0.3),
    gravity: 0.35,
    drag: 0.9,
    sway: 0.9,
    size: r(0.13, 0.19),
    color: [1.0, 1.0, 1.0, 1.0],
    color_end: [1.0, 1.0, 1.0, 0.0],
    spin: r(-2.5, 2.5),
    jitter: [0.45, 0.0, 0.45],
    ..D
}
.with_behav(behav::DIE_ON_HIT | behav::DIE_IN_WATER | behav::FADE_LATE);

pub const FIREFLY: EffectDef = EffectDef {
    sprite: sprites::SOFT,
    life: r(5.0, 9.0),
    speed: r(0.05, 0.25),
    drag: 0.4,
    sway: 0.6,
    size: r(0.05, 0.085),
    color: [0.8, 1.0, 0.35, 0.95],
    color_end: [0.8, 1.0, 0.35, 0.0],
    ..D
}
.emissive()
.with_behav(behav::FADE_IN);

// ---------------------------------------------------------------------------- feu et lave

/// Étincelle montante (lave, magma, feu).
pub const EMBER: EffectDef = EffectDef {
    sprite: sprites::SPARK,
    life: r(1.2, 2.8),
    speed: r(0.3, 1.1),
    spread: 0.5,
    gravity: -0.45,
    drag: 0.6,
    sway: 0.9,
    size: r(0.045, 0.09),
    size_end: 0.3,
    color: [1.0, 0.65, 0.2, 1.0],
    color_end: [0.9, 0.12, 0.0, 0.0],
    spin: r(-3.0, 3.0),
    jitter: [0.3, 0.0, 0.3],
    ..D
}
.emissive()
.with_behav(behav::DIE_ON_HIT);

/// Goutte de lave projetée (anciennement `lava`).
pub const LAVA_SPIT: EffectDef = EffectDef {
    sprite: sprites::LAVA_DROP,
    life: r(0.9, 1.8),
    speed: r(1.8, 3.8),
    spread: 0.25,
    gravity: 7.5,
    size: r(0.07, 0.12),
    color: [1.0, 1.0, 1.0, 1.0],
    color_end: [1.0, 0.6, 0.4, 0.0],
    jitter: [0.35, 0.0, 0.35],
    ..D
}
.emissive()
.with_behav(behav::DIE_IN_LAVA | behav::FADE_LATE);

/// Goutte qui tombe sous une nappe de lave.
pub const LAVA_DRIP: EffectDef = EffectDef {
    sprite: sprites::LAVA_DROP,
    life: r(1.0, 2.0),
    speed: r(0.0, 0.3),
    gravity: 9.0,
    size: r(0.07, 0.1),
    color: [1.0, 1.0, 1.0, 1.0],
    color_end: [1.0, 0.7, 0.5, 0.0],
    jitter: [0.4, 0.0, 0.4],
    ..D
}
.emissive()
.with_behav(behav::DIE_ON_HIT | behav::DIE_IN_WATER);

pub const LAVA_POP: EffectDef = EffectDef {
    sprite: sprites::SOFT,
    life: r(0.25, 0.5),
    speed: r(0.0, 0.4),
    size: r(0.12, 0.2),
    size_end: 1.8,
    color: [1.0, 0.55, 0.1, 0.9],
    color_end: [1.0, 0.2, 0.0, 0.0],
    jitter: [0.4, 0.0, 0.4],
    ..D
}
.emissive();

pub const SMOKE: EffectDef = EffectDef {
    sprite: sprites::SMOKE,
    life: r(1.6, 2.8),
    speed: r(0.15, 0.4),
    spread: 0.3,
    gravity: -0.4,
    drag: 0.8,
    sway: 0.35,
    size: r(0.22, 0.34),
    size_end: 4.0,
    color: [0.28, 0.27, 0.27, 0.5],
    color_end: [0.18, 0.18, 0.18, 0.0],
    spin: r(-0.6, 0.6),
    jitter: [0.2, 0.0, 0.2],
    ..D
}
.with_behav(behav::FADE_IN);

pub const FLAME: EffectDef = EffectDef {
    sprite: sprites::FLAME,
    life: r(0.4, 0.8),
    speed: r(0.15, 0.5),
    spread: 0.2,
    gravity: -2.2,
    drag: 0.8,
    size: r(0.2, 0.32),
    size_end: 0.15,
    color: [1.0, 0.82, 0.35, 1.0],
    color_end: [1.0, 0.25, 0.02, 0.0],
    jitter: [0.12, 0.0, 0.12],
    ..D
}
.emissive();

// ---------------------------------------------------------------------------- combat

pub const HIT_SPARK: EffectDef = EffectDef {
    sprite: sprites::SPARK,
    life: r(0.2, 0.45),
    speed: r(1.8, 4.2),
    gravity: 7.0,
    drag: 1.0,
    size: r(0.07, 0.13),
    size_end: 0.3,
    color: [1.0, 0.35, 0.28, 1.0],
    color_end: [0.8, 0.05, 0.05, 0.0],
    spin: r(-8.0, 8.0),
    ..D
}
.emissive();

pub const DEATH_POOF: EffectDef = EffectDef {
    sprite: sprites::SMOKE,
    life: r(0.5, 0.95),
    speed: r(0.8, 2.2),
    gravity: -0.2,
    drag: 3.2,
    size: r(0.3, 0.5),
    size_end: 2.2,
    color: [0.93, 0.93, 0.93, 0.8],
    color_end: [0.8, 0.8, 0.85, 0.0],
    spin: r(-1.0, 1.0),
    jitter: [0.3, 0.4, 0.3],
    ..D
};

/// Retrouve un effet par son nom (outils : `spawn-particles NOM N`).
pub fn by_name(name: &str) -> Option<&'static EffectDef> {
    Some(match name {
        "debris" => &BLOCK_DEBRIS,
        "chip" => &FOOT_CHIP,
        "dust" => &DUST_PUFF,
        "splash" => &SPLASH_DROP,
        "foam" => &SPLASH_FOAM,
        "bubble" => &BUBBLE,
        "mote" => &WATER_MOTE,
        "rain" => &RAIN,
        "rain_splash" => &RAIN_SPLASH,
        "ripple" => &RAIN_RIPPLE,
        "snow" => &SNOW,
        "leaf" => &LEAF,
        "firefly" => &FIREFLY,
        "ember" => &EMBER,
        "lava" => &LAVA_SPIT,
        "lava_drip" => &LAVA_DRIP,
        "lava_pop" => &LAVA_POP,
        "smoke" => &SMOKE,
        "flame" => &FLAME,
        "spark" => &HIT_SPARK,
        "poof" => &DEATH_POOF,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------- raccourcis

/// Une couche de bloc + sa teinte moyenne (RGB 0..1) : ce qu'il faut pour des éclats crédibles.
#[derive(Clone, Copy, Debug)]
pub struct BlockLook {
    pub layer: u16,
    pub tint: [f32; 3],
}

impl ParticleSystem {
    /// Bloc cassé : éclats texturés + poussière.
    pub fn block_break(&mut self, center: Vec3, look: BlockLook, power: f32) {
        let n = (14.0 * power) as u32;
        self.spawn(&BLOCK_DEBRIS, &Emit::at(center, n).block(look.layer));
        self.spawn(&DUST_PUFF, &Emit::at(center, 5).tint(look.tint));
    }

    /// Bloc posé : petit nuage sur la face.
    pub fn block_place(&mut self, center: Vec3, look: BlockLook) {
        self.spawn(&DUST_PUFF, &Emit::at(center, 3).tint(look.tint));
    }

    /// Un pas sur un bloc (`feet` = sol sous le joueur).
    pub fn footstep(&mut self, feet: Vec3, look: BlockLook, vel: Vec3) {
        let e = Emit::at(feet + Vec3::Y * 0.05, 2).inherit(vel * 0.15);
        self.spawn(&FOOT_CHIP, &e.block(look.layer));
        self.spawn(&DUST_PUFF, &Emit::at(feet, 1).tint(look.tint).dir(Vec3::Y));
    }

    /// Atterrissage : anneau de poussière et éclats, proportionnels à la vitesse d'impact.
    pub fn landing(&mut self, feet: Vec3, look: BlockLook, impact_speed: f32) {
        let k = ((impact_speed - 4.0) / 12.0).clamp(0.0, 1.0);
        let n = 4 + (10.0 * k) as u32;
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU + self.rng.f32();
            let dir = Vec3::new(a.cos(), 0.15, a.sin());
            let e = Emit::at(feet + dir * 0.3 + Vec3::Y * 0.05, 1)
                .dir(dir)
                .tint(look.tint);
            self.spawn(&DUST_PUFF, &e);
        }
        self.spawn(
            &FOOT_CHIP,
            &Emit::at(feet + Vec3::Y * 0.05, 3 + (8.0 * k) as u32).block(look.layer),
        );
    }

    /// Entrée dans l'eau (`surface` = point de la surface), `speed` = vitesse verticale.
    pub fn water_splash(&mut self, surface: Vec3, speed: f32) {
        let k = (speed / 10.0).clamp(0.15, 1.0);
        let up = Vec3::Y;
        self.spawn(&SPLASH_DROP, &Emit::at(surface, (22.0 * k) as u32 + 4).dir(up));
        self.spawn(&SPLASH_FOAM, &Emit::at(surface, (6.0 * k) as u32 + 2).dir(up));
        self.spawn(&RAIN_RIPPLE, &Emit::at(surface, 2));
    }

    /// Mob touché.
    pub fn mob_hit(&mut self, center: Vec3) {
        self.spawn(&HIT_SPARK, &Emit::at(center, 8));
    }

    /// Mob mort : nuage blanc de la taille du mob.
    pub fn mob_death(&mut self, center: Vec3) {
        self.spawn(&DEATH_POOF, &Emit::at(center, 14));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_resolves() {
        for n in [
            "debris", "chip", "dust", "splash", "foam", "bubble", "mote", "rain", "rain_splash",
            "ripple", "snow", "leaf", "firefly", "ember", "lava", "lava_drip", "lava_pop",
            "smoke", "flame", "spark", "poof",
        ] {
            assert!(by_name(n).is_some(), "{n}");
        }
    }
}
