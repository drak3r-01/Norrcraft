//! Registre des objets (inventaire, craft).
//!
//! Un **objet** est soit un bloc (identifiant d'objet = identifiant de bloc, `0..=255`), soit un
//! objet pur (bâton, lingot…) défini dans `assets/items.json` avec un identifiant `>= 256`.
//! Les sauvegardes référencent les objets par leur **nom**, jamais par leur identifiant.

use std::collections::HashMap;

use nb_core::{Error, Result};
use serde::Deserialize;

use crate::blocks::{BlockId, BlockRegistry};

/// Identifiant d'objet : `0..=255` = bloc du même identifiant, `>= 256` = objet pur.
pub type ItemId = u16;

/// Premier identifiant réservé aux objets purs.
pub const FIRST_PURE_ITEM: ItemId = 256;

/// Taille maximale d'une pile (tous objets).
pub const STACK_MAX: i32 = 64;

/// Contenu de `assets/items.json`, embarqué dans le binaire.
pub const ITEMS_JSON: &str = include_str!("../../../assets/items.json");

#[derive(Debug, Deserialize)]
struct RawItem {
    id: ItemId,
    name: String,
    /// Nom de la texture d'icône (cherchée dans l'atlas des blocs du pack).
    texture: String,
}

/// Un objet connu du jeu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInfo {
    pub id: ItemId,
    pub name: String,
    /// Bloc posé par cet objet (`None` pour un objet pur).
    pub block: Option<BlockId>,
    /// Texture d'icône d'un objet pur (vide pour un bloc : l'icône est celle du bloc).
    pub texture: String,
}

/// Registre immuable des objets.
#[derive(Debug, Clone)]
pub struct ItemRegistry {
    items: Vec<ItemInfo>,
    by_id: HashMap<ItemId, usize>,
    by_name: HashMap<String, ItemId>,
}

impl ItemRegistry {
    /// Registre intégré : tous les blocs (hors air) + `items.json`.
    pub fn builtin(blocks: &BlockRegistry) -> Self {
        Self::from_json(blocks, ITEMS_JSON).expect("items.json embarqué invalide")
    }

    pub fn from_json(blocks: &BlockRegistry, json: &str) -> Result<Self> {
        let raw: Vec<RawItem> =
            serde_json::from_str(json).map_err(|e| Error::data("items.json", e))?;
        let air = blocks.ids().air;
        let mut items = Vec::new();
        for b in blocks.iter().filter(|b| b.id != air) {
            items.push(ItemInfo {
                id: ItemId::from(b.id),
                name: b.name.clone(),
                block: Some(b.id),
                texture: String::new(),
            });
        }
        for r in raw {
            if r.id < FIRST_PURE_ITEM {
                return Err(Error::data(
                    "items.json",
                    format!("{} : identifiant {} < 256 réservé aux blocs", r.name, r.id),
                ));
            }
            items.push(ItemInfo {
                id: r.id,
                name: r.name,
                block: None,
                texture: r.texture,
            });
        }
        let mut by_id = HashMap::new();
        let mut by_name = HashMap::new();
        for (i, it) in items.iter().enumerate() {
            if by_id.insert(it.id, i).is_some() {
                return Err(Error::data(
                    "items.json",
                    format!("identifiant d'objet en double : {}", it.id),
                ));
            }
            if by_name.insert(it.name.clone(), it.id).is_some() {
                return Err(Error::data(
                    "items.json",
                    format!("nom d'objet en double : {}", it.name),
                ));
            }
        }
        Ok(Self {
            items,
            by_id,
            by_name,
        })
    }

    pub fn get(&self, id: ItemId) -> Option<&ItemInfo> {
        self.by_id.get(&id).map(|&i| &self.items[i])
    }

    pub fn id_of(&self, name: &str) -> Option<ItemId> {
        self.by_name.get(name).copied()
    }

    pub fn name_of(&self, id: ItemId) -> &str {
        self.get(id).map_or("air", |i| i.name.as_str())
    }

    /// Bloc posé par l'objet, s'il en pose un.
    pub fn block_of(&self, id: ItemId) -> Option<BlockId> {
        self.get(id).and_then(|i| i.block)
    }

    /// Objet qui correspond à un bloc.
    pub fn item_of_block(block: BlockId) -> ItemId {
        ItemId::from(block)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ItemInfo> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> (BlockRegistry, ItemRegistry) {
        let b = BlockRegistry::builtin_with_craft();
        let i = ItemRegistry::builtin(&b);
        (b, i)
    }

    #[test]
    fn blocks_are_items_with_same_id() {
        let (b, i) = reg();
        let stone = b.get("stone").id;
        assert_eq!(i.id_of("stone"), Some(ItemId::from(stone)));
        assert_eq!(i.block_of(ItemId::from(stone)), Some(stone));
        assert_eq!(i.id_of("air"), None, "l'air n'est pas un objet");
    }

    #[test]
    fn pure_items_have_no_block() {
        let (_, i) = reg();
        let stick = i.id_of("stick").unwrap();
        assert!(stick >= FIRST_PURE_ITEM);
        assert_eq!(i.block_of(stick), None);
        assert_eq!(i.get(stick).unwrap().texture, "item_stick");
    }

    #[test]
    fn rejects_low_ids_and_duplicates() {
        let b = BlockRegistry::builtin();
        assert!(ItemRegistry::from_json(&b, r#"[{"id":5,"name":"x","texture":"x"}]"#).is_err());
        assert!(ItemRegistry::from_json(
            &b,
            r#"[{"id":300,"name":"x","texture":"x"},{"id":300,"name":"y","texture":"y"}]"#
        )
        .is_err());
        assert!(
            ItemRegistry::from_json(&b, r#"[{"id":300,"name":"stone","texture":"x"}]"#).is_err()
        );
    }
}
