//! HUD de jeu (`PlayingState`) : viseur, barre d'inventaire, sélecteur, icônes et quantités,
//! nom de l'objet tenu, cœurs et faim (mode survie).

use crate::draw::{DrawList, IconStyle, WHITE};
use crate::font;
use crate::input::UiInput;
use crate::state::GameState;
use crate::textures::UiTexture;

pub const HOTBAR_W: i32 = 544;
pub const HOTBAR_H: i32 = 64;
pub const ICON_SIZE: f32 = 36.0;
pub const HUD_ICON_SIZE: f32 = 22.0;
pub const HUD_ICON_GAP: f32 = 2.0;
pub const CROSSHAIR_SIZE: i32 = 32;
pub const ITEM_NAME_SHOW: f32 = 1.0;
pub const ITEM_NAME_FADE: f32 = 0.5;

/// Contenu d'une case (vue de `ItemStack`) : couche d'icône = `getTextureIndex(0)` du bloc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotView<'a> {
    pub icon_layer: u32,
    pub style: IconStyle,
    pub count: i32,
    /// Nom interne du bloc (`oak_log`).
    pub name: &'a str,
}

/// Données de jeu nécessaires au HUD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudView<'a> {
    pub hotbar: [Option<SlotView<'a>>; 9],
    pub selected: usize,
    pub creative: bool,
    /// `HealthComponent` (absent → 0 cœur).
    pub health: Option<(f32, f32)>,
}

/// `formatBlockName` : `oak_log` → `Oak Log`.
pub fn format_block_name(name: &str) -> String {
    let mut out = String::new();
    for part in name.split('_').filter(|p| !p.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        let mut cs = part.chars();
        if let Some(c) = cs.next() {
            out.extend(c.to_uppercase());
            out.push_str(cs.as_str());
        }
    }
    out
}

/// Géométrie de la barre pour une fenêtre (`buildUI`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotbarLayout {
    pub hotbar_x: i32,
    pub hotbar_y: i32,
}

impl HotbarLayout {
    pub fn new(win_w: i32, win_h: i32) -> Self {
        Self {
            hotbar_x: (win_w - HOTBAR_W) / 2,
            hotbar_y: win_h - HOTBAR_H - 10,
        }
    }

    pub fn slot_width() -> f32 {
        HOTBAR_W as f32 / 9.0
    }

    /// Position du sélecteur (72 px) sur la case `selected`.
    pub fn selector(&self, selected: usize) -> (i32, i32) {
        let size = HOTBAR_H + 8;
        let sw = Self::slot_width();
        let x = (self.hotbar_x as f32 + selected as f32 * sw + (sw - size as f32) / 2.0) as i32;
        (x, self.hotbar_y + (HOTBAR_H - size) / 2)
    }

    /// Coin haut-gauche de l'icône de la case `i`.
    pub fn icon(&self, i: usize) -> (f32, f32) {
        let sw = Self::slot_width();
        (
            self.hotbar_x as f32 + i as f32 * sw + (sw - ICON_SIZE) / 2.0,
            self.hotbar_y as f32 + (HOTBAR_H as f32 - ICON_SIZE) / 2.0,
        )
    }
}

/// État du HUD (`PlayingState`).
#[derive(Clone, Debug, PartialEq)]
pub struct Hud {
    last_selected: Option<usize>,
    last_held: Option<String>,
    item_name_timer: f32,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            last_selected: None,
            last_held: None,
            item_name_timer: ITEM_NAME_SHOW + ITEM_NAME_FADE,
        }
    }
}

impl Hud {
    pub fn new() -> Self {
        Self::default()
    }

    /// `enter()`.
    pub fn enter(&mut self) {
        *self = Self::default();
    }

    /// `update` : transitions (Pause → PAUSED, Inventaire → INVENTORY) puis, si l'état reste
    /// PLAYING (l'appelant fait alors `world.update`), minuterie du nom d'objet.
    pub fn update(&mut self, input: &UiInput, view: &HudView<'_>) -> Option<GameState> {
        if input.pause_pressed {
            return Some(GameState::Paused);
        }
        if input.inventory_pressed {
            return Some(GameState::Inventory);
        }
        self.track_held(input.dt, view);
        None
    }

    /// Partie « nom d'objet » de `update` (après `world.update`).
    pub fn track_held(&mut self, dt: f64, view: &HudView<'_>) {
        let selected = view.selected;
        let held = view.hotbar.get(selected).copied().flatten().map(|s| s.name);
        if Some(selected) != self.last_selected || held != self.last_held.as_deref() {
            self.last_selected = Some(selected);
            self.last_held = held.map(str::to_owned);
            if held.is_some() {
                self.item_name_timer = 0.0;
            }
        }
        self.item_name_timer =
            (self.item_name_timer + dt as f32).min(ITEM_NAME_SHOW + ITEM_NAME_FADE);
    }

    /// `itemNameAlpha()`.
    pub fn item_name_alpha(&self) -> f32 {
        let t = self.item_name_timer;
        if t >= ITEM_NAME_SHOW + ITEM_NAME_FADE {
            0.0
        } else if t < ITEM_NAME_SHOW {
            1.0
        } else {
            1.0 - (t - ITEM_NAME_SHOW) / ITEM_NAME_FADE
        }
    }

    /// Rendu du HUD (après la scène).
    pub fn draw(&self, dl: &mut DrawList, win_w: i32, win_h: i32, view: &HudView<'_>) {
        let l = HotbarLayout::new(win_w, win_h);
        // Éléments statiques : viseur, barre, sélecteur.
        let c = CROSSHAIR_SIZE;
        dl.image(
            UiTexture::Crosshair,
            ((win_w - c) / 2) as f32,
            ((win_h - c) / 2) as f32,
            c as f32,
            c as f32,
            WHITE,
        );
        dl.image(
            UiTexture::Hotbar,
            l.hotbar_x as f32,
            l.hotbar_y as f32,
            HOTBAR_W as f32,
            HOTBAR_H as f32,
            WHITE,
        );
        let (sx, sy) = l.selector(view.selected);
        let ss = (HOTBAR_H + 8) as f32;
        dl.image(UiTexture::Selector, sx as f32, sy as f32, ss, ss, WHITE);

        for (i, slot) in view.hotbar.iter().enumerate() {
            let Some(s) = slot else { continue };
            let (ix, iy) = l.icon(i);
            dl.icon(s.icon_layer, s.style, ix, iy, ICON_SIZE);
            if s.count > 1 {
                let text = s.count.to_string();
                let scale = 1.8;
                let tx = ix + ICON_SIZE - text.len() as f32 * 7.3 * scale - 2.0;
                let ty = iy + ICON_SIZE - 10.0 * scale - 2.0;
                dl.text(&text, tx, ty, scale, WHITE);
            }
        }

        let alpha = self.item_name_alpha();
        if alpha > 0.0 {
            if let Some(Some(held)) = self.last_selected.and_then(|i| view.hotbar.get(i)) {
                let name = format_block_name(held.name);
                let scale = 2.2;
                let tw = font::measure_width(&name, scale);
                let tx = l.hotbar_x as f32 + HOTBAR_W as f32 / 2.0 - tw / 2.0;
                let ty = (l.hotbar_y - 30) as f32;
                dl.text(&name, tx, ty, scale, [1.0, 1.0, 1.0, alpha]);
            }
        }

        if !view.creative {
            self.draw_vitals(dl, &l, view);
        }
    }

    fn draw_vitals(&self, dl: &mut DrawList, l: &HotbarLayout, view: &HudView<'_>) {
        let step = HUD_ICON_SIZE + HUD_ICON_GAP;
        let hud_y = l.hotbar_y as f32 - HUD_ICON_SIZE - 4.0;
        let scaled = match view.health {
            Some((h, max)) if max > 0.0 => (h / max) * 10.0,
            _ => 0.0,
        };
        let s = HUD_ICON_SIZE;
        for i in 0..10 {
            let remaining = scaled - i as f32;
            let tex = if remaining >= 1.0 {
                UiTexture::HeartFull
            } else if remaining >= 0.5 {
                UiTexture::HeartHalf
            } else {
                UiTexture::HeartEmpty
            };
            dl.image(tex, l.hotbar_x as f32 + i as f32 * step, hud_y, s, s, WHITE);
        }
        // La faim n'est pas simulée : toujours 10 icônes pleines.
        let total = 10.0 * step - HUD_ICON_GAP;
        let start = l.hotbar_x as f32 + HOTBAR_W as f32 - total;
        for i in 0..10 {
            dl.image(
                UiTexture::HungerFull,
                start + i as f32 * step,
                hud_y,
                s,
                s,
                WHITE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::TextureRef;

    fn view<'a>(hotbar: [Option<SlotView<'a>>; 9], selected: usize, creative: bool) -> HudView<'a> {
        HudView {
            hotbar,
            selected,
            creative,
            health: Some((15.0, 20.0)),
        }
    }

    fn stone(count: i32) -> Option<SlotView<'static>> {
        Some(SlotView {
            icon_layer: 5,
            style: Default::default(),
            count,
            name: "cobble_stone",
        })
    }

    #[test]
    fn block_names() {
        assert_eq!(format_block_name("oak_log"), "Oak Log");
        assert_eq!(format_block_name("_a__b_"), "A B");
        assert_eq!(format_block_name("stone"), "Stone");
    }

    #[test]
    fn layout_1280x720() {
        let l = HotbarLayout::new(1280, 720);
        assert_eq!((l.hotbar_x, l.hotbar_y), (368, 646));
        // Case 0 : 368 + (60,44 - 72)/2 = 362,22 → 362 ; y = 646 - 4.
        assert_eq!(l.selector(0), (362, 642));
        assert_eq!(l.selector(8), (845, 642));
        let (ix, iy) = l.icon(0);
        assert!((ix - (368.0 + (544.0 / 9.0 - 36.0) / 2.0)).abs() < 1e-4);
        assert_eq!(iy, 660.0);
    }

    #[test]
    fn transitions() {
        let mut hud = Hud::new();
        let v = view([None; 9], 0, true);
        let mut i = UiInput {
            pause_pressed: true,
            inventory_pressed: true,
            ..Default::default()
        };
        assert_eq!(hud.update(&i, &v), Some(GameState::Paused));
        i.pause_pressed = false;
        assert_eq!(hud.update(&i, &v), Some(GameState::Inventory));
        i.inventory_pressed = false;
        assert_eq!(hud.update(&i, &v), None);
    }

    #[test]
    fn item_name_toast() {
        let mut hud = Hud::new();
        let mut bar = [None; 9];
        bar[2] = stone(1);
        let empty = view(bar, 0, true);
        hud.track_held(0.1, &empty);
        assert_eq!(hud.item_name_alpha(), 0.0, "case vide : pas de nom");
        let held = view(bar, 2, true);
        hud.track_held(0.0, &held);
        assert_eq!(hud.item_name_alpha(), 1.0);
        hud.track_held(1.25, &held);
        assert!((hud.item_name_alpha() - 0.5).abs() < 1e-5);
        hud.track_held(1.0, &held);
        assert_eq!(hud.item_name_alpha(), 0.0);
        // Même case, même bloc : pas de relance.
        hud.track_held(0.0, &held);
        assert_eq!(hud.item_name_alpha(), 0.0);
    }

    #[test]
    fn draw_lists_icons_counts_and_vitals() {
        let mut bar = [None; 9];
        bar[0] = stone(1);
        bar[3] = stone(64);
        let hud = Hud::new();
        let mut dl = DrawList::new(1280, 720);
        hud.draw(&mut dl, 1280, 720, &view(bar, 0, true));
        let icon_quads: u32 = dl
            .batches()
            .iter()
            .filter(|b| b.texture == TextureRef::BlockLayer(5))
            .map(|b| b.count / 6)
            .sum();
        assert_eq!(icon_quads, 2);
        let creative_quads = dl.quad_count();
        // 3 images + 2 icônes + glyphes de « 64 ».
        assert!(creative_quads > 5);
        let mut dl = DrawList::new(1280, 720);
        hud.draw(&mut dl, 1280, 720, &view(bar, 0, false));
        assert_eq!(dl.quad_count(), creative_quads + 20);
        // 15/20 → 7,5 cœurs : 7 pleins, 1 demi, 2 vides.
        let count = |t| {
            dl.batches()
                .iter()
                .filter(|b| b.texture == TextureRef::Ui(t))
                .map(|b| b.count / 6)
                .sum::<u32>()
        };
        assert_eq!(count(UiTexture::HeartFull), 7);
        assert_eq!(count(UiTexture::HeartHalf), 1);
        assert_eq!(count(UiTexture::HeartEmpty), 2);
        assert_eq!(count(UiTexture::HungerFull), 10);
    }
}
