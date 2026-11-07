//! Registre des blocs (`blocks.json`).

use std::collections::HashMap;

use nb_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Identifiant d'un bloc tel que stocké dans les tableaux de chunks (un octet).
pub type BlockId = u8;

/// Contenu de `assets/blocks.json`, embarqué dans le binaire (comme la ressource Java).
pub const BLOCKS_JSON: &str = include_str!("../../../assets/blocks.json");

/// Entrée brute de `blocks.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockProperties {
    pub id: BlockId,
    pub name: String,
    /// Textures des 6 faces : `[+X, -X, +Z, -Z, +Y, -Y]` ; chaîne vide = pas de texture.
    #[serde(default)]
    pub textures: Vec<String>,
    #[serde(default)]
    pub transparent: bool,
    #[serde(default)]
    pub cross_shaped: bool,
    #[serde(default)]
    pub grass_tinted: bool,
    #[serde(default)]
    pub leaves_tinted: bool,
    #[serde(default)]
    pub solid: bool,
    #[serde(default)]
    pub light_attenuation: u8,
    #[serde(default)]
    pub light_emission: u8,
}

/// Un type de bloc chargé. Les indices de texture sont résolus plus tard par l'atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockType {
    pub id: BlockId,
    pub name: String,
    pub texture_names: [String; 6],
    pub transparent: bool,
    pub cross_shaped: bool,
    pub grass_tinted: bool,
    pub leaves_tinted: bool,
    pub solid: bool,
    pub light_attenuation: u8,
    pub light_emission: u8,
}

impl From<BlockProperties> for BlockType {
    fn from(p: BlockProperties) -> Self {
        let mut texture_names: [String; 6] = Default::default();
        for (slot, name) in texture_names.iter_mut().zip(p.textures) {
            *slot = name;
        }
        Self {
            id: p.id,
            name: p.name,
            texture_names,
            transparent: p.transparent,
            cross_shaped: p.cross_shaped,
            grass_tinted: p.grass_tinted,
            leaves_tinted: p.leaves_tinted,
            solid: p.solid,
            light_attenuation: p.light_attenuation,
            light_emission: p.light_emission,
        }
    }
}

/// Noms des blocs que le code du jeu référence directement (`BlockRegistry.AIR`, …).
const WELL_KNOWN: &[&str] = &[
    "air",
    "dirt",
    "grass",
    "stone",
    "water",
    "sand",
    "bedrock",
    "clay",
    "snow",
    "wood",
    "leaves",
    "birch_wood",
    "birch_leaves",
    "spruce_wood",
    "spruce_leaves",
    "jungle_wood",
    "jungle_leaves",
    "acacia_wood",
    "acacia_leaves",
    "tall_grass",
    "flower_red",
    "flower_yellow",
    "fern",
    "dead_bush",
    "mushroom_brown",
    "mushroom_red",
    "lily_pad",
    "cactus",
    "coal_ore",
    "iron_ore",
    "gold_ore",
    "diamond_ore",
    "emerald_ore",
    "lava",
    "sandstone",
    "gravel",
    "red_sand",
    "terracotta",
    "terracotta_red",
    "terracotta_orange",
    "terracotta_yellow",
    "terracotta_white",
    "terracotta_brown",
    "ice",
    "packed_ice",
    "podzol",
    "mycelium",
    "basalt",
    "marble",
    "slate",
    "ash",
    "magma",
    "cherry_wood",
    "dark_oak_wood",
    "mushroom_stem",
    "mushroom_cap_red",
    "mushroom_cap_brown",
    "coral_pink",
    "coral_purple",
    "coral_yellow",
    "coral_blue",
    "flower_blue",
    "flower_pink",
    "flower_white",
    "flower_orange",
    "lavender",
    "reeds",
    "moss",
];

/// Identifiants des blocs connus du code (équivalents des champs statiques du `BlockRegistry`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockIds {
    pub air: BlockId,
    pub dirt: BlockId,
    pub grass: BlockId,
    pub stone: BlockId,
    pub water: BlockId,
    pub sand: BlockId,
    pub bedrock: BlockId,
    pub clay: BlockId,
    pub snow: BlockId,
    pub wood: BlockId,
    pub leaves: BlockId,
    pub birch_wood: BlockId,
    pub birch_leaves: BlockId,
    pub spruce_wood: BlockId,
    pub spruce_leaves: BlockId,
    pub jungle_wood: BlockId,
    pub jungle_leaves: BlockId,
    pub acacia_wood: BlockId,
    pub acacia_leaves: BlockId,
    pub tall_grass: BlockId,
    pub flower_red: BlockId,
    pub flower_yellow: BlockId,
    pub fern: BlockId,
    pub dead_bush: BlockId,
    pub mushroom_brown: BlockId,
    pub mushroom_red: BlockId,
    pub lily_pad: BlockId,
    pub cactus: BlockId,
    pub coal_ore: BlockId,
    pub iron_ore: BlockId,
    pub gold_ore: BlockId,
    pub diamond_ore: BlockId,
    pub emerald_ore: BlockId,
    pub lava: BlockId,
    pub sandstone: BlockId,
    pub gravel: BlockId,
    pub red_sand: BlockId,
    pub terracotta: BlockId,
    pub terracotta_red: BlockId,
    pub terracotta_orange: BlockId,
    pub terracotta_yellow: BlockId,
    pub terracotta_white: BlockId,
    pub terracotta_brown: BlockId,
    pub ice: BlockId,
    pub packed_ice: BlockId,
    pub podzol: BlockId,
    pub mycelium: BlockId,
    pub basalt: BlockId,
    pub marble: BlockId,
    pub slate: BlockId,
    pub ash: BlockId,
    pub magma: BlockId,
    pub cherry_wood: BlockId,
    pub dark_oak_wood: BlockId,
    pub mushroom_stem: BlockId,
    pub mushroom_cap_red: BlockId,
    pub mushroom_cap_brown: BlockId,
    pub coral_pink: BlockId,
    pub coral_purple: BlockId,
    pub coral_yellow: BlockId,
    pub coral_blue: BlockId,
    pub flower_blue: BlockId,
    pub flower_pink: BlockId,
    pub flower_white: BlockId,
    pub flower_orange: BlockId,
    pub lavender: BlockId,
    pub reeds: BlockId,
    pub moss: BlockId,
}

/// Registre immuable des blocs.
#[derive(Debug, Clone)]
pub struct BlockRegistry {
    /// Indexé par identifiant ; les identifiants non définis valent `None` (→ air).
    by_id: Vec<Option<BlockType>>,
    by_name: HashMap<String, BlockId>,
    ids: BlockIds,
}

impl BlockRegistry {
    /// Registre construit depuis le `blocks.json` embarqué.
    pub fn builtin() -> Self {
        Self::from_json(BLOCKS_JSON).expect("blocks.json embarqué invalide")
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let props: Vec<BlockProperties> =
            serde_json::from_str(json).map_err(|e| Error::data("blocks.json", e))?;
        Self::from_properties(props)
    }

    pub fn from_properties(props: Vec<BlockProperties>) -> Result<Self> {
        let mut by_id: Vec<Option<BlockType>> = vec![None; 256];
        let mut by_name = HashMap::new();
        for p in props {
            let block = BlockType::from(p);
            by_name.insert(block.name.clone(), block.id);
            let slot = usize::from(block.id);
            by_id[slot] = Some(block);
        }
        let find = |name: &str| -> Result<BlockId> {
            by_name
                .get(name)
                .copied()
                .ok_or_else(|| Error::data("blocks.json", format!("bloc requis manquant: {name}")))
        };
        let mut v = [0; WELL_KNOWN.len()];
        for (slot, name) in v.iter_mut().zip(WELL_KNOWN) {
            *slot = find(name)?;
        }
        let ids = BlockIds {
            air: v[0],
            dirt: v[1],
            grass: v[2],
            stone: v[3],
            water: v[4],
            sand: v[5],
            bedrock: v[6],
            clay: v[7],
            snow: v[8],
            wood: v[9],
            leaves: v[10],
            birch_wood: v[11],
            birch_leaves: v[12],
            spruce_wood: v[13],
            spruce_leaves: v[14],
            jungle_wood: v[15],
            jungle_leaves: v[16],
            acacia_wood: v[17],
            acacia_leaves: v[18],
            tall_grass: v[19],
            flower_red: v[20],
            flower_yellow: v[21],
            fern: v[22],
            dead_bush: v[23],
            mushroom_brown: v[24],
            mushroom_red: v[25],
            lily_pad: v[26],
            cactus: v[27],
            coal_ore: v[28],
            iron_ore: v[29],
            gold_ore: v[30],
            diamond_ore: v[31],
            emerald_ore: v[32],
            lava: v[33],
            sandstone: v[34],
            gravel: v[35],
            red_sand: v[36],
            terracotta: v[37],
            terracotta_red: v[38],
            terracotta_orange: v[39],
            terracotta_yellow: v[40],
            terracotta_white: v[41],
            terracotta_brown: v[42],
            ice: v[43],
            packed_ice: v[44],
            podzol: v[45],
            mycelium: v[46],
            basalt: v[47],
            marble: v[48],
            slate: v[49],
            ash: v[50],
            magma: v[51],
            cherry_wood: v[52],
            dark_oak_wood: v[53],
            mushroom_stem: v[54],
            mushroom_cap_red: v[55],
            mushroom_cap_brown: v[56],
            coral_pink: v[57],
            coral_purple: v[58],
            coral_yellow: v[59],
            coral_blue: v[60],
            flower_blue: v[61],
            flower_pink: v[62],
            flower_white: v[63],
            flower_orange: v[64],
            lavender: v[65],
            reeds: v[66],
            moss: v[67],
        };
        Ok(Self {
            by_id,
            by_name,
            ids,
        })
    }

    /// Identifiants des blocs connus (`air`, `stone`, …).
    pub fn ids(&self) -> &BlockIds {
        &self.ids
    }

    /// `BlockRegistry.fromId` : un identifiant inconnu retourne l'air.
    pub fn from_id(&self, id: BlockId) -> &BlockType {
        self.by_id[usize::from(id)]
            .as_ref()
            .unwrap_or_else(|| self.by_id[usize::from(self.ids.air)].as_ref().expect("air"))
    }

    /// `BlockRegistry.get(name)` : un nom inconnu retourne l'air.
    pub fn get(&self, name: &str) -> &BlockType {
        match self.by_name.get(name) {
            Some(id) => self.from_id(*id),
            None => self.from_id(self.ids.air),
        }
    }

    /// Itère sur les blocs définis, par identifiant croissant.
    pub fn iter(&self) -> impl Iterator<Item = &BlockType> {
        self.by_id.iter().flatten()
    }

    /// Nombre de blocs définis.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_68_blocks_with_expected_ids() {
        let reg = BlockRegistry::builtin();
        assert_eq!(reg.len(), 68);
        let ids = reg.ids();
        assert_eq!(ids.air, 0);
        assert_eq!(ids.stone, 3);
        assert_eq!(ids.water, 4);
        assert_eq!(ids.lava, 33);
        assert_eq!(ids.sandstone, 34);
        assert_eq!(ids.gravel, 35);
        assert_eq!(ids.moss, 67);
    }

    #[test]
    fn lava_emits_light_and_water_attenuates() {
        let reg = BlockRegistry::builtin();
        let lava = reg.get("lava");
        assert_eq!(lava.light_emission, 15);
        assert_eq!(lava.light_attenuation, 3);
        assert!(lava.transparent && !lava.solid);
        assert_eq!(reg.get("water").light_attenuation, 2);
    }

    #[test]
    fn grass_faces_and_tints() {
        let reg = BlockRegistry::builtin();
        let grass = reg.get("grass");
        assert!(grass.grass_tinted && !grass.leaves_tinted);
        assert_eq!(grass.texture_names[4], "grass_block_top");
        assert_eq!(grass.texture_names[5], "dirt");
        assert!(reg.get("leaves").leaves_tinted);
        assert!(reg.get("tall_grass").cross_shaped);
    }

    #[test]
    fn unknown_ids_and_names_fall_back_to_air() {
        let reg = BlockRegistry::builtin();
        assert_eq!(reg.from_id(200).id, 0);
        assert_eq!(reg.get("nope").id, 0);
    }
}
