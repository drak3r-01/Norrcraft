//! Banc d'essai commun : monde de blocs en mémoire, registres intégrés, simulation complète.
#![allow(dead_code)]

use nb_core::glam::{DVec3, IVec3, Vec3};
use nb_data::{BlockId, BlockIds, BlockRegistry, MobRegistry};
use nb_ecs::{
    execute_command, CameraState, EcsEvent, EcsRngs, EntityId, GameAction, GameMode, Health,
    InventoryComp, MemoryBlocks, Player, PlayerInput, Simulation, TickCtx, TimeControl, Transform,
    Velocity, TICK_DT,
};
use nb_world::WorldConfig;

/// Heure / durée du jour enregistrées par les commandes.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FakeTime {
    pub time: Option<f32>,
    pub day_length: Option<f32>,
}

impl TimeControl for FakeTime {
    fn set_time(&mut self, t: f32) {
        self.time = Some(t);
    }
    fn set_day_length_seconds(&mut self, s: f32) {
        self.day_length = Some(s);
    }
}

pub struct Harness {
    pub reg: BlockRegistry,
    pub mobs: MobRegistry,
    pub cfg: WorldConfig,
    pub blocks: MemoryBlocks,
    pub rngs: EcsRngs,
    pub events: Vec<EcsEvent>,
    pub input: PlayerInput,
    pub sim: Simulation,
    pub time: FakeTime,
    pub ticks: u64,
}

impl Harness {
    /// Monde vide, **aucun spawn de mob** (tous les `maxCount` à 0) ; graines fixes.
    pub fn new() -> Self {
        Self::with_seed(1234)
    }

    pub fn with_seed(seed: i64) -> Self {
        let reg = BlockRegistry::builtin();
        let mut mobs = MobRegistry::builtin();
        for c in mobs.all_mut() {
            c.max_count = 0;
        }
        let air = reg.ids().air;
        Self {
            reg,
            mobs,
            cfg: WorldConfig::default(),
            blocks: MemoryBlocks::new(air),
            rngs: EcsRngs::from_seed(seed),
            events: Vec::new(),
            input: PlayerInput::default(),
            sim: Simulation::new(),
            time: FakeTime::default(),
            ticks: 0,
        }
    }

    pub fn ids(&self) -> BlockIds {
        *self.reg.ids()
    }

    /// Remet le `maxCount` d'origine d'un mob intégré (réactive son spawn).
    pub fn enable_spawn(&mut self, id: &str) {
        let builtin = MobRegistry::builtin().get(id).unwrap().max_count;
        for c in self.mobs.all_mut() {
            if c.id == id {
                c.max_count = builtin;
            }
        }
    }

    /// Dalle horizontale `[x0, x1] × [z0, z1]` à la hauteur `y`.
    pub fn slab(&mut self, x0: i32, x1: i32, y: i32, z0: i32, z1: i32, b: BlockId) {
        self.blocks
            .fill(IVec3::new(x0, y, z0), IVec3::new(x1, y, z1), b);
    }

    /// Sol de pierre en `y = 64` sur ±32 (les pieds reposent en `y = 65`).
    pub fn stone_floor(&mut self) {
        let stone = self.ids().stone;
        self.slab(-32, 32, 64, -32, 32, stone);
    }

    pub fn put(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        self.blocks.put(x, y, z, b);
    }

    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId {
        use nb_ecs::BlockAccess;
        self.blocks.block(x, y, z)
    }

    /// Joueur dont les pieds sont en `feet`.
    pub fn spawn_player(&mut self, feet: DVec3, mode: GameMode) -> EntityId {
        let cam = CameraState::new(
            Vec3::new(feet.x as f32, feet.y as f32 + 1.62, feet.z as f32),
            70f32.to_radians(),
        );
        let air = self.ids().air;
        let id = self.sim.init_player(cam, mode, air);
        // Pieds exacts (l'aller-retour f32 de la caméra arrondirait).
        self.sim.entities.get_mut::<Transform>(id).unwrap().pos = feet;
        id
    }

    pub fn player(&self) -> EntityId {
        self.sim.player.expect("joueur")
    }

    pub fn tick(&mut self) {
        let mut ctx = TickCtx {
            blocks: &mut self.blocks,
            block_reg: &self.reg,
            mobs: &self.mobs,
            config: &mut self.cfg,
            input: &self.input,
            rngs: &mut self.rngs,
            events: &mut self.events,
        };
        self.sim.tick(&mut ctx, TICK_DT);
        self.ticks += 1;
        // Les fronts montants ne durent qu'un tick.
        self.input.just_pressed = Default::default();
        self.input.mouse_delta = Default::default();
    }

    pub fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick();
        }
    }

    /// Tick avec une action « vient d'être pressée » (et maintenue pendant ce tick).
    pub fn press(&mut self, a: GameAction) {
        let held = self.input.pressed.contains(a);
        self.input.pressed.insert(a);
        self.input.just_pressed.insert(a);
        self.tick();
        if !held {
            self.input.pressed.remove(a);
        }
    }

    pub fn hold(&mut self, a: GameAction) {
        self.input.pressed.insert(a);
    }

    pub fn release(&mut self, a: GameAction) {
        self.input.pressed.remove(a);
    }

    pub fn cmd(&mut self, s: &str) -> Option<String> {
        execute_command(s, &mut self.sim, &mut self.cfg, &mut self.time, true)
    }

    pub fn t(&self, id: EntityId) -> Transform {
        self.sim.entities.copy::<Transform>(id).unwrap()
    }

    pub fn v(&self, id: EntityId) -> Velocity {
        self.sim.entities.copy::<Velocity>(id).unwrap()
    }

    pub fn h(&self, id: EntityId) -> Health {
        self.sim.entities.copy::<Health>(id).unwrap()
    }

    pub fn p(&self) -> Player {
        self.sim.entities.copy::<Player>(self.player()).unwrap()
    }

    pub fn p_mut(&mut self) -> &mut Player {
        let id = self.player();
        self.sim.entities.get_mut::<Player>(id).unwrap()
    }

    pub fn inv_mut(&mut self) -> &mut nb_ecs::Inventory {
        let id = self.player();
        &mut self.sim.entities.get_mut::<InventoryComp>(id).unwrap().0
    }

    pub fn inv(&self) -> nb_ecs::Inventory {
        let id = self.player();
        self.sim
            .entities
            .get::<InventoryComp>(id)
            .unwrap()
            .0
            .clone()
    }

    /// Fait apparaître un mob du registre en `(x, y, z)` (pieds).
    pub fn spawn_mob(&mut self, mob: &str, x: f64, y: f64, z: f64) -> EntityId {
        let cfg = self.mobs.get(mob).unwrap().clone();
        nb_ecs::spawn::spawn_mob(&mut self.sim.entities, &cfg, x, y, z, &mut self.rngs.spawn)
    }

    pub fn count_mobs(&self, mob: &str) -> usize {
        self.sim
            .entities
            .query_ids::<&nb_ecs::MobType>()
            .into_iter()
            .filter(|&id| self.sim.entities.get::<nb_ecs::MobType>(id).unwrap().mob_id == mob)
            .count()
    }

    pub fn take_events(&mut self) -> Vec<EcsEvent> {
        std::mem::take(&mut self.events)
    }
}
