//! Composants (spec §4, §19.1). Types Java respectés : positions `f64`, le reste en `f32`.

use nb_core::glam::{DVec3, IVec3, Vec3};
use nb_data::BlockId;

use crate::inventory::Inventory;
use crate::jmath::{jmax, jmin};

/// Identifiant d'entité façon Java : création monotone à partir de 0, jamais réutilisé.
/// Sert aussi d'ordre d'itération (ids croissants, décision de Gabriel).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EntityId(pub i32);

/// Position des **pieds** (bas de l'AABB, centre en X/Z). `yaw`/`pitch` en radians pour les mobs.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Transform {
    pub pos: DVec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl Transform {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self {
            pos: DVec3::new(x, y, z),
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

/// Vitesse courante et consignes de l'IA (`target*`, inutilisées pour le joueur).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Velocity {
    pub v: Vec3,
    pub grounded: bool,
    pub target: Vec3,
}

/// AABB : **largeurs complètes** (±`width/2`), de `y` à `y + height`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Collider {
    pub width_x: f32,
    pub width_z: f32,
    pub height: f32,
    pub eye_offset: f32,
}

impl Collider {
    pub fn new(width_x: f32, width_z: f32, height: f32, eye_offset: f32) -> Self {
        Self {
            width_x,
            width_z,
            height,
            eye_offset,
        }
    }

    /// `widthX / 2f`.
    #[inline]
    pub fn half_x(&self) -> f32 {
        self.width_x / 2.0
    }

    /// `widthZ / 2f`.
    #[inline]
    pub fn half_z(&self) -> f32 {
        self.width_z / 2.0
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Health {
    pub health: f32,
    pub max_health: f32,
    pub alive: bool,
}

impl Health {
    pub fn new(max_health: f32) -> Self {
        Self {
            health: max_health,
            max_health,
            alive: true,
        }
    }
}

/// Clé du mob dans le `MobRegistry` (résolue à chaque usage).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MobType {
    pub mob_id: String,
}

impl MobType {
    pub fn new(id: impl Into<String>) -> Self {
        Self { mob_id: id.into() }
    }
}

/// `PassiveMobAIStateComponent`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct PassiveAi {
    pub move_timer: f32,
    pub moving: bool,
    pub flee_timer: f32,
}

/// `AggressiveMobAIStateComponent`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct AggressiveAi {
    pub move_timer: f32,
    pub moving: bool,
    pub attack_cooldown: f32,
}

/// `InventoryComponent`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct InventoryComp(pub Inventory);

/// Mode de jeu ; les noms Java (`SURVIVAL`, `CREATIVE`) servent aux sauvegardes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GameMode {
    Survival,
    Creative,
}

impl GameMode {
    /// Nom Java de l'énumération.
    pub fn java_name(self) -> &'static str {
        match self {
            GameMode::Survival => "SURVIVAL",
            GameMode::Creative => "CREATIVE",
        }
    }

    /// Lecture d'une sauvegarde : valeur inconnue → `CREATIVE`.
    pub fn from_java_name(s: &str) -> Self {
        match s {
            "SURVIVAL" => GameMode::Survival,
            _ => GameMode::Creative,
        }
    }
}

/// État de la caméra lu/écrit par l'ECS (remplace la référence `Camera` du Java).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CameraState {
    /// Yeux.
    pub position: Vec3,
    /// Tangage en degrés (positif = regard vers le bas), borné à ±89.
    pub pitch_deg: f32,
    /// Lacet en degrés (non borné).
    pub yaw_deg: f32,
    /// Champ de vision en radians.
    pub fov: f32,
}

impl CameraState {
    pub fn new(position: Vec3, fov: f32) -> Self {
        Self {
            position,
            pitch_deg: 0.0,
            yaw_deg: 0.0,
            fov,
        }
    }

    /// `Camera.rotate(dPitch, dYaw)`.
    pub fn rotate(&mut self, d_pitch: f32, d_yaw: f32) {
        self.pitch_deg = jmax(-89.0, jmin(89.0, self.pitch_deg + d_pitch));
        self.yaw_deg += d_yaw;
    }

    /// Direction du regard normalisée : `(cos p · sin y, −sin p, −cos p · cos y)`.
    pub fn look_dir(&self) -> Vec3 {
        let p = f64::from(self.pitch_deg).to_radians() as f32;
        let y = f64::from(self.yaw_deg).to_radians() as f32;
        let (sp, cp) = (p.sin(), p.cos());
        let (sy, cy) = (y.sin(), y.cos());
        Vec3::new(cp * sy, -sp, -cp * cy).normalize()
    }
}

/// `PlayerComponent`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Player {
    pub camera: CameraState,
    pub game_mode: GameMode,
    pub flying: bool,
    pub lava_timer: f32,
    pub double_tap_timer: f32,
    pub current_fov: f32,
    pub targeted_block: Option<IVec3>,
    pub place_target: Option<IVec3>,
    pub fluid_at_eye_level: BlockId,
}

impl Player {
    pub fn new(camera: CameraState, game_mode: GameMode, base_fov: f32, air: BlockId) -> Self {
        Self {
            camera,
            game_mode,
            flying: false,
            lava_timer: 0.0,
            double_tap_timer: f32::MAX,
            current_fov: base_fov,
            targeted_block: None,
            place_target: None,
            fluid_at_eye_level: air,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_rotation_and_direction() {
        let mut c = CameraState::new(Vec3::ZERO, 1.0);
        c.rotate(100.0, 30.0);
        assert_eq!(c.pitch_deg, 89.0);
        assert_eq!(c.yaw_deg, 30.0);
        c.rotate(-200.0, 400.0);
        assert_eq!(c.pitch_deg, -89.0);
        assert_eq!(c.yaw_deg, 430.0);

        let mut c = CameraState::new(Vec3::ZERO, 1.0);
        assert!((c.look_dir() - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);
        c.yaw_deg = 90.0;
        assert!((c.look_dir() - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6);
        c.yaw_deg = 0.0;
        c.pitch_deg = 89.0;
        assert!(c.look_dir().y < -0.99, "pitch positif = vers le bas");
    }

    #[test]
    fn game_mode_names() {
        assert_eq!(GameMode::from_java_name("SURVIVAL"), GameMode::Survival);
        assert_eq!(GameMode::from_java_name("CREATIVE"), GameMode::Creative);
        assert_eq!(GameMode::from_java_name("adventure"), GameMode::Creative);
        assert_eq!(GameMode::Survival.java_name(), "SURVIVAL");
    }

    #[test]
    fn defaults() {
        let h = Health::new(5.0);
        assert_eq!((h.health, h.max_health, h.alive), (5.0, 5.0, true));
        let p = Player::new(
            CameraState::new(Vec3::ZERO, 1.2),
            GameMode::Survival,
            1.2,
            0,
        );
        assert_eq!(p.double_tap_timer, f32::MAX);
        assert!(!p.flying);
        assert_eq!(Collider::new(0.6, 1.0, 1.8, 1.62).half_z(), 0.5);
    }
}
