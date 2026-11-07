//! Grille de craft et clics de l'écran d'inventaire / de l'établi / de la fonderie.
//!
//! La logique est pure (aucun accès au monde) : l'interface lui transmet les clics et lit l'état.
//! Numérotation des cases d'un écran : `0..36` = inventaire du joueur, [`GRID_BASE`]`+k` = case
//! `k` de la grille (ligne par ligne), [`OUTPUT_SLOT`] = case de résultat.

use nb_data::{ItemId, Output, RecipeBook, Station};

use crate::inventory::{Inventory, InventoryCursor, ItemStack, HOTBAR_START, SIZE, STACK_MAX};

/// Première case de la grille de craft dans la numérotation d'un écran.
pub const GRID_BASE: i32 = 36;
/// Case de résultat.
pub const OUTPUT_SLOT: i32 = 45;
/// Nombre maximal de cases d'une grille (établi 3×3).
pub const MAX_GRID: usize = 9;

/// Bouton de souris.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

/// Grille de craft d'une station et son résultat courant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CraftingGrid {
    station: Station,
    cells: [Option<ItemStack>; MAX_GRID],
    output: Option<Output>,
}

impl CraftingGrid {
    pub fn new(station: Station) -> Self {
        Self {
            station,
            cells: [None; MAX_GRID],
            output: None,
        }
    }

    pub fn station(&self) -> Station {
        self.station
    }

    /// `(largeur, hauteur)`.
    pub fn size(&self) -> (usize, usize) {
        self.station.grid_size()
    }

    pub fn len(&self) -> usize {
        let (w, h) = self.size();
        w * h
    }

    pub fn is_empty(&self) -> bool {
        self.cells[..self.len()].iter().all(Option::is_none)
    }

    /// Case `k` de la grille ; hors grille → `None`.
    pub fn cell(&self, k: usize) -> Option<ItemStack> {
        (k < self.len()).then(|| self.cells[k]).flatten()
    }

    pub fn set_cell(&mut self, k: usize, stack: Option<ItemStack>) {
        if k < self.len() {
            self.cells[k] = stack;
        }
    }

    /// Résultat que produirait un craft maintenant.
    pub fn output(&self) -> Option<ItemStack> {
        self.output.map(|o| ItemStack::new(o.item, o.count))
    }

    /// Recalcule le résultat d'après le contenu de la grille.
    pub fn update(&mut self, book: &RecipeBook) {
        let (w, h) = self.size();
        let items: Vec<Option<ItemId>> = self.cells[..w * h]
            .iter()
            .map(|c| c.map(|s| s.item))
            .collect();
        self.output = book.find(self.station, &items, w, h).map(|r| r.output);
    }

    /// Consomme un exemplaire de chaque ingrédient (le résultat est recalculé ensuite).
    fn consume(&mut self, book: &RecipeBook) {
        for c in &mut self.cells[..self.station.grid_size().0 * self.station.grid_size().1] {
            if let Some(s) = c {
                s.count -= 1;
                if s.count <= 0 {
                    *c = None;
                }
            }
        }
        self.update(book);
    }

    /// Vide la grille dans `inv` ; retourne le nombre d'objets **perdus** (inventaire plein).
    pub fn return_to(&mut self, inv: &mut Inventory, book: &RecipeBook) -> i32 {
        let mut lost = 0;
        for c in &mut self.cells {
            if let Some(s) = c.take() {
                lost += inv.add_stack(s);
            }
        }
        self.update(book);
        lost
    }
}

impl InventoryCursor {
    /// Clic sur la case `slot` d'un écran (`-1` ou hors numérotation = hors cases).
    ///
    /// * clic gauche : prendre / poser / fusionner / échanger la pile ;
    /// * clic droit : prendre la moitié / poser un exemplaire ;
    /// * Maj : déplacement rapide (barre ↔ grille de l'inventaire, grille → inventaire,
    ///   résultat → craft en série dans l'inventaire) ;
    /// * case de résultat : récupère le craft, consomme un exemplaire des ingrédients.
    pub fn click(
        &mut self,
        inv: &mut Inventory,
        grid: &mut CraftingGrid,
        book: &RecipeBook,
        slot: i32,
        button: Button,
        shift: bool,
    ) {
        let grid_len = grid.len() as i32;
        let in_inv = (0..SIZE as i32).contains(&slot);
        let in_grid = (GRID_BASE..GRID_BASE + grid_len).contains(&slot);
        let is_out = slot == OUTPUT_SLOT;
        if !(in_inv || in_grid || is_out) {
            if self.held.is_some() && !shift {
                self.return_held(inv);
            }
            return;
        }
        if is_out {
            self.click_output(inv, grid, book, shift);
            return;
        }
        if shift && self.held.is_none() {
            if in_inv {
                quick_move_inventory(inv, slot);
            } else {
                let k = (slot - GRID_BASE) as usize;
                if let Some(s) = grid.cell(k) {
                    let left = inv.add_stack(s);
                    grid.set_cell(k, (left > 0).then(|| ItemStack::new(s.item, left)));
                }
            }
            grid.update(book);
            return;
        }
        if in_inv {
            let mut cell = inv.slot(slot);
            match button {
                Button::Left => self.left_on_cell(&mut cell, slot),
                Button::Right => self.right_on_cell(&mut cell, slot),
            }
            inv.set_slot(slot, cell);
        } else {
            let k = (slot - GRID_BASE) as usize;
            let mut cell = grid.cell(k);
            match button {
                Button::Left => self.left_on_cell(&mut cell, slot),
                Button::Right => self.right_on_cell(&mut cell, slot),
            }
            grid.set_cell(k, cell);
            grid.update(book);
        }
    }

    fn click_output(
        &mut self,
        inv: &mut Inventory,
        grid: &mut CraftingGrid,
        book: &RecipeBook,
        shift: bool,
    ) {
        let Some(out) = grid.output() else {
            return;
        };
        if shift {
            // Craft en série : tant que la recette est valide et que tout rentre.
            for _ in 0..(STACK_MAX as usize * MAX_GRID) {
                let Some(out) = grid.output() else { break };
                if inv.room_for(out.item) < out.count {
                    break;
                }
                inv.add_stack(out);
                grid.consume(book);
            }
            return;
        }
        match self.held {
            None => {
                self.held = Some(out);
                self.held_from = -1;
                grid.consume(book);
            }
            Some(mut h) if h.item == out.item && h.count + out.count <= STACK_MAX => {
                h.count += out.count;
                self.held = Some(h);
                grid.consume(book);
            }
            Some(_) => {}
        }
    }

    /// Ferme l'écran : la pile tenue et la grille retournent dans l'inventaire. Retourne le
    /// nombre d'objets perdus (inventaire plein, comme `returnHeld` du Java).
    pub fn close(
        &mut self,
        inv: &mut Inventory,
        grid: &mut CraftingGrid,
        book: &RecipeBook,
    ) -> i32 {
        let held_lost = match self.held {
            Some(h) => {
                let before = inv.count_of(h.item);
                self.return_held(inv);
                (h.count - (inv.count_of(h.item) - before)).max(0)
            }
            None => 0,
        };
        grid.return_to(inv, book) + held_lost
    }
}

/// Déplacement rapide dans l'inventaire : barre rapide ↔ grille principale.
fn quick_move_inventory(inv: &mut Inventory, slot: i32) {
    let Some(mut s) = inv.slot(slot) else {
        return;
    };
    let range: Vec<i32> = if slot >= HOTBAR_START {
        (0..HOTBAR_START).collect()
    } else {
        (HOTBAR_START..SIZE as i32).collect()
    };
    for &i in &range {
        if s.count == 0 {
            break;
        }
        if let Some(mut t) = inv.slot(i) {
            if t.item == s.item && t.count < STACK_MAX {
                let n = (STACK_MAX - t.count).min(s.count);
                t.count += n;
                s.count -= n;
                inv.set_slot(i, Some(t));
            }
        }
    }
    for &i in &range {
        if s.count == 0 {
            break;
        }
        if inv.slot(i).is_none() {
            let n = s.count.min(STACK_MAX);
            inv.set_slot(i, Some(ItemStack::new(s.item, n)));
            s.count -= n;
        }
    }
    inv.set_slot(slot, (s.count > 0).then_some(s));
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_data::{BlockRegistry, ItemRegistry};

    struct Env {
        items: ItemRegistry,
        book: RecipeBook,
    }

    fn env() -> Env {
        let b = BlockRegistry::builtin_with_craft();
        let items = ItemRegistry::builtin(&b);
        let book = RecipeBook::builtin(&items);
        Env { items, book }
    }

    impl Env {
        fn id(&self, n: &str) -> ItemId {
            self.items.id_of(n).unwrap()
        }
        fn click(
            &self,
            c: &mut InventoryCursor,
            inv: &mut Inventory,
            g: &mut CraftingGrid,
            slot: i32,
            b: Button,
            shift: bool,
        ) {
            c.click(inv, g, &self.book, slot, b, shift);
        }
    }

    #[test]
    fn right_click_picks_half_and_places_one() {
        let e = env();
        let (mut inv, mut g, mut c) = (
            Inventory::new(),
            CraftingGrid::new(Station::Hand),
            InventoryCursor::default(),
        );
        let dirt = e.id("dirt");
        inv.set_slot(0, Some(ItemStack::new(dirt, 5)));
        e.click(&mut c, &mut inv, &mut g, 0, Button::Right, false);
        assert_eq!(
            c.held,
            Some(ItemStack::new(dirt, 3)),
            "moitié arrondie au-dessus"
        );
        assert_eq!(inv.slot(0), Some(ItemStack::new(dirt, 2)));
        // Pose d'un exemplaire dans une case vide, puis sur une pile identique.
        e.click(&mut c, &mut inv, &mut g, 1, Button::Right, false);
        assert_eq!(inv.slot(1), Some(ItemStack::new(dirt, 1)));
        e.click(&mut c, &mut inv, &mut g, 0, Button::Right, false);
        assert_eq!(inv.slot(0), Some(ItemStack::new(dirt, 3)));
        assert_eq!(c.held, Some(ItemStack::new(dirt, 1)));
        e.click(&mut c, &mut inv, &mut g, 1, Button::Right, false);
        assert_eq!(c.held, None);
        assert_eq!(c.held_from, -1);
        // Une seule pile : la moitié de 1 est 1.
        inv.set_slot(2, Some(ItemStack::new(dirt, 1)));
        e.click(&mut c, &mut inv, &mut g, 2, Button::Right, false);
        assert_eq!(inv.slot(2), None);
        assert_eq!(c.held, Some(ItemStack::new(dirt, 1)));
    }

    #[test]
    fn right_click_never_overfills_a_stack() {
        let e = env();
        let (mut inv, mut g, mut c) = (
            Inventory::new(),
            CraftingGrid::new(Station::Hand),
            InventoryCursor::default(),
        );
        let dirt = e.id("dirt");
        inv.set_slot(0, Some(ItemStack::new(dirt, 64)));
        c.held = Some(ItemStack::new(dirt, 3));
        e.click(&mut c, &mut inv, &mut g, 0, Button::Right, false);
        assert_eq!(inv.slot(0), Some(ItemStack::new(dirt, 64)));
        assert_eq!(c.held, Some(ItemStack::new(dirt, 3)));
    }

    #[test]
    fn hand_crafting_planks_into_sticks_consumes_ingredients() {
        let e = env();
        let (mut inv, mut g, mut c) = (
            Inventory::new(),
            CraftingGrid::new(Station::Hand),
            InventoryCursor::default(),
        );
        let planks = e.id("planks_oak");
        inv.set_slot(0, Some(ItemStack::new(planks, 4)));
        // Prendre la pile et poser un exemplaire dans les cases 0 et 2 de la grille (colonne).
        e.click(&mut c, &mut inv, &mut g, 0, Button::Left, false);
        e.click(&mut c, &mut inv, &mut g, GRID_BASE, Button::Right, false);
        assert_eq!(g.output(), None);
        e.click(
            &mut c,
            &mut inv,
            &mut g,
            GRID_BASE + 2,
            Button::Right,
            false,
        );
        assert_eq!(g.output(), Some(ItemStack::new(e.id("stick"), 4)));
        // Reposer le reste des planches en main, puis récupérer : les deux planches de la grille
        // sont consommées et la grille se vide.
        e.click(&mut c, &mut inv, &mut g, 0, Button::Left, false);
        assert_eq!(inv.slot(0), Some(ItemStack::new(planks, 2)));
        e.click(&mut c, &mut inv, &mut g, OUTPUT_SLOT, Button::Left, false);
        assert_eq!(g.cell(0), None);
        assert_eq!(g.cell(2), None);
        assert_eq!(g.output(), None);
        assert_eq!(c.held, Some(ItemStack::new(e.id("stick"), 4)));
    }

    #[test]
    fn output_click_with_other_item_in_hand_does_nothing() {
        let e = env();
        let (mut inv, mut g, mut c) = (
            Inventory::new(),
            CraftingGrid::new(Station::Hand),
            InventoryCursor::default(),
        );
        g.set_cell(0, Some(ItemStack::new(e.id("planks_oak"), 1)));
        g.set_cell(2, Some(ItemStack::new(e.id("planks_oak"), 1)));
        g.update(&e.book);
        c.held = Some(ItemStack::new(e.id("dirt"), 1));
        e.click(&mut c, &mut inv, &mut g, OUTPUT_SLOT, Button::Left, false);
        assert_eq!(c.held, Some(ItemStack::new(e.id("dirt"), 1)));
        assert!(g.output().is_some() && g.cell(0).is_some());
        // Même objet en main : les piles s'additionnent jusqu'à 64 seulement.
        c.held = Some(ItemStack::new(e.id("stick"), 61));
        e.click(&mut c, &mut inv, &mut g, OUTPUT_SLOT, Button::Left, false);
        assert_eq!(
            c.held,
            Some(ItemStack::new(e.id("stick"), 61)),
            "61 + 4 > 64"
        );
        c.held = Some(ItemStack::new(e.id("stick"), 60));
        e.click(&mut c, &mut inv, &mut g, OUTPUT_SLOT, Button::Left, false);
        assert_eq!(c.held, Some(ItemStack::new(e.id("stick"), 64)));
    }

    #[test]
    fn shift_click_output_crafts_in_bulk() {
        let e = env();
        let (mut inv, mut g, mut c) = (
            Inventory::new(),
            CraftingGrid::new(Station::Hand),
            InventoryCursor::default(),
        );
        g.set_cell(0, Some(ItemStack::new(e.id("planks_oak"), 5)));
        g.set_cell(2, Some(ItemStack::new(e.id("planks_oak"), 3)));
        g.update(&e.book);
        e.click(&mut c, &mut inv, &mut g, OUTPUT_SLOT, Button::Left, true);
        // 3 crafts possibles (limité par la plus petite pile) → 12 bâtons ; reste 2 planches.
        assert_eq!(inv.count_of(e.id("stick")), 12);
        assert_eq!(g.cell(0), Some(ItemStack::new(e.id("planks_oak"), 2)));
        assert_eq!(g.cell(2), None);
        assert_eq!(g.output(), None);
        assert_eq!(c.held, None);
    }

    #[test]
    fn shift_craft_stops_when_inventory_is_full() {
        let e = env();
        let (mut inv, mut g, mut c) = (
            Inventory::new(),
            CraftingGrid::new(Station::Hand),
            InventoryCursor::default(),
        );
        for i in 0..SIZE as i32 {
            inv.set_slot(i, Some(ItemStack::new(e.id("dirt"), 64)));
        }
        inv.set_slot(5, Some(ItemStack::new(e.id("stick"), 62)));
        g.set_cell(0, Some(ItemStack::new(e.id("planks_oak"), 9)));
        g.set_cell(2, Some(ItemStack::new(e.id("planks_oak"), 9)));
        g.update(&e.book);
        e.click(&mut c, &mut inv, &mut g, OUTPUT_SLOT, Button::Left, true);
        // Place pour 2 bâtons seulement : aucun craft n'est lancé (4 par craft).
        assert_eq!(inv.slot(5), Some(ItemStack::new(e.id("stick"), 62)));
        assert_eq!(g.cell(0).unwrap().count, 9);
    }

    #[test]
    fn shift_click_moves_between_hotbar_and_main_and_from_grid() {
        let e = env();
        let (mut inv, mut g, mut c) = (
            Inventory::new(),
            CraftingGrid::new(Station::Workbench),
            InventoryCursor::default(),
        );
        let stone = e.id("stone");
        inv.set_slot(27, Some(ItemStack::new(stone, 10)));
        inv.set_slot(3, Some(ItemStack::new(stone, 60)));
        e.click(&mut c, &mut inv, &mut g, 27, Button::Left, true);
        assert_eq!(inv.slot(27), None);
        assert_eq!(inv.slot(3), Some(ItemStack::new(stone, 64)));
        assert_eq!(inv.slot(0), Some(ItemStack::new(stone, 6)));
        e.click(&mut c, &mut inv, &mut g, 0, Button::Left, true);
        assert_eq!(inv.slot(27), Some(ItemStack::new(stone, 6)));
        // Grille → inventaire.
        g.set_cell(4, Some(ItemStack::new(e.id("sand"), 2)));
        e.click(&mut c, &mut inv, &mut g, GRID_BASE + 4, Button::Left, true);
        assert_eq!(g.cell(4), None);
        assert_eq!(inv.count_of(e.id("sand")), 2);
    }

    #[test]
    fn workbench_forge_and_close_return_items() {
        let e = env();
        let (mut inv, mut c) = (Inventory::new(), InventoryCursor::default());
        // Établi : anneau de pierre + charbon → fonderie.
        let mut wb = CraftingGrid::new(Station::Workbench);
        for k in 0..9 {
            let it = if k == 4 { "coal" } else { "stone" };
            wb.set_cell(k, Some(ItemStack::new(e.id(it), 1)));
        }
        wb.update(&e.book);
        assert_eq!(wb.output(), Some(ItemStack::new(e.id("forge"), 1)));
        e.click(&mut c, &mut inv, &mut wb, OUTPUT_SLOT, Button::Left, false);
        assert!(wb.is_empty());
        assert_eq!(c.held, Some(ItemStack::new(e.id("forge"), 1)));
        // Fonderie : minerai de charbon + planche → 2 charbons.
        let mut fg = CraftingGrid::new(Station::Forge);
        fg.set_cell(0, Some(ItemStack::new(e.id("coal_ore"), 2)));
        fg.set_cell(1, Some(ItemStack::new(e.id("planks_oak"), 1)));
        fg.update(&e.book);
        assert_eq!(fg.output(), Some(ItemStack::new(e.id("coal"), 2)));
        // Fermeture : pile tenue + grille retournent dans l'inventaire.
        let lost = c.close(&mut inv, &mut fg, &e.book);
        assert_eq!(lost, 0);
        assert_eq!(c.held, None);
        assert_eq!(inv.count_of(e.id("forge")), 1);
        assert_eq!(inv.count_of(e.id("coal_ore")), 2);
        assert_eq!(inv.count_of(e.id("planks_oak")), 1);
        assert!(fg.is_empty());
    }

    #[test]
    fn close_reports_loss_when_full() {
        let e = env();
        let (mut inv, mut c, mut g) = (
            Inventory::new(),
            InventoryCursor::default(),
            CraftingGrid::new(Station::Hand),
        );
        for i in 0..SIZE as i32 {
            inv.set_slot(i, Some(ItemStack::new(e.id("dirt"), 64)));
        }
        g.set_cell(0, Some(ItemStack::new(e.id("sand"), 3)));
        c.held = Some(ItemStack::new(e.id("stone"), 5));
        // 3 sables de la grille + 5 pierres tenues : tout est perdu, rien ne rentre.
        assert_eq!(c.close(&mut inv, &mut g, &e.book), 8);
    }

    #[test]
    fn click_outside_returns_held_and_bad_slots_are_ignored() {
        let e = env();
        let (mut inv, mut c, mut g) = (
            Inventory::new(),
            InventoryCursor::default(),
            CraftingGrid::new(Station::Hand),
        );
        c.held = Some(ItemStack::new(e.id("dirt"), 3));
        c.held_from = 4;
        e.click(&mut c, &mut inv, &mut g, -1, Button::Left, false);
        assert_eq!(inv.slot(4), Some(ItemStack::new(e.id("dirt"), 3)));
        // La grille 2×2 n'a que 4 cases : GRID_BASE+4 est hors grille.
        e.click(&mut c, &mut inv, &mut g, GRID_BASE + 4, Button::Left, false);
        assert!(g.is_empty());
        e.click(&mut c, &mut inv, &mut g, OUTPUT_SLOT, Button::Left, false);
        assert_eq!(c.held, None);
    }

    #[test]
    fn grid_cell_bounds() {
        let g = CraftingGrid::new(Station::Forge);
        assert_eq!(g.size(), (2, 1));
        assert_eq!(g.len(), 2);
        assert_eq!(g.cell(2), None);
        let mut g = g;
        g.set_cell(5, Some(ItemStack::new(1u8, 1)));
        assert!(g.is_empty());
    }
}
