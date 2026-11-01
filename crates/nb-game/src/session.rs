//! Monde chargé : blocs (nb-world) + entités (nb-ecs) + identité de sauvegarde
//! (`GameContext.world/entityWorld/currentWorldName/allowCheats`, `WorldSaveManager`).

use std::path::Path;
use std::sync::Arc;

use nb_core::glam::Vec3;
use nb_core::Result;
use nb_data::{BlockRegistry, MobRegistry, ParticleRegistry};
use nb_ecs::player::EYE_OFFSET;
use nb_ecs::{
    CameraState, EcsEvent, EcsRngs, EntityId, GameMode, Health, InventoryComp, ItemStack,
    PlayerInput, Simulation, TickCtx, TimeControl, Transform, WorldBlocks,
};
use nb_gfx::SkyClock;
use nb_mesh::BiomeSource;
use nb_world::save::{self, InventorySlot, PlayerState, WorldMetadata};
use nb_world::{BiomeSystem, ChunkStorage, Noise, World, WorldConfig};

use crate::chunks::WorldBiomes;

/// Registres immuables partagés (le registre de mobs est éditable : écran des mobs).
#[derive(Clone, Debug)]
pub struct Registries {
    pub blocks: Arc<BlockRegistry>,
    pub mobs: MobRegistry,
    pub particles: ParticleRegistry,
}

impl Registries {
    /// Données intégrées (`blocks.json`, mobs et particules par défaut).
    pub fn builtin() -> Self {
        Self {
            blocks: Arc::new(BlockRegistry::builtin()),
            mobs: MobRegistry::builtin(),
            particles: ParticleRegistry::builtin(),
        }
    }
}

/// Horloge jour/nuit vue par `/time` (`MasterRenderer.setTime/setDayLengthSeconds`).
pub struct SkyTime<'a>(pub &'a mut SkyClock);

impl TimeControl for SkyTime<'_> {
    fn set_time(&mut self, t: f32) {
        self.0.set_time(t);
    }
    fn set_day_length_seconds(&mut self, s: f32) {
        self.0.set_day_length_seconds(s);
    }
}

/// Un monde jouable.
pub struct WorldSession {
    pub world: World,
    pub sim: Simulation,
    pub rngs: EcsRngs,
    /// `currentWorldName` : `None` pour le monde du menu (jamais sauvegardé).
    pub name: Option<String>,
    pub allow_cheats: bool,
    biomes: Arc<dyn BiomeSource>,
}

impl std::fmt::Debug for WorldSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldSession")
            .field("name", &self.name)
            .field("seed", &self.world.seed())
            .finish_non_exhaustive()
    }
}

impl WorldSession {
    /// `new World(seed, storage, cfg)` + `initPlayer(camera, mode)`.
    pub fn new(
        seed: i64,
        storage: Arc<dyn ChunkStorage>,
        cfg: WorldConfig,
        regs: &Registries,
        camera: CameraState,
        mode: GameMode,
        rngs: EcsRngs,
    ) -> Self {
        let biomes: Arc<dyn BiomeSource> = Arc::new(WorldBiomes(BiomeSystem::new(
            Arc::new(Noise::new(seed)),
            Arc::new(cfg.clone()),
            regs.blocks.ids(),
        )));
        let world = World::new(seed, storage, cfg, regs.blocks.clone());
        let mut sim = Simulation::new();
        sim.init_player(camera, mode, regs.blocks.ids().air);
        Self {
            world,
            sim,
            rngs,
            name: None,
            allow_cheats: false,
            biomes,
        }
    }

    pub fn biomes(&self) -> Arc<dyn BiomeSource> {
        self.biomes.clone()
    }

    pub fn player(&self) -> Option<EntityId> {
        self.sim.player
    }

    /// Caméra partagée (le `Camera` Java est référencé par le `PlayerComponent`).
    pub fn camera(&self) -> CameraState {
        self.sim.camera().unwrap_or_else(|| {
            CameraState::new(
                Vec3::from(nb_mesh::camera::DEFAULT_POSITION),
                nb_ecs::player::default_fov(),
            )
        })
    }

    pub fn set_camera(&mut self, c: CameraState) {
        if let Some(p) = self.sim.player {
            if let Some(pl) = self.sim.entities.get_mut::<nb_ecs::Player>(p) {
                pl.camera = c;
            }
        }
    }

    pub fn update_camera(&mut self, f: impl FnOnce(&mut CameraState)) {
        let mut c = self.camera();
        f(&mut c);
        self.set_camera(c);
    }

    /// `World.update(playerPosition, dt)` : chunks, puis particules / spawn / systèmes ECS.
    pub fn tick(
        &mut self,
        input: &PlayerInput,
        regs: &Registries,
        events: &mut Vec<EcsEvent>,
        dt: f32,
    ) {
        let pos = self.camera().position;
        self.world.update(pos);
        let world = &self.world;
        let mut cfg = world.config_mut();
        let mut blocks = WorldBlocks(world);
        let mut ctx = TickCtx {
            blocks: &mut blocks,
            block_reg: &regs.blocks,
            mobs: &regs.mobs,
            particle_types: &regs.particles,
            config: &mut cfg,
            input,
            rngs: &mut self.rngs,
            events,
        };
        self.sim.tick(&mut ctx, dt);
    }

    /// `CommandProcessor.executeCommand`.
    pub fn execute_command(&mut self, command: &str, sky: &mut SkyClock) -> Option<String> {
        let mut cfg = self.world.config_mut();
        nb_ecs::execute_command(
            command,
            &mut self.sim,
            &mut cfg,
            &mut SkyTime(sky),
            self.allow_cheats,
        )
    }

    /// Inventaire du joueur (copie).
    pub fn inventory(&self) -> Option<nb_ecs::Inventory> {
        let p = self.sim.player?;
        self.sim
            .entities
            .get::<InventoryComp>(p)
            .map(|i| i.0.clone())
    }

    pub fn inventory_mut(&mut self) -> Option<&mut nb_ecs::Inventory> {
        let p = self.sim.player?;
        self.sim
            .entities
            .get_mut::<InventoryComp>(p)
            .map(|i| &mut i.0)
    }

    /// `WorldSaveManager.saveWorldState` (rien pour le monde du menu).
    pub fn save_state(&self, root: &Path, sky: &SkyClock, regs: &Registries) -> Result<()> {
        let Some(name) = &self.name else {
            return Ok(());
        };
        let flushed = self.world.chunk_manager().flush_dirty();
        let mut meta = save::load_metadata(root, &save::sanitize(name)).unwrap_or_else(|_| {
            // Le Java repart d'un `new WorldMetadata()` ; la graine du monde est gardée.
            WorldMetadata::new(
                name.clone(),
                self.world.seed(),
                GameMode::Creative.java_name(),
            )
        });
        meta.last_played_at = save::now_millis();
        {
            let mut wc = self.world.config_mut();
            wc.day_length_seconds = sky.day_length_seconds;
            wc.time_of_day = sky.time_of_day;
            meta.world_config = wc.clone();
        }
        let cam = self.camera();
        let mut player = meta.player.clone().unwrap_or_default();
        if let Some(p) = self.sim.player {
            let e = &self.sim.entities;
            if let Some(t) = e.copy::<Transform>(p) {
                player.x = t.pos.x;
                player.y = t.pos.y;
                player.z = t.pos.z;
            }
            player.yaw = cam.yaw_deg;
            player.pitch = cam.pitch_deg;
            if let Some(h) = e.copy::<Health>(p) {
                player.health = h.health;
            }
            if let Some(pc) = e.copy::<nb_ecs::Player>(p) {
                meta.game_mode = pc.game_mode.java_name().to_owned();
                player.flying = pc.flying;
            }
        }
        meta.player = Some(player);
        meta.inventory_slots = self
            .inventory()
            .map(|inv| {
                inv.slots()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| {
                        let s = s.as_ref()?;
                        (s.count > 0).then(|| InventorySlot {
                            slot: i as i32,
                            block_name: regs.blocks.from_id(s.block).name.clone(),
                            count: s.count,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        save::save_metadata(root, &meta)?;
        flushed
    }

    /// `WorldSaveManager.applyPlayerState`.
    pub fn apply_player_state(&mut self, meta: &WorldMetadata, regs: &Registries) {
        let p = match &meta.player {
            Some(p) => p.clone(),
            None => {
                // Premier lancement : surface de terre ferme, mémorisée comme spawn.
                let loc = self.world.find_spawn_location();
                self.world.config_mut().spawn = Some(loc);
                PlayerState {
                    x: loc[0],
                    y: loc[1],
                    z: loc[2],
                    ..PlayerState::default()
                }
            }
        };
        let Some(id) = self.sim.player else {
            return;
        };
        let e = &mut self.sim.entities;
        if let Some(t) = e.get_mut::<Transform>(id) {
            t.pos = nb_core::glam::DVec3::new(p.x, p.y, p.z);
            t.yaw = p.yaw;
            t.pitch = p.pitch;
        }
        if let Some(h) = e.get_mut::<Health>(id) {
            h.health = p.health;
            h.alive = h.health > 0.0;
        }
        if let Some(pc) = e.get_mut::<nb_ecs::Player>(id) {
            pc.flying = p.flying;
            pc.camera.position =
                Vec3::new(p.x as f32, (p.y + f64::from(EYE_OFFSET)) as f32, p.z as f32);
            pc.camera.pitch_deg = p.pitch;
            pc.camera.yaw_deg = p.yaw;
        }
        if let Some(inv) = e.get_mut::<InventoryComp>(id) {
            inv.0.clear();
            let air = regs.blocks.ids().air;
            for sd in &meta.inventory_slots {
                let b = regs.blocks.get(&sd.block_name).id;
                if b != air {
                    inv.0.set_slot(sd.slot, Some(ItemStack::new(b, sd.count)));
                }
            }
        }
    }

    /// Arrêt des threads de chunks (`getChunkManager().shutdown()`).
    pub fn shutdown(&self) {
        self.world.shutdown();
    }
}
