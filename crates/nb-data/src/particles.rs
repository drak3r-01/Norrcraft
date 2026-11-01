//! Registre des particules (`particles.json`).

use std::collections::HashMap;

use nb_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Contenu de `assets/particles.json`, embarqué dans le binaire.
pub const PARTICLES_JSON: &str = include_str!("../../../assets/particles.json");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticleType {
    pub id: String,
    /// Nom de la texture (atlas des particules).
    pub texture: String,
    pub life_time: f32,
    pub scale: f32,
    pub gravity: f32,
}

#[derive(Debug, Clone)]
pub struct ParticleRegistry {
    particles: Vec<ParticleType>,
    by_id: HashMap<String, usize>,
}

impl ParticleRegistry {
    pub fn builtin() -> Self {
        Self::from_json(PARTICLES_JSON).expect("particles.json embarqué invalide")
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let particles: Vec<ParticleType> =
            serde_json::from_str(json).map_err(|e| Error::data("particles.json", e))?;
        let by_id = particles
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();
        let reg = Self { particles, by_id };
        for required in ["leaf", "lava"] {
            if reg.get(required).is_none() {
                return Err(Error::data(
                    "particles.json",
                    format!("particule requise manquante: {required}"),
                ));
            }
        }
        Ok(reg)
    }

    /// `ParticleRegistry.get(id)` (`None` si inconnu, comme le `null` Java).
    pub fn get(&self, id: &str) -> Option<&ParticleType> {
        self.by_id.get(id).map(|&i| &self.particles[i])
    }

    pub fn iter(&self) -> impl Iterator<Item = &ParticleType> {
        self.particles.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_leaf_and_lava() {
        let reg = ParticleRegistry::builtin();
        let leaf = reg.get("leaf").unwrap();
        assert_eq!(leaf.texture, "leaf");
        assert_eq!(leaf.life_time, 5.0);
        assert_eq!(leaf.scale, 0.15);
        assert_eq!(leaf.gravity, 0.5);
        let lava = reg.get("lava").unwrap();
        assert_eq!(lava.texture, "lava_drop");
        assert_eq!(lava.gravity, -0.2);
        assert!(reg.get("missing").is_none());
    }
}
