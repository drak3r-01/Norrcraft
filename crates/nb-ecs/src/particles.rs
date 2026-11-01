//! Particules (spec §17) : simulation et émission ambiante (feuilles, lave).

use nb_core::glam::Vec3;
use nb_core::JavaRandom;
use nb_data::{BlockRegistry, ParticleRegistry, ParticleType};

use crate::blocks::BlockAccess;
use crate::jmath::floor_f;

/// Une particule vivante.
#[derive(Clone, PartialEq, Debug)]
pub struct Particle {
    pub kind: ParticleType,
    pub position: Vec3,
    pub velocity: Vec3,
    pub color: Vec3,
    pub life: f32,
}

impl Particle {
    pub fn new(kind: &ParticleType, position: Vec3, velocity: Vec3, color: Vec3) -> Self {
        Self {
            life: kind.life_time,
            kind: kind.clone(),
            position,
            velocity,
            color,
        }
    }

    /// Avance d'un pas ; `true` si encore vivante. Gravité positive = chute (la lave, −0.2,
    /// accélère vers le haut : B23).
    pub fn update(&mut self, dt: f32) -> bool {
        self.velocity.y -= self.kind.gravity * dt;
        self.position.x += self.velocity.x * dt;
        self.position.y += self.velocity.y * dt;
        self.position.z += self.velocity.z * dt;
        self.life -= dt;
        self.life > 0.0
    }
}

/// `ParticleManager` : liste ordonnée, sans plafond ni collision.
#[derive(Clone, Debug, Default)]
pub struct ParticleManager {
    particles: Vec<Particle>,
}

impl ParticleManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Met à jour et retire les mortes en préservant l'ordre.
    pub fn update(&mut self, dt: f32) {
        self.particles.retain_mut(|p| p.update(dt));
    }

    /// Type absent (`None`) → ignoré.
    pub fn spawn(&mut self, kind: Option<&ParticleType>, pos: Vec3, vel: Vec3, color: Vec3) {
        if let Some(k) = kind {
            self.particles.push(Particle::new(k, pos, vel, color));
        }
    }

    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }

    /// `World.generateRandomParticles(playerPos)` : 200 cellules tirées dans un cube de 48 autour
    /// du joueur ; feuilles qui tombent sous les feuillages, gouttes au-dessus et sous la lave.
    pub fn generate_ambient(
        &mut self,
        player_pos: Vec3,
        blocks: &dyn BlockAccess,
        reg: &BlockRegistry,
        types: &ParticleRegistry,
        rng: &mut JavaRandom,
    ) {
        const RADIUS: i32 = 24;
        let ids = reg.ids();
        let leaf = types.get("leaf");
        let lava_p = types.get("lava");
        let (px, py, pz) = (
            floor_f(player_pos.x),
            floor_f(player_pos.y),
            floor_f(player_pos.z),
        );
        for _ in 0..200 {
            let rx = px + rng.next_int_bound(RADIUS * 2) - RADIUS;
            let ry = py + rng.next_int_bound(RADIUS * 2) - RADIUS;
            let rz = pz + rng.next_int_bound(RADIUS * 2) - RADIUS;
            let b = blocks.block(rx, ry, rz);
            let (fx, fy, fz) = (rx as f32, ry as f32, rz as f32);
            if reg.from_id(b).name.contains("leaves") {
                if blocks.block(rx, ry - 1, rz) == ids.air && rng.next_float() < 0.15 {
                    let tint = blocks.leaves_tint(rx, rz);
                    let color = Vec3::new(
                        (tint[0] * 2.2) as f32,
                        (tint[1] * 2.2) as f32,
                        (tint[2] * 2.2) as f32,
                    );
                    let pos = Vec3::new(fx + rng.next_float(), fy - 0.1, fz + rng.next_float());
                    let vx = (rng.next_float() - 0.5) * 0.5;
                    let vz = (rng.next_float() - 0.5) * 0.5;
                    self.spawn(leaf, pos, Vec3::new(vx, -0.5, vz), color);
                }
            } else if b == ids.lava {
                let color = Vec3::ONE;
                if blocks.block(rx, ry + 1, rz) == ids.air && rng.next_float() < 0.05 {
                    let pos = Vec3::new(fx + rng.next_float(), fy + 1.1, fz + rng.next_float());
                    let vx = (rng.next_float() - 0.5) * 0.5;
                    let vy = 1.5 + rng.next_float();
                    let vz = (rng.next_float() - 0.5) * 0.5;
                    self.spawn(lava_p, pos, Vec3::new(vx, vy, vz), color);
                }
                if blocks.block(rx, ry - 1, rz) == ids.air && rng.next_float() < 0.02 {
                    let pos = Vec3::new(fx + rng.next_float(), fy - 0.1, fz + rng.next_float());
                    self.spawn(lava_p, pos, Vec3::new(0.0, -1.0, 0.0), color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::MemoryBlocks;
    use nb_core::glam::IVec3;

    #[test]
    fn particle_physics_and_expiry() {
        let types = ParticleRegistry::builtin();
        let leaf = types.get("leaf").unwrap();
        let lava = types.get("lava").unwrap();
        let mut m = ParticleManager::new();
        m.spawn(Some(leaf), Vec3::ZERO, Vec3::ZERO, Vec3::ONE);
        m.spawn(Some(lava), Vec3::ZERO, Vec3::ZERO, Vec3::ONE);
        m.spawn(None, Vec3::ZERO, Vec3::ZERO, Vec3::ONE);
        assert_eq!(m.len(), 2);
        m.update(1.0);
        assert!(m.particles()[0].position.y < 0.0, "la feuille tombe");
        assert!(
            m.particles()[1].position.y > 0.0,
            "la lave monte (gravité négative)"
        );
        m.update(1.5); // lave : vie 2.0 → morte
        assert_eq!(m.len(), 1);
        assert_eq!(m.particles()[0].kind.id, "leaf");
        m.update(10.0);
        assert!(m.is_empty());
    }

    #[test]
    fn ambient_emission_is_deterministic_and_located() {
        let reg = BlockRegistry::builtin();
        let types = ParticleRegistry::builtin();
        let ids = *reg.ids();
        let mut blocks = MemoryBlocks::new(ids.air);
        // Canopée de feuilles au-dessus du joueur, nappe de lave suspendue.
        blocks.fill(IVec3::new(-24, 70, -24), IVec3::new(23, 70, 23), ids.leaves);
        blocks.fill(IVec3::new(-24, 60, -24), IVec3::new(23, 60, 23), ids.lava);
        let run = |seed| {
            let mut m = ParticleManager::new();
            let mut rng = JavaRandom::new(seed);
            for _ in 0..200 {
                m.generate_ambient(Vec3::new(0.5, 64.0, 0.5), &blocks, &reg, &types, &mut rng);
            }
            m
        };
        let a = run(7);
        let b = run(7);
        assert!(!a.is_empty());
        assert_eq!(a.particles(), b.particles());
        for p in a.particles() {
            match p.kind.id.as_str() {
                "leaf" => {
                    assert!((p.position.y - 69.9).abs() < 1e-4);
                    assert_eq!(p.velocity.y, -0.5);
                    assert!((p.color.x - 0.66).abs() < 1e-5);
                }
                "lava" => assert!(
                    (p.position.y - 61.1).abs() < 1e-4 || (p.position.y - 59.9).abs() < 1e-4
                ),
                other => panic!("type inattendu {other}"),
            }
        }
        assert!(a.particles().iter().any(|p| p.kind.id == "lava"));
    }
}
