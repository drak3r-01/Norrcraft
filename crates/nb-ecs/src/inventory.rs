//! Inventaire (spec §14) : 36 cases, barre rapide en 27–35, piles de 64, et logique « curseur »
//! de l'écran d'inventaire.

use nb_data::{BlockId, ItemId};

/// Nombre de cases.
pub const SIZE: usize = 36;
/// Première case de la barre rapide (barre `i` → case `27 + i`).
pub const HOTBAR_START: i32 = 27;
/// Taille maximale d'une pile.
pub const STACK_MAX: i32 = nb_data::STACK_MAX;

/// Ordre de remplissage : barre rapide 27..=35 puis grille 0..=26.
pub const FILL_ORDER: [usize; SIZE] = {
    let mut o = [0usize; SIZE];
    let mut i = 0;
    while i < 9 {
        o[i] = HOTBAR_START as usize + i;
        i += 1;
    }
    while i < SIZE {
        o[i] = i - 9;
        i += 1;
    }
    o
};

/// Pile d'un objet (bloc ou objet pur ; `count` non borné par le type, comme en Java).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: i32,
}

impl ItemStack {
    /// `item` : un [`ItemId`] ou un `BlockId` (un bloc est l'objet de même identifiant).
    pub fn new(item: impl Into<ItemId>, count: i32) -> Self {
        Self {
            item: item.into(),
            count,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Inventory {
    slots: [Option<ItemStack>; SIZE],
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: [None; SIZE],
        }
    }
}

impl Inventory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute un exemplaire de `block` : d'abord sur une pile non pleine, puis dans une case
    /// vide (selon [`FILL_ORDER`]). `false` si `block == air` ou inventaire plein (objet perdu).
    pub fn add_block(&mut self, block: BlockId, air: BlockId) -> bool {
        if block == air {
            return false;
        }
        self.add_item(ItemId::from(block))
    }

    /// Ajoute un exemplaire de `item` (mêmes règles que [`Inventory::add_block`]).
    pub fn add_item(&mut self, item: ItemId) -> bool {
        self.add_stack(ItemStack::new(item, 1)) == 0
    }

    /// Ajoute une pile : complète d'abord les piles non pleines, puis les cases vides (selon
    /// [`FILL_ORDER`]). Retourne le **reliquat** non placé (0 si tout est rentré).
    pub fn add_stack(&mut self, stack: ItemStack) -> i32 {
        let mut left = stack.count;
        if left <= 0 {
            return 0;
        }
        for i in FILL_ORDER {
            if let Some(s) = &mut self.slots[i] {
                if s.item == stack.item && s.count < STACK_MAX {
                    let t = (STACK_MAX - s.count).min(left);
                    s.count += t;
                    left -= t;
                    if left == 0 {
                        return 0;
                    }
                }
            }
        }
        for i in FILL_ORDER {
            if self.slots[i].is_none() {
                let t = STACK_MAX.min(left);
                self.slots[i] = Some(ItemStack::new(stack.item, t));
                left -= t;
                if left == 0 {
                    return 0;
                }
            }
        }
        left
    }

    /// Nombre d'exemplaires de `item` qui rentreraient encore (piles non pleines + cases vides).
    pub fn room_for(&self, item: ItemId) -> i32 {
        self.slots
            .iter()
            .map(|s| match s {
                None => STACK_MAX,
                Some(s) if s.item == item => (STACK_MAX - s.count).max(0),
                Some(_) => 0,
            })
            .sum()
    }

    /// Nombre total d'exemplaires de `item`.
    pub fn count_of(&self, item: ItemId) -> i32 {
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.item == item)
            .map(|s| s.count)
            .sum()
    }

    /// Case `index` ; hors `[0, 36)` → `None`.
    pub fn slot(&self, index: i32) -> Option<ItemStack> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.slots.get(i).copied().flatten())
    }

    /// Écrit une case ; hors bornes → ignoré.
    pub fn set_slot(&mut self, index: i32, stack: Option<ItemStack>) {
        if let Some(s) = usize::try_from(index)
            .ok()
            .and_then(|i| self.slots.get_mut(i))
        {
            *s = stack;
        }
    }

    /// Retire un exemplaire (case vidée si le compte tombe à 0 ou moins).
    pub fn remove_one(&mut self, index: i32) {
        let Some(slot) = usize::try_from(index)
            .ok()
            .and_then(|i| self.slots.get_mut(i))
        else {
            return;
        };
        if let Some(s) = slot {
            s.count -= 1;
            if s.count <= 0 {
                *slot = None;
            }
        }
    }

    /// Case `27 + hotbar`.
    pub fn hotbar_slot(&self, hotbar: i32) -> Option<ItemStack> {
        self.slot(HOTBAR_START + hotbar)
    }

    pub fn clear(&mut self) {
        self.slots = [None; SIZE];
    }

    /// Toutes les cases (pour la sauvegarde et le rendu).
    pub fn slots(&self) -> &[Option<ItemStack>; SIZE] {
        &self.slots
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }
}

/// État « objet tenu au curseur » de l'écran d'inventaire (spec §14.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InventoryCursor {
    pub held: Option<ItemStack>,
    pub held_from: i32,
}

impl Default for InventoryCursor {
    fn default() -> Self {
        Self {
            held: None,
            held_from: -1,
        }
    }
}

impl InventoryCursor {
    /// Remise à zéro à l'ouverture de l'écran.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Clic gauche sur `slot` (`-1` = hors grille).
    pub fn left_click(&mut self, inv: &mut Inventory, slot: i32) {
        if !(0..SIZE as i32).contains(&slot) {
            if self.held.is_some() {
                self.return_held(inv);
            }
            return;
        }
        let mut cell = inv.slot(slot);
        self.left_on_cell(&mut cell, slot);
        inv.set_slot(slot, cell);
    }

    /// Clic gauche sur une case : prendre, poser, fusionner ou échanger.
    pub(crate) fn left_on_cell(&mut self, cell: &mut Option<ItemStack>, slot: i32) {
        match (self.held, *cell) {
            (None, None) => {}
            (None, Some(s)) => {
                self.held = Some(s);
                self.held_from = slot;
                *cell = None;
            }
            (Some(h), None) => {
                *cell = Some(h);
                self.held = None;
                self.held_from = -1;
            }
            (Some(mut h), Some(mut s)) if h.item == s.item => {
                let t = (STACK_MAX - s.count).min(h.count);
                s.count += t;
                h.count -= t;
                *cell = Some(s);
                if h.count <= 0 {
                    self.held = None;
                    self.held_from = -1;
                } else {
                    self.held = Some(h);
                }
            }
            (Some(h), Some(s)) => {
                *cell = Some(h);
                self.held = Some(s);
                self.held_from = slot;
            }
        }
    }

    /// Clic droit sur une case : prendre la moitié, poser un exemplaire ou échanger.
    pub(crate) fn right_on_cell(&mut self, cell: &mut Option<ItemStack>, slot: i32) {
        match (self.held, *cell) {
            (None, None) => {}
            (None, Some(s)) => {
                let take = (s.count + 1) / 2;
                self.held = Some(ItemStack::new(s.item, take));
                self.held_from = slot;
                *cell = (s.count - take > 0).then(|| ItemStack::new(s.item, s.count - take));
            }
            (Some(mut h), None) => {
                *cell = Some(ItemStack::new(h.item, 1));
                h.count -= 1;
                self.set_held_or_clear(h);
            }
            (Some(mut h), Some(mut s)) if h.item == s.item => {
                if s.count < STACK_MAX {
                    s.count += 1;
                    h.count -= 1;
                    *cell = Some(s);
                    self.set_held_or_clear(h);
                }
            }
            (Some(h), Some(s)) => {
                *cell = Some(h);
                self.held = Some(s);
                self.held_from = slot;
            }
        }
    }

    fn set_held_or_clear(&mut self, h: ItemStack) {
        if h.count <= 0 {
            self.held = None;
            self.held_from = -1;
        } else {
            self.held = Some(h);
        }
    }

    /// `returnHeld()` : repose l'objet tenu (case d'origine si vide, sinon première place en
    /// ordre naturel 0..35). Le reliquat éventuel est **perdu** (B20).
    pub fn return_held(&mut self, inv: &mut Inventory) {
        let Some(mut h) = self.held else {
            return;
        };
        if self.held_from >= 0 && inv.slot(self.held_from).is_none() {
            inv.set_slot(self.held_from, Some(h));
        } else {
            for i in 0..SIZE as i32 {
                if h.count <= 0 {
                    break;
                }
                match inv.slot(i) {
                    None => {
                        inv.set_slot(i, Some(h));
                        break;
                    }
                    Some(mut s) if s.item == h.item && s.count < STACK_MAX => {
                        let t = (STACK_MAX - s.count).min(h.count);
                        s.count += t;
                        h.count -= t;
                        inv.set_slot(i, Some(s));
                    }
                    Some(_) => {}
                }
            }
        }
        self.held = None;
        self.held_from = -1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIR: BlockId = 0;
    const DIRT: BlockId = 1;
    const STONE: BlockId = 3;

    #[test]
    fn fill_order_is_hotbar_then_grid() {
        assert_eq!(FILL_ORDER[0], 27);
        assert_eq!(FILL_ORDER[8], 35);
        assert_eq!(FILL_ORDER[9], 0);
        assert_eq!(FILL_ORDER[35], 26);
    }

    #[test]
    fn add_block_stacks_then_uses_empty_slots() {
        let mut inv = Inventory::new();
        assert!(!inv.add_block(AIR, AIR));
        for _ in 0..64 {
            assert!(inv.add_block(DIRT, AIR));
        }
        assert_eq!(inv.slot(27), Some(ItemStack::new(DIRT, 64)));
        assert!(inv.add_block(DIRT, AIR));
        assert_eq!(inv.slot(28), Some(ItemStack::new(DIRT, 1)));
        assert!(inv.add_block(STONE, AIR));
        assert_eq!(inv.slot(29), Some(ItemStack::new(STONE, 1)));
        // Une pile partielle plus loin est préférée à une case vide plus tôt.
        inv.set_slot(27, None);
        assert!(inv.add_block(DIRT, AIR));
        assert_eq!(inv.slot(28), Some(ItemStack::new(DIRT, 2)));
        assert_eq!(inv.slot(27), None);
    }

    #[test]
    fn full_inventory_loses_item() {
        let mut inv = Inventory::new();
        for i in 0..SIZE as i32 {
            inv.set_slot(i, Some(ItemStack::new(STONE, 64)));
        }
        assert!(!inv.add_block(STONE, AIR));
        assert!(!inv.add_block(DIRT, AIR));
    }

    #[test]
    fn bounds_and_remove_one() {
        let mut inv = Inventory::new();
        assert_eq!(inv.slot(-1), None);
        assert_eq!(inv.slot(36), None);
        inv.set_slot(36, Some(ItemStack::new(DIRT, 1)));
        assert!(inv.is_empty());
        inv.set_slot(30, Some(ItemStack::new(DIRT, 2)));
        assert_eq!(inv.hotbar_slot(3), Some(ItemStack::new(DIRT, 2)));
        inv.remove_one(30);
        assert_eq!(inv.slot(30).unwrap().count, 1);
        inv.remove_one(30);
        assert_eq!(inv.slot(30), None);
        inv.remove_one(30);
        inv.remove_one(99);
        inv.set_slot(0, Some(ItemStack::new(DIRT, 1)));
        inv.clear();
        assert!(inv.is_empty());
    }

    #[test]
    fn cursor_pick_place_merge_swap() {
        let mut inv = Inventory::new();
        let mut c = InventoryCursor::default();
        inv.set_slot(0, Some(ItemStack::new(DIRT, 40)));
        inv.set_slot(1, Some(ItemStack::new(DIRT, 50)));
        inv.set_slot(2, Some(ItemStack::new(STONE, 3)));

        c.left_click(&mut inv, 0); // prendre
        assert_eq!(c.held, Some(ItemStack::new(DIRT, 40)));
        assert_eq!(c.held_from, 0);
        assert_eq!(inv.slot(0), None);

        c.left_click(&mut inv, 1); // fusion partielle : 14 transférés
        assert_eq!(inv.slot(1), Some(ItemStack::new(DIRT, 64)));
        assert_eq!(c.held, Some(ItemStack::new(DIRT, 26)));

        c.left_click(&mut inv, 2); // échange
        assert_eq!(inv.slot(2), Some(ItemStack::new(DIRT, 26)));
        assert_eq!(c.held, Some(ItemStack::new(STONE, 3)));
        assert_eq!(c.held_from, 2);

        c.left_click(&mut inv, 5); // poser dans une case vide
        assert_eq!(inv.slot(5), Some(ItemStack::new(STONE, 3)));
        assert_eq!((c.held, c.held_from), (None, -1));

        c.left_click(&mut inv, 7); // rien en main, case vide : rien
        assert_eq!(c.held, None);
    }

    #[test]
    fn cursor_full_merge_clears_held() {
        let mut inv = Inventory::new();
        let mut c = InventoryCursor::default();
        inv.set_slot(0, Some(ItemStack::new(DIRT, 10)));
        inv.set_slot(1, Some(ItemStack::new(DIRT, 10)));
        c.left_click(&mut inv, 0);
        c.left_click(&mut inv, 1);
        assert_eq!(inv.slot(1), Some(ItemStack::new(DIRT, 20)));
        assert_eq!((c.held, c.held_from), (None, -1));
    }

    #[test]
    fn return_held_prefers_origin_then_natural_order() {
        let mut inv = Inventory::new();
        let mut c = InventoryCursor::default();
        inv.set_slot(30, Some(ItemStack::new(DIRT, 5)));
        c.left_click(&mut inv, 30);
        c.left_click(&mut inv, -1); // hors grille → retour à l'origine
        assert_eq!(inv.slot(30), Some(ItemStack::new(DIRT, 5)));
        assert_eq!(c.held, None);

        // Origine occupée : ordre naturel 0..35 (pas FILL_ORDER).
        c.held = Some(ItemStack::new(STONE, 70));
        c.held_from = 30;
        inv.set_slot(0, Some(ItemStack::new(STONE, 60)));
        c.return_held(&mut inv);
        assert_eq!(inv.slot(0), Some(ItemStack::new(STONE, 64)));
        assert_eq!(inv.slot(1), Some(ItemStack::new(STONE, 66)));
        assert_eq!(c.held, None);
    }

    #[test]
    fn return_held_loses_remainder_when_full() {
        let mut inv = Inventory::new();
        for i in 0..SIZE as i32 {
            inv.set_slot(i, Some(ItemStack::new(DIRT, 63)));
        }
        let mut c = InventoryCursor {
            held: Some(ItemStack::new(DIRT, 40)),
            held_from: -1,
        };
        c.return_held(&mut inv);
        assert_eq!(c.held, None);
        let total: i32 = inv.slots().iter().flatten().map(|s| s.count).sum();
        assert_eq!(total, 63 * 36 + 36); // 36 transférés, 4 perdus
    }
}
