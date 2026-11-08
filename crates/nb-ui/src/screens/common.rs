//! Éléments partagés des écrans de menu : fond de terre, onglets des réglages, défilement,
//! détection de clic et panneau « avancé » repliable (`AdvRow` de `WorldSelectState` et
//! `MobsSettingsState`).

use crate::draw::{gray, DrawList, Scissor, WHITE};
use crate::input::UiInput;
use crate::textures::UiTexture;
use crate::widgets::{button, hit_test, tab_button, Slider};

pub const SPACER_H: i32 = 64;
pub const DIRT_TILE: f32 = 32.0;
pub const TAB_W: i32 = 140;
pub const TAB_H: i32 = 36;
pub const TAB_GAP: i32 = 8;
pub const TAB_COUNT: usize = 5;
pub const TAB_LABELS: [&str; TAB_COUNT] = ["Graphics", "Controls", "Res. Packs", "Mobs", "Audio"];

/// Teinte et tuilage `renderDirt` : `uv = (0, 0, w/32, h/32)` (REPEAT), teinte 0,32.
pub fn draw_dirt(dl: &mut DrawList, x: f32, y: f32, w: f32, h: f32) {
    dl.region_gl(
        UiTexture::Dirt,
        x,
        y,
        w,
        h,
        [0.0, 0.0, w / DIRT_TILE, h / DIRT_TILE],
        gray(0.32),
    );
}

/// `renderBackground` : terre plein écran + bande noire α 0,55 entre l'en-tête et le pied.
pub fn draw_background(dl: &mut DrawList, win_w: i32, win_h: i32, bottom_h: i32) {
    draw_dirt(dl, 0.0, 0.0, win_w as f32, win_h as f32);
    dl.fill(
        0.0,
        SPACER_H as f32,
        win_w as f32,
        (win_h - SPACER_H - bottom_h) as f32,
        [0.0, 0.0, 0.0, 0.55],
    );
}

/// Titre d'écran (`renderScreenTitle`) : échelle 2,5 centrée, `y = (64 - 20)/2`.
pub fn draw_title(dl: &mut DrawList, title: &str, win_w: i32) {
    crate::widgets::centered_text(dl, title, win_w, (SPACER_H - 20) as f32 / 2.0, 2.5, WHITE);
}

/// x du premier onglet.
pub fn tabs_x(win_w: i32) -> i32 {
    win_w / 2 - (TAB_COUNT as i32 * TAB_W + (TAB_COUNT as i32 - 1) * TAB_GAP) / 2
}

/// y des onglets.
pub const TAB_Y: i32 = SPACER_H + 10;

/// Onglet sous la souris (0..4), bornes incluses, dans l'ordre de test Java.
pub fn tab_at(win_w: i32, mx: f64, my: f64, h: i32) -> Option<usize> {
    (0..TAB_COUNT).find(|&i| {
        hit_test(
            mx,
            my,
            tabs_x(win_w) + i as i32 * (TAB_W + TAB_GAP),
            TAB_Y,
            TAB_W,
            h,
        )
    })
}

/// Barre d'onglets, `active` en pleine teinte.
pub fn draw_tabs(dl: &mut DrawList, win_w: i32, h: i32, active: Option<usize>) {
    for (i, label) in TAB_LABELS.iter().enumerate() {
        let x = tabs_x(win_w) + i as i32 * (TAB_W + TAB_GAP);
        tab_button(dl, x, TAB_Y, TAB_W, h, label, active == Some(i));
    }
}

/// Défilement Java : la molette s'accumule, les pas entiers déplacent de `step` pixels
/// (molette vers le haut = remonter), borné à `[0, max(0, content - visible)]`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scroll {
    pub offset: i32,
    pub accum: f64,
}

impl Scroll {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn apply(&mut self, scroll_y: f64, step: i32, content_h: i32, visible_h: i32) {
        if scroll_y == 0.0 {
            return;
        }
        self.accum += scroll_y;
        let steps = self.accum as i32;
        self.accum -= steps as f64;
        let max = (content_h - visible_h).max(0);
        self.offset = (self.offset - steps * step).min(max).max(0);
    }
}

/// Indicateur de défilement (`thumbH ≥ 20`, gris 0,6 α 0,8, 5 px de large).
pub fn draw_scrollbar(
    dl: &mut DrawList,
    x: i32,
    top: i32,
    visible: i32,
    content: i32,
    offset: i32,
) {
    if content <= visible {
        return;
    }
    let ratio = visible as f32 / content as f32;
    let thumb_h = 20.max((visible as f32 * ratio) as i32);
    let scroll_ratio = offset as f32 / (content - visible) as f32;
    let thumb_y = top + (scroll_ratio * (visible - thumb_h) as f32) as i32;
    dl.fill(
        x as f32,
        thumb_y as f32,
        5.0,
        thumb_h as f32,
        [0.6, 0.6, 0.6, 0.8],
    );
}

/// Suivi « clic venant d'arriver » des écrans qui lisent `glfwGetMouseButton` eux-mêmes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEdge {
    pub was_down: bool,
}

impl MouseEdge {
    /// `(justClicked, isDown)`.
    pub fn update(&mut self, input: &UiInput) -> bool {
        let just = input.mouse_down && !self.was_down;
        self.was_down = input.mouse_down;
        just
    }
}

/// Idem pour la touche Échap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyEdge {
    pub was_down: bool,
}

impl KeyEdge {
    pub fn update(&mut self, down: bool) -> bool {
        let just = down && !self.was_down;
        self.was_down = down;
        just
    }
}

// ── Panneau avancé ──────────────────────────────────────────────────────────

pub const CONTENT_W: i32 = 800;
pub const ADV_ROW_H: i32 = 40;
pub const ADV_ROW_GAP: i32 = 5;
pub const ADV_HDR_H: i32 = 30;
pub const ADV_SLIDER_W: i32 = 260;
pub const ADV_LABEL_W: i32 = CONTENT_W - ADV_SLIDER_W - 12;

/// Type de ligne.
#[derive(Clone, Debug, PartialEq)]
pub enum AdvKind {
    Header {
        collapsed: bool,
    },
    /// `norm = Some((raw0, raw1))` : curseur [0, 1] → valeur brute `raw0 + t·(raw1 - raw0)`.
    Slider {
        slider: Slider,
        norm: Option<(f64, f64)>,
    },
    Toggle(bool),
    Cycle {
        options: Vec<&'static str>,
        index: usize,
    },
}

/// `AdvRow`.
#[derive(Clone, Debug, PartialEq)]
pub struct AdvRow {
    pub label: String,
    pub kind: AdvKind,
}

impl AdvRow {
    pub fn header(label: &str, collapsed: bool) -> Self {
        Self {
            label: label.into(),
            kind: AdvKind::Header { collapsed },
        }
    }

    /// Curseur direct (`new UISlider(0, 0, 260, 32, min, max, v)`).
    pub fn slider(label: &str, min: f32, max: f32, v: f32) -> Self {
        Self {
            label: label.into(),
            kind: AdvKind::Slider {
                slider: Slider::new(ADV_SLIDER_W, ADV_ROW_H - 8, min, max, v),
                norm: None,
            },
        }
    }

    /// Curseur normalisé [0, 1] → `[raw0, raw1]`.
    pub fn normalized(label: &str, v: f32, raw0: f64, raw1: f64) -> Self {
        Self {
            label: label.into(),
            kind: AdvKind::Slider {
                slider: Slider::new(ADV_SLIDER_W, ADV_ROW_H - 8, 0.0, 1.0, v),
                norm: Some((raw0, raw1)),
            },
        }
    }

    pub fn toggle(label: &str, on: bool) -> Self {
        Self {
            label: label.into(),
            kind: AdvKind::Toggle(on),
        }
    }

    pub fn cycle(label: &str, options: &[&'static str], index: usize) -> Self {
        Self {
            label: label.into(),
            kind: AdvKind::Cycle {
                options: options.to_vec(),
                index,
            },
        }
    }

    pub fn is_header(&self) -> bool {
        matches!(self.kind, AdvKind::Header { .. })
    }

    /// `rawValue()` (0 hors curseur).
    pub fn raw_value(&self) -> f64 {
        match &self.kind {
            AdvKind::Slider {
                slider,
                norm: Some((r0, r1)),
            } => r0 + slider.value as f64 * (r1 - r0),
            AdvKind::Slider { slider, norm: None } => slider.value as f64,
            _ => 0.0,
        }
    }

    fn heights(&self) -> (i32, i32) {
        if self.is_header() {
            (ADV_HDR_H, ADV_HDR_H / 2)
        } else {
            (ADV_ROW_H, ADV_ROW_GAP)
        }
    }
}

/// Résultat d'un clic dans le panneau.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvClick {
    /// Clic hors du panneau (non consommé).
    Outside,
    /// Clic dans le panneau sans effet.
    Consumed,
    HeaderToggled(usize),
    SliderGrabbed(usize),
    Toggled(usize),
    Cycled(usize),
}

/// Liste de lignes + défilement + curseur saisi.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AdvPanel {
    pub rows: Vec<AdvRow>,
    pub scroll: Scroll,
    pub active_slider: Option<usize>,
}

impl AdvPanel {
    /// `computeAdvContentHeight`.
    pub fn content_height(&self) -> i32 {
        let mut total = 4;
        let mut collapsed = false;
        for r in &self.rows {
            match r.kind {
                AdvKind::Header { collapsed: c } => {
                    collapsed = c;
                    total += ADV_HDR_H + ADV_HDR_H / 2;
                }
                _ if !collapsed => total += ADV_ROW_H + ADV_ROW_GAP,
                _ => {}
            }
        }
        total
    }

    /// Ligne par libellé (première occurrence de ce type).
    pub fn find(&self, label: &str, header: bool) -> Option<&AdvRow> {
        self.rows
            .iter()
            .find(|r| r.label == label && r.is_header() == header)
    }

    /// État replié des en-têtes (pour reconstruire en le conservant).
    pub fn collapsed_headers(&self) -> Vec<(String, bool)> {
        self.rows
            .iter()
            .filter_map(|r| match r.kind {
                AdvKind::Header { collapsed } => Some((r.label.clone(), collapsed)),
                _ => None,
            })
            .collect()
    }

    pub fn restore_collapsed(&mut self, saved: &[(String, bool)]) {
        for r in &mut self.rows {
            if let AdvKind::Header { collapsed } = &mut r.kind {
                if let Some((_, c)) = saved.iter().find(|(l, _)| *l == r.label) {
                    *collapsed = *c;
                }
            }
        }
    }

    /// Glisser : met à jour le curseur saisi tant que le bouton est enfoncé, le relâche sinon.
    /// Renvoie vrai si une valeur a été modifiée.
    pub fn drag(&mut self, input: &UiInput, content_x: i32) -> bool {
        if input.mouse_down {
            if let Some(i) = self.active_slider {
                if let AdvKind::Slider { slider, .. } = &mut self.rows[i].kind {
                    slider.x = content_x + ADV_LABEL_W + 12;
                    slider.set_value_from_mouse_x(input.mouse_x);
                    return true;
                }
            }
        } else {
            self.active_slider = None;
        }
        false
    }

    pub fn apply_scroll(&mut self, scroll_y: f64, panel_h: i32) {
        let content = self.content_height();
        self.scroll
            .apply(scroll_y, ADV_ROW_H + ADV_ROW_GAP, content, panel_h);
    }

    /// Itère les lignes visibles (non repliées) avec leur y.
    fn layout(&self, panel_top: i32) -> Vec<(usize, i32)> {
        let mut out = Vec::new();
        let mut y = panel_top + 4 - self.scroll.offset;
        let mut collapsed = false;
        for (i, r) in self.rows.iter().enumerate() {
            let (rh, gap) = r.heights();
            if let AdvKind::Header { collapsed: c } = r.kind {
                collapsed = c;
                out.push((i, y));
                y += rh + gap;
            } else if !collapsed {
                out.push((i, y));
                y += rh + gap;
            }
        }
        out
    }

    /// `handleAdvClick` (le panneau consomme tout clic dans son rectangle).
    pub fn click(
        &mut self,
        mx: f64,
        my: f64,
        content_x: i32,
        panel_top: i32,
        panel_h: i32,
    ) -> AdvClick {
        if !hit_test(mx, my, content_x, panel_top, CONTENT_W, panel_h) {
            return AdvClick::Outside;
        }
        let slider_x = content_x + ADV_LABEL_W + 12;
        for (i, y) in self.layout(panel_top) {
            let (rh, _) = self.rows[i].heights();
            if !(y + rh >= panel_top && y < panel_top + panel_h) {
                continue;
            }
            match &mut self.rows[i].kind {
                AdvKind::Header { collapsed } => {
                    if hit_test(mx, my, content_x, y, CONTENT_W, rh) {
                        *collapsed = !*collapsed;
                        return AdvClick::HeaderToggled(i);
                    }
                }
                AdvKind::Slider { slider, .. } => {
                    slider.set_position(slider_x, y + (rh - slider.h) / 2);
                    if slider.contains(mx, my) {
                        slider.set_value_from_mouse_x(mx);
                        self.active_slider = Some(i);
                        return AdvClick::SliderGrabbed(i);
                    }
                }
                AdvKind::Toggle(on) => {
                    if hit_test(mx, my, slider_x, y + 2, ADV_SLIDER_W, rh - 4) {
                        *on = !*on;
                        return AdvClick::Toggled(i);
                    }
                }
                AdvKind::Cycle { options, index } => {
                    if hit_test(mx, my, slider_x, y + 2, ADV_SLIDER_W, rh - 4) {
                        *index = (*index + 1) % options.len();
                        return AdvClick::Cycled(i);
                    }
                }
            }
        }
        AdvClick::Consumed
    }

    /// `renderAdvPanel` ; `value` = `valueDisplay()` propre à l'écran.
    pub fn draw(
        &self,
        dl: &mut DrawList,
        content_x: i32,
        panel_top: i32,
        panel_h: i32,
        value: impl Fn(&AdvRow) -> String,
    ) {
        dl.fill(
            content_x as f32,
            panel_top as f32,
            CONTENT_W as f32,
            panel_h as f32,
            [0.05, 0.05, 0.05, 0.85],
        );
        let prev = dl.scissor();
        dl.set_scissor(Some(Scissor {
            x: content_x,
            y: panel_top,
            w: CONTENT_W,
            h: panel_h,
        }));
        let slider_x = content_x + ADV_LABEL_W + 12;
        for (i, y) in self.layout(panel_top) {
            let r = &self.rows[i];
            let (rh, _) = r.heights();
            if !(y + rh >= panel_top && y <= panel_top + panel_h) {
                continue;
            }
            let label_y = (y + (rh - 18) / 2) as f32;
            match &r.kind {
                AdvKind::Header { collapsed } => {
                    dl.fill(
                        content_x as f32,
                        y as f32,
                        CONTENT_W as f32,
                        rh as f32,
                        gray(0.22),
                    );
                    let hdr = format!("{}{}", if *collapsed { "> " } else { "v " }, r.label);
                    dl.text(
                        &hdr,
                        (content_x + 8) as f32,
                        (y + 5) as f32,
                        2.2,
                        [0.9, 0.85, 0.6, 1.0],
                    );
                }
                AdvKind::Slider { slider, .. } => {
                    let mut s = *slider;
                    s.set_position(slider_x, y + (rh - s.h) / 2);
                    let text = format!("{}: {}", r.label, value(r));
                    dl.text(&text, (content_x + 6) as f32, label_y, 1.8, WHITE);
                    s.draw(dl);
                }
                AdvKind::Toggle(on) => {
                    dl.text(
                        &format!("{}:", r.label),
                        (content_x + 6) as f32,
                        label_y,
                        1.8,
                        WHITE,
                    );
                    let l = if *on { "ON" } else { "OFF" };
                    button(dl, slider_x, y + 2, ADV_SLIDER_W, rh - 4, l, true);
                }
                AdvKind::Cycle { options, index } => {
                    dl.text(
                        &format!("{}:", r.label),
                        (content_x + 6) as f32,
                        label_y,
                        1.8,
                        WHITE,
                    );
                    button(
                        dl,
                        slider_x,
                        y + 2,
                        ADV_SLIDER_W,
                        rh - 4,
                        options[*index],
                        true,
                    );
                }
            }
        }
        dl.set_scissor(prev);
        draw_scrollbar(
            dl,
            content_x + CONTENT_W - 6,
            panel_top,
            panel_h,
            self.content_height(),
            self.scroll.offset,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> AdvPanel {
        AdvPanel {
            rows: vec![
                AdvRow::header("A", false),
                AdvRow::slider("S", 0.0, 10.0, 5.0),
                AdvRow::toggle("T", false),
                AdvRow::header("B", true),
                AdvRow::slider("Hidden", 0.0, 1.0, 0.0),
                AdvRow::cycle("C", &["X", "Y"], 0),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn scroll_accumulates_and_clamps() {
        let mut s = Scroll::default();
        s.apply(-0.5, 88, 1000, 400);
        assert_eq!(s.offset, 0);
        s.apply(-0.6, 88, 1000, 400);
        assert_eq!(s.offset, 88);
        s.apply(-20.0, 88, 1000, 400);
        assert_eq!(s.offset, 600);
        s.apply(3.0, 88, 1000, 400);
        assert_eq!(s.offset, 424);
        s.apply(1.0, 88, 100, 400);
        assert_eq!(s.offset, 0);
    }

    #[test]
    fn content_height_skips_collapsed() {
        let p = panel();
        // 4 + 2 en-têtes × 45 + 2 lignes visibles × 45 (« Hidden » et « C » repliées).
        assert_eq!(p.content_height(), 4 + 2 * 45 + 2 * 45);
    }

    #[test]
    fn clicks() {
        let mut p = panel();
        let (cx, top, h) = (240, 300, 400);
        assert_eq!(p.click(10.0, 310.0, cx, top, h), AdvClick::Outside);
        // En-tête A : y = 304..334.
        assert_eq!(
            p.click(300.0, 310.0, cx, top, h),
            AdvClick::HeaderToggled(0)
        );
        assert_eq!(p.content_height(), 4 + 2 * 45);
        assert_eq!(
            p.click(300.0, 310.0, cx, top, h),
            AdvClick::HeaderToggled(0)
        );
        // Curseur S : ligne à y = 304 + 45 = 349, curseur 32 px à 353 ; x = 240 + 528 + 12.
        let sx = 240.0 + 528.0 + 12.0;
        assert_eq!(
            p.click(sx + 130.0, 360.0, cx, top, h),
            AdvClick::SliderGrabbed(1)
        );
        assert_eq!(p.active_slider, Some(1));
        assert!((p.rows[1].raw_value() - 5.0).abs() < 1e-5);
        let mut i = UiInput {
            mouse_down: true,
            mouse_x: sx + 260.0,
            ..Default::default()
        };
        assert!(p.drag(&i, cx));
        assert_eq!(p.rows[1].raw_value(), 10.0);
        i.mouse_down = false;
        assert!(!p.drag(&i, cx));
        assert_eq!(p.active_slider, None);
        // Bascule T : ligne y = 394, bouton 396..432.
        assert_eq!(p.click(sx + 10.0, 400.0, cx, top, h), AdvClick::Toggled(2));
        assert_eq!(p.rows[2].kind, AdvKind::Toggle(true));
        // Zone vide du panneau : consommé.
        assert_eq!(p.click(300.0, 690.0, cx, top, h), AdvClick::Consumed);
    }

    #[test]
    fn normalized_raw_value() {
        let r = AdvRow::normalized("N", 0.5, 0.003, 0.0004);
        assert!((r.raw_value() - 0.0017).abs() < 1e-9);
    }

    #[test]
    fn draw_is_clipped() {
        let p = panel();
        let mut dl = DrawList::new(1280, 720);
        p.draw(&mut dl, 240, 300, 400, |_| "v".into());
        let b = dl.batches();
        assert!(b[0].scissor.is_none());
        assert_eq!(
            b[1].scissor,
            Some(Scissor {
                x: 240,
                y: 300,
                w: 800,
                h: 400
            })
        );
        assert!(dl.scissor().is_none());
    }

    #[test]
    fn collapsed_state_survives_rebuild() {
        let mut p = panel();
        let saved = p.collapsed_headers();
        p.rows = vec![AdvRow::header("B", false), AdvRow::header("Z", false)];
        p.restore_collapsed(&saved);
        assert_eq!(p.rows[0].kind, AdvKind::Header { collapsed: true });
        assert_eq!(p.rows[1].kind, AdvKind::Header { collapsed: false });
    }
}
