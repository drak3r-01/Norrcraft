//! Façade appelée par nb-game dans l'état PLAYING (équivaut à `World.update(pos, dt)` côté
//! entités ; la mise à jour des chunks reste à nb-game / nb-world).

use nb_core::glam::Vec3;
use nb_data::BlockId;
use nb_world::WorldConfig;

use crate::components::{CameraState, EntityId, GameMode, MobType, Player};
use crate::player::{init_player, respawn_player};
use crate::schedule::{Schedule, TickCtx};
use crate::world::EntityWorld;

/// Pas de temps fixe de la boucle de jeu (`(float) (1.0 / 60.0)`).
pub const TICK_DT: f32 = (1.0f64 / 60.0) as f32;

#[derive(Debug)]
pub struct Simulation {
    pub entities: EntityWorld,
    pub schedule: Schedule,
    pub player: Option<EntityId>,
    /// Dernière position (yeux) utilisée pour le spawn et les particules.
    spawn_player_pos: Vec3,
}

impl Default for Simulation {
    fn default() -> Self {
        Self::new()
    }
}

impl Simulation {
    /// Monde vide avec l'ordonnancement Java.
    pub fn new() -> Self {
        Self {
            entities: EntityWorld::new(),
            schedule: Schedule::java_default(),
            player: None,
            spawn_player_pos: Vec3::ZERO,
        }
    }

    /// `World.initPlayer` (une fois par monde).
    pub fn init_player(&mut self, camera: CameraState, mode: GameMode, air: BlockId) -> EntityId {
        self.spawn_player_pos = camera.position;
        let id = init_player(&mut self.entities, camera, mode, air);
        self.player = Some(id);
        id
    }

    /// Caméra du joueur (yeux, orientation, FOV) pour le rendu.
    pub fn camera(&self) -> Option<CameraState> {
        self.player
            .and_then(|p| self.entities.copy::<Player>(p))
            .map(|p| p.camera)
    }

    /// Un tick PLAYING : position de spawn (= caméra du tick
    /// précédent), puis les systèmes dans l'ordre Java.
    pub fn tick(&mut self, ctx: &mut TickCtx<'_>, dt: f32) {
        if let Some(cam) = self.camera() {
            self.spawn_player_pos = cam.position;
        }
        let pos = self.spawn_player_pos;
        self.entities.spawn_origin = pos;
        self.schedule.run(&mut self.entities, ctx, dt);
    }

    /// `World.hitMobEntity` (§9.1).
    pub fn hit_mob(&mut self, id: EntityId, ctx: &mut TickCtx<'_>) {
        crate::health::hit_mob(
            &mut self.entities,
            id,
            self.player,
            ctx.config,
            ctx.mobs,
            &mut ctx.rngs.passive,
        );
    }

    /// `World.respawnPlayer` (§13).
    pub fn respawn_player(&mut self, cfg: &WorldConfig) {
        if let Some(p) = self.player {
            respawn_player(&mut self.entities, p, cfg);
        }
    }

    /// Détruit toutes les entités ayant un `MobType` (sortie des réglages de mobs si `dirty`).
    pub fn despawn_all_mobs(&mut self) {
        for id in self.entities.query_ids::<&MobType>() {
            self.entities.destroy(id);
        }
    }
}
