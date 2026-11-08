//! Réglages (`SettingsState`) : onglets Graphics / Controls, copie de travail des réglages,
//! curseurs, bascules, réaffectation des touches.

use nb_data::keys::{key_name, KEY_ESCAPE};
use nb_data::{Action, GameSettings, ShadowQuality};

use super::common::{
    draw_background, draw_scrollbar, draw_tabs, tab_at, KeyEdge, MouseEdge, Scroll, SPACER_H,
};
use crate::draw::{DrawList, WHITE};
use crate::input::UiInput;
use crate::jfmt::format_fixed;
use crate::state::{GameState, UiContext};
use crate::widgets::{button, centered_text, hit_test, Slider};

const BOTTOM_H: i32 = 80;
const CONTENT_W: i32 = 720;
const ROW_H: i32 = 44;
const ROW_GAP: i32 = 12;
const LABEL_W: i32 = 210;
const CTRL_W: i32 = 280;
const SLIDER_H: i32 = 32;
const BTN_H: i32 = 36;
const TAB_W: i32 = 140;
const TAB_GAP: i32 = 8;
const RESET_BTN_W: i32 = TAB_W * 2 + TAB_GAP;
const RESET_AREA_H: i32 = BTN_H + 16;
const LIST_TOP: i32 = SPACER_H + BTN_H + 24;
const CONTENT_TOP: i32 = LIST_TOP + RESET_AREA_H;

/// Résolutions proposées en fenêtré.
pub const RESOLUTIONS: [(i32, i32); 3] = [(1280, 720), (1600, 900), (1920, 1080)];

/// Actions réaffectables, dans l'ordre d'affichage.
pub const REBINDABLE: [Action; 9] = [
    Action::Forward,
    Action::Backward,
    Action::Left,
    Action::Right,
    Action::Jump,
    Action::Sprint,
    Action::Crouch,
    Action::Inventory,
    Action::Drop,
];

/// `actionLabel`.
pub fn action_label(a: Action) -> &'static str {
    match a {
        Action::Forward => "Forward",
        Action::Backward => "Backward",
        Action::Left => "Left",
        Action::Right => "Right",
        Action::Jump => "Jump",
        Action::Sprint => "Sprint",
        Action::Crouch => "Crouch",
        Action::Inventory => "Inventory",
        Action::Drop => "Drop",
        Action::Pause => "PAUSE",
    }
}

fn shadow_label(q: ShadowQuality) -> &'static str {
    match q {
        ShadowQuality::Off => "Off",
        ShadowQuality::Low => "Low",
        ShadowQuality::High => "High",
    }
}

fn on_off(b: bool) -> &'static str {
    if b {
        "On"
    } else {
        "Off"
    }
}

/// Onglet courant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    Graphics,
    Controls,
    Audio,
}

/// Libellés des curseurs de l'onglet Audio, dans l'ordre de `GameSettings::volumes()`.
pub const VOLUME_LABELS: [&str; 5] = ["Master", "Music", "Ambience", "Effects", "Creatures"];

/// Action demandée par l'écran.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsAction {
    /// Échap / Cancel : retour à `previousState` sans rien appliquer.
    Back(GameState),
    /// Onglets Res. Packs / Mobs.
    SwitchTo(GameState),
    /// Apply : réglages finaux (à appliquer et sauvegarder par nb-game : distance de rendu,
    /// FOV, VSync, plein écran, taille de fenêtre si fenêtré, ombres, bloom, touches ; puis
    /// despawn des mobs si le registre est « dirty ») puis retour à `previousState`.
    Apply(Box<GameSettings>, GameState),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SliderId {
    RenderDistance,
    Fov,
    Sensitivity,
    Volume(usize),
}

/// État de l'écran des réglages.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsScreen {
    pub tab: SettingsTab,
    pub working: GameSettings,
    rd: Slider,
    fov: Slider,
    sens: Slider,
    vol: [Slider; 5],
    dragging: Option<SliderId>,
    pub resolution_index: usize,
    pub rebinding: Option<Action>,
    mouse: MouseEdge,
    esc: KeyEdge,
    scroll: Scroll,
}

impl SettingsScreen {
    /// `enter()` : copie de travail de `current`.
    pub fn enter(current: &GameSettings, ctx: &mut UiContext) -> Self {
        let tab = if ctx.pending_audio_tab {
            SettingsTab::Audio
        } else if ctx.pending_controls_tab {
            SettingsTab::Controls
        } else {
            SettingsTab::Graphics
        };
        ctx.pending_controls_tab = false;
        ctx.pending_audio_tab = false;
        let working = copy_settings(current);
        let mut s = Self {
            tab,
            rd: Slider::new(0, 0, 0.0, 0.0, 0.0),
            fov: Slider::new(0, 0, 0.0, 0.0, 0.0),
            sens: Slider::new(0, 0, 0.0, 0.0, 0.0),
            vol: [Slider::new(0, 0, 0.0, 0.0, 0.0); 5],
            resolution_index: 0,
            working,
            dragging: None,
            rebinding: None,
            mouse: MouseEdge::default(),
            esc: KeyEdge::default(),
            scroll: Scroll::default(),
        };
        s.build_sliders();
        s.resolution_index = find_resolution(s.working.window_width, s.working.window_height);
        s
    }

    fn build_sliders(&mut self) {
        let w = &self.working;
        self.rd = Slider::new(CTRL_W, SLIDER_H, 4.0, 24.0, w.render_distance as f32);
        self.fov = Slider::new(CTRL_W, SLIDER_H, 60.0, 110.0, w.fov);
        self.sens = Slider::new(CTRL_W, SLIDER_H, 0.02, 0.50, w.sensitivity);
        for (i, v) in w.volumes().into_iter().enumerate() {
            self.vol[i] = Slider::new(CTRL_W, SLIDER_H, 0.0, 1.0, v);
        }
    }

    /// Volume affiché / appliqué du curseur `i` : pas de 5 %.
    pub fn volume(&self, i: usize) -> f32 {
        (self.vol[i].value * 20.0).round() / 20.0
    }

    fn slider_mut(&mut self, id: SliderId) -> &mut Slider {
        match id {
            SliderId::Volume(i) => &mut self.vol[i],
            SliderId::RenderDistance => &mut self.rd,
            SliderId::Fov => &mut self.fov,
            SliderId::Sensitivity => &mut self.sens,
        }
    }

    /// Distance de rendu affichée / appliquée : `round(v / 2) · 2`.
    pub fn render_distance(&self) -> i32 {
        java_round(self.rd.value / 2.0) * 2
    }

    /// FOV affiché / appliqué : `round(v / 5) · 5`.
    pub fn fov(&self) -> i32 {
        java_round(self.fov.value / 5.0) * 5
    }

    pub fn sensitivity(&self) -> f32 {
        self.sens.value
    }

    /// Écouteur de réaffectation (`setRebindListener`), appelé **avant** `update` pour chaque
    /// touche pressée. Renvoie vrai si la touche a été consommée.
    pub fn key_pressed(&mut self, key: i32) -> bool {
        let Some(action) = self.rebinding.take() else {
            return false;
        };
        if key != KEY_ESCAPE {
            self.working.key_bindings.insert(action, key);
        }
        true
    }

    fn tab_content_height(&self) -> i32 {
        let rows = match self.tab {
            SettingsTab::Graphics => 7,
            SettingsTab::Controls => 1 + REBINDABLE.len() as i32,
            SettingsTab::Audio => VOLUME_LABELS.len() as i32,
        };
        rows * (ROW_H + ROW_GAP) - ROW_GAP
    }

    pub fn update(
        &mut self,
        input: &UiInput,
        current: &GameSettings,
        ctx: &UiContext,
    ) -> Option<SettingsAction> {
        let (w, h) = (input.width, input.height);
        let (mx, my) = (input.mouse_x, input.mouse_y);
        let just_clicked = self.mouse.update(input);
        let esc_just = self.esc.update(input.esc_down);

        if self.rebinding.is_some() && input.esc_down {
            self.rebinding = None;
        } else if esc_just {
            return Some(SettingsAction::Back(ctx.previous_state));
        }
        if !input.mouse_down {
            self.dragging = None;
        }
        if input.mouse_down {
            if let Some(id) = self.dragging {
                self.slider_mut(id).set_value_from_mouse_x(mx);
                return None;
            }
        }
        let list_h = h - CONTENT_TOP - BOTTOM_H;
        let content = self.tab_content_height();
        self.scroll
            .apply(input.scroll_y, ROW_H + ROW_GAP, content, list_h);
        if !just_clicked {
            return None;
        }

        match tab_at(w, mx, my, BTN_H) {
            Some(0) => {
                if self.tab != SettingsTab::Graphics {
                    self.tab = SettingsTab::Graphics;
                    self.scroll.reset();
                }
                return None;
            }
            Some(1) => {
                if self.tab != SettingsTab::Controls {
                    self.tab = SettingsTab::Controls;
                    self.scroll.reset();
                }
                return None;
            }
            Some(2) => return Some(SettingsAction::SwitchTo(GameState::ResourcePacks)),
            Some(3) => return Some(SettingsAction::SwitchTo(GameState::MobSettings)),
            Some(_) => {
                if self.tab != SettingsTab::Audio {
                    self.tab = SettingsTab::Audio;
                    self.scroll.reset();
                }
                return None;
            }
            None => {}
        }
        if hit_test(
            mx,
            my,
            w / 2 - RESET_BTN_W / 2,
            LIST_TOP + 8,
            RESET_BTN_W,
            BTN_H,
        ) {
            self.working = GameSettings::default();
            self.build_sliders();
            self.resolution_index =
                find_resolution(self.working.window_width, self.working.window_height);
            self.scroll.reset();
            return None;
        }
        let (btn_x, btn_y) = bottom_buttons(w, h);
        if hit_test(mx, my, btn_x, btn_y, 190, 50) {
            return Some(SettingsAction::Back(ctx.previous_state));
        }
        if hit_test(mx, my, btn_x + 200, btn_y, 190, 50) {
            return Some(SettingsAction::Apply(
                Box::new(self.applied(current)),
                ctx.previous_state,
            ));
        }
        if my < CONTENT_TOP as f64 || my >= (h - BOTTOM_H) as f64 {
            return None;
        }
        let row_start = CONTENT_TOP - self.scroll.offset;
        let ctrl_x = (w - CONTENT_W) / 2 + LABEL_W + 16;
        let row = |i: i32| row_start + i * (ROW_H + ROW_GAP);
        let btn =
            |i: i32, bw: i32| hit_test(mx, my, ctrl_x, row(i) + (ROW_H - BTN_H) / 2, bw, BTN_H);
        match self.tab {
            SettingsTab::Graphics => {
                self.rd
                    .set_position(ctrl_x, row(0) + (ROW_H - SLIDER_H) / 2);
                if self.rd.contains(mx, my) {
                    self.dragging = Some(SliderId::RenderDistance);
                    return None;
                }
                self.fov
                    .set_position(ctrl_x, row(1) + (ROW_H - SLIDER_H) / 2);
                if self.fov.contains(mx, my) {
                    self.dragging = Some(SliderId::Fov);
                    return None;
                }
                let s = &mut self.working;
                if btn(2, 120) {
                    s.shadow_quality = match s.shadow_quality {
                        ShadowQuality::Off => ShadowQuality::Low,
                        ShadowQuality::Low => ShadowQuality::High,
                        ShadowQuality::High => ShadowQuality::Off,
                    };
                } else if btn(3, 80) {
                    s.bloom_enabled = !s.bloom_enabled;
                } else if btn(4, 80) {
                    s.vsync_enabled = !s.vsync_enabled;
                } else if btn(5, 80) {
                    s.fullscreen = !s.fullscreen;
                } else if btn(6, 120) {
                    self.resolution_index = (self.resolution_index + 1) % RESOLUTIONS.len();
                }
            }
            SettingsTab::Controls => {
                self.sens
                    .set_position(ctrl_x, row(0) + (ROW_H - SLIDER_H) / 2);
                if self.sens.contains(mx, my) {
                    self.dragging = Some(SliderId::Sensitivity);
                    return None;
                }
                if let Some(i) = (0..REBINDABLE.len()).find(|&i| btn(i as i32 + 1, 120)) {
                    self.rebinding = Some(REBINDABLE[i]);
                }
            }
            SettingsTab::Audio => {
                for i in 0..self.vol.len() {
                    self.vol[i].set_position(ctrl_x, row(i as i32) + (ROW_H - SLIDER_H) / 2);
                    if self.vol[i].contains(mx, my) {
                        self.dragging = Some(SliderId::Volume(i));
                        return None;
                    }
                }
            }
        }
        None
    }

    /// `applyAndSave` : champs de la copie de travail recopiés dans `current`.
    fn applied(&self, current: &GameSettings) -> GameSettings {
        let mut s = current.clone();
        s.render_distance = self.render_distance();
        s.fov = self.fov() as f32;
        s.sensitivity = self.sens.value;
        s.set_volumes([
            self.volume(0),
            self.volume(1),
            self.volume(2),
            self.volume(3),
            self.volume(4),
        ]);
        s.shadow_quality = self.working.shadow_quality;
        s.bloom_enabled = self.working.bloom_enabled;
        s.vsync_enabled = self.working.vsync_enabled;
        s.fullscreen = self.working.fullscreen;
        for (&a, &k) in &self.working.key_bindings {
            s.key_bindings.insert(a, k);
        }
        let (rw, rh) = RESOLUTIONS[self.resolution_index];
        s.window_width = rw;
        s.window_height = rh;
        s
    }

    pub fn draw(&self, dl: &mut DrawList, input: &UiInput) {
        let (w, h) = (input.width, input.height);
        let content_x = (w - CONTENT_W) / 2;
        draw_background(dl, w, h, BOTTOM_H);
        let title = match self.tab {
            SettingsTab::Graphics => "Graphics",
            SettingsTab::Controls => "Controls",
            SettingsTab::Audio => "Audio",
        };
        centered_text(dl, title, w, (SPACER_H - 20) as f32 / 2.0, 2.5, WHITE);
        let active = match self.tab {
            SettingsTab::Graphics => 0,
            SettingsTab::Controls => 1,
            SettingsTab::Audio => 4,
        };
        draw_tabs(dl, w, BTN_H, Some(active));

        let list_h = (h - BOTTOM_H) - CONTENT_TOP;
        let row_start = CONTENT_TOP - self.scroll.offset;
        button(
            dl,
            w / 2 - RESET_BTN_W / 2,
            LIST_TOP + 8,
            RESET_BTN_W,
            BTN_H,
            "Reset to Defaults",
            true,
        );
        dl.set_scissor(Some(crate::draw::Scissor::from_gl(
            0, BOTTOM_H, w, list_h, h,
        )));
        let ctrl_x = content_x + LABEL_W + 16;
        let value_x = (ctrl_x + CTRL_W + 10) as f32;
        let row = |i: i32| row_start + i * (ROW_H + ROW_GAP);
        let label_off = (ROW_H - 16) as f32 / 2.0;
        let label = |dl: &mut DrawList, text: &str, i: i32| {
            dl.text(
                text,
                content_x as f32,
                row(i) as f32 + label_off,
                2.0,
                WHITE,
            );
        };
        let value = |dl: &mut DrawList, text: &str, i: i32| {
            dl.text(text, value_x, row(i) as f32 + label_off, 2.0, WHITE);
        };
        let btn = |dl: &mut DrawList, i: i32, bw: i32, text: &str, enabled: bool| {
            button(
                dl,
                ctrl_x,
                row(i) + (ROW_H - BTN_H) / 2,
                bw,
                BTN_H,
                text,
                enabled,
            );
        };
        let slider = |dl: &mut DrawList, s: &Slider, i: i32| {
            let mut s = *s;
            s.set_position(ctrl_x, row(i) + (ROW_H - SLIDER_H) / 2);
            s.draw(dl);
        };
        let ws = &self.working;
        match self.tab {
            SettingsTab::Graphics => {
                label(dl, "Render Dist.", 0);
                slider(dl, &self.rd, 0);
                value(dl, &format!("{} chunks", self.render_distance()), 0);
                label(dl, "FOV", 1);
                slider(dl, &self.fov, 1);
                value(dl, &self.fov().to_string(), 1);
                label(dl, "Shadows", 2);
                btn(dl, 2, 120, shadow_label(ws.shadow_quality), true);
                label(dl, "Bloom", 3);
                btn(dl, 3, 80, on_off(ws.bloom_enabled), true);
                label(dl, "VSync", 4);
                btn(dl, 4, 80, on_off(ws.vsync_enabled), true);
                label(dl, "Fullscreen", 5);
                btn(dl, 5, 80, on_off(ws.fullscreen), true);
                label(dl, "Resolution", 6);
                let (rw, rh) = RESOLUTIONS[self.resolution_index];
                btn(dl, 6, 120, &format!("{rw}x{rh}"), !ws.fullscreen);
                if ws.fullscreen {
                    dl.text(
                        "(fullscreen)",
                        (ctrl_x + 130) as f32,
                        row(6) as f32 + label_off,
                        1.6,
                        WHITE,
                    );
                }
            }
            SettingsTab::Controls => {
                label(dl, "Sensitivity", 0);
                slider(dl, &self.sens, 0);
                value(dl, &format_fixed(self.sens.value as f64, 2), 0);
                for (i, &a) in REBINDABLE.iter().enumerate() {
                    let r = i as i32 + 1;
                    label(dl, action_label(a), r);
                    let code = ws.key_bindings.get(&a).copied().unwrap_or(-1);
                    let name = if self.rebinding == Some(a) {
                        "...".to_owned()
                    } else if code >= 0 {
                        key_name(code)
                    } else {
                        "---".to_owned()
                    };
                    btn(dl, r, 120, &name, true);
                }
            }
            SettingsTab::Audio => {
                for (i, name) in VOLUME_LABELS.iter().enumerate() {
                    label(dl, name, i as i32);
                    slider(dl, &self.vol[i], i as i32);
                    let pct = (self.volume(i) * 100.0).round() as i32;
                    value(dl, &format!("{pct}%"), i as i32);
                }
            }
        }
        dl.set_scissor(None);
        // [JAVA-BUG] conservé : la barre est ancrée sur listTop, pas sur contentTop.
        draw_scrollbar(
            dl,
            w - 8,
            LIST_TOP,
            list_h,
            self.tab_content_height(),
            self.scroll.offset,
        );
        let (bx, by) = bottom_buttons(w, h);
        button(dl, bx, by, 190, 50, "Cancel", true);
        button(dl, bx + 200, by, 190, 50, "Apply", true);

        if let Some(a) = self.rebinding {
            dl.fill(0.0, 0.0, w as f32, h as f32, [0.0, 0.0, 0.0, 0.7]);
            let cy = h as f32 / 2.0 - 30.0;
            centered_text(dl, "Press any key to rebind", w, cy, 2.5, WHITE);
            centered_text(dl, action_label(a), w, cy + 36.0, 2.0, WHITE);
            centered_text(dl, "(Esc to cancel)", w, cy + 68.0, 1.8, WHITE);
        }
    }
}

fn bottom_buttons(w: i32, h: i32) -> (i32, i32) {
    (
        w / 2 - (190 * 2 + 10) / 2,
        h - BOTTOM_H + (BOTTOM_H - 50) / 2,
    )
}

fn find_resolution(w: i32, h: i32) -> usize {
    RESOLUTIONS.iter().position(|&r| r == (w, h)).unwrap_or(0)
}

/// `Math.round(float)` : `floor(v + 0.5)`.
fn java_round(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}

/// `copySettings` (tous les champs, touches comprises).
fn copy_settings(src: &GameSettings) -> GameSettings {
    src.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: i32 = 1280;
    const H: i32 = 720;

    fn input(x: f64, y: f64, down: bool) -> UiInput {
        UiInput {
            width: W,
            height: H,
            mouse_x: x,
            mouse_y: y,
            mouse_down: down,
            ..Default::default()
        }
    }

    /// Clic complet (enfoncé puis relâché).
    fn click(s: &mut SettingsScreen, cur: &GameSettings, x: f64, y: f64) -> Option<SettingsAction> {
        let ctx = UiContext::default();
        let r = s.update(&input(x, y, true), cur, &ctx);
        s.update(&input(x, y, false), cur, &ctx);
        r
    }

    // contentX = 280, ctrlX = 506 ; rangée i à y = 176 + 56·i, bouton à +4.
    fn row_btn(i: i32) -> (f64, f64) {
        (510.0, (176 + 56 * i + 10) as f64)
    }

    #[test]
    fn toggles_and_apply() {
        let cur = GameSettings::default();
        let mut ctx = UiContext {
            previous_state: GameState::Paused,
            pending_controls_tab: false,
            pending_audio_tab: false,
        };
        let mut s = SettingsScreen::enter(&cur, &mut ctx);
        assert_eq!(s.render_distance(), 12);
        let (x, y) = row_btn(2);
        click(&mut s, &cur, x, y);
        assert_eq!(s.working.shadow_quality, ShadowQuality::High);
        let (x, y) = row_btn(3);
        click(&mut s, &cur, x, y);
        assert!(!s.working.bloom_enabled);
        let (x, y) = row_btn(6);
        click(&mut s, &cur, x, y);
        assert_eq!(s.resolution_index, 1);
        // Apply : (1280/2 - 195) + 200 = 645, y = 720 - 80 + 15 = 655.
        let r = s.update(&input(700.0, 660.0, true), &cur, &ctx);
        let Some(SettingsAction::Apply(new, back)) = r else {
            panic!("Apply attendu : {r:?}");
        };
        assert_eq!(back, GameState::Paused);
        assert_eq!(new.shadow_quality, ShadowQuality::High);
        assert!(!new.bloom_enabled);
        assert_eq!((new.window_width, new.window_height), (1600, 900));
        assert_eq!(new.render_distance, 12);
    }

    #[test]
    fn audio_tab_sliders_edit_volumes() {
        let cur = GameSettings::default();
        let mut ctx = UiContext {
            pending_audio_tab: true,
            ..Default::default()
        };
        let mut s = SettingsScreen::enter(&cur, &mut ctx);
        assert_eq!(s.tab, SettingsTab::Audio);
        assert!(!ctx.pending_audio_tab);
        assert_eq!(s.volume(1), 0.5);
        let ctx = UiContext::default();
        // Ligne 1 (Music) : y = 176 + 56 + 6..38. Saisie puis glissé tout à gauche.
        s.update(&input(650.0, 245.0, true), &cur, &ctx);
        s.update(&input(500.0, 400.0, true), &cur, &ctx);
        assert_eq!(s.volume(1), 0.0);
        s.update(&input(0.0, 0.0, false), &cur, &ctx);
        // Ligne 3 (Effects) : pas de 5 %.
        s.update(&input(650.0, 357.0, true), &cur, &ctx);
        s.update(&input(506.0 + 0.33 * 280.0, 400.0, true), &cur, &ctx);
        assert_eq!(s.volume(3), 0.35);
        let applied = s.applied(&cur);
        assert_eq!(applied.music_volume, 0.0);
        assert_eq!(applied.effects_volume, 0.35);
        assert_eq!(applied.master_volume, 1.0);
        s.update(&input(0.0, 0.0, false), &cur, &ctx);
        // Un clic sur l'onglet Audio depuis un autre onglet.
        s.tab = SettingsTab::Graphics;
        assert_eq!(click(&mut s, &cur, 274.0 + 4.0 * 148.0 + 5.0, 80.0), None);
        assert_eq!(s.tab, SettingsTab::Audio);
    }

    #[test]
    fn slider_drag_snaps_values() {
        let cur = GameSettings::default();
        let mut s = SettingsScreen::enter(&cur, &mut UiContext::default());
        let ctx = UiContext::default();
        // Curseur de distance : x 506..786, y 182..214. Le clic saisit sans changer la valeur.
        assert_eq!(s.update(&input(506.0, 190.0, true), &cur, &ctx), None);
        assert_eq!(s.render_distance(), 12);
        s.update(&input(786.0, 400.0, true), &cur, &ctx);
        assert_eq!(s.render_distance(), 24);
        s.update(&input(506.0 + 0.3 * 280.0, 400.0, true), &cur, &ctx);
        // 4 + 0,3 × 20 = 10 → 10.
        assert_eq!(s.render_distance(), 10);
        s.update(&input(0.0, 0.0, false), &cur, &ctx);
        s.update(&input(786.0, 400.0, true), &cur, &ctx);
        assert_eq!(s.render_distance(), 10, "relâché : plus de glisser");
        assert_eq!(java_round(87.5 / 5.0), 18);
    }

    #[test]
    fn tabs_and_rebind() {
        let cur = GameSettings::default();
        let mut ctx = UiContext {
            pending_controls_tab: true,
            ..Default::default()
        };
        let mut s = SettingsScreen::enter(&cur, &mut ctx);
        assert_eq!(s.tab, SettingsTab::Controls);
        assert!(!ctx.pending_controls_tab);
        // Onglets à x = 640 - 366 = 274, pas 148 ; y 74..110.
        assert_eq!(
            click(&mut s, &cur, 274.0 + 2.0 * 148.0 + 5.0, 80.0),
            Some(SettingsAction::SwitchTo(GameState::ResourcePacks))
        );
        assert_eq!(
            click(&mut s, &cur, 274.0 + 3.0 * 148.0 + 5.0, 80.0),
            Some(SettingsAction::SwitchTo(GameState::MobSettings))
        );
        // Ligne « Jump » (5ᵉ action → rangée 5).
        let (x, y) = row_btn(5);
        click(&mut s, &cur, x, y);
        assert_eq!(s.rebinding, Some(Action::Jump));
        let mut dl = DrawList::new(W as u32, H as u32);
        s.draw(&mut dl, &input(0.0, 0.0, false));
        assert_eq!(*dl.vertices().last().map(|v| &v.color).unwrap(), WHITE);
        assert!(s.key_pressed(nb_data::keys::KEY_E));
        assert_eq!(s.working.key_bindings[&Action::Jump], nb_data::keys::KEY_E);
        assert!(!s.key_pressed(nb_data::keys::KEY_Q));
        // Échap pendant une réaffectation : annule seulement.
        click(&mut s, &cur, x, y);
        assert!(s.key_pressed(KEY_ESCAPE));
        assert_eq!(s.working.key_bindings[&Action::Jump], nb_data::keys::KEY_E);
    }

    #[test]
    fn escape_and_reset() {
        let cur = GameSettings {
            fov: 70.0,
            ..Default::default()
        };
        let mut s = SettingsScreen::enter(&cur, &mut UiContext::default());
        assert_eq!(s.fov(), 70);
        // Reset : x = 640 - 144, y = 132.
        click(&mut s, &cur, 500.0, 140.0);
        assert_eq!(s.fov(), 90);
        let mut i = input(0.0, 0.0, false);
        i.esc_down = true;
        let ctx = UiContext::default();
        assert_eq!(
            s.update(&i, &cur, &ctx),
            Some(SettingsAction::Back(GameState::MainMenu))
        );
        assert_eq!(s.update(&i, &cur, &ctx), None, "Échap maintenu");
    }

    #[test]
    fn list_is_clipped_and_scrolls() {
        let cur = GameSettings::default();
        let mut s = SettingsScreen::enter(
            &cur,
            &mut UiContext {
                pending_controls_tab: true,
                ..Default::default()
            },
        );
        let mut i = input(0.0, 0.0, false);
        i.height = 500;
        i.scroll_y = -3.0;
        s.update(&i, &cur, &UiContext::default());
        // Contenu 10 × 56 - 12 = 548 ; visible 500 - 176 - 80 = 244 → max 304 ; 3 pas = 168.
        assert_eq!(s.scroll.offset, 168);
        let mut dl = DrawList::new(W as u32, 500);
        s.draw(&mut dl, &i);
        assert!(dl.batches().iter().any(|b| b.scissor
            == Some(crate::draw::Scissor {
                x: 0,
                y: CONTENT_TOP,
                w: W,
                h: 244
            })));
    }
}
