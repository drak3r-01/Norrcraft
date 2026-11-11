//! Données du mode survie nordique (`assets/survival.json`, généré par `scripts/gen_survival.py`) :
//! propriétés des objets (outils, nourriture, armures, combustible), dureté et butin des blocs,
//! butin des mobs, croissance des cultures et température des biomes.
//!
//! Tout est piloté par données : ajouter un outil ou changer la dureté d'un bloc ne demande
//! pas de toucher au code.

use std::collections::HashMap;

use nb_core::{Error, Result};
use serde::Deserialize;

use crate::blocks::{BlockId, BlockRegistry};
use crate::items::{ItemId, ItemRegistry};
use crate::universe::WorldKind;

/// Contenu de `assets/survival.json`, embarqué dans le binaire.
pub const SURVIVAL_JSON: &str = include_str!("../../../assets/vanilla/survival.json");

/// Famille d'un outil.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
    Sword,
    /// Canne à pêche.
    Rod,
}

impl ToolKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pickaxe" => ToolKind::Pickaxe,
            "axe" => ToolKind::Axe,
            "shovel" => ToolKind::Shovel,
            "hoe" => ToolKind::Hoe,
            "sword" => ToolKind::Sword,
            "rod" => ToolKind::Rod,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            ToolKind::Pickaxe => "Pickaxe",
            ToolKind::Axe => "Axe",
            ToolKind::Shovel => "Shovel",
            ToolKind::Hoe => "Hoe",
            ToolKind::Sword => "Sword",
            ToolKind::Rod => "Fishing rod",
        }
    }
}

/// Noms des paliers (0 = bois … 4 = aurore).
pub const TIER_NAMES: [&str; 5] = ["Wood", "Stone", "Iron", "Diamond", "Aurora"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolInfo {
    pub kind: ToolKind,
    pub tier: u8,
    pub durability: u16,
    /// Multiplicateur de vitesse de minage sur les blocs de sa famille.
    pub speed: f32,
    /// Dégâts infligés à un mob.
    pub damage: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoodInfo {
    /// Points de faim rendus (barre de 20).
    pub hunger: f32,
    /// Saturation rendue (retarde la faim).
    pub saturation: f32,
    /// Chaleur corporelle rendue (0..100).
    pub warmth: f32,
    /// Santé rendue immédiatement.
    pub health: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArmorSlot {
    Head,
    Chest,
    Legs,
    Feet,
}

impl ArmorSlot {
    pub const ALL: [ArmorSlot; 4] = [
        ArmorSlot::Head,
        ArmorSlot::Chest,
        ArmorSlot::Legs,
        ArmorSlot::Feet,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "head" => ArmorSlot::Head,
            "chest" => ArmorSlot::Chest,
            "legs" => ArmorSlot::Legs,
            "feet" => ArmorSlot::Feet,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmorInfo {
    pub slot: ArmorSlot,
    pub defense: f32,
    /// Isolation thermique de la pièce (somme sur les 4 pièces ≤ 1).
    pub warmth: f32,
    pub durability: u16,
    /// Coupe-vent de la pièce, 0 à 1 (météo, session 3.3).
    pub windproof: f32,
    /// Imperméabilité de la pièce, 0 à 1 (météo, session 3.3).
    pub waterproof: f32,
}

impl ArmorSlot {
    /// Part de la pièce dans le coupe-vent et l'imperméabilité de la tenue (somme 1).
    pub fn coverage(self) -> f32 {
        match self {
            ArmorSlot::Chest => 0.4,
            ArmorSlot::Legs => 0.3,
            ArmorSlot::Head | ArmorSlot::Feet => 0.15,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItemProps {
    pub tool: Option<ToolInfo>,
    pub food: Option<FoodInfo>,
    pub armor: Option<ArmorInfo>,
    /// Secondes de chauffe fournies à la fonderie (information ; la fonderie utilise les étiquettes).
    pub fuel: Option<f32>,
}

/// Un butin : entre `min` et `max` exemplaires avec la probabilité `chance`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drop {
    pub item: ItemId,
    pub min: i32,
    pub max: i32,
    pub chance: f32,
}

/// Règle de minage d'un bloc.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockRule {
    /// Secondes de base ; `< 0` : incassable ; `0` : instantané.
    pub hardness: f32,
    /// Outil conseillé (accélère le minage ; obligatoire pour la pioche).
    pub tool: Option<ToolKind>,
    /// Palier minimal de l'outil pour obtenir le butin.
    pub tier: u8,
    pub drops: Vec<Drop>,
}

impl BlockRule {
    /// Faut-il un outil pour récolter ? (pierre, minerais : seule la pioche récolte).
    pub fn needs_tool(&self) -> bool {
        self.tool == Some(ToolKind::Pickaxe)
    }

    pub fn unbreakable(&self) -> bool {
        self.hardness < 0.0
    }
}

/// Résultat du calcul de minage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mining {
    /// Durée d'un minage complet, en secondes.
    pub seconds: f32,
    /// Le bloc donne-t-il son butin avec l'outil tenu ?
    pub harvest: bool,
    /// L'outil tenu est-il de la bonne famille ?
    pub right_tool: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRule {
    pub next: BlockId,
    /// Secondes moyennes avant le stade suivant.
    pub grow: f32,
}

#[derive(Debug, Deserialize)]
struct RawTool {
    kind: String,
    tier: u8,
    durability: u16,
    speed: f32,
    damage: f32,
}

#[derive(Debug, Deserialize)]
struct RawFood {
    hunger: f32,
    saturation: f32,
    #[serde(default)]
    warmth: f32,
    #[serde(default)]
    health: f32,
}

#[derive(Debug, Deserialize)]
struct RawArmor {
    slot: String,
    defense: f32,
    warmth: f32,
    durability: u16,
    #[serde(default)]
    windproof: f32,
    #[serde(default)]
    waterproof: f32,
}

#[derive(Debug, Default, Deserialize)]
struct RawItem {
    tool: Option<RawTool>,
    food: Option<RawFood>,
    armor: Option<RawArmor>,
    fuel: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct RawDrop {
    item: String,
    min: i32,
    max: i32,
    chance: f32,
}

#[derive(Debug, Deserialize)]
struct RawBlock {
    hardness: f32,
    tool: String,
    tier: u8,
    drops: Vec<RawDrop>,
}

#[derive(Debug, Deserialize)]
struct RawCrop {
    next: String,
    grow: f32,
}

#[derive(Debug, Deserialize)]
struct RawCatch {
    item: String,
    min: i32,
    max: i32,
    weight: f32,
}

#[derive(Debug, Deserialize)]
struct RawData {
    plants: HashMap<String, String>,
    tillable: Vec<String>,
    farmland: String,
    heat: HashMap<String, f32>,
    fishing: Vec<RawCatch>,
    items: HashMap<String, RawItem>,
    blocks: HashMap<String, RawBlock>,
    mobs: HashMap<String, Vec<RawDrop>>,
    crops: HashMap<String, RawCrop>,
    biomes: HashMap<String, f32>,
}

/// Données de survie (immuables).
#[derive(Clone, Debug, Default)]
pub struct SurvivalData {
    items: HashMap<ItemId, ItemProps>,
    blocks: HashMap<BlockId, BlockRule>,
    mobs: HashMap<String, Vec<Drop>>,
    crops: HashMap<BlockId, CropRule>,
    biomes: HashMap<String, f32>,
    plants: HashMap<ItemId, BlockId>,
    tillable: Vec<BlockId>,
    farmland: BlockId,
    heat: HashMap<BlockId, f32>,
    fishing: Vec<(Drop, f32)>,
}

impl SurvivalData {
    /// Données intégrées de vanilla.
    pub fn builtin(blocks: &BlockRegistry, items: &ItemRegistry) -> Self {
        Self::from_json(blocks, items, SURVIVAL_JSON).expect("survival.json embarqué invalide")
    }

    /// Règles de survie d'un univers. Norrcraft n'a pas encore de table (elle viendra avec ses
    /// mécaniques) : aucune règle, donc ni dureté, ni butin, ni outil.
    pub fn for_kind(kind: WorldKind, blocks: &BlockRegistry, items: &ItemRegistry) -> Self {
        match kind {
            WorldKind::Vanilla => Self::builtin(blocks, items),
            WorldKind::Norrcraft => Self::default(),
        }
    }

    pub fn from_json(blocks: &BlockRegistry, items: &ItemRegistry, json: &str) -> Result<Self> {
        let ctx = "survival.json";
        let raw: RawData = serde_json::from_str(json).map_err(|e| Error::data(ctx, e))?;
        let item = |n: &str| {
            items
                .id_of(n)
                .ok_or_else(|| Error::data(ctx, format!("objet inconnu : {n}")))
        };
        let block = |n: &str| {
            blocks
                .iter()
                .find(|b| b.name == n)
                .map(|b| b.id)
                .ok_or_else(|| Error::data(ctx, format!("bloc inconnu : {n}")))
        };
        let drops = |v: &[RawDrop]| -> Result<Vec<Drop>> {
            v.iter()
                .map(|d| {
                    Ok(Drop {
                        item: item(&d.item)?,
                        min: d.min,
                        max: d.max.max(d.min),
                        chance: d.chance,
                    })
                })
                .collect()
        };
        let mut out = Self::default();
        for (name, r) in &raw.items {
            let tool = match &r.tool {
                Some(t) => Some(ToolInfo {
                    kind: ToolKind::parse(&t.kind)
                        .ok_or_else(|| Error::data(ctx, format!("outil inconnu : {}", t.kind)))?,
                    tier: t.tier,
                    durability: t.durability,
                    speed: t.speed,
                    damage: t.damage,
                }),
                None => None,
            };
            let armor = match &r.armor {
                Some(a) => Some(ArmorInfo {
                    slot: ArmorSlot::parse(&a.slot)
                        .ok_or_else(|| Error::data(ctx, format!("pièce inconnue : {}", a.slot)))?,
                    defense: a.defense,
                    warmth: a.warmth,
                    durability: a.durability,
                    windproof: a.windproof.clamp(0.0, 1.0),
                    waterproof: a.waterproof.clamp(0.0, 1.0),
                }),
                None => None,
            };
            let food = r.food.as_ref().map(|f| FoodInfo {
                hunger: f.hunger,
                saturation: f.saturation,
                warmth: f.warmth,
                health: f.health,
            });
            out.items.insert(
                item(name)?,
                ItemProps {
                    tool,
                    food,
                    armor,
                    fuel: r.fuel,
                },
            );
        }
        for (name, r) in &raw.blocks {
            out.blocks.insert(
                block(name)?,
                BlockRule {
                    hardness: r.hardness,
                    tool: ToolKind::parse(&r.tool),
                    tier: r.tier,
                    drops: drops(&r.drops)?,
                },
            );
        }
        for (mob, v) in &raw.mobs {
            out.mobs.insert(mob.clone(), drops(v)?);
        }
        for (name, c) in &raw.crops {
            out.crops.insert(
                block(name)?,
                CropRule {
                    next: block(&c.next)?,
                    grow: c.grow,
                },
            );
        }
        out.biomes = raw.biomes;
        for (seed, crop) in &raw.plants {
            out.plants.insert(item(seed)?, block(crop)?);
        }
        for n in &raw.tillable {
            out.tillable.push(block(n)?);
        }
        out.farmland = block(&raw.farmland)?;
        for (n, h) in &raw.heat {
            out.heat.insert(block(n)?, *h);
        }
        for c in &raw.fishing {
            out.fishing.push((
                Drop {
                    item: item(&c.item)?,
                    min: c.min,
                    max: c.max.max(c.min),
                    chance: 1.0,
                },
                c.weight,
            ));
        }
        Ok(out)
    }

    pub fn props(&self, item: ItemId) -> Option<&ItemProps> {
        self.items.get(&item)
    }

    pub fn tool(&self, item: ItemId) -> Option<ToolInfo> {
        self.items.get(&item).and_then(|p| p.tool)
    }

    pub fn food(&self, item: ItemId) -> Option<FoodInfo> {
        self.items.get(&item).and_then(|p| p.food)
    }

    pub fn armor(&self, item: ItemId) -> Option<ArmorInfo> {
        self.items.get(&item).and_then(|p| p.armor)
    }

    /// Durabilité maximale de l'objet (outil ou armure), 0 s'il ne s'use pas.
    pub fn max_durability(&self, item: ItemId) -> u16 {
        self.items.get(&item).map_or(0, |p| {
            p.tool
                .map(|t| t.durability)
                .or_else(|| p.armor.map(|a| a.durability))
                .unwrap_or(0)
        })
    }

    pub fn block_rule(&self, block: BlockId) -> Option<&BlockRule> {
        self.blocks.get(&block)
    }

    pub fn mob_loot(&self, mob_id: &str) -> &[Drop] {
        self.mobs.get(mob_id).map_or(&[], Vec::as_slice)
    }

    pub fn crop(&self, block: BlockId) -> Option<CropRule> {
        self.crops.get(&block).copied()
    }

    pub fn crops(&self) -> impl Iterator<Item = (&BlockId, &CropRule)> {
        self.crops.iter()
    }

    /// Bloc de culture planté par `item` (graine, pomme de terre, carotte).
    pub fn plant_of(&self, item: ItemId) -> Option<BlockId> {
        self.plants.get(&item).copied()
    }

    /// La houe peut-elle labourer ce bloc ?
    pub fn is_tillable(&self, block: BlockId) -> bool {
        self.tillable.contains(&block)
    }

    pub fn farmland(&self) -> BlockId {
        self.farmland
    }

    /// Intensité de chaleur d'un bloc (0 si ce n'est pas une source).
    pub fn heat_of(&self, block: BlockId) -> f32 {
        self.heat.get(&block).copied().unwrap_or(0.0)
    }

    pub fn has_heat_sources(&self) -> bool {
        !self.heat.is_empty()
    }

    /// Tirage de la table de pêche : `r` ∈ [0, 1) ; `None` si la table est vide.
    pub fn fishing_catch(&self, r: f32) -> Option<Drop> {
        let total: f32 = self.fishing.iter().map(|(_, w)| w).sum();
        let mut x = r * total;
        for (d, w) in &self.fishing {
            if x < *w {
                return Some(*d);
            }
            x -= w;
        }
        self.fishing.last().map(|(d, _)| *d)
    }

    /// Température ambiante d'un biome (`Biome::name()`), de −1 (glacial) à +1 (torride).
    pub fn biome_temp(&self, name: &str) -> f32 {
        self.biomes.get(name).copied().unwrap_or(0.0)
    }

    /// Dégâts infligés à un mob avec `held` (`base` à mains nues).
    pub fn attack_damage(&self, held: Option<ItemId>, base: f32) -> f32 {
        held.and_then(|i| self.tool(i)).map_or(base, |t| {
            if t.kind == ToolKind::Rod {
                base
            } else {
                t.damage.max(base)
            }
        })
    }

    /// Durée et butin du minage de `block` avec l'objet `held`.
    pub fn mining(&self, block: BlockId, held: Option<ItemId>) -> Option<Mining> {
        let rule = self.blocks.get(&block)?;
        if rule.unbreakable() {
            return None;
        }
        let tool = held.and_then(|i| self.tool(i));
        let right_tool = match (rule.tool, tool) {
            (Some(want), Some(t)) => t.kind == want,
            _ => false,
        };
        let harvest = if rule.needs_tool() {
            right_tool && tool.is_some_and(|t| t.tier >= rule.tier)
        } else {
            true
        };
        let speed = if right_tool {
            tool.map_or(1.0, |t| t.speed)
        } else {
            1.0
        };
        let base = if harvest { 1.5 } else { 5.0 };
        let seconds = if rule.hardness == 0.0 {
            0.0
        } else {
            (rule.hardness * base / speed).max(0.06)
        };
        Some(Mining {
            seconds,
            harvest,
            right_tool,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> (BlockRegistry, ItemRegistry, SurvivalData) {
        let b = BlockRegistry::builtin_with_craft();
        let i = ItemRegistry::builtin(&b);
        let s = SurvivalData::builtin(&b, &i);
        (b, i, s)
    }

    #[test]
    fn data_loads_and_resolves_names() {
        let (b, i, s) = data();
        let pick = i.id_of("iron_pickaxe").unwrap();
        assert_eq!(s.tool(pick).unwrap().tier, 2);
        assert!(s.food(i.id_of("cooked_meat").unwrap()).unwrap().hunger > 5.0);
        assert_eq!(
            s.armor(i.id_of("fur_tunic").unwrap()).unwrap().slot,
            ArmorSlot::Chest
        );
        let stone = b.get("stone").id;
        assert!(s.block_rule(stone).unwrap().needs_tool());
        assert!(!s.mob_loot("grey_wolf").is_empty());
        assert!(s.biome_temp("SNOWY_PEAKS") < -0.9);
        assert!(s.crop(b.get("wheat_0").id).is_some());
    }

    #[test]
    fn mining_needs_the_right_tool_and_tier() {
        let (b, i, s) = data();
        let stone = b.get("stone").id;
        let iron_ore = b.get("iron_ore").id;
        let wood = i.id_of("wood_pickaxe").unwrap();
        let iron = i.id_of("iron_pickaxe").unwrap();
        // À mains nues : la pierre ne donne rien et prend un temps fou.
        let bare = s.mining(stone, None).unwrap();
        assert!(!bare.harvest && bare.seconds > 8.0);
        // Pioche en bois : la pierre se récolte, le fer non.
        assert!(s.mining(stone, Some(wood)).unwrap().harvest);
        assert!(!s.mining(iron_ore, Some(wood)).unwrap().harvest);
        let stone_pick = i.id_of("stone_pickaxe").unwrap();
        assert!(s.mining(iron_ore, Some(stone_pick)).unwrap().harvest);
        // Un meilleur outil est plus rapide.
        assert!(
            s.mining(stone, Some(iron)).unwrap().seconds
                < s.mining(stone, Some(wood)).unwrap().seconds
        );
        // Terre : se récolte à la main, la pelle accélère.
        let dirt = b.get("dirt").id;
        let by_hand = s.mining(dirt, None).unwrap();
        assert!(by_hand.harvest);
        let shovel = i.id_of("iron_shovel").unwrap();
        assert!(s.mining(dirt, Some(shovel)).unwrap().seconds < by_hand.seconds);
        // Roche mère : incassable.
        assert!(s.mining(b.get("bedrock").id, None).is_none());
    }

    #[test]
    fn weapons_hit_harder_than_fists() {
        let (_, i, s) = data();
        let sword = i.id_of("iron_sword").unwrap();
        assert!(s.attack_damage(Some(sword), 1.0) > 3.0);
        assert_eq!(s.attack_damage(None, 1.0), 1.0);
        let rod = i.id_of("fishing_rod").unwrap();
        assert_eq!(s.attack_damage(Some(rod), 1.0), 1.0);
    }
}
