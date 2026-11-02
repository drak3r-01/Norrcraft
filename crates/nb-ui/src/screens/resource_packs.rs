//! Choix du pack de ressources (`ResourcePackSelectState`).

use super::common::{
    draw_background, draw_tabs, draw_title, tab_at, MouseEdge, Scroll, SPACER_H, TAB_GAP, TAB_H,
    TAB_W,
};
use crate::draw::{DrawList, Scissor, WHITE};
use crate::input::UiInput;
use crate::state::{GameState, UiContext};
use crate::widgets::{button, hit_test};

const BOTTOM_H: i32 = 80;
const ROW_H: i32 = 72;
const ROW_GAP: i32 = 6;
const CONTENT_W: i32 = 720;
const BTN_H: i32 = 50;
const BTN_W: i32 = 190;
const BTN_GAP: i32 = 10;
const TAB_BAR_H: i32 = TAB_H + 24;
const LIST_TOP: i32 = SPACER_H + TAB_BAR_H;
const RESET_BTN_W: i32 = TAB_W * 2 + TAB_GAP;
const RESET_AREA_H: i32 = TAB_H + 16;
const CONTENT_TOP: i32 = LIST_TOP + RESET_AREA_H;

/// Pack listé (`ResourcePackMeta`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackEntry {
    pub name: String,
    pub description: String,
}

/// Action demandée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackAction {
    /// Retour aux réglages (onglet Graphics, ou Controls si `ctx.pending_controls_tab`).
    SwitchTo(GameState),
    /// Apply avec un pack différent : recharger l'atlas, régénérer les icônes, invalider les
    /// maillages, enregistrer `resourcePack`, puis revenir aux réglages.
    Apply(String),
}

/// État de l'écran.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourcePackScreen {
    pub packs: Vec<PackEntry>,
    pub selected: usize,
    scroll: Scroll,
    mouse: MouseEdge,
}

impl ResourcePackScreen {
    /// `enter()` : sélection = pack courant (sinon 0).
    pub fn enter(packs: Vec<PackEntry>, current: &str) -> Self {
        let selected = packs.iter().position(|p| p.name == current).unwrap_or(0);
        Self {
            packs,
            selected,
            scroll: Scroll::default(),
            mouse: MouseEdge::default(),
        }
    }

    fn row_y(&self, i: usize) -> i32 {
        CONTENT_TOP + 8 + i as i32 * (ROW_H + ROW_GAP) - self.scroll.offset
    }

    pub fn update(
        &mut self,
        input: &UiInput,
        current: &str,
        ctx: &mut UiContext,
    ) -> Option<PackAction> {
        let (w, h) = (input.width, input.height);
        let list_h = h - CONTENT_TOP - BOTTOM_H;
        let content = 8 + self.packs.len() as i32 * (ROW_H + ROW_GAP) - ROW_GAP;
        self.scroll
            .apply(input.scroll_y, ROW_H + ROW_GAP, content, list_h);
        if !self.mouse.update(input) {
            return None;
        }
        let (mx, my) = (input.mouse_x, input.mouse_y);
        match tab_at(w, mx, my, TAB_H) {
            Some(0) => return Some(PackAction::SwitchTo(GameState::Settings)),
            Some(1) => {
                ctx.pending_controls_tab = true;
                return Some(PackAction::SwitchTo(GameState::Settings));
            }
            Some(3) => return Some(PackAction::SwitchTo(GameState::MobSettings)),
            _ => {}
        }
        if hit_test(
            mx,
            my,
            w / 2 - RESET_BTN_W / 2,
            LIST_TOP + 8,
            RESET_BTN_W,
            TAB_H,
        ) {
            if let Some(i) = self.packs.iter().position(|p| p.name == "Default") {
                self.selected = i;
            }
            return None;
        }
        let (bx, by) = bottom_buttons(w, h);
        if hit_test(mx, my, bx, by, BTN_W, BTN_H) {
            return Some(PackAction::SwitchTo(GameState::Settings));
        }
        if hit_test(mx, my, bx + BTN_W + BTN_GAP, by, BTN_W, BTN_H) {
            return Some(match self.packs.get(self.selected) {
                Some(p) if p.name != current => PackAction::Apply(p.name.clone()),
                _ => PackAction::SwitchTo(GameState::Settings),
            });
        }
        let content_x = (w - CONTENT_W) / 2;
        let list_bottom = h - BOTTOM_H;
        for i in 0..self.packs.len() {
            let y = self.row_y(i);
            if y + ROW_H < CONTENT_TOP || y > list_bottom {
                continue;
            }
            if hit_test(mx, my, content_x, y, CONTENT_W, ROW_H) {
                self.selected = i;
                return None;
            }
        }
        None
    }

    pub fn draw(&self, dl: &mut DrawList, input: &UiInput) {
        let (w, h) = (input.width, input.height);
        let content_x = (w - CONTENT_W) / 2;
        draw_background(dl, w, h, BOTTOM_H);
        draw_tabs(dl, w, TAB_H, Some(2));
        draw_title(dl, "Resource Packs", w);
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
        for (i, p) in self.packs.iter().enumerate() {
            let y = self.row_y(i);
            if y + ROW_H < CONTENT_TOP || y > list_bottom {
                continue;
            }
            let bg = if i == self.selected { 0.30 } else { 0.15 };
            dl.fill(
                content_x as f32,
                y as f32,
                CONTENT_W as f32,
                ROW_H as f32,
                [bg, bg, bg, 0.9],
            );
            let x = (content_x + 12) as f32;
            dl.text(&p.name, x, (y + 12) as f32, 2.5, WHITE);
            if !p.description.is_empty() {
                dl.text(
                    &p.description,
                    x,
                    (y + 44) as f32,
                    1.8,
                    [0.75, 0.75, 0.75, 1.0],
                );
            }
        }
        dl.set_scissor(None);
        let (bx, by) = bottom_buttons(w, h);
        button(dl, bx, by, BTN_W, BTN_H, "Cancel", true);
        button(dl, bx + BTN_W + BTN_GAP, by, BTN_W, BTN_H, "Apply", true);
    }
}

fn bottom_buttons(w: i32, h: i32) -> (i32, i32) {
    (
        (w - (BTN_W * 2 + BTN_GAP)) / 2,
        h - BOTTOM_H + (BOTTOM_H - BTN_H) / 2,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packs() -> Vec<PackEntry> {
        ["Default", "Faithful", "Retro"]
            .iter()
            .map(|n| PackEntry {
                name: (*n).into(),
                description: format!("{n} pack"),
            })
            .collect()
    }

    fn press(
        s: &mut ResourcePackScreen,
        x: f64,
        y: f64,
        ctx: &mut UiContext,
    ) -> Option<PackAction> {
        let mut i = UiInput {
            width: 1280,
            height: 720,
            mouse_x: x,
            mouse_y: y,
            mouse_down: true,
            ..Default::default()
        };
        let r = s.update(&i, "Default", ctx);
        i.mouse_down = false;
        s.update(&i, "Default", ctx);
        r
    }

    #[test]
    fn select_and_apply() {
        let mut ctx = UiContext::default();
        let mut s = ResourcePackScreen::enter(packs(), "Faithful");
        assert_eq!(s.selected, 1);
        // Ligne 2 : y = 176 + 8 + 2 × 78 = 340.
        assert_eq!(press(&mut s, 640.0, 350.0, &mut ctx), None);
        assert_eq!(s.selected, 2);
        // Apply : x = (1280 - 390)/2 + 200 = 645, y = 655.
        assert_eq!(
            press(&mut s, 700.0, 660.0, &mut ctx),
            Some(PackAction::Apply("Retro".into()))
        );
        // Reset → Default ; Apply sans changement → simple retour.
        press(&mut s, 640.0, 140.0, &mut ctx);
        assert_eq!(s.selected, 0);
        assert_eq!(
            press(&mut s, 700.0, 660.0, &mut ctx),
            Some(PackAction::SwitchTo(GameState::Settings))
        );
        assert_eq!(
            press(&mut s, 348.0 + 150.0, 80.0, &mut ctx),
            Some(PackAction::SwitchTo(GameState::Settings))
        );
        assert!(ctx.pending_controls_tab);
    }

    #[test]
    fn draw_rows() {
        let s = ResourcePackScreen::enter(packs(), "Default");
        let mut dl = DrawList::new(1280, 720);
        let i = UiInput {
            width: 1280,
            height: 720,
            ..Default::default()
        };
        s.draw(&mut dl, &i);
        let row = dl
            .vertices()
            .iter()
            .position(|v| v.color == [0.30, 0.30, 0.30, 0.9])
            .expect("ligne sélectionnée");
        assert_eq!(dl.vertices()[row].pos, [280.0, 184.0]);
    }
}
