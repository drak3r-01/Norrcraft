//! Réglages des mobs (`MobsSettingsState`) : liste, édition, ajout, suppression,
//! réinitialisation. L'écran modifie directement le [`MobRegistry`] ; la sauvegarde
//! (`mobs.json`, drapeau « dirty »), le despawn et le rechargement des modèles sont faits par
//! nb-game à partir des [`MobAction`].

use nb_data::{MobBehaviorType, MobConfig, MobMedium, MobRegistry};

use super::common::{
    draw_background, draw_tabs, draw_title, tab_at, AdvClick, AdvKind, AdvPanel, AdvRow, KeyEdge,
    MouseEdge, Scroll, CONTENT_W, SPACER_H, TAB_GAP, TAB_H, TAB_W,
};
use crate::draw::{gray, DrawList, Scissor, WHITE, YELLOW};
use crate::font;
use crate::input::{TypingBuffers, UiInput};
use crate::jfmt::format_fixed;
use crate::state::{GameState, UiContext};
use crate::widgets::{button, hit_test};

const BOTTOM_H: i32 = 130;
const ROW_H: i32 = 80;
const ROW_GAP: i32 = 8;
const GAP: i32 = 10;
const BTN1_H: i32 = 50;
const PAD_V: i32 = 12;
const PAD_MID: i32 = 8;
const TAB_BAR_H: i32 = TAB_H + 24;
const LIST_TOP: i32 = SPACER_H + TAB_BAR_H;
const RESET_BTN_W: i32 = TAB_W * 2 + TAB_GAP;
const RESET_AREA_H: i32 = TAB_H + 16;
const CONTENT_TOP: i32 = LIST_TOP + RESET_AREA_H;
const BIG_BTN_W: i32 = (CONTENT_W - GAP) / 2;
const EDIT_PANEL_TOP: i32 = SPACER_H + 8 + 24 + 44 + 8;
const ADD_PANEL_TOP: i32 = SPACER_H + 8 + 24 + 44 + 8 + 24 + 44 + 8;
const NAME_INPUT_Y: i32 = SPACER_H + 8 + 24;
const ADD_NAME_INPUT_Y: i32 = NAME_INPUT_Y + 44 + 8 + 24;

/// Action demandée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MobAction {
    /// Onglets Graphics / Controls / Res. Packs.
    SwitchTo(GameState),
    /// `applyAndLeave` : recharger les modèles absents, despawn si « dirty », puis SETTINGS.
    Leave,
    /// Registre modifié (édition ou suppression) : `MobRegistry.save()`.
    Saved,
    /// Mob ajouté : `save()` puis `reloadMobModel(id)`.
    Added(String),
    /// Registre remis aux valeurs intégrées : `save()`, despawn de tous les mobs, `clearDirty`.
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Edit,
    Add,
    Delete,
    ResetConfirm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveInput {
    Id,
    Name,
}

fn behavior_name(b: MobBehaviorType) -> &'static str {
    match b {
        MobBehaviorType::Passive => "PASSIVE",
        MobBehaviorType::Aggressive => "AGGRESSIVE",
    }
}

fn medium_name(m: MobMedium) -> &'static str {
    match m {
        MobMedium::Land => "LAND",
        MobMedium::Water => "WATER",
        MobMedium::Air => "AIR",
    }
}

/// `String.trim()` Java.
fn jtrim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// `Math.round(float)`.
fn jround(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}

/// `initAdvRows` (en-têtes dépliés).
pub fn adv_rows(c: &MobConfig) -> Vec<AdvRow> {
    let s = AdvRow::slider;
    let h = |l| AdvRow::header(l, false);
    let mut rows = vec![
        h("Physical"),
        s("Max Health", 1.0, 100.0, c.max_health),
        s("Width X", 0.1, 5.0, c.width_x),
        s("Width Z", 0.1, 10.0, c.width_z),
        s("Height", 0.1, 10.0, c.height),
        h("Spawn"),
        s("Max Count", 0.0, 64.0, c.max_count as f32),
        s("Group Min", 1.0, 16.0, c.group_size_min as f32),
        s("Group Max", 1.0, 16.0, c.group_size_max as f32),
        s("Scatter Radius", 0.0, 30.0, c.spawn_scatter_radius as f32),
        h("Medium"),
        AdvRow::cycle(
            "Medium",
            &["LAND", "WATER"],
            usize::from(c.medium == MobMedium::Water),
        ),
    ];
    if c.medium == MobMedium::Water {
        rows.push(s(
            "Spawn Water Depth",
            1.0,
            20.0,
            c.spawn_min_water_depth as f32,
        ));
    }
    rows.extend([
        h("Movement"),
        s("Speed Min", 0.1, 10.0, c.move_speed_min),
        s("Speed Max", 0.1, 10.0, c.move_speed_max),
        s("Move Dur. Min", 0.1, 20.0, c.move_duration_min),
        s("Move Dur. Max", 0.1, 20.0, c.move_duration_max),
        s("Pause Dur. Min", 0.0, 20.0, c.pause_duration_min),
        s("Pause Dur. Max", 0.0, 20.0, c.pause_duration_max),
        h("Behavior"),
        AdvRow::cycle(
            "Behavior Type",
            &["PASSIVE", "AGGRESSIVE"],
            usize::from(c.behavior_type == MobBehaviorType::Aggressive),
        ),
    ]);
    if c.behavior_type != MobBehaviorType::Aggressive {
        let radius = if c.herd_radius > 0.0 {
            c.herd_radius
        } else {
            10.0
        };
        let bias = if c.herd_bias > 0.0 { c.herd_bias } else { 0.55 };
        rows.extend([
            h("Herding"),
            AdvRow::toggle("Herding", c.herding),
            s("Herd Radius", 1.0, 50.0, radius),
            s("Herd Bias", 0.0, 1.0, bias),
            h("Passive Avoidance"),
            s("Aggressor Range", 0.0, 100.0, c.aggressor_detect_range),
            s("Aggressor Flee", 0.0, 15.0, c.aggressor_flee_speed),
            s("Player Flee Speed", 0.0, 15.0, c.player_flee_speed),
            s("Flee Dur. Min", 0.0, 15.0, c.player_flee_duration_min),
            s("Flee Dur. Max", 0.0, 15.0, c.player_flee_duration_max),
        ]);
    } else {
        rows.extend([
            h("Aggressive Combat"),
            s("Aggro Range", 0.0, 500.0, c.aggro_player_range),
            s("Chase Speed", 0.0, 15.0, c.chase_speed),
            s("Attack Range", 0.0, 20.0, c.attack_range),
            s("Attack Damage", 0.0, 50.0, c.attack_damage),
            s("Attack Cooldown", 0.1, 15.0, c.attack_cooldown),
            s("Knockback Force", 0.0, 30.0, c.knockback_force),
        ]);
    }
    rows
}

/// `applyToConfig` : correspondance par libellé.
pub fn apply_to_config(rows: &[AdvRow], c: &mut MobConfig) {
    for row in rows {
        match &row.kind {
            AdvKind::Slider { slider, .. } => {
                let v = slider.value;
                match row.label.as_str() {
                    "Max Health" => c.max_health = v,
                    "Width X" => c.width_x = v,
                    "Width Z" => c.width_z = v,
                    "Height" => c.height = v,
                    "Max Count" => c.max_count = jround(v),
                    "Group Min" => c.group_size_min = jround(v),
                    "Group Max" => c.group_size_max = jround(v),
                    "Scatter Radius" => c.spawn_scatter_radius = jround(v),
                    "Spawn Water Depth" => c.spawn_min_water_depth = jround(v),
                    "Speed Min" => c.move_speed_min = v,
                    "Speed Max" => c.move_speed_max = v,
                    "Move Dur. Min" => c.move_duration_min = v,
                    "Move Dur. Max" => c.move_duration_max = v,
                    "Pause Dur. Min" => c.pause_duration_min = v,
                    "Pause Dur. Max" => c.pause_duration_max = v,
                    "Herd Radius" => c.herd_radius = v,
                    "Herd Bias" => c.herd_bias = v,
                    "Aggressor Range" => c.aggressor_detect_range = v,
                    "Aggressor Flee" => c.aggressor_flee_speed = v,
                    "Player Flee Speed" => c.player_flee_speed = v,
                    "Flee Dur. Min" => c.player_flee_duration_min = v,
                    "Flee Dur. Max" => c.player_flee_duration_max = v,
                    "Aggro Range" => c.aggro_player_range = v,
                    "Chase Speed" => c.chase_speed = v,
                    "Attack Range" => c.attack_range = v,
                    "Attack Damage" => c.attack_damage = v,
                    "Attack Cooldown" => c.attack_cooldown = v,
                    "Knockback Force" => c.knockback_force = v,
                    _ => {}
                }
            }
            AdvKind::Toggle(on) if row.label == "Herding" => c.herding = *on,
            AdvKind::Cycle { options, index } => {
                let opt = options[*index];
                if row.label == "Medium" {
                    c.medium = if opt == "WATER" {
                        MobMedium::Water
                    } else {
                        MobMedium::Land
                    };
                } else if row.label == "Behavior Type" {
                    let new = if opt == "AGGRESSIVE" {
                        MobBehaviorType::Aggressive
                    } else {
                        MobBehaviorType::Passive
                    };
                    if new == MobBehaviorType::Aggressive
                        && c.behavior_type != MobBehaviorType::Aggressive
                        && c.chase_speed == 0.0
                        && c.attack_range == 0.0
                    {
                        c.aggro_player_range = 16.0;
                        c.chase_speed = 2.0;
                        c.attack_range = 2.0;
                        c.attack_damage = 2.0;
                        c.attack_cooldown = 1.5;
                        c.knockback_force = 5.0;
                    }
                    c.behavior_type = new;
                }
            }
            _ => {}
        }
    }
}

/// `valueDisplay()` de `MobsSettingsState`.
pub fn value_display(row: &AdvRow) -> String {
    match &row.kind {
        AdvKind::Cycle { options, index } => options[*index].into(),
        AdvKind::Toggle(on) => if *on { "ON" } else { "OFF" }.into(),
        AdvKind::Slider { slider, .. } => {
            let v = slider.value;
            let r = (v + 0.5).floor() as i64;
            if (v - r as f32).abs() < 0.01 {
                r.to_string()
            } else if v.abs() < 1.0 {
                format_fixed(v as f64, 3)
            } else {
                format_fixed(v as f64, 1)
            }
        }
        AdvKind::Header { .. } => String::new(),
    }
}

/// `copyFields(src, dst)` : tout sauf l'identifiant et la géométrie.
fn copy_fields(src: &MobConfig, dst: &mut MobConfig) {
    let (id, geometry) = (std::mem::take(&mut dst.id), dst.geometry.take());
    *dst = src.clone();
    dst.id = id;
    dst.geometry = geometry;
}

/// État de l'écran.
#[derive(Clone, Debug, PartialEq)]
pub struct MobSettingsScreen {
    pub screen: Screen,
    pub selected: Option<usize>,
    scroll: Scroll,
    pub editing: Option<MobConfig>,
    target_id: Option<String>,
    active_input: ActiveInput,
    typing_active: bool,
    cursor_blink: f64,
    mouse: MouseEdge,
    esc: KeyEdge,
    pub adv: AdvPanel,
}

impl Default for MobSettingsScreen {
    fn default() -> Self {
        Self {
            screen: Screen::Main,
            selected: None,
            scroll: Scroll::default(),
            editing: None,
            target_id: None,
            active_input: ActiveInput::Id,
            typing_active: false,
            cursor_blink: 0.0,
            mouse: MouseEdge::default(),
            esc: KeyEdge::default(),
            adv: AdvPanel::default(),
        }
    }
}

impl MobSettingsScreen {
    /// `enter()`.
    pub fn enter(&mut self) {
        self.screen = Screen::Main;
        self.selected = None;
        self.scroll.reset();
        self.mouse = MouseEdge::default();
        self.cursor_blink = 0.0;
    }

    /// `exit()`.
    pub fn exit(&mut self, b: &mut TypingBuffers) {
        self.close_typing(b);
        self.adv.active_slider = None;
    }

    fn open_typing(&mut self, b: &mut TypingBuffers) {
        if !self.typing_active {
            self.typing_active = true;
            b.typing_mode = true;
            b.suppress_chat_filter = true;
            b.use_secondary = false;
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

    fn init_adv(&mut self) {
        self.adv.rows = self.editing.as_ref().map(adv_rows).unwrap_or_default();
        self.adv.scroll.reset();
        self.adv.active_slider = None;
    }

    fn apply(&mut self) {
        if let Some(c) = &mut self.editing {
            apply_to_config(&self.adv.rows, c);
        }
    }

    /// `rebuildAdvRows` : applique, reconstruit, garde les en-têtes repliés.
    fn rebuild(&mut self) {
        let saved = self.adv.collapsed_headers();
        self.apply();
        self.adv.rows = self.editing.as_ref().map(adv_rows).unwrap_or_default();
        self.adv.restore_collapsed(&saved);
    }

    /// `isValidNewId`.
    fn valid_id(id: &str, reg: &MobRegistry) -> bool {
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && !reg.id_exists(id)
    }

    pub fn update(
        &mut self,
        input: &UiInput,
        reg: &mut MobRegistry,
        buffers: &mut TypingBuffers,
        ctx: &mut UiContext,
    ) -> Option<MobAction> {
        self.cursor_blink = (self.cursor_blink + input.dt) % 1.0;
        let clicked = self.mouse.update(input);
        if self.esc.update(input.esc_down) {
            match self.screen {
                Screen::Main => return Some(MobAction::Leave),
                Screen::Edit | Screen::Add => {
                    self.close_typing(buffers);
                    self.screen = Screen::Main;
                    return None;
                }
                _ => {
                    self.screen = Screen::Main;
                    return None;
                }
            }
        }
        match self.screen {
            Screen::Main => self.update_main(input, clicked, reg, buffers, ctx),
            Screen::Edit | Screen::Add => self.update_form(input, clicked, reg, buffers),
            Screen::Delete | Screen::ResetConfirm => self.update_confirm(input, clicked, reg),
        }
    }

    fn update_main(
        &mut self,
        input: &UiInput,
        clicked: bool,
        reg: &mut MobRegistry,
        b: &mut TypingBuffers,
        ctx: &mut UiContext,
    ) -> Option<MobAction> {
        let (w, h) = (input.width, input.height);
        let list_h = h - CONTENT_TOP - BOTTOM_H;
        let content = 8 + reg.all().len() as i32 * (ROW_H + ROW_GAP) - ROW_GAP;
        self.scroll
            .apply(input.scroll_y, ROW_H + ROW_GAP, content, list_h);
        if !clicked {
            return None;
        }
        let (mx, my) = (input.mouse_x, input.mouse_y);
        match tab_at(w, mx, my, TAB_H) {
            Some(0) => return Some(MobAction::SwitchTo(GameState::Settings)),
            Some(1) => {
                ctx.pending_controls_tab = true;
                return Some(MobAction::SwitchTo(GameState::Settings));
            }
            Some(2) => return Some(MobAction::SwitchTo(GameState::ResourcePacks)),
            Some(4) => {
                ctx.pending_audio_tab = true;
                return Some(MobAction::SwitchTo(GameState::Settings));
            }
            _ => {}
        }
        let cx = (w - CONTENT_W) / 2;
        let row1 = h - BOTTOM_H + PAD_V;
        let row2 = row1 + BTN1_H + PAD_MID;
        if hit_test(
            mx,
            my,
            w / 2 - RESET_BTN_W / 2,
            LIST_TOP + 8,
            RESET_BTN_W,
            TAB_H,
        ) {
            self.screen = Screen::ResetConfirm;
            return None;
        }
        if let Some(i) = self.selected {
            if hit_test(mx, my, cx, row1, BIG_BTN_W, BTN1_H) {
                let cfg = reg.all()[i].clone();
                b.command = cfg.display_name.clone();
                self.target_id = Some(cfg.id.clone());
                self.editing = Some(cfg);
                self.init_adv();
                self.open_typing(b);
                self.active_input = ActiveInput::Name;
                b.use_secondary = false;
                self.screen = Screen::Edit;
                return None;
            }
        }
        if hit_test(mx, my, cx + BIG_BTN_W + GAP, row1, BIG_BTN_W, BTN1_H) {
            let mut cfg = MobRegistry::builtin().all()[0].clone();
            cfg.id.clear();
            cfg.display_name.clear();
            cfg.aggressor_detect_range = 10.0;
            cfg.aggressor_flee_speed = 3.0;
            self.editing = Some(cfg);
            self.target_id = None;
            self.init_adv();
            b.command.clear();
            b.secondary.clear();
            self.open_typing(b);
            self.active_input = ActiveInput::Id;
            b.use_secondary = false;
            self.screen = Screen::Add;
            return None;
        }
        if let Some(i) = self.selected {
            if hit_test(mx, my, cx, row2, BIG_BTN_W, BTN1_H) {
                self.target_id = Some(reg.all()[i].id.clone());
                self.selected = None;
                self.screen = Screen::Delete;
                return None;
            }
        }
        if hit_test(mx, my, cx + BIG_BTN_W + GAP, row2, BIG_BTN_W, BTN1_H) {
            return Some(MobAction::Leave);
        }
        let list_bottom = h - BOTTOM_H;
        for i in 0..reg.all().len() {
            let y = CONTENT_TOP + 8 + i as i32 * (ROW_H + ROW_GAP) - self.scroll.offset;
            if y + ROW_H < CONTENT_TOP || y > list_bottom {
                continue;
            }
            if hit_test(mx, my, cx, y, CONTENT_W, ROW_H) {
                self.selected = Some(i);
                return None;
            }
        }
        None
    }

    fn update_form(
        &mut self,
        input: &UiInput,
        clicked: bool,
        reg: &mut MobRegistry,
        b: &mut TypingBuffers,
    ) -> Option<MobAction> {
        let (w, h) = (input.width, input.height);
        let (mx, my) = (input.mouse_x, input.mouse_y);
        let cx = (w - CONTENT_W) / 2;
        let add = self.screen == Screen::Add;
        self.adv.drag(input, cx);
        let top = if add { ADD_PANEL_TOP } else { EDIT_PANEL_TOP };
        let panel_h = h - BOTTOM_H - top;
        self.adv.apply_scroll(input.scroll_y, panel_h);
        if !clicked {
            return None;
        }
        let row1 = h - BOTTOM_H + PAD_V;
        if hit_test(mx, my, cx, NAME_INPUT_Y, CONTENT_W, 44) {
            // Édition : champ « Display Name » ; ajout : champ « Mob ID ».
            self.active_input = if add {
                ActiveInput::Id
            } else {
                ActiveInput::Name
            };
            b.use_secondary = false;
            return None;
        }
        if add && hit_test(mx, my, cx, ADD_NAME_INPUT_Y, CONTENT_W, 44) {
            self.active_input = ActiveInput::Name;
            b.use_secondary = true;
            return None;
        }
        if panel_h > 0 && hit_test(mx, my, cx, top, CONTENT_W, panel_h) {
            match self.adv.click(mx, my, cx, top, panel_h) {
                AdvClick::Toggled(_) => self.apply(),
                AdvClick::Cycled(_) => {
                    self.apply();
                    self.rebuild();
                }
                _ => {}
            }
            return None;
        }
        if add {
            let id = jtrim(&b.command).to_owned();
            let name = jtrim(&b.secondary).to_owned();
            if Self::valid_id(&id, reg)
                && !name.is_empty()
                && hit_test(mx, my, cx, row1, BIG_BTN_W, BTN1_H)
            {
                self.apply();
                let mut cfg = self.editing.take().expect("mob en cours d'ajout");
                cfg.id = id.clone();
                cfg.display_name = name;
                reg.all_mut().push(cfg);
                self.close_typing(b);
                self.screen = Screen::Main;
                return Some(MobAction::Added(id));
            }
        } else {
            let name = jtrim(&b.command).to_owned();
            if !name.is_empty() && hit_test(mx, my, cx, row1, BIG_BTN_W, BTN1_H) {
                self.apply();
                let mut action = None;
                if let Some(mut cfg) = self.editing.take() {
                    cfg.display_name = name;
                    let id = self.target_id.clone().unwrap_or_default();
                    if let Some(t) = reg.all_mut().iter_mut().find(|c| c.id == id) {
                        copy_fields(&cfg, t);
                    }
                    action = Some(MobAction::Saved);
                }
                self.close_typing(b);
                self.screen = Screen::Main;
                return action;
            }
        }
        if hit_test(mx, my, cx + BIG_BTN_W + GAP, row1, BIG_BTN_W, BTN1_H) {
            self.close_typing(b);
            self.screen = Screen::Main;
        }
        None
    }

    fn update_confirm(
        &mut self,
        input: &UiInput,
        clicked: bool,
        reg: &mut MobRegistry,
    ) -> Option<MobAction> {
        if !clicked {
            return None;
        }
        let cx = (input.width - CONTENT_W) / 2;
        let row1 = input.height - BOTTOM_H + PAD_V;
        let (mx, my) = (input.mouse_x, input.mouse_y);
        if hit_test(mx, my, cx, row1, BIG_BTN_W, BTN1_H) {
            let action = if self.screen == Screen::Delete {
                let id = self.target_id.take().unwrap_or_default();
                reg.all_mut().retain(|c| c.id != id);
                MobAction::Saved
            } else {
                reg.reset_to_defaults();
                MobAction::Reset
            };
            self.screen = Screen::Main;
            return Some(action);
        }
        if hit_test(mx, my, cx + BIG_BTN_W + GAP, row1, BIG_BTN_W, BTN1_H) {
            self.screen = Screen::Main;
        }
        None
    }

    // ── Rendu ───────────────────────────────────────────────────────────────

    pub fn draw(&self, dl: &mut DrawList, input: &UiInput, reg: &MobRegistry, b: &TypingBuffers) {
        let (w, h) = (input.width, input.height);
        draw_background(dl, w, h, BOTTOM_H);
        let cx = (w - CONTENT_W) / 2;
        let row1 = h - BOTTOM_H + PAD_V;
        let confirm_cancel = |dl: &mut DrawList, ok_label: &str, ok: bool| {
            button(dl, cx, row1, BIG_BTN_W, BTN1_H, ok_label, ok);
            button(
                dl,
                cx + BIG_BTN_W + GAP,
                row1,
                BIG_BTN_W,
                BTN1_H,
                "Cancel",
                true,
            );
        };
        match self.screen {
            Screen::Main => self.draw_main(dl, w, h, reg),
            Screen::Edit => {
                draw_title(dl, "Edit Mob", w);
                dl.text(
                    "Display Name:",
                    cx as f32,
                    (SPACER_H + 8) as f32,
                    2.0,
                    WHITE,
                );
                let text = format!("{}{}", b.command, self.cursor(true));
                field(dl, cx, NAME_INPUT_Y, 0.22, &text);
                let panel_h = h - BOTTOM_H - EDIT_PANEL_TOP;
                if panel_h > 20 {
                    self.adv
                        .draw(dl, cx, EDIT_PANEL_TOP, panel_h, value_display);
                }
                confirm_cancel(dl, "Save", !jtrim(&b.command).is_empty());
            }
            Screen::Add => {
                draw_title(dl, "Add New Mob", w);
                let label = |dl: &mut DrawList, t: &str, y: i32| {
                    dl.text(t, cx as f32, y as f32, 2.0, WHITE);
                };
                label(dl, "Mob ID (must match .jem filename):", SPACER_H + 8);
                let id_ok = Self::valid_id(jtrim(&b.command), reg);
                let id_on = self.active_input == ActiveInput::Id;
                let id_fg = if id_on {
                    0.28
                } else if !b.command.is_empty() && !id_ok {
                    0.35
                } else {
                    0.20
                };
                let text = format!("{}{}", b.command, self.cursor(id_on));
                field(dl, cx, NAME_INPUT_Y, id_fg, &text);
                label(dl, "Display Name:", NAME_INPUT_Y + 44 + 8);
                let name_on = self.active_input == ActiveInput::Name;
                let text = format!("{}{}", b.secondary, self.cursor(name_on));
                field(
                    dl,
                    cx,
                    ADD_NAME_INPUT_Y,
                    if name_on { 0.28 } else { 0.20 },
                    &text,
                );
                let panel_h = h - BOTTOM_H - ADD_PANEL_TOP;
                if panel_h > 20 {
                    self.adv.draw(dl, cx, ADD_PANEL_TOP, panel_h, value_display);
                }
                confirm_cancel(dl, "Add", id_ok && !jtrim(&b.secondary).is_empty());
            }
            Screen::Delete => {
                draw_title(dl, "Delete Mob", w);
                let id = self.target_id.as_deref().unwrap_or("null");
                let name = reg.get(id).map_or(id, |m| m.display_name.as_str());
                let l1 = format!("Are you sure you want to delete '{name}'?");
                let l2 = if MobRegistry::is_built_in(id) {
                    format!("Re-add with ID \"{id}\" to restore defaults.")
                } else {
                    "This cannot be undone.".to_owned()
                };
                two_lines(dl, w, h, &l1, &l2);
                confirm_cancel(dl, "Confirm", true);
            }
            Screen::ResetConfirm => {
                draw_title(dl, "Reset to Defaults", w);
                two_lines(
                    dl,
                    w,
                    h,
                    "Reset all mobs to built-in defaults?",
                    "Custom mobs will be removed.",
                );
                confirm_cancel(dl, "Confirm", true);
            }
        }
    }

    fn cursor(&self, on: bool) -> &'static str {
        if on && self.cursor_blink < 0.5 {
            "|"
        } else {
            ""
        }
    }

    fn draw_main(&self, dl: &mut DrawList, w: i32, h: i32, reg: &MobRegistry) {
        let cx = (w - CONTENT_W) / 2;
        draw_title(dl, "Mobs", w);
        draw_tabs(dl, w, TAB_H, Some(3));
        button(
            dl,
            w / 2 - RESET_BTN_W / 2,
            LIST_TOP + 8,
            RESET_BTN_W,
            TAB_H,
            "Reset to Defaults",
            true,
        );
        let list_bottom = h - BOTTOM_H;
        let list_h = list_bottom - CONTENT_TOP;
        dl.set_scissor(Some(Scissor::from_gl(0, BOTTOM_H, w, list_h, h)));
        if reg.all().is_empty() {
            let msg = "No mobs. Add one!";
            let mw = font::measure_width(msg, 3.5);
            dl.text(
                msg,
                w as f32 / 2.0 - mw / 2.0,
                CONTENT_TOP as f32 + list_h as f32 / 2.0 - 16.0,
                3.5,
                WHITE,
            );
        }
        for (i, m) in reg.all().iter().enumerate() {
            let y = CONTENT_TOP + 8 + i as i32 * (ROW_H + ROW_GAP) - self.scroll.offset;
            if y + ROW_H < CONTENT_TOP || y > list_bottom {
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
            dl.text(
                &m.display_name,
                (cx + 12) as f32,
                (y + 14) as f32,
                2.5,
                WHITE,
            );
            let sub = format!(
                "ID: {}  |  {}  |  {}  |  Health: {}",
                m.id,
                behavior_name(m.behavior_type),
                medium_name(m.medium),
                m.max_health as i32
            );
            dl.text(
                &sub,
                (cx + 12) as f32,
                (y + 50) as f32,
                1.8,
                [0.7, 0.7, 0.7, 1.0],
            );
        }
        dl.set_scissor(None);
        let row1 = h - BOTTOM_H + PAD_V;
        let row2 = row1 + BTN1_H + PAD_MID;
        let sel = self.selected.is_some();
        let x2 = cx + BIG_BTN_W + GAP;
        button(dl, cx, row1, BIG_BTN_W, BTN1_H, "Edit", sel);
        button(dl, x2, row1, BIG_BTN_W, BTN1_H, "Add New Mob", true);
        button(dl, cx, row2, BIG_BTN_W, BTN1_H, "Delete", sel);
        button(dl, x2, row2, BIG_BTN_W, BTN1_H, "Apply", true);
    }
}

fn field(dl: &mut DrawList, x: i32, y: i32, g: f32, text: &str) {
    dl.fill(x as f32, y as f32, CONTENT_W as f32, 44.0, gray(g));
    dl.text(text, (x + 8) as f32, (y + 13) as f32, 2.0, WHITE);
}

/// Deux lignes centrées (échelles 3 et 2,5, la seconde en jaune).
fn two_lines(dl: &mut DrawList, w: i32, h: i32, l1: &str, l2: &str) {
    let (s1, s2) = (3.0, 2.5);
    let total = 9.0 * s1 + 10.0 + 9.0 * s2;
    let mid = SPACER_H as f32 + (h - SPACER_H - BOTTOM_H) as f32 / 2.0 - total / 2.0;
    let half = w as f32 / 2.0;
    dl.text(l1, half - font::measure_width(l1, s1) / 2.0, mid, s1, WHITE);
    dl.text(
        l2,
        half - font::measure_width(l2, s2) / 2.0,
        mid + 9.0 * s1 + 10.0,
        s2,
        YELLOW,
    );
}

#[cfg(test)]
#[path = "mobs_tests.rs"]
mod tests;
