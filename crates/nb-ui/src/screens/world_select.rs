//! Sélection / création / édition / suppression / re-création de mondes (`WorldSelectState`).
//!
//! nb-ui ne voit pas nb-world : la configuration de génération passe par [`WorldGenParams`]
//! (les champs que le panneau « Advanced » édite) et les préréglages par un rappel fourni par
//! nb-game (`WorldConfig.preset`). Les opérations sur disque sont des [`WorldAction`].

use std::time::{SystemTime, UNIX_EPOCH};

use nb_core::JavaRandom;

use super::common::{
    draw_background, draw_title, AdvClick, AdvKind, AdvPanel, AdvRow, MouseEdge, Scroll, CONTENT_W,
    SPACER_H,
};
use crate::draw::{gray, DrawList, Scissor, WHITE, YELLOW};
use crate::font;
use crate::input::{TypingBuffers, UiInput};
use crate::jfmt::format_fixed;
use crate::widgets::{button, hit_test};

const BOTTOM_H: i32 = 130;
const ROW_H: i32 = 80;
const ROW_GAP: i32 = 8;
const GAP: i32 = 10;
const BTN1_H: i32 = 50;
const BTN2_H: i32 = 40;
const PAD_V: i32 = 12;
const PAD_MID: i32 = 8;
const BIG_BTN_W: i32 = (CONTENT_W - GAP) / 2;
const SMALL_BTN_W: i32 = (BIG_BTN_W - GAP) / 2;

// Formulaire de création.
const FORM_Y: i32 = SPACER_H + 8;
const NAME_INPUT_Y: i32 = FORM_Y + 24;
const SEED_LABEL_Y: i32 = NAME_INPUT_Y + 44 + 8;
const SEED_INPUT_Y: i32 = SEED_LABEL_Y + 24;
const GAME_MODE_ROW_Y: i32 = SEED_INPUT_Y + 44 + 12;
const CHEATS_ROW_Y: i32 = GAME_MODE_ROW_Y + 40 + 8;
const WORLD_TYPE_ROW_Y: i32 = CHEATS_ROW_Y + 40 + 8;
const ADV_TOGGLE_Y: i32 = WORLD_TYPE_ROW_Y + 50 + 10;
/// `createAdvPanelTop`.
pub const ADV_PANEL_TOP: i32 = ADV_TOGGLE_Y + 36 + 4;

/// `String.trim()` Java.
fn jtrim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// `(int) Math.round(double)`.
fn jround(v: f64) -> i32 {
    (v + 0.5).floor() as i64 as i32
}

/// Monde listé (`WorldMetadata`, champs utiles).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldEntry {
    pub name: String,
    /// `lastPlayedAt` (ms depuis l'époque).
    pub last_played_at: i64,
    pub game_mode: String,
    pub allow_cheats: bool,
}

/// `WorldConfig.WorldType` (ordre des ordinaux).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldType {
    Normal,
    Flat,
    Amplified,
    Island,
    Archipelago,
    Highlands,
    Classic,
}

impl WorldType {
    pub const ALL: [WorldType; 7] = [
        WorldType::Normal,
        WorldType::Flat,
        WorldType::Amplified,
        WorldType::Island,
        WorldType::Archipelago,
        WorldType::Highlands,
        WorldType::Classic,
    ];

    /// `worldTypeName`.
    pub fn display_name(self) -> &'static str {
        match self {
            WorldType::Normal => "Normal",
            WorldType::Flat => "Flat",
            WorldType::Amplified => "Amplified",
            WorldType::Island => "Island",
            WorldType::Archipelago => "Archipelago",
            WorldType::Highlands => "Highlands",
            WorldType::Classic => "Classic",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&t| t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Champs de `WorldConfig` édités par le panneau avancé (mêmes types que Java).
#[derive(Clone, Debug, PartialEq)]
pub struct WorldGenParams {
    pub sea_level: i32,
    pub base_height: i32,
    pub height_scale: i32,
    pub ocean_frequency: f64,
    pub ocean_threshold: f64,
    pub height_noise_frequency: f64,
    pub height_noise_octaves: i32,
    pub height_noise_lacunarity: f64,
    pub height_noise_gain: f64,
    pub river_width: f64,
    pub river_depth: i32,
    pub river_frequency: f64,
    pub temp_scale: f64,
    pub humid_scale: f64,
    pub mountain_frequency: f64,
    pub snow_line_mountains: i32,
    pub snow_line_taiga: i32,
    pub vegetation_density: f64,
    pub structure_density: f64,
    /// `< 0` = automatique.
    pub tree_density_overrides: [f64; 8],
    pub caves_enabled: bool,
    pub cave_threshold: f64,
    pub cavern_threshold: f64,
    pub cave_depth_max: i32,
    pub coal_chance: f64,
    pub coal_min_y: i32,
    pub coal_max_y: i32,
    pub iron_chance: f64,
    pub iron_min_y: i32,
    pub iron_max_y: i32,
    pub gold_chance: f64,
    pub gold_min_y: i32,
    pub gold_max_y: i32,
    pub diamond_chance: f64,
    pub diamond_min_y: i32,
    pub diamond_max_y: i32,
    pub emerald_chance: f64,
    pub emerald_min_y: i32,
    pub emerald_max_y: i32,
}

/// Préréglage d'un type de monde (`WorldConfig.preset`, fourni par nb-game).
pub type PresetFn<'a> = &'a dyn Fn(WorldType) -> WorldGenParams;

/// Action demandée.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldAction {
    /// Retour au menu principal.
    MainMenu,
    /// `loadAndPlay` d'un monde existant.
    Play(String),
    /// Création : `parseSeed(seed_text)`, `createWorldDirectory`, `loadAndPlay`. `params`
    /// s'applique par-dessus `preset(world_type)`.
    Create {
        name: String,
        seed_text: String,
        game_mode: String,
        allow_cheats: bool,
        world_type: WorldType,
        params: Box<WorldGenParams>,
    },
    /// `renameWorld` + mode de jeu / triche ; l'appelant rafraîchit la liste
    /// ([`WorldSelectScreen::set_worlds`]).
    Edit {
        old_name: String,
        new_name: String,
        game_mode: String,
        allow_cheats: bool,
    },
    /// `deleteWorld` ; l'appelant rafraîchit la liste.
    Delete(String),
    /// `cloneWorld(source, new_name)` puis `loadAndPlay`.
    Recreate { source: String, new_name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Create,
    Edit,
    Delete,
    Recreate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveInput {
    Name,
    Seed,
}

/// `freqToNorm`.
fn freq_to_norm(raw: f64, r0: f64, r1: f64) -> f32 {
    if (r1 - r0).abs() < 1e-12 {
        return 0.0;
    }
    (((raw - r0) / (r1 - r0)) as f32).clamp(0.0, 1.0)
}

const TREE_NAMES: [&str; 8] = [
    "Plains Trees",
    "Forest Trees",
    "Desert Trees",
    "Taiga Trees",
    "Jungle Trees",
    "Swamp Trees",
    "Savanna Trees",
    "Mountain Trees",
];

/// `initAdvRows` (en-têtes repliés par défaut).
pub fn adv_rows(c: &WorldGenParams) -> Vec<AdvRow> {
    let s = AdvRow::slider;
    let n = AdvRow::normalized;
    let h = |l| AdvRow::header(l, true);
    let mut rows = vec![
        h("Terrain"),
        s("Sea Level", 1.0, 80.0, c.sea_level as f32),
        s("Base Height", 1.0, 80.0, c.base_height as f32),
        s("Height Scale", 0.0, 100.0, c.height_scale as f32),
        n(
            "Continent Size: Small <-> Large",
            freq_to_norm(c.ocean_frequency, 0.003, 0.0004),
            0.003,
            0.0004,
        ),
        s("Ocean Coverage", 0.10, 0.70, c.ocean_threshold as f32),
        n(
            "Terrain Detail: Smooth <-> Sharp",
            freq_to_norm(c.height_noise_frequency, 0.003, 0.025),
            0.003,
            0.025,
        ),
        h("Noise"),
        s("Octaves", 1.0, 8.0, c.height_noise_octaves as f32),
        s("Lacunarity", 1.0, 3.0, c.height_noise_lacunarity as f32),
        s("Gain", 0.2, 0.7, c.height_noise_gain as f32),
        h("Water"),
        s("River Width", 0.0, 0.12, c.river_width as f32),
        s("River Depth", 0.0, 20.0, c.river_depth as f32),
        n(
            "River Frequency: Rare <-> Common",
            freq_to_norm(c.river_frequency, 0.0005, 0.008),
            0.0005,
            0.008,
        ),
        h("Biomes"),
        n(
            "Temperature Scale: Uniform <-> Varied",
            freq_to_norm(c.temp_scale, 0.0005, 0.008),
            0.0005,
            0.008,
        ),
        n(
            "Humidity Scale: Uniform <-> Varied",
            freq_to_norm(c.humid_scale, 0.0005, 0.008),
            0.0005,
            0.008,
        ),
        n(
            "Mountain Frequency: Rare <-> Common",
            freq_to_norm(c.mountain_frequency, 0.0008, 0.012),
            0.0008,
            0.012,
        ),
        h("Features"),
        s(
            "Snow Line (Mountains)",
            40.0,
            90.0,
            c.snow_line_mountains as f32,
        ),
        s("Snow Line (Taiga)", 30.0, 80.0, c.snow_line_taiga as f32),
        s("Vegetation Density", 0.0, 2.0, c.vegetation_density as f32),
        s("Structure Density", 0.0, 3.0, c.structure_density as f32),
    ];
    for (i, name) in TREE_NAMES.iter().enumerate() {
        let o = c.tree_density_overrides[i];
        let v = if o < 0.0 { -0.001 } else { o as f32 };
        rows.push(s(name, -0.001, 0.05, v));
    }
    rows.extend([
        h("Caves"),
        AdvRow::toggle("Cave Generation", c.caves_enabled),
        s("Cave Threshold", 0.2, 0.7, c.cave_threshold as f32),
        s("Cavern Threshold", 0.4, 0.9, c.cavern_threshold as f32),
        s("Cave Depth Max", 20.0, 120.0, c.cave_depth_max as f32),
        h("Ores"),
        s("Coal Chance", 0.001, 0.03, c.coal_chance as f32),
        s("Coal Min Y", 1.0, 40.0, c.coal_min_y as f32),
        s("Coal Max Y", 20.0, 120.0, c.coal_max_y as f32),
        s("Iron Chance", 0.001, 0.02, c.iron_chance as f32),
        s("Iron Min Y", 1.0, 40.0, c.iron_min_y as f32),
        s("Iron Max Y", 10.0, 100.0, c.iron_max_y as f32),
        s("Gold Chance", 0.0005, 0.01, c.gold_chance as f32),
        s("Gold Min Y", 1.0, 40.0, c.gold_min_y as f32),
        s("Gold Max Y", 5.0, 60.0, c.gold_max_y as f32),
        s("Diamond Chance", 0.0001, 0.005, c.diamond_chance as f32),
        s("Diamond Min Y", 1.0, 20.0, c.diamond_min_y as f32),
        s("Diamond Max Y", 5.0, 40.0, c.diamond_max_y as f32),
        s("Emerald Chance", 0.0001, 0.003, c.emerald_chance as f32),
        s("Emerald Min Y", 1.0, 20.0, c.emerald_min_y as f32),
        s("Emerald Max Y", 5.0, 50.0, c.emerald_max_y as f32),
    ]);
    rows
}

/// `applyAdvToConfig` : correspondance par libellé.
pub fn apply_adv(rows: &[AdvRow], c: &mut WorldGenParams) {
    for row in rows {
        match &row.kind {
            AdvKind::Slider { .. } => {
                let v = row.raw_value();
                let tree = |v: f64| if v < 0.0 { -1.0 } else { v };
                match row.label.as_str() {
                    "Sea Level" => c.sea_level = jround(v),
                    "Base Height" => c.base_height = jround(v),
                    "Height Scale" => c.height_scale = jround(v),
                    "Continent Size: Small <-> Large" => c.ocean_frequency = v,
                    "Ocean Coverage" => c.ocean_threshold = v,
                    "Terrain Detail: Smooth <-> Sharp" => c.height_noise_frequency = v,
                    "Octaves" => c.height_noise_octaves = jround(v),
                    "Lacunarity" => c.height_noise_lacunarity = v,
                    "Gain" => c.height_noise_gain = v,
                    "River Width" => c.river_width = v,
                    "River Depth" => c.river_depth = jround(v),
                    "River Frequency: Rare <-> Common" => c.river_frequency = v,
                    "Temperature Scale: Uniform <-> Varied" => c.temp_scale = v,
                    "Humidity Scale: Uniform <-> Varied" => c.humid_scale = v,
                    "Mountain Frequency: Rare <-> Common" => c.mountain_frequency = v,
                    "Snow Line (Mountains)" => c.snow_line_mountains = jround(v),
                    "Snow Line (Taiga)" => c.snow_line_taiga = jround(v),
                    "Vegetation Density" => c.vegetation_density = v,
                    "Structure Density" => c.structure_density = v,
                    "Cave Threshold" => c.cave_threshold = v,
                    "Cavern Threshold" => c.cavern_threshold = v,
                    "Cave Depth Max" => c.cave_depth_max = jround(v),
                    "Coal Chance" => c.coal_chance = v,
                    "Coal Min Y" => c.coal_min_y = jround(v),
                    "Coal Max Y" => c.coal_max_y = jround(v),
                    "Iron Chance" => c.iron_chance = v,
                    "Iron Min Y" => c.iron_min_y = jround(v),
                    "Iron Max Y" => c.iron_max_y = jround(v),
                    "Gold Chance" => c.gold_chance = v,
                    "Gold Min Y" => c.gold_min_y = jround(v),
                    "Gold Max Y" => c.gold_max_y = jround(v),
                    "Diamond Chance" => c.diamond_chance = v,
                    "Diamond Min Y" => c.diamond_min_y = jround(v),
                    "Diamond Max Y" => c.diamond_max_y = jround(v),
                    "Emerald Chance" => c.emerald_chance = v,
                    "Emerald Min Y" => c.emerald_min_y = jround(v),
                    "Emerald Max Y" => c.emerald_max_y = jround(v),
                    other => {
                        if let Some(i) = TREE_NAMES.iter().position(|n| *n == other) {
                            c.tree_density_overrides[i] = tree(v);
                        }
                    }
                }
            }
            AdvKind::Toggle(on) if row.label == "Cave Generation" => c.caves_enabled = *on,
            _ => {}
        }
    }
}

/// `AdvRow.valueDisplay()` de `WorldSelectState`.
pub fn value_display(row: &AdvRow) -> String {
    match &row.kind {
        AdvKind::Toggle(on) => if *on { "ON" } else { "OFF" }.into(),
        AdvKind::Slider { slider, norm } => {
            let raw = row.raw_value();
            let (min, max) = (slider.min, slider.max);
            if min >= 0.0
                && max >= 1.0
                && (max - min) >= 1.0
                && max <= 120.0
                && norm.is_none()
                && (raw == raw.floor() || (max - min) <= 100.0)
            {
                return jround(raw).to_string();
            }
            if raw.abs() < 0.001 && raw < 0.0 {
                return "Auto".into();
            }
            if raw.abs() < 0.1 {
                format_fixed(raw, 4)
            } else if raw.abs() < 1.0 {
                format_fixed(raw, 3)
            } else {
                format_fixed(raw, 1)
            }
        }
        _ => String::new(),
    }
}

/// `SimpleDateFormat("yyyy-MM-dd")` ([DÉVIATION] : en UTC, Java utilise le fuseau local).
pub fn format_date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // Algorithme « civil from days » (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// État de l'écran (conservé entre deux entrées, comme les champs de l'état Java).
#[derive(Clone, Debug)]
pub struct WorldSelectScreen {
    pub screen: Screen,
    pub worlds: Vec<WorldEntry>,
    pub selected: Option<usize>,
    scroll: Scroll,
    target: Option<WorldEntry>,
    cursor_blink: f64,
    typing_active: bool,
    mouse: MouseEdge,
    active_input: ActiveInput,
    pub create_game_mode: String,
    pub create_allow_cheats: bool,
    pub edit_game_mode: String,
    pub edit_allow_cheats: bool,
    pub create_world_type: WorldType,
    pub pending: WorldGenParams,
    pub advanced_expanded: bool,
    pub adv: AdvPanel,
    /// `new WorldConfig()` (valeurs remises à chaque « Create New World »).
    pub defaults: WorldGenParams,
    rng: JavaRandom,
    /// [JAVA-BUG] corrigé (décision de Gabriel, 2026-11-02) : vrai par défaut, le germe est
    /// lu avant `closeTyping`. Faux = comportement Java (`closeTyping` vide le tampon du
    /// germe **avant** sa lecture : germe saisi ignoré, toujours aléatoire).
    pub fix_seed_input: bool,
}

impl WorldSelectScreen {
    /// Écran neuf ; `defaults` = `new WorldConfig()`.
    pub fn new(defaults: WorldGenParams) -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        Self {
            screen: Screen::Main,
            worlds: Vec::new(),
            selected: None,
            scroll: Scroll::default(),
            target: None,
            cursor_blink: 0.0,
            typing_active: false,
            mouse: MouseEdge::default(),
            active_input: ActiveInput::Name,
            create_game_mode: "SURVIVAL".into(),
            create_allow_cheats: false,
            edit_game_mode: "CREATIVE".into(),
            edit_allow_cheats: false,
            create_world_type: WorldType::Normal,
            pending: defaults.clone(),
            defaults,
            advanced_expanded: false,
            adv: AdvPanel::default(),
            rng: JavaRandom::new(seed),
            fix_seed_input: true,
        }
    }

    /// `enter()` : écran MAIN, aucune sélection, liste rafraîchie.
    pub fn enter(&mut self, worlds: Vec<WorldEntry>) {
        self.screen = Screen::Main;
        self.selected = None;
        self.scroll.reset();
        self.set_worlds(worlds);
    }

    /// `refreshWorlds` : la sélection est gardée si elle reste dans la liste.
    pub fn set_worlds(&mut self, worlds: Vec<WorldEntry>) {
        self.worlds = worlds;
        if self.selected.is_some_and(|i| i >= self.worlds.len()) {
            self.selected = None;
        }
    }

    /// `exit()`.
    pub fn exit(&mut self, buffers: &mut TypingBuffers) {
        self.close_typing(buffers);
        self.adv.active_slider = None;
    }

    fn open_typing(&mut self, b: &mut TypingBuffers) {
        if !self.typing_active {
            self.typing_active = true;
            self.active_input = ActiveInput::Name;
            b.use_secondary = false;
            b.typing_mode = true;
            b.suppress_chat_filter = true;
        }
    }

    fn close_typing(&mut self, b: &mut TypingBuffers) {
        if self.typing_active {
            self.typing_active = false;
            b.typing_mode = false;
            b.suppress_chat_filter = false;
            b.use_secondary = false;
            b.secondary.clear();
        }
    }

    fn focus(&mut self, input: ActiveInput, b: &mut TypingBuffers) {
        self.active_input = input;
        b.use_secondary = input == ActiveInput::Seed;
    }

    fn init_adv_rows(&mut self) {
        let saved = self.adv.collapsed_headers();
        self.adv.rows = adv_rows(&self.pending);
        self.adv.restore_collapsed(&saved);
    }

    /// `isValidNewName`.
    fn valid_name(name: &str, current: Option<&str>, exists: &dyn Fn(&str) -> bool) -> bool {
        if name.is_empty() {
            return false;
        }
        if !exists(name) {
            return true;
        }
        current.is_some_and(|c| c == name)
    }

    pub fn update(
        &mut self,
        input: &UiInput,
        buffers: &mut TypingBuffers,
        name_exists: &dyn Fn(&str) -> bool,
        preset: PresetFn<'_>,
    ) -> Option<WorldAction> {
        self.cursor_blink = (self.cursor_blink + input.dt) % 1.0;
        let clicked = self.mouse.update(input);
        match self.screen {
            Screen::Main => self.update_main(input, clicked, buffers),
            Screen::Create => self.update_create(input, clicked, buffers, name_exists, preset),
            Screen::Edit => self.update_edit(input, clicked, buffers, name_exists),
            Screen::Delete => self.update_delete(input, clicked),
            Screen::Recreate => self.update_recreate(input, clicked, buffers, name_exists),
        }
    }

    fn update_main(
        &mut self,
        input: &UiInput,
        clicked: bool,
        buffers: &mut TypingBuffers,
    ) -> Option<WorldAction> {
        let (w, h) = (input.width, input.height);
        let list_h = h - SPACER_H - BOTTOM_H;
        let content = 8 + self.worlds.len() as i32 * (ROW_H + ROW_GAP) - ROW_GAP;
        self.scroll
            .apply(input.scroll_y, ROW_H + ROW_GAP, content, list_h);
        if !clicked {
            return None;
        }
        let (mx, my) = (input.mouse_x, input.mouse_y);
        let cx = (w - CONTENT_W) / 2;
        let row1 = h - BOTTOM_H + PAD_V;
        let row2 = row1 + BTN1_H + PAD_MID;
        let sel = self.selected;
        if let Some(i) = sel {
            if hit_test(mx, my, cx, row1, BIG_BTN_W, BTN1_H) {
                return Some(WorldAction::Play(self.worlds[i].name.clone()));
            }
        }
        if hit_test(mx, my, cx + BIG_BTN_W + GAP, row1, BIG_BTN_W, BTN1_H) {
            self.create_game_mode = "SURVIVAL".into();
            self.create_allow_cheats = false;
            self.create_world_type = WorldType::Normal;
            self.pending = self.defaults.clone();
            self.advanced_expanded = false;
            self.adv.scroll.offset = 0;
            self.adv.active_slider = None;
            self.init_adv_rows();
            buffers.command = "My World".into();
            buffers.secondary.clear();
            self.open_typing(buffers);
            self.screen = Screen::Create;
            return None;
        }
        if let Some(i) = sel {
            if hit_test(mx, my, cx, row2, SMALL_BTN_W, BTN2_H) {
                let t = self.worlds[i].clone();
                self.edit_game_mode = if t.game_mode.is_empty() {
                    "CREATIVE".into()
                } else {
                    t.game_mode.clone()
                };
                self.edit_allow_cheats = t.allow_cheats;
                buffers.command = t.name.clone();
                self.target = Some(t);
                self.open_typing(buffers);
                self.screen = Screen::Edit;
                return None;
            }
            if hit_test(mx, my, cx + SMALL_BTN_W + GAP, row2, SMALL_BTN_W, BTN2_H) {
                self.target = Some(self.worlds[i].clone());
                self.selected = None;
                self.screen = Screen::Delete;
                return None;
            }
            if hit_test(mx, my, cx + BIG_BTN_W + GAP, row2, SMALL_BTN_W, BTN2_H) {
                self.target = Some(self.worlds[i].clone());
                buffers.command.clear();
                self.open_typing(buffers);
                self.screen = Screen::Recreate;
                return None;
            }
        }
        let cancel_x = cx + BIG_BTN_W + GAP + SMALL_BTN_W + GAP;
        if hit_test(mx, my, cancel_x, row2, SMALL_BTN_W, BTN2_H) {
            return Some(WorldAction::MainMenu);
        }
        let list_bottom = h - BOTTOM_H;
        for i in 0..self.worlds.len() {
            let y = SPACER_H + 8 + i as i32 * (ROW_H + ROW_GAP) - self.scroll.offset;
            if y + ROW_H < SPACER_H || y > list_bottom {
                continue;
            }
            if hit_test(mx, my, cx, y, CONTENT_W, ROW_H) {
                self.selected = Some(i);
                return None;
            }
        }
        None
    }

    fn update_create(
        &mut self,
        input: &UiInput,
        clicked: bool,
        buffers: &mut TypingBuffers,
        exists: &dyn Fn(&str) -> bool,
        preset: PresetFn<'_>,
    ) -> Option<WorldAction> {
        let (w, h) = (input.width, input.height);
        let (mx, my) = (input.mouse_x, input.mouse_y);
        let cx = (w - CONTENT_W) / 2;
        if self.adv.drag(input, cx) {
            apply_adv(&self.adv.rows, &mut self.pending);
        }
        let panel_h = h - BOTTOM_H - ADV_PANEL_TOP;
        if input.scroll_y != 0.0 && self.advanced_expanded {
            self.adv.apply_scroll(input.scroll_y, panel_h);
        }
        if !clicked {
            return None;
        }
        let row1 = h - BOTTOM_H + PAD_V;
        let name = jtrim(&buffers.command).to_owned();
        let valid = Self::valid_name(&name, None, exists);

        if hit_test(mx, my, cx, NAME_INPUT_Y, CONTENT_W, 44) {
            self.focus(ActiveInput::Name, buffers);
            return None;
        }
        if hit_test(mx, my, cx, SEED_INPUT_Y, CONTENT_W - 110, 44) {
            self.focus(ActiveInput::Seed, buffers);
            return None;
        }
        if hit_test(mx, my, cx + CONTENT_W - 100, SEED_INPUT_Y, 100, 44) {
            buffers.secondary = (self.rng.next_long() & i64::MAX).to_string();
            self.focus(ActiveInput::Name, buffers);
            return None;
        }
        if hit_test(mx, my, cx + CONTENT_W - 170, GAME_MODE_ROW_Y, 170, 40) {
            self.create_game_mode = if self.create_game_mode == "SURVIVAL" {
                "CREATIVE".into()
            } else {
                "SURVIVAL".into()
            };
            return None;
        }
        if hit_test(mx, my, cx + CONTENT_W - 80, CHEATS_ROW_Y, 80, 40) {
            self.create_allow_cheats = !self.create_allow_cheats;
            return None;
        }
        if hit_test(mx, my, cx + CONTENT_W - 250, WORLD_TYPE_ROW_Y, 250, 50) {
            self.create_world_type = self.create_world_type.next();
            self.pending = preset(self.create_world_type);
            self.init_adv_rows();
            return None;
        }
        if hit_test(mx, my, cx, ADV_TOGGLE_Y, CONTENT_W, 36) {
            self.advanced_expanded = !self.advanced_expanded;
            self.adv.scroll.offset = 0;
            return None;
        }
        if self.advanced_expanded {
            match self.adv.click(mx, my, cx, ADV_PANEL_TOP, panel_h) {
                AdvClick::Outside => {}
                AdvClick::SliderGrabbed(_) | AdvClick::Toggled(_) => {
                    apply_adv(&self.adv.rows, &mut self.pending);
                    return None;
                }
                _ => return None,
            }
        }
        if valid && hit_test(mx, my, cx, row1, BIG_BTN_W, BTN1_H) {
            let seed_before = jtrim(&buffers.secondary).to_owned();
            self.close_typing(buffers);
            apply_adv(&self.adv.rows, &mut self.pending);
            let seed_text = if self.fix_seed_input {
                seed_before
            } else {
                jtrim(&buffers.secondary).to_owned()
            };
            return Some(WorldAction::Create {
                name,
                seed_text,
                game_mode: self.create_game_mode.clone(),
                allow_cheats: self.create_allow_cheats,
                world_type: self.create_world_type,
                params: Box::new(self.pending.clone()),
            });
        }
        if hit_test(mx, my, cx + BIG_BTN_W + GAP, row1, BIG_BTN_W, BTN1_H) {
            self.close_typing(buffers);
            self.screen = Screen::Main;
        }
        None
    }

    fn edit_rows(h: i32) -> (i32, i32, i32, i32) {
        let mid = SPACER_H + (h - SPACER_H - BOTTOM_H) / 3;
        let input_y = mid + 30;
        let gm = input_y + 44 + 32;
        (mid, input_y, gm, gm + 40 + 20)
    }

    fn update_edit(
        &mut self,
        input: &UiInput,
        clicked: bool,
        buffers: &mut TypingBuffers,
        exists: &dyn Fn(&str) -> bool,
    ) -> Option<WorldAction> {
        if !clicked {
            return None;
        }
        let (w, h) = (input.width, input.height);
        let (mx, my) = (input.mouse_x, input.mouse_y);
        let cx = (w - CONTENT_W) / 2;
        let row1 = h - BOTTOM_H + PAD_V;
        let (_, _, gm, cheats) = Self::edit_rows(h);
        if hit_test(mx, my, cx + CONTENT_W - 170, gm, 170, 40) {
            self.edit_game_mode = if self.edit_game_mode == "SURVIVAL" {
                "CREATIVE".into()
            } else {
                "SURVIVAL".into()
            };
            return None;
        }
        if hit_test(mx, my, cx + CONTENT_W - 80, cheats, 80, 40) {
            self.edit_allow_cheats = !self.edit_allow_cheats;
            return None;
        }
        let name = jtrim(&buffers.command).to_owned();
        let old = self
            .target
            .as_ref()
            .map(|t| t.name.clone())
            .unwrap_or_default();
        if Self::valid_name(&name, Some(&old), exists)
            && hit_test(mx, my, cx, row1, BIG_BTN_W, BTN1_H)
        {
            self.close_typing(buffers);
            self.screen = Screen::Main;
            return Some(WorldAction::Edit {
                old_name: old,
                new_name: name,
                game_mode: self.edit_game_mode.clone(),
                allow_cheats: self.edit_allow_cheats,
            });
        }
        if hit_test(mx, my, cx + BIG_BTN_W + GAP, row1, BIG_BTN_W, BTN1_H) {
            self.close_typing(buffers);
            self.screen = Screen::Main;
        }
        None
    }

    fn update_delete(&mut self, input: &UiInput, clicked: bool) -> Option<WorldAction> {
        if !clicked {
            return None;
        }
        let cx = (input.width - CONTENT_W) / 2;
        let row1 = input.height - BOTTOM_H + PAD_V;
        let (mx, my) = (input.mouse_x, input.mouse_y);
        if hit_test(mx, my, cx, row1, BIG_BTN_W, BTN1_H) {
            self.screen = Screen::Main;
            let name = self.target.take().map(|t| t.name).unwrap_or_default();
            return Some(WorldAction::Delete(name));
        }
        if hit_test(mx, my, cx + BIG_BTN_W + GAP, row1, BIG_BTN_W, BTN1_H) {
            self.screen = Screen::Main;
        }
        None
    }

    fn update_recreate(
        &mut self,
        input: &UiInput,
        clicked: bool,
        buffers: &mut TypingBuffers,
        exists: &dyn Fn(&str) -> bool,
    ) -> Option<WorldAction> {
        if !clicked {
            return None;
        }
        let cx = (input.width - CONTENT_W) / 2;
        let row1 = input.height - BOTTOM_H + PAD_V;
        let (mx, my) = (input.mouse_x, input.mouse_y);
        let name = jtrim(&buffers.command).to_owned();
        if Self::valid_name(&name, None, exists) && hit_test(mx, my, cx, row1, BIG_BTN_W, BTN1_H) {
            self.close_typing(buffers);
            let source = self
                .target
                .as_ref()
                .map(|t| t.name.clone())
                .unwrap_or_default();
            return Some(WorldAction::Recreate {
                source,
                new_name: name,
            });
        }
        if hit_test(mx, my, cx + BIG_BTN_W + GAP, row1, BIG_BTN_W, BTN1_H) {
            self.close_typing(buffers);
            self.screen = Screen::Main;
        }
        None
    }

    // ── Rendu ───────────────────────────────────────────────────────────────

    pub fn draw(
        &self,
        dl: &mut DrawList,
        input: &UiInput,
        buffers: &TypingBuffers,
        name_exists: &dyn Fn(&str) -> bool,
    ) {
        let (w, h) = (input.width, input.height);
        draw_background(dl, w, h, BOTTOM_H);
        match self.screen {
            Screen::Main => self.draw_main(dl, w, h),
            Screen::Create => self.draw_create(dl, w, h, buffers, name_exists),
            Screen::Edit => self.draw_edit(dl, w, h, buffers, name_exists),
            Screen::Delete => self.draw_delete(dl, w, h),
            Screen::Recreate => self.draw_recreate(dl, w, h, buffers, name_exists),
        }
    }

    fn title_y() -> f32 {
        (SPACER_H - 20) as f32 / 2.0
    }

    fn draw_main(&self, dl: &mut DrawList, w: i32, h: i32) {
        let cx = (w - CONTENT_W) / 2;
        dl.text(
            "Select World",
            w as f32 / 2.0 - 80.0,
            Self::title_y(),
            2.5,
            WHITE,
        );
        let list_top = SPACER_H;
        let list_bottom = h - BOTTOM_H;
        let list_h = list_bottom - list_top;
        dl.set_scissor(Some(Scissor::from_gl(0, BOTTOM_H, w, list_h, h)));
        if self.worlds.is_empty() {
            let s = 3.5;
            let msg = "No worlds yet. Create one!";
            let mw = font::measure_width(msg, s);
            dl.text(
                msg,
                w as f32 / 2.0 - mw / 2.0,
                list_top as f32 + list_h as f32 / 2.0 - 9.0 * s / 2.0,
                s,
                WHITE,
            );
        } else {
            for (i, m) in self.worlds.iter().enumerate() {
                let y = list_top + 8 + i as i32 * (ROW_H + ROW_GAP) - self.scroll.offset;
                if y + ROW_H < list_top || y > list_bottom {
                    continue;
                }
                let bg = if self.selected == Some(i) { 0.30 } else { 0.15 };
                dl.fill(
                    cx as f32,
                    y as f32,
                    CONTENT_W as f32,
                    ROW_H as f32,
                    [bg, bg, bg, 0.9],
                );
                dl.text(&m.name, (cx + 12) as f32, (y + 14) as f32, 2.5, WHITE);
                let date = format!("Last played: {}", format_date(m.last_played_at));
                dl.text(&date, (cx + 12) as f32, (y + 48) as f32, 1.8, WHITE);
            }
        }
        dl.set_scissor(None);
        let row1 = h - BOTTOM_H + PAD_V;
        let row2 = row1 + BTN1_H + PAD_MID;
        let s = self.selected.is_some();
        button(dl, cx, row1, BIG_BTN_W, BTN1_H, "Play Selected World", s);
        button(
            dl,
            cx + BIG_BTN_W + GAP,
            row1,
            BIG_BTN_W,
            BTN1_H,
            "Create New World",
            true,
        );
        button(dl, cx, row2, SMALL_BTN_W, BTN2_H, "Edit", s);
        button(
            dl,
            cx + SMALL_BTN_W + GAP,
            row2,
            SMALL_BTN_W,
            BTN2_H,
            "Delete",
            s,
        );
        button(
            dl,
            cx + BIG_BTN_W + GAP,
            row2,
            SMALL_BTN_W,
            BTN2_H,
            "Re-Create",
            s,
        );
        let cancel_x = cx + BIG_BTN_W + GAP + SMALL_BTN_W + GAP;
        button(dl, cancel_x, row2, SMALL_BTN_W, BTN2_H, "Cancel", true);
    }

    fn field(dl: &mut DrawList, x: i32, y: i32, w: i32, g: f32, text: &str) {
        dl.fill(x as f32, y as f32, w as f32, 44.0, gray(g));
        dl.text(text, (x + 8) as f32, (y + 13) as f32, 2.0, WHITE);
    }

    fn cursor(&self, on: bool) -> &'static str {
        if on && self.cursor_blink < 0.5 {
            "|"
        } else {
            ""
        }
    }

    fn draw_create(
        &self,
        dl: &mut DrawList,
        w: i32,
        h: i32,
        b: &TypingBuffers,
        exists: &dyn Fn(&str) -> bool,
    ) {
        let cx = (w - CONTENT_W) / 2;
        let row1 = h - BOTTOM_H + PAD_V;
        draw_title(dl, "Create New World", w);
        let t = |dl: &mut DrawList, s: &str, x: i32, y: i32| {
            dl.text(s, x as f32, y as f32, 2.0, WHITE);
        };
        t(dl, "World Name:", cx, FORM_Y);
        let name_on = self.active_input == ActiveInput::Name;
        let name_disp = format!("{}{}", b.command, self.cursor(name_on));
        Self::field(
            dl,
            cx,
            NAME_INPUT_Y,
            CONTENT_W,
            if name_on { 0.28 } else { 0.20 },
            &name_disp,
        );
        t(dl, "Seed (optional):", cx, SEED_LABEL_Y);
        let seed_on = self.active_input == ActiveInput::Seed;
        let seed_disp = if b.secondary.is_empty() && !seed_on {
            "Random".to_owned()
        } else {
            format!("{}{}", b.secondary, self.cursor(seed_on))
        };
        Self::field(
            dl,
            cx,
            SEED_INPUT_Y,
            CONTENT_W - 110,
            if seed_on { 0.28 } else { 0.20 },
            &seed_disp,
        );
        button(
            dl,
            cx + CONTENT_W - 100,
            SEED_INPUT_Y,
            100,
            44,
            "Random",
            true,
        );
        t(dl, "Game Mode:", cx, GAME_MODE_ROW_Y + 12);
        button(
            dl,
            cx + CONTENT_W - 170,
            GAME_MODE_ROW_Y,
            170,
            40,
            &self.create_game_mode,
            true,
        );
        t(dl, "Allow Cheats:", cx, CHEATS_ROW_Y + 12);
        let cheats = if self.create_allow_cheats {
            "ON"
        } else {
            "OFF"
        };
        button(dl, cx + CONTENT_W - 80, CHEATS_ROW_Y, 80, 40, cheats, true);
        t(dl, "World Type:", cx, WORLD_TYPE_ROW_Y + 16);
        let wt = format!("World Type: {}", self.create_world_type.display_name());
        button(
            dl,
            cx + CONTENT_W - 250,
            WORLD_TYPE_ROW_Y,
            250,
            50,
            &wt,
            true,
        );
        let adv = format!(
            "{}Advanced",
            if self.advanced_expanded { "v " } else { "> " }
        );
        button(dl, cx, ADV_TOGGLE_Y, CONTENT_W, 36, &adv, true);
        if self.advanced_expanded {
            let panel_h = h - BOTTOM_H - ADV_PANEL_TOP;
            if panel_h > 20 {
                self.adv.draw(dl, cx, ADV_PANEL_TOP, panel_h, value_display);
            }
        }
        let ok = Self::valid_name(jtrim(&b.command), None, exists);
        button(dl, cx, row1, BIG_BTN_W, BTN1_H, "Create New World", ok);
        button(
            dl,
            cx + BIG_BTN_W + GAP,
            row1,
            BIG_BTN_W,
            BTN1_H,
            "Cancel",
            true,
        );
    }

    fn draw_edit(
        &self,
        dl: &mut DrawList,
        w: i32,
        h: i32,
        b: &TypingBuffers,
        exists: &dyn Fn(&str) -> bool,
    ) {
        let cx = (w - CONTENT_W) / 2;
        let row1 = h - BOTTOM_H + PAD_V;
        dl.text(
            "Edit World",
            w as f32 / 2.0 - 60.0,
            Self::title_y(),
            2.5,
            WHITE,
        );
        let (mid, input_y, gm, cheats) = Self::edit_rows(h);
        dl.text("World Name:", cx as f32, mid as f32, 2.0, WHITE);
        Self::field(
            dl,
            cx,
            input_y,
            CONTENT_W,
            0.22,
            &format!("{}{}", b.command, self.cursor(true)),
        );
        dl.text("Game Mode:", cx as f32, (gm + 12) as f32, 2.0, WHITE);
        button(
            dl,
            cx + CONTENT_W - 170,
            gm,
            170,
            40,
            &self.edit_game_mode,
            true,
        );
        dl.text("Allow Cheats:", cx as f32, (cheats + 12) as f32, 2.0, WHITE);
        let c = if self.edit_allow_cheats { "ON" } else { "OFF" };
        button(dl, cx + CONTENT_W - 80, cheats, 80, 40, c, true);
        let old = self.target.as_ref().map(|t| t.name.as_str());
        let ok = Self::valid_name(jtrim(&b.command), old, exists);
        button(dl, cx, row1, BIG_BTN_W, BTN1_H, "Confirm", ok);
        button(
            dl,
            cx + BIG_BTN_W + GAP,
            row1,
            BIG_BTN_W,
            BTN1_H,
            "Cancel",
            true,
        );
    }

    /// `renderInputScreen("Re-Create World", "New World Name:", "Confirm", true)`.
    fn draw_recreate(
        &self,
        dl: &mut DrawList,
        w: i32,
        h: i32,
        b: &TypingBuffers,
        exists: &dyn Fn(&str) -> bool,
    ) {
        let cx = (w - CONTENT_W) / 2;
        let row1 = h - BOTTOM_H + PAD_V;
        let title = "Re-Create World";
        let tx = w as f32 / 2.0 - title.chars().count() as f32 * 6.0;
        dl.text(title, tx, Self::title_y(), 2.5, WHITE);
        let (mid, input_y, _, _) = Self::edit_rows(h);
        dl.text("New World Name:", cx as f32, mid as f32, 2.0, WHITE);
        Self::field(
            dl,
            cx,
            input_y,
            CONTENT_W,
            0.22,
            &format!("{}{}", b.command, self.cursor(true)),
        );
        let ok = Self::valid_name(jtrim(&b.command), None, exists);
        button(dl, cx, row1, BIG_BTN_W, BTN1_H, "Confirm", ok);
        button(
            dl,
            cx + BIG_BTN_W + GAP,
            row1,
            BIG_BTN_W,
            BTN1_H,
            "Cancel",
            true,
        );
    }

    fn draw_delete(&self, dl: &mut DrawList, w: i32, h: i32) {
        let cx = (w - CONTENT_W) / 2;
        let row1 = h - BOTTOM_H + PAD_V;
        dl.text(
            "Delete World",
            w as f32 / 2.0 - 60.0,
            Self::title_y(),
            2.5,
            WHITE,
        );
        let s = 3.5;
        let line_h = 9.0 * s;
        let total = line_h * 2.0 + 10.0;
        let mid = SPACER_H as f32 + (h - SPACER_H - BOTTOM_H) as f32 / 2.0;
        let y1 = mid - total / 2.0;
        let y2 = y1 + line_h + 10.0;
        let name = self.target.as_ref().map_or("?", |t| t.name.as_str());
        let l1 = format!("Are you sure you want to delete '{name}'?");
        let l2 = "This cannot be undone.";
        let half = w as f32 / 2.0;
        dl.text(&l1, half - font::measure_width(&l1, s) / 2.0, y1, s, WHITE);
        dl.text(l2, half - font::measure_width(l2, s) / 2.0, y2, s, YELLOW);
        button(dl, cx, row1, BIG_BTN_W, BTN1_H, "Confirm", true);
        button(
            dl,
            cx + BIG_BTN_W + GAP,
            row1,
            BIG_BTN_W,
            BTN1_H,
            "Cancel",
            true,
        );
    }
}

#[cfg(test)]
#[path = "world_select_tests.rs"]
mod tests;
