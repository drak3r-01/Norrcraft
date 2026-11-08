//! Les écrans de nb-ui branchés sur le jeu : implémentation de [`Ui`] utilisée par nb-app.
//!
//! Rôle de l'adaptateur (aucune logique de jeu ici) :
//! - entrées : [`UiInput`] construit depuis l'[`InputManager`] ; tampons de saisie
//!   (`TypingBuffers`) recopiés depuis / vers l'`InputManager` autour de chaque écran ;
//! - actions nb-ui → [`UiCommand`] exécutées par [`crate::Game::command`] ;
//! - correspondance [`WorldGenParams`] ↔ [`WorldConfig`] et préréglages
//!   ([`WorldConfig::preset`]) ;
//! - dessin : écran de l'état puis chat (journal et zone de saisie), icônes de blocs =
//!   couche `getTextureIndex(0)` de la table de maillage courante.

use nb_data::keys::KEY_ESCAPE;
use nb_data::MobRegistry;
use nb_ecs::{Button, GameMode, ItemStack};
use nb_ui::screens::{
    draw_player_list, draw_terrain_loading, LanPause, MainMenu, MenuAction, MobAction,
    MobSettingsScreen, MultiplayerAction, MultiplayerScreen, MultiplayerView, PackAction,
    PackEntry, PauseMenu, PlayerRow, ResourcePackScreen, ServerRow, SettingsAction, SettingsScreen,
    WorldAction, WorldEntry, WorldGenParams, WorldSelectScreen, WorldType as UiWorldType,
};
use nb_ui::{
    ClickButton, DrawList, Hud, HudView, InventoryAction, InventoryScreen, SlotView, TypingBuffers,
    UiContext, UiInput,
};
use nb_world::save;
use nb_world::{OreRange, WorldConfig, WorldType};

use crate::chat::ChatColor;
use crate::input::{glfw, Action, InputManager};
use crate::state::{GameState, SettingsTab};
use crate::ui::{NewWorld, Ui, UiCommand, UiView};

/// `nb_ui::GameState` → état du jeu (mêmes variantes).
pub fn from_ui_state(s: nb_ui::GameState) -> GameState {
    use nb_ui::GameState as U;
    match s {
        U::MainMenu => GameState::MainMenu,
        U::WorldSelect => GameState::WorldSelect,
        U::Playing => GameState::Playing,
        U::Paused => GameState::Paused,
        U::Inventory => GameState::Inventory,
        U::Settings => GameState::Settings,
        U::ResourcePacks => GameState::ResourcePacks,
        U::MobSettings => GameState::MobSettings,
        U::Multiplayer => GameState::Multiplayer,
    }
}

/// État du jeu → `nb_ui::GameState`.
pub fn to_ui_state(s: GameState) -> nb_ui::GameState {
    use nb_ui::GameState as U;
    match s {
        GameState::MainMenu => U::MainMenu,
        GameState::WorldSelect => U::WorldSelect,
        GameState::Playing => U::Playing,
        GameState::Paused => U::Paused,
        GameState::Inventory => U::Inventory,
        GameState::Settings => U::Settings,
        GameState::ResourcePacks => U::ResourcePacks,
        GameState::MobSettings => U::MobSettings,
        GameState::Multiplayer => U::Multiplayer,
    }
}

pub fn to_world_type(t: UiWorldType) -> WorldType {
    match t {
        UiWorldType::Normal => WorldType::Normal,
        UiWorldType::Flat => WorldType::Flat,
        UiWorldType::Amplified => WorldType::Amplified,
        UiWorldType::Island => WorldType::Island,
        UiWorldType::Archipelago => WorldType::Archipelago,
        UiWorldType::Highlands => WorldType::Highlands,
        UiWorldType::Classic => WorldType::Classic,
    }
}

/// Champs de `WorldConfig` édités par le panneau « Advanced ».
pub fn params_from_config(c: &WorldConfig) -> WorldGenParams {
    let o = &c.ores;
    WorldGenParams {
        sea_level: c.sea_level,
        base_height: c.base_height,
        height_scale: c.height_scale,
        ocean_frequency: c.ocean_frequency,
        ocean_threshold: c.ocean_threshold,
        height_noise_frequency: c.height_noise_frequency,
        height_noise_octaves: c.height_noise_octaves,
        height_noise_lacunarity: c.height_noise_lacunarity,
        height_noise_gain: c.height_noise_gain,
        river_width: c.river_width,
        river_depth: c.river_depth,
        river_frequency: c.river_frequency,
        temp_scale: c.temp_scale,
        humid_scale: c.humid_scale,
        mountain_frequency: c.mountain_frequency,
        snow_line_mountains: c.snow_line_mountains,
        snow_line_taiga: c.snow_line_taiga,
        vegetation_density: c.vegetation_density,
        structure_density: c.structure_density,
        tree_density_overrides: c.tree_density_overrides,
        caves_enabled: c.caves_enabled,
        cave_threshold: c.cave_threshold,
        cavern_threshold: c.cavern_threshold,
        cave_depth_max: c.cave_depth_max,
        coal_chance: o.coal.chance,
        coal_min_y: o.coal.min_y,
        coal_max_y: o.coal.max_y,
        iron_chance: o.iron.chance,
        iron_min_y: o.iron.min_y,
        iron_max_y: o.iron.max_y,
        gold_chance: o.gold.chance,
        gold_min_y: o.gold.min_y,
        gold_max_y: o.gold.max_y,
        diamond_chance: o.diamond.chance,
        diamond_min_y: o.diamond.min_y,
        diamond_max_y: o.diamond.max_y,
        emerald_chance: o.emerald.chance,
        emerald_min_y: o.emerald.min_y,
        emerald_max_y: o.emerald.max_y,
    }
}

/// Recopie les champs édités dans `c` (les autres — côtes, gameplay, mobs… — restent ceux du
/// préréglage, comme le `pendingConfig` Java).
pub fn apply_params(p: &WorldGenParams, c: &mut WorldConfig) {
    c.sea_level = p.sea_level;
    c.base_height = p.base_height;
    c.height_scale = p.height_scale;
    c.ocean_frequency = p.ocean_frequency;
    c.ocean_threshold = p.ocean_threshold;
    c.height_noise_frequency = p.height_noise_frequency;
    c.height_noise_octaves = p.height_noise_octaves;
    c.height_noise_lacunarity = p.height_noise_lacunarity;
    c.height_noise_gain = p.height_noise_gain;
    c.river_width = p.river_width;
    c.river_depth = p.river_depth;
    c.river_frequency = p.river_frequency;
    c.temp_scale = p.temp_scale;
    c.humid_scale = p.humid_scale;
    c.mountain_frequency = p.mountain_frequency;
    c.snow_line_mountains = p.snow_line_mountains;
    c.snow_line_taiga = p.snow_line_taiga;
    c.vegetation_density = p.vegetation_density;
    c.structure_density = p.structure_density;
    c.tree_density_overrides = p.tree_density_overrides;
    c.caves_enabled = p.caves_enabled;
    c.cave_threshold = p.cave_threshold;
    c.cavern_threshold = p.cavern_threshold;
    c.cave_depth_max = p.cave_depth_max;
    let r = |min_y, max_y, chance| OreRange::new(min_y, max_y, chance);
    c.ores.coal = r(p.coal_min_y, p.coal_max_y, p.coal_chance);
    c.ores.iron = r(p.iron_min_y, p.iron_max_y, p.iron_chance);
    c.ores.gold = r(p.gold_min_y, p.gold_max_y, p.gold_chance);
    c.ores.diamond = r(p.diamond_min_y, p.diamond_max_y, p.diamond_chance);
    c.ores.emerald = r(p.emerald_min_y, p.emerald_max_y, p.emerald_chance);
}

/// Configuration d'un monde créé : `preset(type)` puis les champs édités.
pub fn config_for(world_type: UiWorldType, params: &WorldGenParams) -> WorldConfig {
    let mut c = WorldConfig::preset(to_world_type(world_type));
    apply_params(params, &mut c);
    c
}

fn preset_params(t: UiWorldType) -> WorldGenParams {
    params_from_config(&WorldConfig::preset(to_world_type(t)))
}

/// Entrées de la frame vues par les écrans.
pub fn ui_input(view: &UiView<'_>, input: &InputManager, dt: f64) -> UiInput {
    let (mx, my) = input.cursor();
    UiInput {
        width: view.window_size.0 as i32,
        height: view.window_size.1 as i32,
        mouse_x: mx,
        mouse_y: my,
        mouse_down: input.is_mouse_down(glfw::MOUSE_BUTTON_LEFT),
        primary_pressed: input.is_just_pressed(Action::Primary),
        secondary_pressed: input.is_just_pressed(Action::Secondary),
        shift_down: input.is_key_down(nb_data::keys::KEY_LEFT_SHIFT)
            || input.is_key_down(nb_data::keys::KEY_RIGHT_SHIFT),
        scroll_y: input.scroll_y(),
        esc_down: input.is_key_down(KEY_ESCAPE),
        pause_pressed: input.is_just_pressed(Action::Pause),
        inventory_pressed: input.is_just_pressed(Action::Inventory),
        enter_pressed: input.is_just_pressed(Action::SubmitChat),
        cancel_pressed: input.is_just_pressed(Action::CancelChat),
        dt,
    }
}

/// Tampons de saisie de l'`InputManager`.
pub fn buffers_of(input: &InputManager) -> TypingBuffers {
    TypingBuffers {
        typing_mode: input.is_typing_mode(),
        suppress_chat_filter: input.is_chat_filter_suppressed(),
        use_secondary: input.is_using_secondary_buffer(),
        command: input.command_buffer().to_owned(),
        secondary: input.secondary_buffer().to_owned(),
    }
}

/// Recopie les tampons modifiés par un écran dans l'`InputManager`.
pub fn write_buffers(b: TypingBuffers, input: &mut InputManager) {
    input.set_typing_mode(b.typing_mode);
    input.set_suppress_chat_filter(b.suppress_chat_filter);
    input.set_use_secondary_buffer(b.use_secondary);
    *input.command_buffer_mut() = b.command;
    *input.secondary_buffer_mut() = b.secondary;
}

fn world_entries(view: &UiView<'_>) -> Vec<WorldEntry> {
    view.worlds
        .iter()
        .map(|m| WorldEntry {
            name: m.name.clone(),
            last_played_at: m.last_played_at,
            game_mode: m.game_mode.clone(),
            allow_cheats: m.allow_cheats,
        })
        .collect()
}

fn pack_entries(view: &UiView<'_>) -> Vec<PackEntry> {
    view.resource_packs
        .iter()
        .map(|p| PackEntry {
            name: p.name.clone(),
            description: p.description.clone(),
        })
        .collect()
}

/// Icône d'un bloc : cube isométrique pour un bloc plein, sprite pour une plante / un objet fin.
pub fn block_icon_style(info: &nb_mesh::BlockMeshInfo, name: &str) -> (u32, nb_ui::IconStyle) {
    let l = info.texture_layers;
    let flat = info.cross_shaped
        || matches!(
            name,
            "torch" | "lantern" | "lily_pad" | "cobweb" | "vines" | "wheat" | "reeds" | "water"
        );
    if flat {
        return (l[0], nb_ui::IconStyle::Item);
    }
    const GRASS: [f32; 3] = [0.56, 0.80, 0.34];
    const LEAF: [f32; 3] = [0.46, 0.72, 0.28];
    let tint_top = if info.grass_tinted {
        GRASS
    } else if info.leaves_tinted {
        LEAF
    } else {
        [1.0; 3]
    };
    let tint_side = if info.leaves_tinted { LEAF } else { [1.0; 3] };
    (
        l[4],
        nb_ui::IconStyle::Cube {
            left: l[2],
            right: l[0],
            tint_top,
            tint_side,
        },
    )
}

/// Case d'inventaire vue par nb-ui : icône = `getTextureIndex(0)`, nom interne du bloc.
fn slot_view<'a>(view: &UiView<'a>, s: Option<ItemStack>) -> Option<SlotView<'a>> {
    let s = s?;
    // Bloc : couche de la table de maillage ; objet pur : couche de son icône.
    let (icon_layer, style) = match nb_data::BlockId::try_from(s.item) {
        Ok(b) => block_icon_style(view.block_table.get(b), view.items.name_of(s.item)),
        Err(_) => (view.item_icons.get(s.item), nb_ui::IconStyle::Item),
    };
    Some(SlotView {
        icon_layer,
        style,
        count: s.count,
        name: view.items.name_of(s.item),
    })
}

fn hud_view<'a>(view: &UiView<'a>) -> HudView<'a> {
    let inv = view.inventory.as_ref();
    HudView {
        hotbar: std::array::from_fn(|i| {
            slot_view(view, inv.and_then(|inv| inv.hotbar_slot(i as i32)))
        }),
        selected: usize::from(view.selected_slot),
        creative: view.game_mode == Some(GameMode::Creative),
        health: view.health,
    }
}

/// Ce que le menu pause propose pour le réseau local.
fn lan_pause(view: &UiView<'_>) -> LanPause {
    use crate::net::LanRole;
    let lan = view.lan;
    match lan.role {
        LanRole::Off if view.current_world.is_some() => LanPause::CanOpen,
        LanRole::Off => LanPause::Hidden,
        LanRole::Host => LanPause::Hosting {
            address: lan.addresses.first().cloned().unwrap_or_default(),
            players: lan.players.iter().map(|p| p.name.clone()).collect(),
        },
        LanRole::Client => LanPause::Joined {
            host: lan
                .players
                .iter()
                .find(|p| p.is_host)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            ping_ms: lan
                .players
                .iter()
                .find(|p| p.is_self)
                .and_then(|p| p.ping_ms),
        },
    }
}

fn server_rows(view: &UiView<'_>) -> Vec<ServerRow> {
    view.lan
        .servers
        .iter()
        .map(|s| ServerRow {
            title: s.world.clone(),
            subtitle: format!(
                "Hosted by {} - {}/{} players - {}",
                s.host, s.players, s.max_players, s.address
            ),
            address: s.address.clone(),
            full: s.players >= s.max_players,
        })
        .collect()
}

fn menu_action_cmd(a: MenuAction) -> UiCommand {
    match a {
        MenuAction::SwitchTo(nb_ui::GameState::Settings) => UiCommand::OpenSettings,
        MenuAction::SwitchTo(nb_ui::GameState::Playing) => UiCommand::Resume,
        MenuAction::SwitchTo(s) => UiCommand::SwitchState(from_ui_state(s)),
        MenuAction::Quit => UiCommand::Quit,
        MenuAction::SaveAndMainMenu => UiCommand::SaveAndMainMenu,
        MenuAction::OpenLan => UiCommand::OpenToLan,
        MenuAction::CloseLan => UiCommand::CloseLan,
    }
}

fn settings_tab_cmd(ctx: &UiContext) -> UiCommand {
    UiCommand::OpenSettingsTab(if ctx.pending_audio_tab {
        SettingsTab::Audio
    } else if ctx.pending_controls_tab {
        SettingsTab::Controls
    } else {
        SettingsTab::Graphics
    })
}

/// Les écrans de nb-ui (état conservé entre deux entrées, comme les champs des états Java).
#[derive(Debug)]
pub struct NbUi {
    main: MainMenu,
    pause: PauseMenu,
    hud: Hud,
    inventory: InventoryScreen,
    worlds: WorldSelectScreen,
    settings: Option<SettingsScreen>,
    packs: Option<ResourcePackScreen>,
    mobs: MobSettingsScreen,
    multiplayer: MultiplayerScreen,
}

impl Default for NbUi {
    fn default() -> Self {
        Self::new()
    }
}

impl NbUi {
    pub fn new() -> Self {
        Self {
            main: MainMenu::default(),
            pause: PauseMenu::default(),
            hud: Hud::new(),
            inventory: InventoryScreen::new(1280, 720),
            worlds: WorldSelectScreen::new(params_from_config(&WorldConfig::default())),
            settings: None,
            packs: None,
            mobs: MobSettingsScreen::default(),
            multiplayer: MultiplayerScreen::new(),
        }
    }

    /// Écran de choix du monde (tests, outils).
    pub fn world_select(&self) -> &WorldSelectScreen {
        &self.worlds
    }

    pub fn settings_screen(&self) -> Option<&SettingsScreen> {
        self.settings.as_ref()
    }

    fn ctx(view: &UiView<'_>) -> UiContext {
        UiContext {
            previous_state: to_ui_state(view.previous_state),
            pending_controls_tab: false,
            pending_audio_tab: false,
        }
    }

    fn world_cmd(view: &UiView<'_>, a: WorldAction) -> Option<UiCommand> {
        Some(match a {
            WorldAction::MainMenu => UiCommand::SwitchState(GameState::MainMenu),
            WorldAction::Play(name) => {
                let meta = view.worlds.iter().find(|m| m.name == name)?;
                UiCommand::PlayWorld(Box::new(meta.clone()))
            }
            WorldAction::Create {
                name,
                seed_text,
                game_mode,
                allow_cheats,
                world_type,
                params,
            } => UiCommand::CreateWorld(Box::new(NewWorld {
                name,
                seed_text,
                game_mode: GameMode::from_java_name(&game_mode),
                allow_cheats,
                config: config_for(world_type, &params),
            })),
            WorldAction::Edit {
                old_name,
                new_name,
                game_mode,
                allow_cheats,
            } => UiCommand::EditWorld {
                name: old_name,
                new_name,
                game_mode: GameMode::from_java_name(&game_mode),
                allow_cheats,
            },
            WorldAction::Delete(name) => UiCommand::DeleteWorld(name),
            WorldAction::Recreate { source, new_name } => {
                UiCommand::RecreateWorld { source, new_name }
            }
        })
    }
}

impl Ui for NbUi {
    fn enter(&mut self, state: GameState, view: &UiView<'_>, input: &mut InputManager) {
        match state {
            GameState::MainMenu => self.main.enter(),
            GameState::Paused => self.pause.enter(),
            GameState::Playing => self.hud.enter(),
            GameState::Inventory => {
                let (w, h) = view.window_size;
                self.inventory.set_station(view.craft_station);
                self.inventory.resize(w as i32, h as i32);
            }
            GameState::WorldSelect => self.worlds.enter(world_entries(view)),
            GameState::Settings => {
                let mut ctx = Self::ctx(view);
                ctx.pending_controls_tab = view.settings_tab == SettingsTab::Controls;
                ctx.pending_audio_tab = view.settings_tab == SettingsTab::Audio;
                self.settings = Some(SettingsScreen::enter(view.settings, &mut ctx));
            }
            GameState::ResourcePacks => {
                self.packs = Some(ResourcePackScreen::enter(
                    pack_entries(view),
                    &view.settings.resource_pack,
                ));
            }
            GameState::MobSettings => self.mobs.enter(),
            GameState::Multiplayer => {
                let mut b = buffers_of(input);
                self.multiplayer.enter(
                    &mut b,
                    &view.settings.player_name,
                    &view.settings.last_server,
                );
                write_buffers(b, input);
            }
        }
    }

    fn exit(&mut self, state: GameState, input: &mut InputManager) {
        match state {
            GameState::WorldSelect => {
                let mut b = buffers_of(input);
                self.worlds.exit(&mut b);
                write_buffers(b, input);
            }
            GameState::MobSettings => {
                let mut b = buffers_of(input);
                self.mobs.exit(&mut b);
                write_buffers(b, input);
            }
            GameState::Settings => {
                if let Some(s) = &mut self.settings {
                    s.rebinding = None;
                }
            }
            GameState::Multiplayer => {
                let mut b = buffers_of(input);
                self.multiplayer.exit(&mut b);
                write_buffers(b, input);
            }
            _ => {}
        }
    }

    fn key_pressed(&mut self, state: GameState, key: i32) {
        if state == GameState::Settings {
            if let Some(s) = &mut self.settings {
                s.key_pressed(key);
            }
        }
    }

    fn update(&mut self, view: &UiView<'_>, input: &mut InputManager, dt: f64) -> Vec<UiCommand> {
        let ui = ui_input(view, input, dt);
        let mut out = Vec::new();
        match view.state {
            GameState::MainMenu => {
                let mut ctx = Self::ctx(view);
                if let Some(a) = self.main.update(&ui, &mut ctx) {
                    out.push(menu_action_cmd(a));
                }
            }
            GameState::Paused => {
                let mut ctx = Self::ctx(view);
                self.pause.set_lan(lan_pause(view));
                if let Some(a) = self.pause.update(&ui, &mut ctx) {
                    out.push(menu_action_cmd(a));
                }
            }
            GameState::Playing => {
                // Pause / inventaire sont traités par le jeu avant cet appel.
                let hv = hud_view(view);
                self.hud.track_held(dt, &hv);
            }
            GameState::Inventory => match self.inventory.update(&ui) {
                Some(InventoryAction::Click {
                    slot,
                    button,
                    shift,
                }) => out.push(UiCommand::SlotClick {
                    slot,
                    button: match button {
                        ClickButton::Left => Button::Left,
                        ClickButton::Right => Button::Right,
                    },
                    shift,
                }),
                Some(InventoryAction::Close) => {
                    out.push(UiCommand::SwitchState(GameState::Playing))
                }
                None => {}
            },
            GameState::WorldSelect => {
                let entries = world_entries(view);
                if entries != self.worlds.worlds {
                    // `refreshWorlds` après édition / suppression.
                    self.worlds.set_worlds(entries);
                }
                let root = view.saves_root;
                let exists = |n: &str| save::world_name_exists(root, n);
                let mut b = buffers_of(input);
                let a = self.worlds.update(&ui, &mut b, &exists, &preset_params);
                write_buffers(b, input);
                if let Some(c) = a.and_then(|a| Self::world_cmd(view, a)) {
                    out.push(c);
                }
            }
            GameState::Multiplayer => {
                let rows = server_rows(view);
                let mv = MultiplayerView {
                    servers: &rows,
                    connecting: view.lan.connecting.as_deref(),
                    message: view.lan.message.as_ref().map(|(t, e)| (t.as_str(), *e)),
                    search_problem: None,
                };
                let mut b = buffers_of(input);
                let a = self.multiplayer.update(&ui, &mut b, &mv);
                write_buffers(b, input);
                match a {
                    Some(MultiplayerAction::Back) => {
                        out.push(UiCommand::SwitchState(GameState::MainMenu))
                    }
                    Some(MultiplayerAction::Refresh) => out.push(UiCommand::RefreshLan),
                    Some(MultiplayerAction::Join { address, name }) => {
                        out.push(UiCommand::JoinLan { address, name })
                    }
                    Some(MultiplayerAction::CancelJoin) => out.push(UiCommand::CancelJoin),
                    None => {}
                }
            }
            GameState::Settings => {
                let ctx = Self::ctx(view);
                if let Some(s) = &mut self.settings {
                    match s.update(&ui, view.settings, &ctx) {
                        Some(SettingsAction::Back(_)) => out.push(UiCommand::SettingsBack),
                        Some(SettingsAction::SwitchTo(st)) => {
                            out.push(UiCommand::SwitchState(from_ui_state(st)))
                        }
                        Some(SettingsAction::Apply(new, _)) => {
                            out.push(UiCommand::ApplySettings(new))
                        }
                        None => {}
                    }
                    // Écouteur de réaffectation posé tant qu'une action attend sa touche.
                    if s.rebinding.is_some() && !input.is_rebinding() {
                        input.start_rebind();
                    } else if s.rebinding.is_none() && input.is_rebinding() {
                        input.cancel_rebind();
                    }
                }
            }
            GameState::ResourcePacks => {
                let mut ctx = Self::ctx(view);
                if let Some(p) = &mut self.packs {
                    match p.update(&ui, &view.settings.resource_pack, &mut ctx) {
                        Some(PackAction::SwitchTo(nb_ui::GameState::Settings)) => {
                            out.push(settings_tab_cmd(&ctx))
                        }
                        Some(PackAction::SwitchTo(st)) => {
                            out.push(UiCommand::SwitchState(from_ui_state(st)))
                        }
                        Some(PackAction::Apply(name)) => {
                            out.push(UiCommand::ApplyResourcePack(name))
                        }
                        None => {}
                    }
                }
            }
            GameState::MobSettings => {
                let mut ctx = Self::ctx(view);
                let mut reg: MobRegistry = view.mobs.clone();
                let mut b = buffers_of(input);
                let a = self.mobs.update(&ui, &mut reg, &mut b, &mut ctx);
                write_buffers(b, input);
                match a {
                    Some(MobAction::SwitchTo(nb_ui::GameState::Settings)) => {
                        out.push(settings_tab_cmd(&ctx))
                    }
                    Some(MobAction::SwitchTo(st)) => {
                        out.push(UiCommand::SwitchState(from_ui_state(st)))
                    }
                    Some(MobAction::Leave) => out.push(UiCommand::LeaveMobSettings),
                    Some(MobAction::Saved) => out.push(UiCommand::SetMobRegistry(Box::new(reg))),
                    Some(MobAction::Added(id)) => {
                        out.push(UiCommand::SetMobRegistry(Box::new(reg)));
                        out.push(UiCommand::ReloadMobModel(id));
                    }
                    Some(MobAction::Reset) => out.push(UiCommand::ResetMobRegistry(Box::new(reg))),
                    None => {}
                }
            }
        }
        out
    }

    fn draw(&mut self, view: &UiView<'_>, input: &InputManager, dl: &mut DrawList) {
        let ui = ui_input(view, input, 0.0);
        let (w, h) = (ui.width, ui.height);
        match view.state {
            GameState::MainMenu => self.main.draw(dl, &ui),
            GameState::Paused => {
                self.pause.set_lan(lan_pause(view));
                self.pause.draw(dl, &ui)
            }
            GameState::Multiplayer => {
                let rows = server_rows(view);
                let mv = MultiplayerView {
                    servers: &rows,
                    connecting: view.lan.connecting.as_deref(),
                    message: view.lan.message.as_ref().map(|(t, e)| (t.as_str(), *e)),
                    search_problem: None,
                };
                self.multiplayer.draw(dl, &ui, &buffers_of(input), &mv);
            }
            GameState::Playing => self.hud.draw(dl, w, h, &hud_view(view)),
            GameState::Inventory => {
                self.inventory.set_station(view.craft_station);
                self.inventory.resize(w, h);
                let slots: [Option<SlotView<'_>>; nb_ui::inventory::SLOTS] =
                    std::array::from_fn(|i| {
                        slot_view(
                            view,
                            view.inventory.as_ref().and_then(|inv| inv.slot(i as i32)),
                        )
                    });
                let grid: Vec<Option<SlotView<'_>>> = (0..view.craft_grid.len())
                    .map(|k| slot_view(view, view.craft_grid.cell(k)))
                    .collect();
                self.inventory.draw_craft(
                    dl,
                    &ui,
                    &slots,
                    slot_view(view, view.held),
                    &grid,
                    slot_view(view, view.craft_grid.output()),
                );
            }
            GameState::WorldSelect => {
                let root = view.saves_root;
                let exists = |n: &str| save::world_name_exists(root, n);
                self.worlds.draw(dl, &ui, &buffers_of(input), &exists);
            }
            GameState::Settings => {
                if let Some(s) = &self.settings {
                    s.draw(dl, &ui);
                }
            }
            GameState::ResourcePacks => {
                if let Some(p) = &self.packs {
                    p.draw(dl, &ui);
                }
            }
            GameState::MobSettings => self.mobs.draw(dl, &ui, view.mobs, &buffers_of(input)),
        }
        // Partie réseau : attente du terrain à l'arrivée, liste des joueurs (Tab).
        dl.set_scissor(None);
        if view.state.renders_world() && view.state != GameState::MainMenu {
            if view.lan.loading_terrain {
                draw_terrain_loading(dl, w, h, view.lan.terrain_progress);
            } else if !view.lan.players.is_empty() && input.is_key_down(nb_data::keys::KEY_TAB) {
                let rows: Vec<PlayerRow> = view
                    .lan
                    .players
                    .iter()
                    .map(|p| PlayerRow {
                        name: p.name.clone(),
                        is_self: p.is_self,
                        is_host: p.is_host,
                        ping_ms: p.ping_ms,
                    })
                    .collect();
                draw_player_list(dl, w, h, &rows);
            }
        }
        // `chatController.render` : après l'écran, dans tous les états.
        if !view.chat_log.is_empty() {
            let log: Vec<nb_ui::ChatMessage> = view
                .chat_log
                .iter()
                .map(|m| match m.color {
                    ChatColor::White => nb_ui::ChatMessage::white(m.text.clone()),
                    ChatColor::Yellow => nb_ui::ChatMessage::yellow(m.text.clone()),
                })
                .collect();
            nb_ui::chat::draw_log(dl, w, h, &log);
        }
        if view.chat_open {
            nb_ui::chat::draw_box(dl, w, h, input.command_buffer());
        }
    }
}
