//! Le jeu (`Main` + `GameContext` + `StateManager` + `*State.java`, logique seulement).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nb_core::glam::Vec3;
use nb_core::JavaRandom;
use nb_data::GameSettings;
use nb_data::Station;
use nb_ecs::{
    CameraState, CraftingGrid, EcsEvent, EcsRngs, GameMode, InventoryCursor, MobType, TICK_DT,
};
use nb_gfx::{GfxSettings, SkyClock};
use nb_mesh::{BlockTable, MeshAction, MeshScheduler};
use nb_world::save::{self, WorldMetadata};
use nb_world::{FileChunkStorage, InMemoryChunkStorage, World, WorldConfig};

use crate::chat::Chat;
use crate::chunks::{sorted_active_positions, WorldChunks};
use crate::input::{Action, InputManager};
use crate::net::{LanBrowser, LanView, NetMode};
use crate::render::{build_view, ParticleLayers, RenderView};
use crate::session::{Registries, WorldSession};
use crate::state::{GameState, HudState, SettingsTab};

mod lan;
use crate::ui::{ItemIcons, NewWorld, NullUi, Ui, UiCommand, UiFrame, UiView};

/// Demandes faites à la plateforme (fenêtre, GPU) ; la logique reste sans effet de bord.
#[derive(Clone, Debug, PartialEq)]
pub enum PlatformRequest {
    /// Curseur capturé et caché (`GLFW_CURSOR_DISABLED`) ou normal.
    SetCursorGrab(bool),
    /// Vsync, ombres, bloom, FOV (`masterRenderer.set*`, `window.setVSync`).
    ApplyGraphics(GfxSettings),
    SetFullscreen(bool),
    /// `window.setSize` (ignoré en plein écran par la plateforme).
    SetWindowSize(u32, u32),
    /// `assetManager.reloadBlockTextures(nom)` + icônes ; la plateforme rappelle ensuite
    /// [`Game::set_block_table`] (le maillage est suspendu jusque-là).
    ReloadResourcePack(String),
    /// Sortie de l'écran des mobs (`applyAndLeave`) : `reloadMobModel` de chaque mob du
    /// registre dont le modèle n'est pas chargé.
    ReloadMobModels,
    /// `reloadMobModel(id)` : mob ajouté dans l'écran des mobs.
    ReloadMobModel(String),
    /// Fermeture de la fenêtre (`glfwSetWindowShouldClose`).
    Quit,
}

/// Étapes d'un tick, dans l'ordre de `Main.update` (traçables pour les tests).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Input,
    Time,
    Chat,
    Resize,
    State(GameState),
    /// `World.update(pos, dt)` (PLAYING seulement).
    EcsTick,
    Chunks,
    Meshes,
}

/// Options de construction.
#[derive(Clone, Debug)]
pub struct GameOptions {
    /// Répertoire des sauvegardes (`saves/` : mondes, `settings.json`, `mobs.json`).
    pub saves_root: PathBuf,
    /// Racine des packs de ressources (`None` = pas de liste, « Default » seul).
    pub packs_root: Option<PathBuf>,
    /// Graine du monde du menu (`None` = `new Random().nextInt(1e9)`).
    pub menu_seed: Option<i64>,
    /// Graine des RNG de l'ECS et du jeu (`None` = entropie).
    pub rng_seed: Option<i64>,
    /// Lire / écrire `settings.json` et `mobs.json`.
    pub persist_settings: bool,
    /// Taille initiale du framebuffer.
    pub window_size: (u32, u32),
    /// Threads de maillage (`None` = `max(1, cœurs/2)`).
    pub mesh_threads: Option<usize>,
    /// Port TCP d'une partie LAN hébergée (`0` = un port libre).
    pub lan_port: u16,
    /// Port UDP de la découverte des parties (`None` = port par défaut ; tests).
    pub lan_discovery_port: Option<u16>,
}

impl GameOptions {
    pub fn new(saves_root: impl Into<PathBuf>) -> Self {
        Self {
            saves_root: saves_root.into(),
            packs_root: None,
            menu_seed: None,
            rng_seed: None,
            persist_settings: true,
            window_size: (1280, 720),
            mesh_threads: None,
            lan_port: nb_net::DEFAULT_GAME_PORT,
            lan_discovery_port: None,
        }
    }
}

/// `Random.nextLong() & Long.MAX_VALUE`, entier, sinon `String.hashCode()`
/// (`WorldSelectState.parseSeed`, sur le texte rogné).
pub fn parse_seed(text: &str, rng: &mut JavaRandom) -> i64 {
    let text = nb_ecs::javafmt::java_trim(text);
    if text.is_empty() {
        return rng.next_long() & i64::MAX;
    }
    text.parse::<i64>()
        .unwrap_or_else(|_| i64::from(nb_world::java_string_hash(text)))
}

/// `isValidNewName(name, currentName)`.
pub fn is_valid_new_name(root: &Path, name: &str, current: Option<&str>) -> bool {
    if name.is_empty() {
        return false;
    }
    if !save::world_name_exists(root, name) {
        return true;
    }
    current == Some(name)
}

fn menu_camera(fov: f32) -> CameraState {
    CameraState::new(Vec3::from(nb_mesh::camera::DEFAULT_POSITION), fov)
}

fn fov_rad(deg: f32) -> f32 {
    (f64::from(deg)).to_radians() as f32
}

/// Le jeu complet, sans fenêtre ni GPU.
pub struct Game {
    pub settings: GameSettings,
    pub regs: Registries,
    pub input: InputManager,
    opts: GameOptions,
    chat: Chat,
    sky: SkyClock,
    session: WorldSession,
    state: GameState,
    previous_state: GameState,
    pending_settings_tab: Option<SettingsTab>,
    settings_tab: SettingsTab,
    menu_rotation: f32,
    hud: HudState,
    cursor: InventoryCursor,
    /// Station de craft de l'écran d'inventaire et sa grille (vidée à la fermeture).
    craft_station: Station,
    craft_grid: CraftingGrid,
    item_icons: Arc<ItemIcons>,
    mouse_locked: bool,
    mobs_dirty: bool,
    meshes: MeshScheduler,
    block_table: Arc<BlockTable>,
    awaiting_block_table: bool,
    mesh_actions: Vec<MeshAction>,
    requests: Vec<PlatformRequest>,
    events: Vec<EcsEvent>,
    worlds: Vec<WorldMetadata>,
    packs: Vec<nb_gfx::ResourcePackMeta>,
    window_size: (u32, u32),
    resized: bool,
    aspect: f32,
    rng: JavaRandom,
    ui: Box<dyn Ui>,
    quit: bool,
    ticks: u64,
    trace: Option<Vec<Step>>,
    last_error: Option<String>,
    particle_layers: ParticleLayers,
    /// Partie en réseau local (hôte, client ou rien).
    net: NetMode,
    lan_browser: LanBrowser,
    lan_view: LanView,
    /// Dernier message de l'écran Multiplayer (texte, erreur ?).
    lan_message: Option<(String, bool)>,
}

impl std::fmt::Debug for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Game")
            .field("state", &self.state)
            .field("session", &self.session)
            .field("ticks", &self.ticks)
            .finish_non_exhaustive()
    }
}

impl Game {
    /// `Main.initEngine` (sans fenêtre) : registres, réglages, monde du menu, MAIN_MENU.
    /// `block_table` : table de maillage (layers de l'atlas du renderer).
    pub fn new(opts: GameOptions, regs: Registries, block_table: Arc<BlockTable>) -> Self {
        let settings = if opts.persist_settings {
            load_settings(&opts.saves_root)
        } else {
            GameSettings::default()
        };
        let mut regs = regs;
        if opts.persist_settings {
            if let Ok(json) = std::fs::read_to_string(opts.saves_root.join("mobs.json")) {
                regs.mobs = nb_data::MobRegistry::load(Some(&json));
            }
        }
        let mut rng = match opts.rng_seed {
            Some(s) => JavaRandom::new(s),
            None => JavaRandom::new(entropy_seed()),
        };
        let menu_seed = opts
            .menu_seed
            .unwrap_or_else(|| i64::from(rng.next_int_bound(1_000_000_000)));
        let rngs = ecs_rngs(&opts, &mut rng);
        let camera = menu_camera(fov_rad(70.0));
        let session = WorldSession::new(
            menu_seed,
            Arc::new(InMemoryChunkStorage::new()),
            WorldConfig::default(),
            &regs,
            camera,
            GameMode::Creative,
            rngs,
        );
        let threads = opts.mesh_threads.unwrap_or_else(nb_mesh::default_threads);
        let (w, h) = opts.window_size;
        let mut g = Self {
            settings,
            regs,
            input: InputManager::new(),
            chat: Chat::new(),
            sky: SkyClock::new(),
            session,
            state: GameState::MainMenu,
            previous_state: GameState::MainMenu,
            pending_settings_tab: None,
            settings_tab: SettingsTab::Graphics,
            menu_rotation: 0.0,
            hud: HudState::default(),
            cursor: InventoryCursor::default(),
            craft_station: Station::Hand,
            craft_grid: CraftingGrid::new(Station::Hand),
            item_icons: Arc::new(ItemIcons::default()),
            mouse_locked: true,
            mobs_dirty: false,
            meshes: MeshScheduler::new(threads),
            block_table,
            awaiting_block_table: false,
            mesh_actions: Vec::new(),
            requests: Vec::new(),
            events: Vec::new(),
            worlds: Vec::new(),
            packs: Vec::new(),
            window_size: (w, h),
            resized: false,
            aspect: aspect(w, h),
            rng,
            ui: Box::new(NullUi),
            quit: false,
            ticks: 0,
            trace: None,
            last_error: None,
            particle_layers: ParticleLayers::builtin(),
            net: NetMode::Off,
            lan_browser: LanBrowser::for_options(&opts),
            lan_view: LanView::default(),
            lan_message: None,
            opts,
        };
        g.apply_initial_settings();
        g.enter_state(GameState::MainMenu);
        let pos = g.session.camera().position;
        g.session.world.update(pos);
        g.update_meshes();
        g
    }

    /// Réglages persistés appliqués aux systèmes vivants (`Main.initEngine`).
    fn apply_initial_settings(&mut self) {
        let s = self.settings.clone();
        self.session.update_camera(|c| c.fov = fov_rad(s.fov));
        self.session
            .world
            .chunk_manager()
            .set_render_distance(s.render_distance);
        self.requests
            .push(PlatformRequest::ApplyGraphics(GfxSettings::from_game(&s)));
        self.requests
            .push(PlatformRequest::SetFullscreen(s.fullscreen));
        self.input.apply_key_bindings(&s.key_bindings);
        if s.window_width != 1280 || s.window_height != 720 {
            self.requests.push(PlatformRequest::SetWindowSize(
                s.window_width.max(1) as u32,
                s.window_height.max(1) as u32,
            ));
        }
        if s.resource_pack != nb_gfx::packs::DEFAULT_PACK {
            self.request_pack_reload(s.resource_pack);
        }
    }

    /// Remplace l'interface (nb-ui) ; l'écran courant reçoit `enter`.
    pub fn set_ui(&mut self, ui: Box<dyn Ui>) {
        let mut old = std::mem::replace(&mut self.ui, ui);
        old.exit(self.state, &mut self.input);
        self.with_ui(|ui, view, input| ui.enter(view.state, view, input));
    }

    /// Active l'enregistrement des étapes de chaque tick (tests d'ordre).
    pub fn enable_trace(&mut self) {
        self.trace = Some(Vec::new());
    }

    pub fn take_trace(&mut self) -> Vec<Step> {
        self.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    fn step(&mut self, s: Step) {
        if let Some(t) = &mut self.trace {
            t.push(s);
        }
    }

    // ── Accès ───────────────────────────────────────────────────────────────────────────

    pub fn state(&self) -> GameState {
        self.state
    }

    pub fn previous_state(&self) -> GameState {
        self.previous_state
    }

    pub fn session(&self) -> &WorldSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut WorldSession {
        &mut self.session
    }

    pub fn world(&self) -> &World {
        &self.session.world
    }

    pub fn sky(&self) -> &SkyClock {
        &self.sky
    }

    pub fn sky_mut(&mut self) -> &mut SkyClock {
        &mut self.sky
    }

    pub fn chat(&self) -> &Chat {
        &self.chat
    }

    pub fn hud(&self) -> &HudState {
        &self.hud
    }

    pub fn camera(&self) -> CameraState {
        self.session.camera()
    }

    pub fn menu_rotation(&self) -> f32 {
        self.menu_rotation
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn inventory_cursor(&self) -> &InventoryCursor {
        &self.cursor
    }

    /// Grille de craft de l'écran d'inventaire courant.
    pub fn craft_grid(&self) -> &CraftingGrid {
        &self.craft_grid
    }

    /// Station de l'écran d'inventaire (main, établi, fonderie).
    pub fn craft_station(&self) -> Station {
        self.craft_station
    }

    /// Ouvre l'écran d'une station de craft (clic droit sur un établi / une fonderie).
    pub fn open_station(&mut self, station: Station) {
        if self.state == GameState::Playing {
            self.craft_station = station;
            self.switch_state(GameState::Inventory);
        }
    }

    /// Couches d'atlas des icônes d'objets purs (à fournir avec la table de blocs).
    pub fn set_item_icons(&mut self, icons: Arc<ItemIcons>) {
        self.item_icons = icons;
    }

    pub fn worlds(&self) -> &[WorldMetadata] {
        &self.worlds
    }

    pub fn saves_root(&self) -> &Path {
        &self.opts.saves_root
    }

    pub fn mesh_scheduler(&self) -> &MeshScheduler {
        &self.meshes
    }

    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    /// Dernière erreur d'E/S (sauvegarde, création de monde…), effacée à la lecture.
    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    /// Actions de maillage à appliquer au renderer (upload / libération), dans l'ordre.
    pub fn take_mesh_actions(&mut self) -> Vec<MeshAction> {
        std::mem::take(&mut self.mesh_actions)
    }

    pub fn take_requests(&mut self) -> Vec<PlatformRequest> {
        std::mem::take(&mut self.requests)
    }

    /// Événements ECS des ticks écoulés (sons, statistiques…).
    pub fn take_events(&mut self) -> Vec<EcsEvent> {
        std::mem::take(&mut self.events)
    }

    /// Nouvelle table de maillage (après rechargement du pack) : reprise du maillage.
    pub fn set_block_table(&mut self, table: Arc<BlockTable>) {
        self.block_table = table;
        self.awaiting_block_table = false;
    }

    /// Table de maillage courante (couches de texture : icônes de blocs de l'interface).
    pub fn block_table(&self) -> &Arc<BlockTable> {
        &self.block_table
    }

    /// Couches de l'atlas des particules du renderer (par défaut : atlas intégré).
    pub fn set_particle_layers(&mut self, atlas: &nb_gfx::TextureArrayCpu) {
        self.particle_layers = ParticleLayers::from_atlas(atlas);
    }

    /// Taille du framebuffer (callback de redimensionnement) ; traitée au prochain tick.
    pub fn resize(&mut self, w: u32, h: u32) {
        if (w, h) != self.window_size {
            self.window_size = (w, h);
            self.resized = true;
        }
    }

    pub fn window_size(&self) -> (u32, u32) {
        self.window_size
    }

    /// Données de rendu de la frame.
    pub fn render_view(&self) -> RenderView<'_> {
        build_view(
            self.state,
            &self.session.world,
            &self.session.sim,
            &self.regs.blocks,
            &self.regs.mobs,
            &self.particle_layers,
            &self.sky,
            self.aspect,
            self.avatars(),
        )
    }

    /// Vue de l'interface (même contenu que celle passée à [`Ui::update`]).
    pub fn ui_view(&self) -> UiView<'_> {
        let s = &self.session;
        let p = s.sim.player;
        let e = &s.sim.entities;
        let player = p.and_then(|p| e.copy::<nb_ecs::Player>(p));
        UiView {
            state: self.state,
            window_size: self.window_size,
            settings: &self.settings,
            worlds: &self.worlds,
            resource_packs: &self.packs,
            mobs: &self.regs.mobs,
            blocks: &self.regs.blocks,
            block_table: &self.block_table,
            items: &self.regs.items,
            item_icons: &self.item_icons,
            craft_station: self.craft_station,
            craft_grid: &self.craft_grid,
            previous_state: self.previous_state,
            settings_tab: self.settings_tab,
            hud: &self.hud,
            inventory: s.inventory(),
            held: self.cursor.held,
            selected_slot: self.input.selected_slot(),
            game_mode: player.map(|p| p.game_mode),
            health: p
                .and_then(|p| e.copy::<nb_ecs::Health>(p))
                .map(|h| (h.health, h.max_health)),
            chat_log: self.chat.view(&self.input).log,
            chat_open: self.chat.is_active(),
            current_world: s.name.as_deref(),
            saves_root: &self.opts.saves_root,
            lan: &self.lan_view,
        }
    }

    fn with_ui<R>(
        &mut self,
        f: impl FnOnce(&mut dyn Ui, &UiView<'_>, &mut InputManager) -> R,
    ) -> R {
        let mut ui = std::mem::replace(&mut self.ui, Box::new(NullUi));
        let slot = self.input.selected_slot();
        let mut input = std::mem::take(&mut self.input);
        let r = {
            let mut view = self.ui_view();
            view.selected_slot = slot;
            f(ui.as_mut(), &view, &mut input)
        };
        self.input = input;
        self.ui = ui;
        r
    }

    // ── Tick ────────────────────────────────────────────────────────────────────────────

    /// `Main.update(dt)`.
    pub fn update(&mut self, dt: f64) {
        self.step(Step::Input);
        self.input.update();

        self.step(Step::Time);
        self.sky.update(dt);

        self.step(Step::Chat);
        {
            let role = self.net.role();
            let mut outgoing = Vec::new();
            let (chat, input, session, sky) = (
                &mut self.chat,
                &mut self.input,
                &mut self.session,
                &mut self.sky,
            );
            chat.update(dt, input, |cmd| {
                Self::chat_submit(session, sky, role, &mut outgoing, cmd)
            });
            self.net_send_chat(outgoing);
        }

        if self.resized {
            self.step(Step::Resize);
            self.aspect = aspect(self.window_size.0, self.window_size.1);
            self.resized = false;
        }

        self.net_pre_tick(dt);
        self.step(Step::State(self.state));
        let ev_start = self.events.len();
        self.update_state(dt);
        self.net_post_tick(dt, ev_start);
        self.refresh_lan_view();

        self.step(Step::Chunks);
        let pos = self.session.camera().position;
        self.session.world.update(pos);

        self.step(Step::Meshes);
        self.update_meshes();
        self.ticks += 1;
    }

    /// Avance de `n` ticks fixes (tests, outils).
    pub fn run_ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.update(crate::TIME_STEP);
        }
    }

    fn update_meshes(&mut self) {
        if self.awaiting_block_table {
            return;
        }
        let cm = self.session.world.chunk_manager();
        let active = self.visible_chunks(sorted_active_positions(cm));
        let actions = self.meshes.update(
            &mut WorldChunks(cm),
            &active,
            self.block_table.clone(),
            self.session.biomes(),
        );
        self.mesh_actions.extend(actions);
    }

    fn update_state(&mut self, dt: f64) {
        match self.state {
            GameState::MainMenu | GameState::Multiplayer => {
                self.menu_rotation += (dt * 10.0) as f32;
                let rot = self.menu_rotation;
                self.session.update_camera(|c| {
                    c.position = Vec3::new(0.0, 150.0, 0.0);
                    c.pitch_deg = 20.0;
                    c.yaw_deg = rot;
                });
            }
            GameState::Playing => {
                if self.input.is_just_pressed(Action::Pause) {
                    self.switch_state(GameState::Paused);
                    return;
                }
                if self.input.is_just_pressed(Action::Inventory) {
                    self.craft_station = Station::Hand;
                    self.switch_state(GameState::Inventory);
                    return;
                }
                self.step(Step::EcsTick);
                let first_new_event = self.events.len();
                let pi = self.input.player_input(
                    self.mouse_locked,
                    Some(self.settings.sensitivity),
                    Some(self.settings.fov),
                );
                // Client LAN : on attend les premiers chunks de l'hôte avant de simuler.
                if !self.lan_waiting_terrain() {
                    self.session
                        .tick(&pi, &self.regs, &mut self.events, dt as f32);
                }
                // Clic droit sur un établi / une fonderie : ouvre l'écran de la station.
                let station = self.events[first_new_event.min(self.events.len())..]
                    .iter()
                    .find_map(|e| match e {
                        EcsEvent::StationUsed { station, .. } => Some(*station),
                        _ => None,
                    });
                if let Some(station) = station {
                    self.open_station(station);
                    return;
                }
                let slot = self.input.selected_slot();
                let held = self
                    .session
                    .inventory()
                    .and_then(|i| i.hotbar_slot(i32::from(slot)))
                    .map(|s| s.item);
                self.hud.update(slot, held, dt);
            }
            GameState::Paused => {
                if self.input.is_just_pressed(Action::Pause) {
                    self.switch_state(GameState::Playing);
                    return;
                }
                self.lan_idle_tick(dt);
            }
            GameState::Inventory => {
                if self.input.is_just_pressed(Action::Inventory)
                    || self.input.is_just_pressed(Action::Pause)
                {
                    self.close_craft_screen();
                    self.switch_state(GameState::Playing);
                    return;
                }
                self.lan_idle_tick(dt);
            }
            GameState::WorldSelect
            | GameState::Settings
            | GameState::ResourcePacks
            | GameState::MobSettings => {}
        }
        // Écouteur de réaffectation : la touche capturée passe avant la mise à jour de l'écran
        // (callback GLFW pendant `pollEvents`, avant le tick suivant). Échap y est consommé
        // puis revu enfoncé par l'écran : il quitte les réglages (bogue Java conservé).
        if let Some(key) = self.input.take_rebind() {
            self.ui.key_pressed(self.state, key);
        }
        let cmds = self.with_ui(|ui, view, input| ui.update(view, input, dt));
        for c in cmds {
            self.command(c);
        }
    }

    // ── Machine à états ─────────────────────────────────────────────────────────────────

    /// `StateManager.switchState` : `exit` de l'ancien état puis `enter` du nouveau.
    pub fn switch_state(&mut self, s: GameState) {
        let old = self.state;
        self.exit_state(old);
        self.ui.exit(old, &mut self.input);
        self.state = s;
        self.enter_state(s);
        self.with_ui(|ui, view, input| ui.enter(s, view, input));
    }

    fn set_mouse_locked(&mut self, locked: bool) {
        self.mouse_locked = locked;
        self.requests.push(PlatformRequest::SetCursorGrab(locked));
        if locked {
            self.input.reset_mouse();
        }
    }

    fn reset_camera_to_default(&mut self) {
        self.session.update_camera(|c| {
            c.pitch_deg = 0.0;
            c.yaw_deg = 0.0;
            c.position = Vec3::from(nb_mesh::camera::DEFAULT_POSITION);
        });
    }

    fn enter_state(&mut self, s: GameState) {
        match s {
            GameState::MainMenu | GameState::Paused => self.set_mouse_locked(false),
            GameState::Playing => {
                self.set_mouse_locked(true);
                self.hud = HudState::default();
            }
            GameState::Inventory => {
                self.set_mouse_locked(false);
                self.cursor.reset();
                self.craft_grid = CraftingGrid::new(self.craft_station);
            }
            GameState::WorldSelect => {
                self.requests.push(PlatformRequest::SetCursorGrab(false));
                self.reset_camera_to_default();
                self.refresh_worlds();
            }
            GameState::Settings => {
                self.requests.push(PlatformRequest::SetCursorGrab(false));
                self.settings_tab = self.pending_settings_tab.take().unwrap_or_default();
            }
            GameState::ResourcePacks => {
                self.requests.push(PlatformRequest::SetCursorGrab(false));
                self.packs = match &self.opts.packs_root {
                    Some(root) => nb_gfx::list_resource_packs(root),
                    None => vec![nb_gfx::ResourcePackMeta::builtin()],
                };
            }
            GameState::MobSettings => {
                self.requests.push(PlatformRequest::SetCursorGrab(false));
            }
            GameState::Multiplayer => {
                self.requests.push(PlatformRequest::SetCursorGrab(false));
                self.reset_camera_to_default();
                self.lan_browser.start();
            }
        }
    }

    fn exit_state(&mut self, s: GameState) {
        match s {
            GameState::MainMenu => self.reset_camera_to_default(),
            GameState::Inventory => self.close_craft_screen(),
            GameState::Settings => self.input.cancel_rebind(),
            GameState::Multiplayer => {
                self.lan_browser.stop();
                self.lan_message = None;
                self.cancel_join();
            }
            _ => {}
        }
    }

    /// Ferme l'écran d'inventaire : la pile tenue et la grille de craft retournent dans
    /// l'inventaire (le reliquat qui ne rentre pas est perdu, comme `returnHeld`).
    fn close_craft_screen(&mut self) {
        let mut cursor = self.cursor;
        let mut grid = self.craft_grid.clone();
        let book = self.regs.recipes.clone();
        if let Some(inv) = self.session.inventory_mut() {
            cursor.close(inv, &mut grid, &book);
        }
        self.cursor = cursor;
        self.craft_grid = grid;
    }

    fn refresh_worlds(&mut self) {
        self.worlds = save::list_worlds(&self.opts.saves_root);
    }

    fn fail(&mut self, what: &str, e: impl std::fmt::Display) {
        let msg = format!("{what} : {e}");
        eprintln!("{msg}");
        self.last_error = Some(msg);
    }

    /// Exécute une commande de l'interface (aussi utilisable directement : tests, outils).
    pub fn command(&mut self, c: UiCommand) {
        match c {
            UiCommand::SwitchState(s) => self.switch_state(s),
            UiCommand::OpenSettings => {
                self.previous_state = self.state;
                self.switch_state(GameState::Settings);
            }
            UiCommand::OpenSettingsTab(tab) => {
                if tab == SettingsTab::Controls {
                    self.pending_settings_tab = Some(tab);
                }
                self.switch_state(GameState::Settings);
            }
            UiCommand::SettingsBack => self.switch_state(self.previous_state),
            UiCommand::ApplySettings(s) => {
                self.apply_settings(&s);
                self.despawn_mobs_if_dirty();
                self.switch_state(self.previous_state);
            }
            UiCommand::Resume => self.switch_state(GameState::Playing),
            UiCommand::SaveAndMainMenu => {
                // Correctif (décision de Gabriel) : le monde est sauvegardé puis déchargé. Le
                // Java le laissait chargé (`currentWorldName` posé) et pouvait le resauvegarder
                // plus tard sous un nom périmé (dossier d'un monde renommé/supprimé recréé).
                if self.is_lan_client() {
                    self.leave_client(None);
                } else {
                    self.unload_to_menu_world();
                    self.switch_state(GameState::MainMenu);
                }
            }
            UiCommand::Quit => {
                self.quit = true;
                self.requests.push(PlatformRequest::Quit);
            }
            UiCommand::PlayWorld(meta) => self.load_and_play(&meta),
            UiCommand::CreateWorld(nw) => self.create_world(*nw),
            UiCommand::EditWorld {
                name,
                new_name,
                game_mode,
                allow_cheats,
            } => {
                let new_name = nb_ecs::javafmt::java_trim(&new_name).to_owned();
                if !is_valid_new_name(&self.opts.saves_root, &new_name, Some(&name)) {
                    return;
                }
                let root = self.opts.saves_root.clone();
                match save::load_metadata(&root, &save::sanitize(&name)) {
                    Ok(mut meta) => {
                        meta.game_mode = game_mode.java_name().to_owned();
                        meta.allow_cheats = allow_cheats;
                        if let Err(e) = save::rename_world(&root, &mut meta, &new_name) {
                            self.fail("renommage du monde", e);
                        }
                    }
                    Err(e) => self.fail("lecture du monde", e),
                }
                self.refresh_worlds();
            }
            UiCommand::DeleteWorld(name) => {
                save::delete_world(&self.opts.saves_root, &name);
                self.refresh_worlds();
            }
            UiCommand::RecreateWorld { source, new_name } => {
                let new_name = nb_ecs::javafmt::java_trim(&new_name).to_owned();
                if !is_valid_new_name(&self.opts.saves_root, &new_name, None) {
                    return;
                }
                let root = self.opts.saves_root.clone();
                let src = match save::load_metadata(&root, &save::sanitize(&source)) {
                    Ok(m) => m,
                    Err(e) => return self.fail("lecture du monde", e),
                };
                match save::clone_world(&root, &src, &new_name) {
                    Ok(clone) => self.load_and_play(&clone),
                    Err(e) => self.fail("copie du monde", e),
                }
            }
            UiCommand::RefreshWorlds => self.refresh_worlds(),
            UiCommand::InventoryClick(slot) => self.command(UiCommand::SlotClick {
                slot,
                button: nb_ecs::Button::Left,
                shift: false,
            }),
            UiCommand::SlotClick {
                slot,
                button,
                shift,
            } => {
                if self.state == GameState::Inventory {
                    let mut cursor = self.cursor;
                    let mut grid = self.craft_grid.clone();
                    let book = self.regs.recipes.clone();
                    if let Some(inv) = self.session.inventory_mut() {
                        cursor.click(inv, &mut grid, &book, slot, button, shift);
                    }
                    self.cursor = cursor;
                    self.craft_grid = grid;
                }
            }
            UiCommand::ApplyResourcePack(name) => {
                if name != self.settings.resource_pack {
                    self.request_pack_reload(name.clone());
                    let cm = self.session.world.chunk_manager();
                    let loaded = cm.active_positions();
                    let acts = self.meshes.invalidate_all(&mut WorldChunks(cm), &loaded);
                    self.mesh_actions.extend(acts);
                    self.settings.resource_pack = name;
                    self.save_settings();
                }
                self.switch_state(GameState::Settings);
            }
            UiCommand::SetMobRegistry(reg) => {
                self.regs.mobs = *reg;
                self.mobs_dirty = true;
                if self.opts.persist_settings {
                    let path = self.opts.saves_root.join("mobs.json");
                    if let Err(e) = write_file(&path, &self.regs.mobs.to_json()) {
                        self.fail("écriture de mobs.json", e);
                    }
                }
            }
            UiCommand::ReloadMobModel(id) => {
                self.requests.push(PlatformRequest::ReloadMobModel(id));
            }
            UiCommand::ResetMobRegistry(reg) => {
                self.command(UiCommand::SetMobRegistry(reg));
                // `despawnAll` + `clearDirty`.
                self.despawn_mobs_if_dirty();
            }
            UiCommand::OpenToLan => self.open_to_lan(),
            UiCommand::CloseLan => self.close_lan(),
            UiCommand::JoinLan { address, name } => self.join_lan(&address, &name),
            UiCommand::CancelJoin => self.cancel_join(),
            UiCommand::RefreshLan => self.lan_browser.refresh(),
            UiCommand::LeaveMobSettings => {
                self.requests.push(PlatformRequest::ReloadMobModels);
                self.despawn_mobs_if_dirty();
                self.switch_state(GameState::Settings);
            }
        }
    }

    /// Dessin 2D de la frame (écran de l'état courant puis chat), converti pour nb-gfx.
    pub fn draw_ui(&mut self) -> UiFrame {
        let (w, h) = self.window_size;
        let mut dl = nb_ui::DrawList::new(w, h);
        let mut ui = std::mem::replace(&mut self.ui, Box::new(NullUi));
        {
            let view = self.ui_view();
            ui.draw(&view, &self.input, &mut dl);
        }
        self.ui = ui;
        UiFrame::from_draw_list(&dl)
    }

    fn request_pack_reload(&mut self, name: String) {
        self.requests
            .push(PlatformRequest::ReloadResourcePack(name));
        self.awaiting_block_table = true;
    }

    fn despawn_mobs_if_dirty(&mut self) {
        if self.mobs_dirty {
            for id in self.session.sim.entities.query_ids::<&MobType>() {
                self.session.sim.entities.destroy(id);
            }
            self.mobs_dirty = false;
        }
    }

    /// `SettingsState.applyAndSave` (valeurs déjà arrondies par l'interface).
    fn apply_settings(&mut self, w: &GameSettings) {
        let s = &mut self.settings;
        s.render_distance = w.render_distance;
        s.fov = w.fov;
        s.sensitivity = w.sensitivity;
        s.shadow_quality = w.shadow_quality;
        s.bloom_enabled = w.bloom_enabled;
        s.vsync_enabled = w.vsync_enabled;
        s.fullscreen = w.fullscreen;
        s.key_bindings
            .extend(w.key_bindings.iter().map(|(a, k)| (*a, *k)));
        s.window_width = w.window_width;
        s.window_height = w.window_height;
        let s = self.settings.clone();
        if !s.fullscreen {
            self.requests.push(PlatformRequest::SetWindowSize(
                s.window_width.max(1) as u32,
                s.window_height.max(1) as u32,
            ));
        }
        self.session
            .world
            .chunk_manager()
            .set_render_distance(s.render_distance);
        self.session.update_camera(|c| c.fov = fov_rad(s.fov));
        self.requests
            .push(PlatformRequest::ApplyGraphics(GfxSettings::from_game(&s)));
        self.requests
            .push(PlatformRequest::SetFullscreen(s.fullscreen));
        self.input.apply_key_bindings(&s.key_bindings);
        self.save_settings();
    }

    fn save_settings(&mut self) {
        if !self.opts.persist_settings {
            return;
        }
        let path = self.opts.saves_root.join("settings.json");
        if let Err(e) = write_file(&path, &self.settings.to_json()) {
            self.fail("écriture de settings.json", e);
        }
    }

    // ── Mondes ──────────────────────────────────────────────────────────────────────────

    /// Sauvegarde du monde nommé courant (`WorldSaveManager.saveWorldState`).
    pub fn save_current(&mut self) {
        if self.session.name.is_none() {
            return;
        }
        if let Err(e) = self
            .session
            .save_state(&self.opts.saves_root, &self.sky, &self.regs)
        {
            self.fail("sauvegarde du monde", e);
        }
    }

    fn create_world(&mut self, nw: NewWorld) {
        let name = nb_ecs::javafmt::java_trim(&nw.name).to_owned();
        if !is_valid_new_name(&self.opts.saves_root, &name, None) {
            return;
        }
        let seed = parse_seed(&nw.seed_text, &mut self.rng);
        let mut meta = WorldMetadata::new(name, seed, nw.game_mode.java_name());
        meta.allow_cheats = nw.allow_cheats;
        meta.world_config = nw.config;
        if let Err(e) = save::create_world_directory(&self.opts.saves_root, &mut meta) {
            return self.fail("création du monde", e);
        }
        self.load_and_play(&meta);
    }

    /// `WorldSelectState.loadAndPlay` + `GameContext.loadWorld`.
    pub fn load_and_play(&mut self, meta: &WorldMetadata) {
        if let Err(e) = self.load_world(meta) {
            return self.fail("chargement du monde", e);
        }
        self.switch_state(GameState::Playing);
    }

    /// Charge `meta` (sans changer d'état).
    pub fn load_world(&mut self, meta: &WorldMetadata) -> nb_core::Result<()> {
        let dir = save::world_dir(&self.opts.saves_root, &meta.name);
        let storage = FileChunkStorage::new(&dir)?;
        self.release_session();
        let camera = self.session.camera();
        let mode = GameMode::from_java_name(&meta.game_mode);
        let rngs = ecs_rngs(&self.opts, &mut self.rng);
        let mut s = WorldSession::new(
            meta.seed,
            Arc::new(storage),
            meta.world_config.clone(),
            &self.regs,
            camera,
            mode,
            rngs,
        );
        s.world
            .chunk_manager()
            .set_render_distance(self.settings.render_distance);
        s.name = Some(meta.name.clone());
        s.allow_cheats = meta.allow_cheats;
        self.session = s;
        self.sky
            .set_day_length_seconds(meta.world_config.day_length_seconds);
        self.sky.set_time(meta.world_config.time_of_day);
        self.session.apply_player_state(meta, &self.regs);
        Ok(())
    }

    /// Sauvegarde du monde nommé, arrêt de son gestionnaire de chunks et libération des
    /// maillages (`chunkMeshes.clear()` ; la génération incrémentée écarte ceux en vol).
    fn release_session(&mut self) {
        self.net_stop();
        self.release_session_core();
    }

    /// [`Self::release_session`] sans toucher à la partie réseau.
    fn release_session_core(&mut self) {
        self.save_current();
        self.session.shutdown();
        let cm = self.session.world.chunk_manager();
        let acts = self.meshes.invalidate_all(&mut WorldChunks(cm), &[]);
        self.mesh_actions.extend(acts);
    }

    /// Correctif PAUSED → MAIN MENU : sauvegarde et décharge le monde nommé, puis installe un
    /// monde de menu neuf (comme `Main.initEngine` : graine `nextInt(1e9)`, stockage en
    /// mémoire, joueur CREATIVE, jamais sauvegardé). Sans effet si aucun monde nommé.
    pub fn unload_to_menu_world(&mut self) {
        if self.session.name.is_none() {
            return;
        }
        self.release_session();
        self.install_menu_world();
    }

    /// Monde du menu neuf à la place de la session courante (déjà libérée).
    fn install_menu_world(&mut self) {
        let fov = self.session.camera().fov;
        let seed = self
            .opts
            .menu_seed
            .unwrap_or_else(|| i64::from(self.rng.next_int_bound(1_000_000_000)));
        let rngs = ecs_rngs(&self.opts, &mut self.rng);
        let s = WorldSession::new(
            seed,
            Arc::new(InMemoryChunkStorage::new()),
            WorldConfig::default(),
            &self.regs,
            menu_camera(fov),
            GameMode::Creative,
            rngs,
        );
        s.world
            .chunk_manager()
            .set_render_distance(self.settings.render_distance);
        self.session = s;
        self.cursor.reset();
    }

    /// Fermeture (`Main.cleanup`) : sauvegarde du monde nommé et arrêt des threads.
    pub fn shutdown(mut self) {
        self.net_stop();
        self.save_current();
        self.session.shutdown();
        let Game { meshes, .. } = self;
        meshes.shutdown();
    }
}

fn aspect(w: u32, h: u32) -> f32 {
    if w == 0 || h == 0 {
        1280.0 / 720.0
    } else {
        w as f32 / h as f32
    }
}

fn entropy_seed() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i64)
}

fn ecs_rngs(opts: &GameOptions, rng: &mut JavaRandom) -> EcsRngs {
    match opts.rng_seed {
        Some(_) => EcsRngs::from_seed(rng.next_long()),
        None => EcsRngs::from_entropy(),
    }
}

fn load_settings(root: &Path) -> GameSettings {
    match std::fs::read_to_string(root.join("settings.json")) {
        Ok(json) => GameSettings::from_json(&json).unwrap_or_else(|e| {
            eprintln!("settings.json illisible : {e}");
            GameSettings::default()
        }),
        Err(_) => GameSettings::default(),
    }
}

fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

/// Pas de temps passé à l'ECS (`(float) dt`).
pub const ECS_DT: f32 = TICK_DT;
