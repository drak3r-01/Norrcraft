//! Propriétés de bloc vues par le mesher (spec §3.3).

use nb_data::BlockRegistry;

/// Blocs que le Java compare **par identité** (`BlockRegistry.AIR`, `GRASS`, …).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockRole {
    Air,
    Grass,
    Water,
    Lava,
    LilyPad,
    /// Couche partielle (neige) : dalle de `layer_eighths / 8` de bloc, hors greedy.
    Layer,
    Other,
}

/// Ce que le mesher sait d'un type de bloc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockMeshInfo {
    pub role: BlockRole,
    pub transparent: bool,
    pub cross_shaped: bool,
    pub grass_tinted: bool,
    pub leaves_tinted: bool,
    /// `byte` Java (signé).
    pub light_attenuation: i32,
    /// `byte` Java (signé).
    pub light_emission: i32,
    /// Layers de texture résolus, indexés par face (0 = +Z … 5 = −Y).
    pub texture_layers: [u32; 6],
    /// Hauteur d'une couche en huitièmes de bloc (0 = bloc plein).
    pub layer_eighths: u8,
    /// Feuillage caduc : reçoit la teinte de saison du mailleur.
    pub seasonal: bool,
}

impl BlockMeshInfo {
    /// L'air de `blocks.json`.
    pub fn air() -> Self {
        Self {
            role: BlockRole::Air,
            transparent: true,
            cross_shaped: false,
            grass_tinted: false,
            leaves_tinted: false,
            light_attenuation: 0,
            light_emission: 0,
            texture_layers: [0; 6],
            layer_eighths: 0,
            seasonal: false,
        }
    }

    #[inline]
    pub fn is_fluid(&self) -> bool {
        matches!(self.role, BlockRole::Water | BlockRole::Lava)
    }

    /// Blocs ignorés comme bloc courant par le greedy meshing (air, croix, nénuphar).
    #[inline]
    pub(crate) fn skipped_by_greedy(&self) -> bool {
        matches!(
            self.role,
            BlockRole::Air | BlockRole::LilyPad | BlockRole::Layer
        ) || self.cross_shaped
    }

    /// `isLightBlocking` (§5.1).
    #[inline]
    pub fn is_light_blocking(&self) -> bool {
        !(matches!(
            self.role,
            BlockRole::Air
                | BlockRole::Water
                | BlockRole::Lava
                | BlockRole::LilyPad
                | BlockRole::Layer
        ) || self.cross_shaped
            || self.leaves_tinted)
    }

    /// `isSolidBlock` (occulteur d'AO, §5.1).
    #[inline]
    pub fn is_ao_solid(&self) -> bool {
        !(matches!(
            self.role,
            BlockRole::Air
                | BlockRole::Water
                | BlockRole::Lava
                | BlockRole::LilyPad
                | BlockRole::Layer
        ) || self.cross_shaped)
    }
}

/// Table des blocs indexée par identifiant (`u8`). Un identifiant non défini est traité comme
/// l'air (`BlockRegistry.fromId` → `AIR`).
///
/// Le mesher travaille sur des identifiants **canoniques** : tout identifiant inconnu est ramené
/// à celui de l'air, de sorte que l'égalité d'identifiants reproduise l'égalité d'instances
/// `BlockType` du Java.
#[derive(Clone, Debug)]
pub struct BlockTable {
    infos: Vec<BlockMeshInfo>,
    canon: [u8; 256],
    air: u8,
}

impl BlockTable {
    /// Construit la table. L'entrée de rôle [`BlockRole::Air`] (la première trouvée) définit
    /// l'air ; à défaut l'identifiant 0 reçoit [`BlockMeshInfo::air`].
    pub fn new(entries: Vec<(u8, BlockMeshInfo)>) -> Self {
        let air = entries
            .iter()
            .find(|(_, i)| i.role == BlockRole::Air)
            .map(|(id, _)| *id)
            .unwrap_or(0);
        let mut infos = vec![BlockMeshInfo::air(); 256];
        let mut defined = [false; 256];
        for (id, info) in entries {
            infos[usize::from(id)] = info;
            defined[usize::from(id)] = true;
        }
        let mut canon = [air; 256];
        for (i, slot) in canon.iter_mut().enumerate() {
            if defined[i] {
                *slot = i as u8;
            }
        }
        let air_info = if defined[usize::from(air)] {
            infos[usize::from(air)].clone()
        } else {
            BlockMeshInfo::air()
        };
        infos[usize::from(air)] = air_info.clone();
        for i in 0..256 {
            if !defined[i] {
                infos[i] = air_info.clone();
            }
        }
        Self { infos, canon, air }
    }

    /// Construit la table depuis le registre `blocks.json`. `layer_of` résout un nom de texture
    /// non vide en layer de la texture array (`TextureRegistry.getLayer`). Comme
    /// `BlockType.initializeMapping` : nom vide → layer 0 ; bloc d'identifiant 0 → tout à 0.
    pub fn from_registry(reg: &BlockRegistry, mut layer_of: impl FnMut(&str) -> u32) -> Self {
        let ids = reg.ids();
        let entries = reg
            .iter()
            .map(|b| {
                let role = if b.id == ids.air {
                    BlockRole::Air
                } else if b.id == ids.grass || (b.grass_tinted && !b.cross_shaped) {
                    // Tout cube teinté herbe (vanilla : `grass` seul ; Norrcraft : `gras`, `mossa`).
                    BlockRole::Grass
                } else if b.id == ids.water {
                    BlockRole::Water
                } else if b.id == ids.lava {
                    BlockRole::Lava
                } else if b.id == ids.lily_pad {
                    BlockRole::LilyPad
                } else if (1..8).contains(&b.height) {
                    BlockRole::Layer
                } else {
                    BlockRole::Other
                };
                let mut texture_layers = [0u32; 6];
                if b.id != 0 {
                    for (slot, name) in texture_layers.iter_mut().zip(&b.texture_names) {
                        *slot = if name.is_empty() { 0 } else { layer_of(name) };
                    }
                }
                let info = BlockMeshInfo {
                    role,
                    transparent: b.transparent,
                    cross_shaped: b.cross_shaped,
                    grass_tinted: b.grass_tinted,
                    leaves_tinted: b.leaves_tinted,
                    light_attenuation: i32::from(b.light_attenuation as i8),
                    light_emission: i32::from(b.light_emission as i8),
                    texture_layers,
                    layer_eighths: if (1..8).contains(&b.height) {
                        b.height
                    } else {
                        0
                    },
                    seasonal: b.seasonal,
                };
                (b.id, info)
            })
            .collect();
        Self::new(entries)
    }

    /// Identifiant canonique (inconnu → air).
    #[inline]
    pub fn canonical(&self, id: u8) -> u8 {
        self.canon[usize::from(id)]
    }

    /// Propriétés d'un identifiant (inconnu → air).
    #[inline]
    pub fn get(&self, id: u8) -> &BlockMeshInfo {
        &self.infos[usize::from(id)]
    }

    /// Identifiant de l'air.
    #[inline]
    pub fn air_id(&self) -> u8 {
        self.air
    }
}

/// `shouldRenderFace(current, neighbor)` (§6.1). `same_block` = identité des deux blocs
/// (identifiants canoniques égaux).
pub fn should_render_face(cur: &BlockMeshInfo, nb: &BlockMeshInfo, same_block: bool) -> bool {
    if cur.is_fluid() {
        return (nb.transparent || matches!(nb.role, BlockRole::LilyPad | BlockRole::Layer))
            && !same_block;
    }
    if nb.transparent || nb.cross_shaped || matches!(nb.role, BlockRole::LilyPad | BlockRole::Layer)
    {
        return !(same_block && !cur.leaves_tinted);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> BlockTable {
        BlockTable::from_registry(&BlockRegistry::builtin(), |_| 1)
    }

    #[test]
    fn roles_and_unknown_ids() {
        let t = table();
        assert_eq!(t.air_id(), 0);
        assert_eq!(t.get(2).role, BlockRole::Grass);
        assert_eq!(t.get(4).role, BlockRole::Water);
        assert_eq!(t.get(33).role, BlockRole::Lava);
        assert_eq!(t.get(26).role, BlockRole::LilyPad);
        assert_eq!(t.get(3).role, BlockRole::Other);
        assert_eq!(t.canonical(200), 0);
        assert_eq!(t.canonical(150), 0);
        assert_eq!(t.get(200).role, BlockRole::Air);
        assert_eq!(t.get(0).texture_layers, [0; 6]);
        assert_eq!(t.get(3).texture_layers, [1; 6]);
    }

    #[test]
    fn predicates_match_spec_table() {
        let t = table();
        // isLightBlocking : feuilles et eau laissent passer, cactus bloque.
        assert!(!t.get(10).is_light_blocking());
        assert!(!t.get(4).is_light_blocking());
        assert!(!t.get(19).is_light_blocking());
        assert!(t.get(27).is_light_blocking());
        assert!(t.get(3).is_light_blocking());
        // isSolidBlock (AO) : feuilles et cactus occultent, eau/croix/nénuphar non.
        assert!(t.get(10).is_ao_solid());
        assert!(t.get(27).is_ao_solid());
        assert!(!t.get(4).is_ao_solid());
        assert!(!t.get(26).is_ao_solid());
        assert!(!t.get(20).is_ao_solid());
        assert!(!t.get(0).is_ao_solid());
    }

    #[test]
    fn face_visibility_rules() {
        let t = table();
        let (air, stone, water, lava, leaves, birch, lily, grass_x, cactus) = (
            t.get(0),
            t.get(3),
            t.get(4),
            t.get(33),
            t.get(10),
            t.get(12),
            t.get(26),
            t.get(19),
            t.get(27),
        );
        assert!(should_render_face(stone, air, false));
        assert!(!should_render_face(stone, stone, true));
        assert!(should_render_face(stone, water, false));
        assert!(should_render_face(stone, grass_x, false));
        assert!(should_render_face(stone, lily, false));
        // Feuilles identiques : faces internes émises.
        assert!(should_render_face(leaves, leaves, true));
        assert!(should_render_face(leaves, birch, false));
        assert!(!should_render_face(leaves, stone, false));
        // Fluides.
        assert!(!should_render_face(water, water, true));
        assert!(should_render_face(water, air, false));
        assert!(should_render_face(water, lava, false));
        assert!(should_render_face(lava, water, false));
        assert!(should_render_face(water, lily, false));
        assert!(should_render_face(water, leaves, false));
        assert!(!should_render_face(water, stone, false));
        // Fluide vers une croix : tall_grass est `transparent` dans blocks.json → face émise.
        assert!(should_render_face(water, grass_x, false));
        // Cactus contre cactus : non ; pierre contre cactus : oui.
        assert!(!should_render_face(cactus, cactus, true));
        assert!(should_render_face(stone, cactus, false));
    }
}
