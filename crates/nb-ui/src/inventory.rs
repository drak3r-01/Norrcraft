//! Écran d'inventaire (`InventoryState`) : mise en page, hit-testing des cases et rendu.
//! La logique du curseur (prendre / poser / fusionner / rendre) est `nb_ecs::InventoryCursor`.

use crate::draw::{DrawList, WHITE};
use crate::hud::SlotView;
use crate::input::UiInput;
use crate::textures::UiTexture;
use crate::widgets::{Element, ElementKind, Layout, Panel};

pub const INV_W: i32 = 616;
pub const INV_H: i32 = 581;
pub const SLOT_SIZE: i32 = 56;
pub const SLOT_SPACE: i32 = 63;
pub const ICON_SIZE: i32 = 44;
pub const ICON_PAD: i32 = (SLOT_SIZE - ICON_SIZE) / 2;
/// Nombre de cases (27 + barre de 9, index 27..35).
pub const SLOTS: usize = 36;

const COUNT_SCALE: f32 = 2.2;
const CHAR_W: f32 = 7.3 * COUNT_SCALE;
const CHAR_H: f32 = 10.0 * COUNT_SCALE;
const COUNT_MARGIN: f32 = 3.0;

/// Action demandée par l'écran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryAction {
    /// Clic gauche : case visée (`-1` hors cases → la pile tenue est rendue).
    Click(i32),
    /// Inventaire / Échap : rendre la pile tenue puis revenir à PLAYING.
    Close,
}

/// Mise en page pour une taille de fenêtre (`buildUI`).
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryScreen {
    pub inv_x: i32,
    pub inv_y: i32,
    pub start_x: i32,
    pub main_y: i32,
    pub hotbar_y: i32,
    ui: Panel,
    size: (i32, i32),
}

impl InventoryScreen {
    pub fn new(win_w: i32, win_h: i32) -> Self {
        let inv_x = (win_w - INV_W) / 2;
        let inv_y = (win_h - INV_H) / 2;
        let start_x = inv_x + 28;
        let main_y = inv_y + 294;
        let hotbar_y = inv_y + 497;
        let mut ui = Panel::new(0, 0, win_w, win_h, None);
        ui.add(Element::new(
            inv_x,
            inv_y,
            INV_W,
            INV_H,
            ElementKind::Region {
                tex: UiTexture::Inventory,
                gl_uv: [0.0, 1.0 - 166.0 / 256.0, 176.0 / 256.0, 166.0 / 256.0],
            },
        ));
        let slot = || {
            Element::new(
                0,
                0,
                SLOT_SIZE,
                SLOT_SIZE,
                ElementKind::Slot {
                    highlight: Some(UiTexture::White),
                    action: None,
                },
            )
        };
        let gap = SLOT_SPACE - SLOT_SIZE;
        let mut main = Panel::new(
            start_x,
            main_y,
            SLOT_SPACE * 9,
            SLOT_SPACE * 3,
            Some(Layout::Grid {
                columns: 9,
                spacing_x: gap,
                spacing_y: gap,
            }),
        );
        for _ in 0..27 {
            main.add(slot());
        }
        ui.add_panel(main);
        let mut bar = Panel::new(
            start_x,
            hotbar_y,
            SLOT_SPACE * 9,
            SLOT_SIZE,
            Some(Layout::Grid {
                columns: 9,
                spacing_x: gap,
                spacing_y: 0,
            }),
        );
        for _ in 0..9 {
            bar.add(slot());
        }
        ui.add_panel(bar);
        Self {
            inv_x,
            inv_y,
            start_x,
            main_y,
            hotbar_y,
            ui,
            size: (win_w, win_h),
        }
    }

    /// Reconstruit la mise en page si la fenêtre a changé de taille.
    pub fn resize(&mut self, win_w: i32, win_h: i32) {
        if self.size != (win_w, win_h) {
            *self = Self::new(win_w, win_h);
        }
    }

    /// `getSlotAt` : bornes **exclusives** à droite / en bas, `-1` hors cases.
    pub fn slot_at(&self, mx: f64, my: f64) -> i32 {
        let inside = |sx: i32, sy: i32| {
            mx >= sx as f64
                && mx < (sx + SLOT_SIZE) as f64
                && my >= sy as f64
                && my < (sy + SLOT_SIZE) as f64
        };
        for row in 0..3 {
            for col in 0..9 {
                if inside(
                    self.start_x + col * SLOT_SPACE,
                    self.main_y + row * SLOT_SPACE,
                ) {
                    return row * 9 + col;
                }
            }
        }
        for col in 0..9 {
            if inside(self.start_x + col * SLOT_SPACE, self.hotbar_y) {
                return 27 + col;
            }
        }
        -1
    }

    /// `slotPixels` : coin haut-gauche de la case `idx`.
    pub fn slot_pixels(&self, idx: usize) -> (i32, i32) {
        let idx = idx as i32;
        if idx < 27 {
            (
                self.start_x + (idx % 9) * SLOT_SPACE,
                self.main_y + (idx / 9) * SLOT_SPACE,
            )
        } else {
            (self.start_x + (idx - 27) * SLOT_SPACE, self.hotbar_y)
        }
    }

    /// `update` : fermeture (Inventaire ou Pause) puis clic.
    pub fn update(&mut self, input: &UiInput) -> Option<InventoryAction> {
        self.resize(input.width, input.height);
        if input.inventory_pressed || input.pause_pressed {
            return Some(InventoryAction::Close);
        }
        if input.primary_pressed {
            return Some(InventoryAction::Click(
                self.slot_at(input.mouse_x, input.mouse_y),
            ));
        }
        None
    }

    /// Rendu (après la scène) : voile, fond, surbrillances, icônes, pile tenue au curseur.
    pub fn draw(
        &self,
        dl: &mut DrawList,
        input: &UiInput,
        slots: &[Option<SlotView<'_>>; SLOTS],
        held: Option<SlotView<'_>>,
    ) {
        let (w, h) = (input.width as f32, input.height as f32);
        dl.fill(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
        self.ui.draw(dl, input.mouse_x, input.mouse_y);
        let icon = ICON_SIZE as f32;
        for (i, s) in slots.iter().enumerate() {
            let Some(s) = s else { continue };
            let (px, py) = self.slot_pixels(i);
            dl.block_icon(
                s.icon_layer,
                (px + ICON_PAD) as f32,
                (py + ICON_PAD) as f32,
                icon,
            );
            if s.count > 1 {
                count(
                    dl,
                    s.count,
                    (px + SLOT_SIZE) as f32,
                    (py + SLOT_SIZE) as f32,
                );
            }
        }
        if let Some(s) = held {
            let hx = input.mouse_x as f32 - icon / 2.0;
            let hy = input.mouse_y as f32 - icon / 2.0;
            dl.block_icon(s.icon_layer, hx, hy, icon);
            if s.count > 1 {
                count(dl, s.count, hx + icon, hy + icon);
            }
        }
    }
}

/// `renderCount(text, rightX, bottomY)`.
fn count(dl: &mut DrawList, n: i32, right: f32, bottom: f32) {
    let text = n.to_string();
    dl.text(
        &text,
        right - text.len() as f32 * CHAR_W - COUNT_MARGIN,
        bottom - CHAR_H - COUNT_MARGIN,
        COUNT_SCALE,
        WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::TextureRef;

    #[test]
    fn layout_and_hit_testing() {
        let s = InventoryScreen::new(1280, 720);
        assert_eq!((s.inv_x, s.inv_y), (332, 69));
        assert_eq!((s.start_x, s.main_y, s.hotbar_y), (360, 363, 566));
        assert_eq!(s.slot_at(360.0, 363.0), 0);
        assert_eq!(s.slot_at(360.0 + 55.9, 363.0), 0);
        assert_eq!(s.slot_at(360.0 + 56.0, 363.0), -1, "écart entre cases");
        assert_eq!(s.slot_at(360.0 + 63.0 * 8.0, 363.0 + 63.0 * 2.0), 26);
        assert_eq!(s.slot_at(360.0 + 63.0 * 4.0 + 1.0, 567.0), 31);
        assert_eq!(s.slot_at(0.0, 0.0), -1);
        for i in 0..SLOTS {
            let (x, y) = s.slot_pixels(i);
            assert_eq!(s.slot_at(x as f64 + 1.0, y as f64 + 1.0), i as i32);
        }
    }

    #[test]
    fn update_actions() {
        let mut s = InventoryScreen::new(1280, 720);
        let mut i = UiInput {
            width: 1280,
            height: 720,
            mouse_x: 361.0,
            mouse_y: 364.0,
            primary_pressed: true,
            ..Default::default()
        };
        assert_eq!(s.update(&i), Some(InventoryAction::Click(0)));
        i.pause_pressed = true;
        assert_eq!(s.update(&i), Some(InventoryAction::Close));
        i.width = 1600;
        i.pause_pressed = false;
        i.primary_pressed = false;
        assert_eq!(s.update(&i), None);
        assert_eq!(s.inv_x, (1600 - INV_W) / 2);
    }

    #[test]
    fn draw_slots_and_held() {
        let s = InventoryScreen::new(1280, 720);
        let mut slots = [None; SLOTS];
        slots[0] = Some(SlotView {
            icon_layer: 1,
            count: 1,
            name: "dirt",
        });
        slots[30] = Some(SlotView {
            icon_layer: 2,
            count: 12,
            name: "sand",
        });
        let input = UiInput {
            width: 1280,
            height: 720,
            mouse_x: 361.0,
            mouse_y: 364.0,
            ..Default::default()
        };
        let mut dl = DrawList::new(1280, 720);
        let held = Some(SlotView {
            icon_layer: 3,
            count: 1,
            name: "stone",
        });
        s.draw(&mut dl, &input, &slots, held);
        let tex: Vec<_> = dl.batches().iter().map(|b| b.texture).collect();
        assert_eq!(tex[0], TextureRef::Ui(UiTexture::White)); // voile
        assert_eq!(tex[1], TextureRef::Ui(UiTexture::Inventory));
        assert_eq!(tex[2], TextureRef::Ui(UiTexture::White)); // survol case 0
        assert_eq!(tex[3], TextureRef::BlockLayer(1));
        assert_eq!(*tex.last().unwrap(), TextureRef::BlockLayer(3));
        // Icône tenue centrée sur la souris.
        let v = &dl.vertices()[dl.vertices().len() - 6];
        assert_eq!(v.pos, [361.0 - 22.0, 364.0 - 22.0]);
    }
}
