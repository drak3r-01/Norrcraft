//! Monde chargé : blocs (nb-world) + entités (nb-ecs) + identité de sauvegarde
//! (`GameContext.world/entityWorld/currentWorldName/allowCheats`, `WorldSaveManager`).

use std::path::Path;
use std::sync::Arc;

use nb_core::glam::Vec3;
use nb_core::Result;
use nb_data::{
    BlockPalette, BlockRegistry, ItemRegistry, MobRegistry, PaletteReport, ParticleRegistry,
    RecipeBook, SurvivalData, WorldKind,
};
use nb_ecs::player::EYE_OFFSET;
use nb_ecs::{
    CameraState, EcsEvent, EcsRngs, EntityId, GameMode, Health, InventoryComp, ItemStack,
    PlayerInput, Simulation, TickCtx, TimeControl, Transform, WorldBlocks,
};
use nb_gfx::SkyClock;
use nb_mesh::BiomeSource;
use nb_world::save::{self, InventorySlot, PlayerState, WorldMetadata};
use nb_world::calendar::Calendar;
use nb_world::weather::Environment;
use nb_world::{ChunkStorage, World, WorldConfig};

use crate::chunks::WorldBiomes;

/// Réponse des commandes de calendrier dans un monde sans saisons.
const NO_CALENDAR: &str = "Ce monde n'a pas de calendrier.";

/// Registres immuables partagés (le registre de mobs est éditable : écran des mobs).
///
/// Ils dépendent de l'univers du monde et de sa palette de blocs : [`Registries::for_world`].
#[derive(Clone, Debug)]
pub struct Registries {
    /// Univers auquel appartiennent ces registres.
    pub kind: WorldKind,
    pub blocks: Arc<BlockRegistry>,
    /// Objets : blocs + objets purs (inventaire, craft).
    pub items: Arc<ItemRegistry>,
    /// Recettes de craft.
    pub recipes: Arc<RecipeBook>,
    /// Règles du mode survie : outils, nourriture, armures, dureté des blocs, butins.
    pub survival: Arc<SurvivalData>,
    pub mobs: MobRegistry,
    pub particles: ParticleRegistry,
}

impl Registries {
    /// Données intégrées de vanilla (monde du menu, tests) avec les identifiants d'origine.
    pub fn builtin() -> Self {
        Self::for_world(WorldKind::Vanilla, &BlockPalette::default())
            .expect("registres intégrés invalides")
            .0
    }

    /// Registres de l'univers `kind` pour un monde de palette `palette` (vide = palette par
    /// défaut de l'univers : identifiants des fichiers de données). Le rapport dit ce que la
    /// palette a eu de décalé avec le contenu du jeu (noms inconnus, blocs ajoutés).
    pub fn for_world(kind: WorldKind, palette: &BlockPalette) -> Result<(Self, PaletteReport)> {
        let (blocks, report) = if palette.is_empty() {
            (BlockRegistry::for_kind(kind), PaletteReport::default())
        } else {
            BlockRegistry::from_palette(kind, palette)?
        };
        let blocks = Arc::new(blocks);
        let items = Arc::new(ItemRegistry::for_kind(kind, &blocks));
        let recipes = Arc::new(RecipeBook::for_kind(kind, &items));
        let survival = Arc::new(SurvivalData::for_kind(kind, &blocks, &items));
        // Outils et armures s'usent individuellement : ils ne s'empilent pas.
        nb_ecs::inventory::set_unstackable(
            items
                .iter()
                .filter(|i| survival.max_durability(i.id) > 0)
                .map(|i| i.id),
        );
        Ok((
            Self {
                kind,
                blocks,
                items,
                recipes,
                survival,
                mobs: MobRegistry::builtin_for(kind),
                particles: ParticleRegistry::builtin(),
            },
            report,
        ))
    }

    /// Palette de blocs de ces registres (écrite dans `world.json`, échangée par le LAN).
    pub fn block_palette(&self) -> BlockPalette {
        self.blocks.palette()
    }
}

/// Empreinte, par univers, de la palette par défaut de cette version du jeu : deux joueurs en
/// réseau local doivent avoir les mêmes (poignée de main, voir `nb_net::Welcome`).
pub fn content_fingerprints() -> Vec<(String, u64)> {
    WorldKind::ALL
        .into_iter()
        .map(|k| {
            (
                k.as_str().to_owned(),
                BlockRegistry::for_kind(k).palette().fingerprint(),
            )
        })
        .collect()
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
    /// Météo vécue par le joueur local, réglée par le jeu à chaque tick (univers à météo).
    pub environment: Option<Environment>,
    biomes: Arc<dyn BiomeSource>,
    /// Hardes persistantes du monde (session 5.3), partagées avec `HerdSystem`.
    herds: nb_ecs::HerdHandle,
    /// Traces, pièges et colonies (session 5.8), partagés avec `EcologySystem`.
    ecology: nb_ecs::EcologyHandle,
    /// Lignes de pêche sur glace (session 6.1), partagées avec `IceSystem`.
    ice: nb_ecs::IceHandle,
    /// Claies de séchage de la morue (session 6.3).
    racks: nb_ecs::RackState,
    fields: nb_ecs::FieldState,
    slash: nb_ecs::SlashState,
    hay: nb_ecs::HayState,
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
        let world = World::for_kind(regs.kind, seed, storage, cfg, regs.blocks.clone());
        let biomes: Arc<dyn BiomeSource> = Arc::new(WorldBiomes::new(world.profile().clone()));
        let mut sim = Simulation::new();
        // Survie : faim / froid / pêche juste après l'entrée du joueur, cultures après la santé.
        sim.schedule
            .insert(1, Box::new(nb_ecs::survival::VitalsSystem));
        sim.schedule.push(Box::new(nb_ecs::survival::CropSystem));
        // Bestiaire Norrcraft : moteur d'états juste après les IA d'origine (sans effet sans brain).
        let after_ai = sim
            .schedule
            .names()
            .iter()
            .position(|n| *n == "AggressiveAI")
            .map_or(usize::MAX, |i| i + 1);
        sim.schedule
            .insert(after_ai, Box::new(nb_ecs::BrainSystem::default()));
        // Hardes persistantes : matérialisation et réabsorption (sans effet sans espèce `herd`).
        let herds: nb_ecs::HerdHandle = Arc::default();
        sim.schedule.insert(
            after_ai.saturating_add(1),
            Box::new(nb_ecs::HerdSystem::new(seed, herds.clone())),
        );
        // Élevage (fourrage, production, chien) : sans effet sans animal apprivoisé.
        sim.schedule.insert(
            after_ai.saturating_add(2),
            Box::new(nb_ecs::HusbandrySystem::default()),
        );
        // Écologie (broutage, castors, traces, pièges, carcasses, naturfrid) : avant la santé,
        // qui détruit les morts.
        let ecology: nb_ecs::EcologyHandle = Arc::default();
        sim.schedule.insert(
            after_ai.saturating_add(3),
            Box::new(nb_ecs::EcologySystem::new(ecology.clone())),
        );
        // Glace jouable (craquement, chute, remontée, lignes de pêche) : sans effet hors glace.
        let ice: nb_ecs::IceHandle = Arc::default();
        let racks: nb_ecs::RackState = Arc::default();
        let fields: nb_ecs::FieldState = Arc::default();
        let slash: nb_ecs::SlashState = Arc::default();
        let hay: nb_ecs::HayState = Arc::default();
        sim.schedule.insert(
            after_ai.saturating_add(4),
            Box::new(nb_ecs::IceSystem::new(ice.clone(), regs.items.id_of("isdubbar"))),
        );
        // Neige et glisse, avalanches (6.2) : propres à Norrcraft (blocs de neige, objets).
        if regs.kind == WorldKind::Norrcraft {
            sim.schedule.insert(
                0,
                Box::new(nb_ecs::GlideSystem::new(nb_ecs::GearIds::from_names(|n| {
                    regs.items.id_of(n)
                }))),
            );
            sim.schedule.insert(
                after_ai.saturating_add(6),
                Box::new(nb_ecs::AvalancheSystem::default()),
            );
            // Bateaux (6.3) après la saisie du joueur ; claies de séchage de la morue.
            sim.schedule.insert(after_ai.saturating_add(7), Box::new(nb_ecs::BoatSystem));
            sim.schedule.insert(
                after_ai.saturating_add(8),
                Box::new(nb_ecs::RackSystem::new(racks.clone())),
            );
            // Cultures, brûlis, fenaison et kulning (6.4).
            sim.schedule.insert(
                after_ai.saturating_add(9),
                Box::new(nb_ecs::FarmSystem::new(fields.clone(), slash.clone(), &regs.blocks)),
            );
            sim.schedule
                .insert(after_ai.saturating_add(10), Box::new(nb_ecs::HaySystem::new(hay.clone())));
        }
        sim.init_player(camera, mode, regs.blocks.ids().air);
        Self {
            world,
            sim,
            rngs,
            name: None,
            allow_cheats: false,
            environment: None,
            biomes,
            herds,
            ecology,
            ice,
            racks,
            fields,
            slash,
            hay,
        }
    }

    /// Traces lisibles, pièges posés et colonies de castors du monde.
    /// Lignes de pêche posées sur la glace.
    /// Claies de séchage (hjell) posées, par cellule.
    pub fn racks(&self) -> nb_ecs::RackState {
        self.racks.clone()
    }

    /// Champs cultivés (6.4).
    pub fn fields(&self) -> nb_ecs::FieldState {
        self.fields.clone()
    }

    /// Abattis qui sèchent avant le brûlis (6.4).
    pub fn slash(&self) -> nb_ecs::SlashState {
        self.slash.clone()
    }

    /// Hässjas et granges (6.4).
    pub fn hay(&self) -> nb_ecs::HayState {
        self.hay.clone()
    }

    pub fn ice(&self) -> nb_ecs::IceHandle {
        self.ice.clone()
    }

    pub fn ecology(&self) -> nb_ecs::EcologyHandle {
        self.ecology.clone()
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
            config: &mut cfg,
            input,
            rngs: &mut self.rngs,
            events,
            survival: Some(&regs.survival),
            environment: self.environment.as_ref(),
        };
        let first = ctx.events.len();
        self.sim.tick(&mut ctx, dt);
        // Une digue de castors cassée vide l'étang en amont (doc 03 §4.5, session 5.8).
        let broken: Vec<_> = ctx.events[first.min(ctx.events.len())..]
            .iter()
            .filter_map(|e| match e {
                EcsEvent::BlockBroken { pos, block } => Some((*pos, *block)),
                _ => None,
            })
            .collect();
        for (pos, block) in broken {
            nb_ecs::ecology::dam_broken(ctx.blocks, &regs.blocks, &regs.mobs, pos, block);
        }
        // Eau vivante (doc 07) : l'hôte simule seul, juste après les systèmes ECS.
        if world.has_living_water() {
            let env = self.environment.as_ref();
            let fluid_env = nb_world::fluid::FluidEnv {
                season_step: world.chunk_manager().season(),
                rain: env.map_or(0.0, |e| e.rain),
                air_temp: env.map_or(10.0, |e| e.air),
                daylight: env.is_none_or(|e| e.sun > 0.0),
                centers: vec![pos],
            };
            world.tick_fluids(&fluid_env);
        }
    }

    /// `/water pour [n] | spring | drain | wake | info` (session 6E.1, triches) : mise au point
    /// de l'eau vivante sur la cellule visée. `None` : pas pour nous.
    pub fn water_command(&mut self, command: &str, regs: &Registries) -> Option<String> {
        use nb_core::glam::IVec3;
        use nb_world::fluid::FluidWorld;
        let mut words = command.split_whitespace();
        if words.next()? != "/water" {
            return None;
        }
        if !self.world.has_living_water() {
            return Some("Pas d'eau vivante dans cet univers.".to_owned());
        }
        if !self.allow_cheats {
            return Some("Cheats are not enabled in this world.".to_owned());
        }
        // Cellule visée : on avance le long du regard jusqu'au premier bloc étanche ou à l'eau.
        let cam = self.camera();
        let dir = cam.look_dir();
        let ids = regs.blocks.ids();
        let cell = |p: Vec3| IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let mut prev = cell(cam.position);
        let mut hit = prev;
        let mut d = 0.0f32;
        while d < 16.0 {
            let c = cell(cam.position + dir * d);
            let b = self.world.block(c.x, c.y, c.z);
            if b != ids.air {
                hit = c;
                break;
            }
            prev = c;
            hit = c;
            d += 0.05;
        }
        let sub = words.next().unwrap_or("info");
        let n: Option<u8> = words.next().and_then(|w| w.parse().ok());
        let reply = self.world.with_fluid(|sim, w| match sub {
            "pour" => {
                let target = if sim.open(w, hit) { hit } else { prev };
                let poured = sim.pour(w, target, n.unwrap_or(8).min(8));
                format!("{poured} huitième(s) versé(s) en {target}.")
            }
            "spring" => {
                let at = (if sim.open(w, hit) { hit } else { prev }) - IVec3::Y;
                if sim.place_spring(w, at) {
                    format!("Source posée en {at}.")
                } else {
                    "Impossible de poser une source ici.".to_owned()
                }
            }
            "drain" => format!("{} cellule(s) vidée(s).", sim.drain_radius(w, hit, 8)),
            "wake" => format!("{} cellule(s) réveillée(s).", sim.wake_radius(w, hit, 16)),
            _ => {
                let target = if sim.open(w, hit) { hit } else { prev };
                let code = w.cell(target).map(|c| c.1);
                let flow = sim.current_at(w, target);
                let s = sim.stats;
                format!(
                    "{target} : niveau {} infinie {} code {:?} courant ({:.1} {:.1} {:.1}) — \
                     éveillées {} traitées {} {} µs, {} pas",
                    sim.lvl(w, target),
                    sim.inf(w, target),
                    code.map(|c| c.0),
                    flow.x,
                    flow.y,
                    flow.z,
                    s.awake,
                    s.processed,
                    s.last_tick_us,
                    s.steps
                )
            }
        });
        reply
    }

    /// `CommandProcessor.executeCommand`.
    pub fn execute_command(&mut self, command: &str, sky: &mut SkyClock) -> Option<String> {
        if let Some(reply) = self.calendar_command(command, sky) {
            return Some(reply);
        }
        let mut cfg = self.world.config_mut();
        nb_ecs::execute_command(
            command,
            &mut self.sim,
            &mut cfg,
            &mut SkyTime(sky),
            self.allow_cheats,
        )
    }

    /// `/boat [raft|eka|snipa|knarr|longship] [bare]` (session 6.3, triches) : met un bateau à
    /// l'eau près du joueur et l'y embarque. `None` : pas pour nous.
    pub fn boat_command(&mut self, command: &str, regs: &Registries) -> Option<String> {
        let mut words = command.split_whitespace();
        if words.next()? != "/boat" {
            return None;
        }
        if regs.kind != WorldKind::Norrcraft {
            return Some("Pas de bateaux dans cet univers.".to_owned());
        }
        if !self.allow_cheats {
            return Some("Cheats are not enabled in this world.".to_owned());
        }
        let kind = words.next().and_then(nb_ecs::BoatKind::from_name);
        let Some(kind) = kind else {
            return Some("Usage : /boat <raft|eka|snipa|knarr|longship> [bare]".to_owned());
        };
        let tarred = words.next() != Some("bare");
        let player = self.sim.player?;
        let at = self.sim.entities.get::<Transform>(player)?.pos;
        let blocks = WorldBlocks(&self.world);
        let water = regs.blocks.ids().water;
        let near = at.y.floor() as i32;
        let mut spot = None;
        'ring: for r in 2..=8i32 {
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx.abs().max(dz.abs()) != r {
                        continue;
                    }
                    let (x, z) = (at.x.floor() as i32 + dx, at.z.floor() as i32 + dz);
                    if let Some(top) = nb_ecs::boat::water_top(&blocks, water, x, near, z) {
                        spot = Some(nb_core::glam::DVec3::new(f64::from(x) + 0.5, f64::from(top), f64::from(z) + 0.5));
                        break 'ring;
                    }
                }
            }
        }
        let Some(spot) = spot else {
            return Some("Aucune eau à moins de 8 blocs.".to_owned());
        };
        let yaw = self.sim.entities.get::<nb_ecs::Player>(player).map_or(0.0, |p| p.camera.yaw_deg);
        let id = nb_ecs::boat::spawn_boat(&mut self.sim.entities, kind, spot, yaw, tarred);
        nb_ecs::boat::board(&mut self.sim.entities, player, id);
        Some(format!("{} à l'eau{}.", kind.label(), if tarred { "" } else { " (coque nue)" }))
    }

    /// Calendrier du monde, `None` quand l'univers n'a pas de saisons (vanilla).
    pub fn calendar(&self) -> Option<Calendar> {
        self.world
            .profile()
            .systems
            .seasons
            .then(|| Calendar::new(self.world.config().days_per_month))
    }

    /// Texte de l'Almanach (une ligne par information) pour `/almanach`.
    pub fn almanac_text(&self, sky: &SkyClock) -> String {
        match self.calendar() {
            Some(c) => c
                .almanac(sky.day, sky.time_of_day, sky.latitude)
                .lines()
                .join("
"),
            None => NO_CALENDAR.to_owned(),
        }
    }

    /// `/almanach` et `/date [set <n> | add <n>]` (session 3.1). `None` : pas pour nous.
    ///
    /// `set` et `add` changent le compteur de jours et demandent les triches, comme `/time`.
    pub fn calendar_command(&mut self, command: &str, sky: &mut SkyClock) -> Option<String> {
        let mut words = command.split_whitespace();
        match words.next()? {
            "/almanach" => Some(self.almanac_text(sky)),
            "/date" => {
                let Some(cal) = self.calendar() else {
                    return Some(NO_CALENDAR.to_owned());
                };
                let verb = words.next();
                let value = words.next().and_then(|v| v.parse::<i64>().ok());
                let date = |day: u64| {
                    let d = cal.date(day);
                    format!(
                        "Date : {} (année {}), jour {}/{}, {}",
                        d.label(),
                        d.year,
                        d.day_of_year + 1,
                        cal.days_per_year(),
                        d.season.name()
                    )
                };
                Some(match (verb, value) {
                    (None, _) => date(sky.day),
                    (Some("set" | "add"), Some(_)) if !self.allow_cheats => {
                        "Cheats are not enabled in this world.".to_owned()
                    }
                    (Some("set"), Some(n)) => {
                        let year = cal.date(sky.day).year;
                        let idx = (n.clamp(1, i64::from(cal.days_per_year())) - 1) as u32;
                        sky.day = cal.day_for(year, idx);
                        date(sky.day)
                    }
                    (Some("add"), Some(n)) => {
                        sky.day = sky.day.saturating_add_signed(n);
                        date(sky.day)
                    }
                    _ => "Usage : /date [set <jour de l'année, 1 = solstice d'hiver> | add <jours>]"
                        .to_owned(),
                })
            }
            _ => None,
        }
    }

    /// Place le joueur local (pieds) : arrivée dans une partie LAN, téléportation.
    pub fn place_player(&mut self, pos: [f64; 3]) {
        let Some(id) = self.sim.player else { return };
        let e = &mut self.sim.entities;
        if let Some(t) = e.get_mut::<Transform>(id) {
            t.pos = nb_core::glam::DVec3::new(pos[0], pos[1], pos[2]);
        }
        if let Some(p) = e.get_mut::<nb_ecs::Player>(id) {
            p.camera.position = Vec3::new(
                pos[0] as f32,
                (pos[1] + f64::from(EYE_OFFSET)) as f32,
                pos[2] as f32,
            );
        }
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
        // Univers, palette de blocs et table de biomes : figés avec le monde (world.json v2).
        meta.world_kind = regs.kind;
        meta.block_palette = regs.block_palette();
        meta.biome_palette = self
            .world
            .profile()
            .biomes
            .iter()
            .map(|b| b.name.clone())
            .collect();
        {
            let mut wc = self.world.config_mut();
            wc.day_length_seconds = sky.day_length_seconds;
            wc.time_of_day = sky.time_of_day;
            wc.calendar_day = sky.day;
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
                            block_name: regs.items.name_of(s.item).to_owned(),
                            count: s.count,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        meta.survival = self.survival_save(regs);
        meta.herds = self
            .herds
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .records();
        save::save_metadata(root, &meta)?;
        flushed
    }

    /// Faim, chaleur, endurance, usure des outils et armure portée.
    fn survival_save(&self, regs: &Registries) -> Option<save::SurvivalSave> {
        let p = self.sim.player?;
        let v = self.sim.entities.copy::<nb_ecs::Vitals>(p)?;
        let inv = self.inventory()?;
        let wear = inv
            .slots()
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.filter(|s| s.wear > 0).map(|s| (i as i32, s.wear)))
            .collect();
        let armor = inv
            .armor_slots()
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                s.map(|s| (i as u8, regs.items.name_of(s.item).to_owned(), s.wear))
            })
            .collect();
        Some(save::SurvivalSave {
            hunger: v.hunger,
            saturation: v.saturation,
            warmth: v.warmth,
            stamina: v.stamina,
            wear,
            armor,
        })
    }

    /// `WorldSaveManager.applyPlayerState`.
    pub fn apply_player_state(&mut self, meta: &WorldMetadata, regs: &Registries) {
        *self.herds.lock().unwrap_or_else(|e| e.into_inner()) =
            nb_world::herds::HerdBook::from_records(meta.herds.clone());
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
            for sd in &meta.inventory_slots {
                // Objet inconnu (pack ou version différente) : case ignorée.
                if let Some(item) = regs.items.id_of(&sd.block_name) {
                    let wear = meta
                        .survival
                        .as_ref()
                        .and_then(|s| s.wear.iter().find(|(i, _)| *i == sd.slot))
                        .map_or(0, |(_, w)| *w);
                    inv.0.set_slot(
                        sd.slot,
                        Some(ItemStack::new(item, sd.count).with_wear(wear)),
                    );
                }
            }
            if let Some(sv) = &meta.survival {
                for (k, name, wear) in &sv.armor {
                    let (Some(item), Some(slot)) = (
                        regs.items.id_of(name),
                        nb_data::ArmorSlot::ALL.get(usize::from(*k)).copied(),
                    ) else {
                        continue;
                    };
                    inv.0
                        .set_armor(slot, Some(ItemStack::new(item, 1).with_wear(*wear)));
                }
            }
        }
        if let Some(sv) = &meta.survival {
            if let Some(v) = e.get_mut::<nb_ecs::Vitals>(id) {
                v.hunger = sv.hunger.clamp(0.0, 20.0);
                v.saturation = sv.saturation.clamp(0.0, 20.0);
                v.warmth = sv.warmth.clamp(0.0, 100.0);
                v.stamina = sv.stamina.clamp(0.0, 100.0);
            }
        }
    }

    /// Arrêt des threads de chunks (`getChunkManager().shutdown()`).
    pub fn shutdown(&self) {
        self.world.shutdown();
    }
}
