//! Livre de recettes : liste de tous les crafts du jeu (filtrables par station), détail d'une
//! recette (motif de la grille, ingrédients) et remplissage automatique de la grille de craft.
//!
//! L'écran remplace le panneau de l'inventaire (même position et même taille) ; nb-game fournit
//! une [`RecipeCard`] par recette, dans l'ordre du livre.

use crate::draw::{DrawList, IconStyle, Rgba, WHITE};
use crate::font;
use crate::hud::format_block_name;
use crate::input::UiInput;
use crate::inventory::{INV_H, INV_W};
use nb_data::Station;

/// Icône d'un objet (couche d'atlas + style).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IconSpec {
    pub layer: u32,
    pub style: IconStyle,
}

/// Un ingrédient d'une recette.
#[derive(Clone, Debug, PartialEq)]
pub struct IngredientCard {
    pub icon: IconSpec,
    pub name: String,
    /// Nombre d'objets interchangeables (étiquette `#planks`…) ; 1 = objet précis.
    pub alternatives: usize,
    /// Quantité possédée par le joueur (tous objets compatibles confondus).
    pub have: i32,
}

/// Une recette telle qu'affichée.
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeCard {
    pub out: IconSpec,
    pub out_name: String,
    pub count: i32,
    pub station: Station,
    pub grid_w: usize,
    pub grid_h: usize,
    /// Cases du motif (ligne par ligne) ; pour une recette sans forme, les ingrédients à la file.
    pub cells: Vec<Option<IngredientCard>>,
    pub shapeless: bool,
    /// Tous les ingrédients sont dans l'inventaire.
    pub craftable: bool,
    /// La station courante peut exécuter la recette.
    pub usable: bool,
}

/// Onglets de filtre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    All,
    Craftable,
    Hand,
    Workbench,
    Forge,
}

const TABS: [(Tab, &str); 5] = [
    (Tab::All, "All"),
    (Tab::Craftable, "Craftable"),
    (Tab::Hand, "Hand"),
    (Tab::Workbench, "Workbench"),
    (Tab::Forge, "Forge"),
];

/// Action demandée par le livre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookAction {
    /// Retour à l'inventaire.
    Close,
    /// Remplir la grille de craft avec la recette `n` (indice dans le livre).
    Fill(usize),
}

const TILE: f32 = 64.0;
const TILE_STEP: f32 = 68.0;
const COLS: usize = 8;
const ROWS: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct RecipeBookScreen {
    pub tab: Tab,
    pub scroll: usize,
    pub selected: Option<usize>,
    size: (i32, i32),
}

impl Default for RecipeBookScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl RecipeBookScreen {
    pub fn new() -> Self {
        Self {
            tab: Tab::All,
            scroll: 0,
            selected: None,
            size: (1280, 720),
        }
    }

    fn origin(&self) -> (f32, f32) {
        (
            ((self.size.0 - INV_W) / 2) as f32,
            ((self.size.1 - INV_H) / 2) as f32,
        )
    }

    fn visible(&self, cards: &[RecipeCard]) -> Vec<usize> {
        cards
            .iter()
            .enumerate()
            .filter(|(_, c)| match self.tab {
                Tab::All => true,
                Tab::Craftable => c.craftable && c.usable,
                Tab::Hand => c.station == Station::Hand,
                Tab::Workbench => c.station == Station::Workbench,
                Tab::Forge => c.station == Station::Forge,
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn tab_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (ox, oy) = self.origin();
        (ox + 20.0 + i as f32 * 112.0, oy + 50.0, 106.0, 28.0)
    }

    fn tile_pos(&self, k: usize) -> (f32, f32) {
        let (ox, oy) = self.origin();
        (
            ox + 24.0 + (k % COLS) as f32 * TILE_STEP,
            oy + 92.0 + (k / COLS) as f32 * TILE_STEP,
        )
    }

    fn back_rect(&self) -> (f32, f32, f32, f32) {
        let (ox, oy) = self.origin();
        (ox + INV_W as f32 - 110.0, oy + 12.0, 90.0, 28.0)
    }

    fn fill_rect(&self) -> (f32, f32, f32, f32) {
        let (ox, oy) = self.origin();
        (
            ox + INV_W as f32 - 220.0,
            oy + INV_H as f32 - 56.0,
            200.0,
            36.0,
        )
    }

    fn hit(r: (f32, f32, f32, f32), x: f64, y: f64) -> bool {
        x >= f64::from(r.0)
            && x < f64::from(r.0 + r.2)
            && y >= f64::from(r.1)
            && y < f64::from(r.1 + r.3)
    }

    /// Entrées : onglets, sélection, défilement, retour, remplissage.
    pub fn update(&mut self, input: &UiInput, cards: &[RecipeCard]) -> Option<BookAction> {
        self.size = (input.width, input.height);
        if input.pause_pressed || input.inventory_pressed {
            return Some(BookAction::Close);
        }
        let list = self.visible(cards);
        let max_scroll = list.len().div_ceil(COLS).saturating_sub(ROWS);
        if input.scroll_y > 0.0 {
            self.scroll = self.scroll.saturating_sub(1);
        } else if input.scroll_y < 0.0 {
            self.scroll = (self.scroll + 1).min(max_scroll);
        }
        self.scroll = self.scroll.min(max_scroll);
        if !input.primary_pressed {
            return None;
        }
        let (mx, my) = (input.mouse_x, input.mouse_y);
        if Self::hit(self.back_rect(), mx, my) {
            return Some(BookAction::Close);
        }
        for (i, (tab, _)) in TABS.iter().enumerate() {
            if Self::hit(self.tab_rect(i), mx, my) {
                self.tab = *tab;
                self.scroll = 0;
                return None;
            }
        }
        for k in 0..COLS * ROWS {
            let Some(&ci) = list.get(self.scroll * COLS + k) else {
                break;
            };
            let (x, y) = self.tile_pos(k);
            if Self::hit((x, y, TILE, TILE), mx, my) {
                self.selected = Some(ci);
                return None;
            }
        }
        if let Some(sel) = self.selected {
            if cards.get(sel).is_some_and(|c| c.usable && c.craftable)
                && Self::hit(self.fill_rect(), mx, my)
            {
                return Some(BookAction::Fill(sel));
            }
        }
        None
    }

    /// Rendu complet (voile, panneau, onglets, tuiles, détail).
    pub fn draw(&self, dl: &mut DrawList, input: &UiInput, cards: &[RecipeCard]) {
        let (w, h) = (input.width as f32, input.height as f32);
        let (ox, oy) = (
            ((input.width - INV_W) / 2) as f32,
            ((input.height - INV_H) / 2) as f32,
        );
        let (pw, ph) = (INV_W as f32, INV_H as f32);
        dl.fill(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
        // Panneau : cuir sombre, double filet.
        dl.fill(ox - 4.0, oy - 4.0, pw + 8.0, ph + 8.0, [0.07, 0.05, 0.03, 1.0]);
        dl.fill(ox, oy, pw, ph, [0.27, 0.2, 0.13, 1.0]);
        dl.fill(ox + 6.0, oy + 6.0, pw - 12.0, ph - 12.0, [0.33, 0.25, 0.16, 1.0]);
        dl.text("Recipe Book", ox + 24.0, oy + 14.0, 2.6, [1.0, 0.92, 0.7, 1.0]);
        let total = cards.len();
        let craft_n = cards.iter().filter(|c| c.craftable && c.usable).count();
        dl.text(
            &format!("{total} recipes, {craft_n} craftable"),
            ox + 240.0,
            oy + 20.0,
            1.4,
            [0.85, 0.78, 0.6, 1.0],
        );
        let b = self.back_rect();
        let hover_back = Self::hit(b, input.mouse_x, input.mouse_y);
        button(dl, b, "Back", hover_back, true);
        for (i, (tab, label)) in TABS.iter().enumerate() {
            let r = self.tab_rect(i);
            let on = *tab == self.tab;
            let hover = Self::hit(r, input.mouse_x, input.mouse_y);
            let col: Rgba = if on {
                [0.62, 0.45, 0.2, 1.0]
            } else if hover {
                [0.45, 0.34, 0.2, 1.0]
            } else {
                [0.22, 0.16, 0.1, 1.0]
            };
            dl.fill(r.0, r.1, r.2, r.3, col);
            let tw = font::measure_width(label, 1.8);
            dl.text(label, r.0 + (r.2 - tw) / 2.0, r.1 + 6.0, 1.8, WHITE);
        }

        let list = self.visible(cards);
        let mut hovered: Option<usize> = None;
        for k in 0..COLS * ROWS {
            let Some(&ci) = list.get(self.scroll * COLS + k) else {
                break;
            };
            let c = &cards[ci];
            let (x, y) = self.tile_pos(k);
            let hover = Self::hit((x, y, TILE, TILE), input.mouse_x, input.mouse_y);
            if hover {
                hovered = Some(ci);
            }
            let base: Rgba = if c.craftable && c.usable {
                [0.28, 0.5, 0.26, 1.0]
            } else if c.craftable {
                [0.45, 0.4, 0.2, 1.0]
            } else {
                [0.2, 0.15, 0.1, 1.0]
            };
            dl.fill(x, y, TILE, TILE, [0.05, 0.03, 0.02, 1.0]);
            dl.fill(x + 2.0, y + 2.0, TILE - 4.0, TILE - 4.0, base);
            if self.selected == Some(ci) {
                dl.fill(x, y, TILE, 3.0, [1.0, 0.85, 0.3, 1.0]);
                dl.fill(x, y + TILE - 3.0, TILE, 3.0, [1.0, 0.85, 0.3, 1.0]);
                dl.fill(x, y, 3.0, TILE, [1.0, 0.85, 0.3, 1.0]);
                dl.fill(x + TILE - 3.0, y, 3.0, TILE, [1.0, 0.85, 0.3, 1.0]);
            } else if hover {
                dl.fill(x + 2.0, y + 2.0, TILE - 4.0, TILE - 4.0, [1.0, 1.0, 1.0, 0.15]);
            }
            let dim = if c.craftable { 1.0 } else { 0.55 };
            dl.icon(c.out.layer, tinted(c.out.style, dim), x + 10.0, y + 10.0, 44.0);
            if c.count > 1 {
                let t = c.count.to_string();
                dl.text(&t, x + TILE - 8.0 - t.len() as f32 * 7.3 * 1.8, y + TILE - 22.0, 1.8, WHITE);
            }
            // Pastille de station.
            let (tag, tc): (&str, Rgba) = match c.station {
                Station::Hand => ("H", [0.7, 0.85, 1.0, 1.0]),
                Station::Workbench => ("W", [1.0, 0.8, 0.45, 1.0]),
                Station::Forge => ("F", [1.0, 0.55, 0.35, 1.0]),
            };
            dl.text(tag, x + 5.0, y + 4.0, 1.5, tc);
        }
        // Barre de défilement.
        let rows = list.len().div_ceil(COLS);
        if rows > ROWS {
            let (sx, sy) = (ox + pw - 26.0, oy + 92.0);
            let sh = ROWS as f32 * TILE_STEP - 4.0;
            dl.fill(sx, sy, 8.0, sh, [0.15, 0.1, 0.06, 1.0]);
            let th = sh * ROWS as f32 / rows as f32;
            let ty = sy + (sh - th) * self.scroll as f32 / (rows - ROWS) as f32;
            dl.fill(sx, ty, 8.0, th, [0.8, 0.65, 0.35, 1.0]);
        }
        if list.is_empty() {
            dl.text("Nothing here yet.", ox + 30.0, oy + 110.0, 2.0, [0.9, 0.8, 0.6, 1.0]);
        }

        // Détail de la recette sélectionnée.
        let dy = oy + 92.0 + ROWS as f32 * TILE_STEP + 12.0;
        dl.fill(ox + 18.0, dy, pw - 36.0, oy + ph - 14.0 - dy, [0.18, 0.13, 0.08, 1.0]);
        match self.selected.and_then(|i| cards.get(i).map(|c| (i, c))) {
            Some((_, c)) => self.draw_detail(dl, input, c, ox, dy),
            None => {
                dl.text(
                    "Select a recipe to see how it is made.",
                    ox + 40.0,
                    dy + 64.0,
                    2.0,
                    [0.85, 0.78, 0.6, 1.0],
                );
            }
        }
        // Infobulle d'une tuile.
        if let Some(ci) = hovered {
            let c = &cards[ci];
            let text = format!(
                "{} x{}  ({})",
                format_block_name(&c.out_name),
                c.count,
                station_name(c.station)
            );
            let tw = font::measure_width(&text, 2.0) + 16.0;
            let x = (input.mouse_x as f32 + 16.0).min(w - tw);
            let y = (input.mouse_y as f32 - 34.0).max(0.0);
            dl.fill(x, y, tw, 30.0, [0.06, 0.02, 0.12, 0.94]);
            dl.text(&text, x + 8.0, y + 6.0, 2.0, WHITE);
        }
    }

    fn draw_detail(&self, dl: &mut DrawList, input: &UiInput, c: &RecipeCard, ox: f32, dy: f32) {
        let cell = 46.0;
        let gx = ox + 40.0;
        let gy = dy + 16.0;
        let (gw, gh) = if c.shapeless {
            let n = c.cells.len().max(1);
            (n.min(3), n.div_ceil(3))
        } else {
            (c.grid_w, c.grid_h)
        };
        let mut hover_ing: Option<&IngredientCard> = None;
        for r in 0..gh {
            for col in 0..gw {
                let (x, y) = (gx + col as f32 * (cell + 4.0), gy + r as f32 * (cell + 4.0));
                dl.fill(x, y, cell, cell, [0.05, 0.03, 0.02, 1.0]);
                dl.fill(x + 2.0, y + 2.0, cell - 4.0, cell - 4.0, [0.4, 0.33, 0.25, 1.0]);
                if let Some(Some(ing)) = c.cells.get(r * gw + col) {
                    dl.icon(ing.icon.layer, ing.icon.style, x + 4.0, y + 4.0, cell - 8.0);
                    if ing.alternatives > 1 {
                        dl.text("*", x + cell - 12.0, y + 2.0, 1.8, [1.0, 0.9, 0.4, 1.0]);
                    }
                    if input.mouse_x >= f64::from(x)
                        && input.mouse_x < f64::from(x + cell)
                        && input.mouse_y >= f64::from(y)
                        && input.mouse_y < f64::from(y + cell)
                    {
                        hover_ing = Some(ing);
                    }
                }
            }
        }
        // Flèche et résultat.
        let ax = gx + 3.0 * (cell + 4.0) + 10.0;
        let ay = gy + 1.0 * (cell + 4.0) + 8.0;
        dl.text("=>", ax, ay, 3.0, [1.0, 0.9, 0.6, 1.0]);
        let rx = ax + 56.0;
        dl.fill(rx, ay - 14.0, 60.0, 60.0, [0.05, 0.03, 0.02, 1.0]);
        dl.fill(rx + 2.0, ay - 12.0, 56.0, 56.0, [0.4, 0.33, 0.25, 1.0]);
        dl.icon(c.out.layer, c.out.style, rx + 8.0, ay - 6.0, 44.0);
        if c.count > 1 {
            dl.text(&format!("x{}", c.count), rx + 40.0, ay + 28.0, 1.8, WHITE);
        }
        // Texte : nom, station, ingrédients.
        let tx = rx + 86.0;
        dl.text(&format_block_name(&c.out_name), tx, dy + 14.0, 2.4, [1.0, 0.92, 0.7, 1.0]);
        let kind = if c.shapeless { "Shapeless" } else { "Shaped" };
        dl.text(
            &format!("{} - {kind}", station_name(c.station)),
            tx,
            dy + 40.0,
            1.7,
            [0.8, 0.9, 1.0, 1.0],
        );
        // Ingrédients (fusionnés par nom).
        let mut seen: Vec<(&str, i32, i32, usize)> = Vec::new();
        for ing in c.cells.iter().flatten() {
            if let Some(e) = seen.iter_mut().find(|e| e.0 == ing.name) {
                e.1 += 1;
            } else {
                seen.push((&ing.name, 1, ing.have, ing.alternatives));
            }
        }
        let mut y = dy + 66.0;
        for (name, need, have, alt) in seen.iter().take(5) {
            let ok = have >= need;
            let col: Rgba = if ok {
                [0.7, 1.0, 0.7, 1.0]
            } else {
                [1.0, 0.6, 0.55, 1.0]
            };
            let label = if *alt > 1 {
                format!("{}x any {} ({have})", need, name.trim_start_matches('#'))
            } else {
                format!("{}x {} ({have})", need, format_block_name(name))
            };
            dl.text(&label, tx, y, 1.6, col);
            y += 19.0;
        }
        let r = self.fill_rect();
        let hover = Self::hit(r, input.mouse_x, input.mouse_y);
        if !c.usable {
            let msg = format!("Needs a {}", station_name(c.station));
            dl.text(&msg, r.0, r.1 + 8.0, 1.8, [1.0, 0.7, 0.5, 1.0]);
        } else {
            button(dl, r, "Fill crafting grid", hover && c.craftable, c.craftable);
            if !c.craftable {
                dl.text("Missing ingredients", r.0 + 8.0, r.1 - 20.0, 1.5, [1.0, 0.6, 0.55, 1.0]);
            }
        }
        if let Some(ing) = hover_ing {
            let text = if ing.alternatives > 1 {
                format!("Any {} ({} kinds)", ing.name.trim_start_matches('#'), ing.alternatives)
            } else {
                format_block_name(&ing.name)
            };
            let tw = font::measure_width(&text, 1.8) + 14.0;
            let x = (input.mouse_x as f32 + 14.0).min(input.width as f32 - tw);
            let y = input.mouse_y as f32 - 30.0;
            dl.fill(x, y, tw, 26.0, [0.06, 0.02, 0.12, 0.94]);
            dl.text(&text, x + 7.0, y + 5.0, 1.8, WHITE);
        }
    }
}

fn tinted(style: IconStyle, _dim: f32) -> IconStyle {
    style
}

fn button(dl: &mut DrawList, r: (f32, f32, f32, f32), label: &str, hover: bool, enabled: bool) {
    let col: Rgba = if !enabled {
        [0.25, 0.2, 0.16, 1.0]
    } else if hover {
        [0.5, 0.72, 0.38, 1.0]
    } else {
        [0.36, 0.55, 0.28, 1.0]
    };
    dl.fill(r.0 - 2.0, r.1 - 2.0, r.2 + 4.0, r.3 + 4.0, [0.05, 0.03, 0.02, 1.0]);
    dl.fill(r.0, r.1, r.2, r.3, col);
    let tw = font::measure_width(label, 1.8);
    let c = if enabled { WHITE } else { [0.6, 0.55, 0.5, 1.0] };
    dl.text(label, r.0 + (r.2 - tw) / 2.0, r.1 + (r.3 - 18.0) / 2.0, 1.8, c);
}

/// Nom lisible d'une station.
pub fn station_name(s: Station) -> &'static str {
    match s {
        Station::Hand => "Hand (2x2)",
        Station::Workbench => "Workbench (3x3)",
        Station::Forge => "Forge",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(station: Station, craftable: bool) -> RecipeCard {
        RecipeCard {
            out: IconSpec::default(),
            out_name: "iron_pickaxe".into(),
            count: 1,
            station,
            grid_w: 3,
            grid_h: 3,
            cells: vec![
                Some(IngredientCard {
                    icon: IconSpec::default(),
                    name: "iron_ingot".into(),
                    alternatives: 1,
                    have: 2,
                });
                3
            ],
            shapeless: false,
            craftable,
            usable: true,
        }
    }

    fn input(x: f64, y: f64, click: bool) -> UiInput {
        UiInput {
            width: 1280,
            height: 720,
            mouse_x: x,
            mouse_y: y,
            primary_pressed: click,
            ..Default::default()
        }
    }

    #[test]
    fn tabs_filter_and_select_and_fill() {
        let cards = vec![
            card(Station::Hand, true),
            card(Station::Workbench, false),
            card(Station::Forge, true),
        ];
        let mut s = RecipeBookScreen::new();
        // Premier rendu pour fixer la taille.
        assert!(s.update(&input(0.0, 0.0, false), &cards).is_none());
        assert_eq!(s.visible(&cards).len(), 3);
        // Onglet « Workbench ».
        let (x, y, _, _) = s.tab_rect(3);
        s.update(&input(f64::from(x) + 5.0, f64::from(y) + 5.0, true), &cards);
        assert_eq!(s.tab, Tab::Workbench);
        assert_eq!(s.visible(&cards), vec![1]);
        // Onglet « Craftable ».
        let (x, y, _, _) = s.tab_rect(1);
        s.update(&input(f64::from(x) + 5.0, f64::from(y) + 5.0, true), &cards);
        assert_eq!(s.visible(&cards), vec![0, 2]);
        // Sélection de la première tuile puis remplissage.
        let (tx, ty) = s.tile_pos(0);
        s.update(&input(f64::from(tx) + 5.0, f64::from(ty) + 5.0, true), &cards);
        assert_eq!(s.selected, Some(0));
        let (fx, fy, _, _) = s.fill_rect();
        let a = s.update(&input(f64::from(fx) + 5.0, f64::from(fy) + 5.0, true), &cards);
        assert_eq!(a, Some(BookAction::Fill(0)));
        // Une recette non réalisable ne se remplit pas.
        s.tab = Tab::All;
        s.selected = Some(1);
        let a = s.update(&input(f64::from(fx) + 5.0, f64::from(fy) + 5.0, true), &cards);
        assert_eq!(a, None);
        // Retour.
        let (bx, by, _, _) = s.back_rect();
        let a = s.update(&input(f64::from(bx) + 5.0, f64::from(by) + 5.0, true), &cards);
        assert_eq!(a, Some(BookAction::Close));
    }

    #[test]
    fn draws_without_panicking_and_scrolls() {
        let cards: Vec<_> = (0..70).map(|i| card(Station::Hand, i % 2 == 0)).collect();
        let mut s = RecipeBookScreen::new();
        let mut dl = DrawList::new(1280, 720);
        s.selected = Some(3);
        let mut i = input(600.0, 300.0, false);
        i.scroll_y = -1.0;
        s.update(&i, &cards);
        assert_eq!(s.scroll, 1);
        s.draw(&mut dl, &i, &cards);
        assert!(dl.quad_count() > 100);
    }
}
