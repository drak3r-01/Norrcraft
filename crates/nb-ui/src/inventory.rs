//! Écran d'inventaire (`InventoryState`) : mise en page, hit-testing des cases et rendu.
//! La logique du curseur (prendre / poser / fusionner / rendre) est `nb_ecs::InventoryCursor`.

use crate::draw::{DrawList, WHITE};
use crate::hud::{format_block_name, SlotView};
use crate::input::UiInput;
use crate::textures::UiTexture;
use crate::widgets::{Element, ElementKind, Layout, Panel};
use nb_data::Station;

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

/// Première case de la grille de craft (numérotation commune avec `nb_ecs::crafting`).
pub const GRID_BASE: i32 = 36;
/// Case de résultat.
pub const OUTPUT_SLOT: i32 = 45;

/// Bouton de souris d'un clic de case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickButton {
    Left,
    Right,
}

/// Action demandée par l'écran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryAction {
    /// Clic sur une case (`-1` hors cases → la pile tenue est rendue). Cases : `0..36`
    /// inventaire, [`GRID_BASE`]`+k` grille de craft, [`OUTPUT_SLOT`] résultat.
    Click {
        slot: i32,
        button: ClickButton,
        shift: bool,
    },
    /// Inventaire / Échap : rendre la pile tenue puis revenir à PLAYING.
    Close,
}

/// Position (coin haut-gauche de l'objet, en unités de 176×166) de la case `k` de la grille.
pub fn cell_origin(station: Station, k: usize) -> (i32, i32) {
    match station {
        Station::Hand => (98 + 18 * (k as i32 % 2), 18 + 18 * (k as i32 / 2)),
        Station::Workbench => (30 + 18 * (k as i32 % 3), 17 + 18 * (k as i32 / 3)),
        Station::Forge => (40 + 36 * k as i32, 35),
    }
}

/// Position de la case de résultat (unités de 176×166).
pub fn output_origin(station: Station) -> (i32, i32) {
    match station {
        Station::Hand => (154, 28),
        Station::Workbench | Station::Forge => (124, 35),
    }
}

fn scale(v: i32) -> i32 {
    v * SLOT_SPACE / 18
}

fn background(station: Station) -> (UiTexture, [f32; 4]) {
    let tex = match station {
        Station::Hand => UiTexture::Inventory,
        Station::Workbench => UiTexture::Workbench,
        Station::Forge => UiTexture::Forge,
    };
    (
        tex,
        [0.0, 1.0 - 166.0 / 256.0, 176.0 / 256.0, 166.0 / 256.0],
    )
}

/// Titre affiché en haut de l'écran d'une station (vide pour l'inventaire).
fn title(station: Station) -> &'static str {
    match station {
        Station::Hand => "",
        Station::Workbench => "Workbench",
        Station::Forge => "Forge",
    }
}

/// Mise en page pour une taille de fenêtre (`buildUI`).
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryScreen {
    pub inv_x: i32,
    pub inv_y: i32,
    pub start_x: i32,
    pub main_y: i32,
    pub hotbar_y: i32,
    station: Station,
    ui: Panel,
    size: (i32, i32),
}

impl InventoryScreen {
    /// Écran d'inventaire (grille de craft 2×2 de la main).
    pub fn new(win_w: i32, win_h: i32) -> Self {
        Self::for_station(win_w, win_h, Station::Hand)
    }

    pub fn for_station(win_w: i32, win_h: i32, station: Station) -> Self {
        let inv_x = (win_w - INV_W) / 2;
        let inv_y = (win_h - INV_H) / 2;
        let start_x = inv_x + 28;
        let main_y = inv_y + 294;
        let hotbar_y = inv_y + 497;
        let mut ui = Panel::new(0, 0, win_w, win_h, None);
        let (tex, gl_uv) = background(station);
        ui.add(Element::new(
            inv_x,
            inv_y,
            INV_W,
            INV_H,
            ElementKind::Region { tex, gl_uv },
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
        let mut screen = Self {
            inv_x,
            inv_y,
            start_x,
            main_y,
            hotbar_y,
            station,
            ui,
            size: (win_w, win_h),
        };
        // Survol des cases de craft et du résultat.
        let mut extra = Vec::new();
        for k in 0..screen.grid_len() {
            let (px, py) = screen.slot_pixels_of(GRID_BASE + k as i32);
            let mut e = slot();
            e.x = px;
            e.y = py;
            extra.push(e);
        }
        let (px, py) = screen.slot_pixels_of(OUTPUT_SLOT);
        let mut e = slot();
        e.x = px;
        e.y = py;
        extra.push(e);
        for e in extra {
            screen.ui.add(e);
        }
        screen
    }

    /// Station affichée.
    pub fn station(&self) -> Station {
        self.station
    }

    /// Change de station (reconstruit la mise en page).
    pub fn set_station(&mut self, station: Station) {
        if self.station != station {
            *self = Self::for_station(self.size.0, self.size.1, station);
        }
    }

    /// Nombre de cases de la grille de craft.
    pub fn grid_len(&self) -> usize {
        let (w, h) = self.station.grid_size();
        w * h
    }

    /// Reconstruit la mise en page si la fenêtre a changé de taille.
    pub fn resize(&mut self, win_w: i32, win_h: i32) {
        if self.size != (win_w, win_h) {
            *self = Self::for_station(win_w, win_h, self.station);
        }
    }

    /// Coin haut-gauche de n'importe quelle case (`0..36`, grille, résultat).
    pub fn slot_pixels_of(&self, slot: i32) -> (i32, i32) {
        if (0..SLOTS as i32).contains(&slot) {
            return self.slot_pixels(slot as usize);
        }
        let (ox, oy) = if slot == OUTPUT_SLOT {
            output_origin(self.station)
        } else {
            cell_origin(self.station, (slot - GRID_BASE) as usize)
        };
        (self.inv_x + scale(ox), self.inv_y + scale(oy))
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
        for k in 0..self.grid_len() as i32 {
            let (x, y) = self.slot_pixels_of(GRID_BASE + k);
            if inside(x, y) {
                return GRID_BASE + k;
            }
        }
        let (x, y) = self.slot_pixels_of(OUTPUT_SLOT);
        if inside(x, y) {
            return OUTPUT_SLOT;
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

    /// `update` : fermeture (Inventaire ou Pause) puis clic gauche ou droit.
    pub fn update(&mut self, input: &UiInput) -> Option<InventoryAction> {
        self.resize(input.width, input.height);
        if input.inventory_pressed || input.pause_pressed {
            return Some(InventoryAction::Close);
        }
        let button = if input.primary_pressed {
            ClickButton::Left
        } else if input.secondary_pressed {
            ClickButton::Right
        } else {
            return None;
        };
        Some(InventoryAction::Click {
            slot: self.slot_at(input.mouse_x, input.mouse_y),
            button,
            shift: input.shift_down,
        })
    }

    /// Rendu sans grille de craft (écran d'inventaire nu, tests).
    pub fn draw(
        &self,
        dl: &mut DrawList,
        input: &UiInput,
        slots: &[Option<SlotView<'_>>; SLOTS],
        held: Option<SlotView<'_>>,
    ) {
        self.draw_craft(dl, input, slots, held, &[], None);
    }

    /// Rendu (après la scène) : voile, fond, surbrillances, icônes, grille de craft, résultat,
    /// pile tenue au curseur, infobulle de la case survolée.
    pub fn draw_craft(
        &self,
        dl: &mut DrawList,
        input: &UiInput,
        slots: &[Option<SlotView<'_>>; SLOTS],
        held: Option<SlotView<'_>>,
        grid: &[Option<SlotView<'_>>],
        output: Option<SlotView<'_>>,
    ) {
        let (w, h) = (input.width as f32, input.height as f32);
        dl.fill(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
        self.ui.draw(dl, input.mouse_x, input.mouse_y);
        let icon = ICON_SIZE as f32;
        let draw_slot = |dl: &mut DrawList, s: &SlotView<'_>, px: i32, py: i32| {
            dl.icon(
                s.icon_layer,
                s.style,
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
        };
        for (i, s) in slots.iter().enumerate() {
            let Some(s) = s else { continue };
            let (px, py) = self.slot_pixels(i);
            draw_slot(dl, s, px, py);
        }
        for (k, s) in grid.iter().enumerate().take(self.grid_len()) {
            let Some(s) = s else { continue };
            let (px, py) = self.slot_pixels_of(GRID_BASE + k as i32);
            draw_slot(dl, s, px, py);
        }
        if let Some(s) = &output {
            let (px, py) = self.slot_pixels_of(OUTPUT_SLOT);
            draw_slot(dl, s, px, py);
        }
        let t = title(self.station);
        if !t.is_empty() {
            dl.text(
                t,
                (self.inv_x + 28) as f32,
                (self.inv_y + 18) as f32,
                2.0,
                [0.25, 0.25, 0.25, 1.0],
            );
        }
        if let Some(s) = held {
            let hx = input.mouse_x as f32 - icon / 2.0;
            let hy = input.mouse_y as f32 - icon / 2.0;
            dl.icon(s.icon_layer, s.style, hx, hy, icon);
            if s.count > 1 {
                count(dl, s.count, hx + icon, hy + icon);
            }
        } else {
            // Infobulle : nom de l'objet de la case survolée.
            let hovered = self.slot_at(input.mouse_x, input.mouse_y);
            let view = match hovered {
                OUTPUT_SLOT => output,
                s if s >= GRID_BASE => grid.get((s - GRID_BASE) as usize).copied().flatten(),
                s if s >= 0 => slots[s as usize],
                _ => None,
            };
            if let Some(v) = view {
                tooltip(dl, input, &format_block_name(v.name));
            }
        }
    }
}

/// Fond sombre + texte blanc à côté du curseur (borné à la fenêtre).
fn tooltip(dl: &mut DrawList, input: &UiInput, text: &str) {
    const SCALE: f32 = 2.0;
    let w = crate::font::measure_width(text, SCALE) + 16.0;
    let h = 10.0 * SCALE + 12.0;
    let x = (input.mouse_x as f32 + 18.0)
        .min(input.width as f32 - w)
        .max(0.0);
    let y = (input.mouse_y as f32 - h - 4.0).max(0.0);
    dl.fill(x, y, w, h, [0.06, 0.02, 0.12, 0.92]);
    dl.text(text, x + 8.0, y + 6.0, SCALE, WHITE);
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
        assert_eq!(
            s.update(&i),
            Some(InventoryAction::Click {
                slot: 0,
                button: ClickButton::Left,
                shift: false
            })
        );
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
            style: Default::default(),
            count: 1,
            name: "dirt",
        });
        slots[30] = Some(SlotView {
            icon_layer: 2,
            style: Default::default(),
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
            style: Default::default(),
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

    #[test]
    fn hand_grid_and_output_hit_testing() {
        let s = InventoryScreen::new(1280, 720);
        assert_eq!(s.grid_len(), 4);
        // Cases de la grille 2×2 : origines (98,18) (116,18) (98,36) (116,36) × 3,5.
        assert_eq!(s.slot_pixels_of(GRID_BASE), (332 + 343, 69 + 63));
        assert_eq!(s.slot_pixels_of(GRID_BASE + 3), (332 + 406, 69 + 126));
        assert_eq!(s.slot_pixels_of(OUTPUT_SLOT), (332 + 539, 69 + 98));
        for k in 0..4 {
            let (x, y) = s.slot_pixels_of(GRID_BASE + k);
            assert_eq!(s.slot_at(x as f64 + 1.0, y as f64 + 1.0), GRID_BASE + k);
        }
        let (x, y) = s.slot_pixels_of(OUTPUT_SLOT);
        assert_eq!(s.slot_at(x as f64 + 55.0, y as f64 + 55.0), OUTPUT_SLOT);
        assert_eq!(s.slot_at(x as f64 + 56.0, y as f64), -1);
    }

    #[test]
    fn stations_have_their_own_layouts() {
        let wb = InventoryScreen::for_station(1280, 720, Station::Workbench);
        assert_eq!(wb.grid_len(), 9);
        for k in 0..9 {
            let (x, y) = wb.slot_pixels_of(GRID_BASE + k);
            assert_eq!(wb.slot_at(x as f64 + 2.0, y as f64 + 2.0), GRID_BASE + k);
        }
        let fg = InventoryScreen::for_station(1280, 720, Station::Forge);
        assert_eq!(fg.grid_len(), 2);
        let (x, y) = fg.slot_pixels_of(GRID_BASE + 2);
        // Hors grille : la case 38 n'existe pas à la fonderie.
        assert!(fg.slot_at(x as f64 + 2.0, y as f64 + 2.0) != GRID_BASE + 2);
        // Aucune case de station ne chevauche l'inventaire du joueur.
        for st in [Station::Hand, Station::Workbench, Station::Forge] {
            let s = InventoryScreen::for_station(1280, 720, st);
            let mut seen = std::collections::HashSet::new();
            let n = (s.grid_len() as i32) + 1;
            for i in 0..n {
                let slot = if i == n - 1 {
                    OUTPUT_SLOT
                } else {
                    GRID_BASE + i
                };
                let (x, y) = s.slot_pixels_of(slot);
                assert!(seen.insert((x, y)), "{st:?} : cases superposées");
                assert!(
                    y + SLOT_SIZE < s.main_y,
                    "{st:?} : empiète sur l'inventaire"
                );
                assert_eq!(s.slot_at(x as f64 + 3.0, y as f64 + 3.0), slot);
            }
        }
        let mut s = InventoryScreen::new(1280, 720);
        s.set_station(Station::Forge);
        assert_eq!(s.station(), Station::Forge);
        s.resize(1600, 900);
        assert_eq!(
            s.station(),
            Station::Forge,
            "la station survit au redimensionnement"
        );
    }

    #[test]
    fn right_click_and_shift_are_reported() {
        let mut s = InventoryScreen::new(1280, 720);
        let i = UiInput {
            width: 1280,
            height: 720,
            mouse_x: 361.0,
            mouse_y: 364.0,
            secondary_pressed: true,
            shift_down: true,
            ..Default::default()
        };
        assert_eq!(
            s.update(&i),
            Some(InventoryAction::Click {
                slot: 0,
                button: ClickButton::Right,
                shift: true
            })
        );
    }

    #[test]
    fn draw_craft_grid_output_and_tooltip() {
        let s = InventoryScreen::for_station(1280, 720, Station::Workbench);
        let slots = [None; SLOTS];
        let (ox, oy) = s.slot_pixels_of(OUTPUT_SLOT);
        let input = UiInput {
            width: 1280,
            height: 720,
            mouse_x: ox as f64 + 5.0,
            mouse_y: oy as f64 + 5.0,
            ..Default::default()
        };
        let view = |layer, count| SlotView {
            icon_layer: layer,
            style: Default::default(),
            count,
            name: "iron_ingot",
        };
        let mut grid = vec![None; 9];
        grid[4] = Some(view(7, 3));
        let mut dl = DrawList::new(1280, 720);
        s.draw_craft(&mut dl, &input, &slots, None, &grid, Some(view(9, 4)));
        let tex: Vec<_> = dl.batches().iter().map(|b| b.texture).collect();
        assert!(tex.contains(&crate::draw::TextureRef::Ui(UiTexture::Workbench)));
        assert!(tex.contains(&crate::draw::TextureRef::BlockLayer(7)));
        assert!(tex.contains(&crate::draw::TextureRef::BlockLayer(9)));
        // Infobulle au-dessus du résultat survolé : de nombreux quads de texte après les icônes.
        let with_tip = dl.quad_count();
        let mut dl2 = DrawList::new(1280, 720);
        let away = UiInput {
            mouse_x: 5.0,
            mouse_y: 5.0,
            ..input
        };
        s.draw_craft(&mut dl2, &away, &slots, None, &grid, Some(view(9, 4)));
        assert!(with_tip > dl2.quad_count() + 5);
    }
}
