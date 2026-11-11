//! Générateur de terrain Norrcraft (sessions 1.2, 1.4, 2.1 et 2.2) : le relief de [`HeightFields`]
//! rempli de roche, l'eau de l'hydrographie ([`Hydrography`] : mer, lacs, rivières, toutes
//! étanches) et le sol des biomes ([`super::biomes`]).
//!
//! Couverture d'une colonne, dans l'ordre (voir `NorrBlocks::cover`) :
//!
//! 1. **formes de relief** (session 1.3) : basalte du champ de lave (sable noir sur ses rivages),
//!    gravier des moraines, du pied des glaciers, des eskers et des drumlins, croûte de tourbe et
//!    cœur de `permafrost` des palses, tourbe des tourbières, granit des îlots de l'archipel, sable
//!    des dunes, galets des côtes soulevées ;
//! 2. **pentes raides** (plus de 2 blocs de marche) : roche nue du biome (`rock`), comme les
//!    barrages des lacs ;
//! 3. **sous l'eau et sur les rives** : fond du biome (`floor`) ;
//! 4. **sinon** : couverture meuble du biome (`filler`, `depth`).
//!
//! Puis une **peau** d'un bloc sur les colonnes sèches et plates (`top` du biome, ou une tache
//! `patch`), et la **neige** au-dessus des neiges éternelles `y_n` (tramées), sur toute pente. La
//! glace des glaciers est posée au-dessus du sol et de l'eau, jusqu'à `plancher(h + épaisseur)`.
//! La **banquise** (Packis) remplace la couche d'eau du dessus, hors des chenaux.
//!
//! Le **sous-sol** (session 2.2, [`super::subsoil`]) remplace le granit de fond : roche de la
//! province, djupberg, filons de diabase, minerais et cavités ; la roche nue des pentes et des
//! barrages prend aussi la roche de la province quand le biome ne fixe pas la sienne.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use nb_data::{BlockId, BlockRegistry};

use super::biomes::{BiomeSelector, BiomeSoil, Neighbourhood, NorrBiome, Selection, NEIGHBOURHOOD};
use super::config::NorrConfig;
use super::fields::{FieldGrid, MacroSample};
use super::flora::{Column, FloraBlocks, Understory, Water};
use super::height::{HeightFields, Relief};
use super::hydro::{seal, Hydrography, WaterColumn, WaterKind};
use super::landforms::Coast;
use super::math::GradField;
use super::subsoil::{ColumnIn, DepositInfo, Region, SiteProbe, Sites, Subsoil, SubsoilBlocks};
use super::trees::{TreeBlocks, TreeColumn, TreeProbe, Trees};
use crate::chunk::{Chunk, AIR, CHUNK_SIZE_X, CHUNK_SIZE_Z};
use crate::profile::{BiomeId, Climate, TerrainGenerator};
use crate::world::find_spawn_location;

/// Blocs utilisés par le générateur, lus dans le registre du monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NorrBlocks {
    pub bedrock: BlockId,
    pub granite: BlockId,
    pub till: BlockId,
    pub gravel: BlockId,
    pub water: BlockId,
    pub sand: BlockId,
    pub cobbles: BlockId,
    pub peat: BlockId,
    pub permafrost: BlockId,
    pub ice: BlockId,
    pub basalt: BlockId,
    pub black_sand: BlockId,
    pub snow: BlockId,
    pub sea_ice: BlockId,
    /// Roches de province, minerais et blocs du sous-sol (session 2.2).
    pub sub: SubsoilBlocks,
    /// Plantes de la strate basse (session 2.4).
    pub flora: FloraBlocks,
    /// Troncs et feuillages des arbres (session 2.3).
    pub trees: TreeBlocks,
    /// Sols des biomes, par identifiant de biome.
    pub soils: [BiomeSoil; NorrBiome::COUNT],
}

impl NorrBlocks {
    pub fn from_registry(registry: &BlockRegistry) -> Self {
        Self {
            bedrock: registry.get("bedrock").id,
            granite: registry.get("granit").id,
            till: registry.get("moran").id,
            gravel: registry.get("grus").id,
            water: registry.get("water").id,
            sand: registry.get("flygsand").id,
            cobbles: registry.get("klappersten").id,
            peat: registry.get("torv").id,
            permafrost: registry.get("permafrost").id,
            ice: registry.get("blais").id,
            basalt: registry.get("basaltsten").id,
            black_sand: registry.get("svartsand").id,
            snow: registry.get("sno").id,
            sea_ice: registry.get("havsis").id,
            sub: SubsoilBlocks::from_registry(registry),
            flora: FloraBlocks::from_registry(registry),
            trees: TreeBlocks::from_registry(registry),
            soils: BiomeSoil::table(registry),
        }
    }

    /// Couverture d'une colonne d'eau `c` (sol `c.top`) de relief `r`, de biome `s`, de pente
    /// `slope`, avec le tirage de tache `patch` (millièmes).
    fn cover(
        &self,
        s: &Selection,
        r: &Relief,
        c: &WaterColumn,
        slope: i32,
        sea: i32,
        patch: u16,
    ) -> Cover {
        let f = &r.forms;
        let h = c.top;
        let dry = !c.is_wet();
        let soil = &self.soils[s.biome as usize];
        // Peau d'un bloc : neige au-dessus de y_n ; sinon, sur le plat, tache ou peau du biome.
        let skin = |allowed: bool| -> Option<BlockId> {
            if !dry {
                return None;
            }
            if s.snow {
                return Some(self.snow);
            }
            if !allowed || slope > SKIN_SLOPE {
                return None;
            }
            match soil.patch {
                Some((b, share)) if patch < share => Some(b),
                _ => soil.top,
            }
        };
        let make =
            |block: BlockId, depth: i32, core: Option<(BlockId, i32)>, allowed: bool| Cover {
                block,
                depth,
                core,
                skin: skin(allowed),
            };
        if f.lava > 0.5 {
            if dry && h <= sea + 1 {
                return make(self.black_sand, 3, None, false);
            }
            return make(self.basalt, 4, None, true);
        }
        if c.dam {
            return make(soil.rock, ROCK_DEPTH, None, false);
        }
        if f.moraine > 1.0 {
            return make(self.gravel, 3, None, false);
        }
        if f.ice > 0.5 && slope <= BARE_SLOPE {
            return make(self.gravel, 1, None, false);
        }
        if f.palsa > 1.0 && dry {
            // Croûte de tourbe d'un bloc sur un cœur de glace du sol.
            let core = (f.palsa.floor() as i32).max(1);
            return make(self.peat, 1, Some((self.permafrost, core)), true);
        }
        if f.mire > 0.5 && slope <= BARE_SLOPE {
            return make(self.peat, 3, None, true);
        }
        if (f.esker > 1.0 || f.drumlin > 1.0) && dry {
            return make(self.gravel, 4, None, true);
        }
        if slope > BARE_SLOPE {
            return make(soil.rock, ROCK_DEPTH, None, false);
        }
        match f.coast {
            Coast::Skargard if dry => return make(self.granite, 0, None, true),
            // Dunes : sable nu sur la plage, fixé par la peau du biome (lichen, aiguilles) au-delà.
            Coast::Dunes if h <= sea + 6 => return make(self.sand, 3, None, h > sea + 2),
            Coast::Raised if dry => return make(self.cobbles, 1, None, true),
            _ => {}
        }
        if !dry {
            let core = (soil.filler != soil.floor).then_some((soil.filler, soil.depth));
            make(soil.floor, 2, core, false)
        } else if h <= c.level + 1 {
            make(soil.floor, 2, None, false)
        } else {
            make(soil.filler, (soil.depth - slope).max(1), None, true)
        }
    }
}

/// Couverture d'une colonne : `depth` blocs de `block` sous la surface, puis, dessous, `core`
/// (bloc, épaisseur) s'il y en a un ; enfin la peau `skin` remplace le bloc du dessus.
struct Cover {
    block: BlockId,
    depth: i32,
    core: Option<(BlockId, i32)>,
    skin: Option<BlockId>,
}

/// Haut de la glace d'un glacier (premier `y` libre au-dessus d'elle) : `plancher(h + épaisseur)`,
/// ou `None` sans glace au-dessus de `surface`.
fn ice_top(r: &Relief, surface: i32, world_height: i32) -> Option<i32> {
    if r.forms.ice <= 0.0 {
        return None;
    }
    let t = ((r.h + r.forms.ice).floor() as i32).min(world_height - 1);
    (t > surface).then_some(t)
}

/// Pente (plus grande différence de surface avec les 4 voisines) au-delà de laquelle la roche
/// reste nue.
const BARE_SLOPE: i32 = 2;
/// Pente au-delà de laquelle la peau du biome (mousse, herbe, lichen…) ne tient pas.
const SKIN_SLOPE: i32 = 1;
/// Épaisseur de la roche du biome sur les pentes nues (calcaire du klint, granit rouge…).
const ROCK_DEPTH: i32 = 2;
/// Constante de semence des chenaux de la banquise.
const LEADS_SEED: u64 = 0x4E52_5743_0000_0403;
/// Longueur d'onde des chenaux de la banquise, en blocs.
const LEADS_WAVELENGTH: f64 = 70.0;
/// Demi-largeur des chenaux (écart à 0,5 du bruit de gradient).
const LEADS_WIDTH: f64 = 0.025;
/// Voisines testées pour la proximité de l'eau (aulne : à moins de 4 blocs).
const WET_RING: [(i32, i32); 12] = [
    (2, 0),
    (-2, 0),
    (0, 2),
    (0, -2),
    (4, 0),
    (-4, 0),
    (0, 4),
    (0, -4),
    (3, 3),
    (-3, 3),
    (3, -3),
    (-3, -3),
];
/// Chunks dont les biomes restent en cache (vidé d'un coup au-delà).
const BIOME_CACHE_MAX: usize = 4096;

/// Biomes des 16 × 16 colonnes d'un chunk, `[lz * 16 + lx]`.
type ChunkBiomes = [BiomeId; CHUNK_SIZE_X * CHUNK_SIZE_Z];

/// Colonnes analysées d'un chunk et de sa bordure (`x0 − 1 ..= x0 + 16`, idem en z), 18 × 18.
struct ChunkColumns {
    water: Vec<WaterColumn>,
    reliefs: Vec<Relief>,
    /// Champs macroscopiques des mêmes colonnes.
    macros: Vec<MacroSample>,
    /// Sélections des 16 × 16 colonnes du chunk, `[lz * 16 + lx]`.
    biomes: Vec<Selection>,
}

/// Générateur Norrcraft : fonction pure de (graine, configuration, colonne).
#[derive(Debug, Clone)]
pub struct NorrcraftTerrain {
    hydro: Hydrography,
    blocks: NorrBlocks,
    selector: BiomeSelector,
    subsoil: Subsoil,
    understory: Understory,
    trees: Trees,
    leads: GradField,
    /// Biomes des chunks déjà analysés, partagés par toutes les copies du générateur (le maillage
    /// relit les biomes des chunks que la génération vient de calculer).
    biome_cache: Arc<Mutex<HashMap<(i32, i32), ChunkBiomes>>>,
}

impl NorrcraftTerrain {
    pub fn new(seed: i64, cfg: NorrConfig, blocks: NorrBlocks) -> Self {
        Self {
            hydro: Hydrography::new(seed, cfg),
            blocks,
            selector: BiomeSelector::new(seed, cfg.sea_level),
            subsoil: Subsoil::new(seed, cfg, blocks.sub),
            understory: Understory::new(seed, blocks.flora),
            trees: Trees::new(seed, cfg.sea_level, blocks.trees),
            leads: GradField::new(seed, LEADS_SEED),
            biome_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn heights(&self) -> &HeightFields {
        self.hydro.heights()
    }

    pub fn hydrography(&self) -> &Hydrography {
        &self.hydro
    }

    fn sea(&self) -> i32 {
        self.heights().config().sea_level
    }

    fn world_height(&self) -> i32 {
        self.hydro.hydrology().world_height()
    }

    /// Colonnes d'eau scellées, relief et champs du chunk et de sa bordure (`x0 − 1 ..= x0 + 16`,
    /// idem en z) : les colonnes brutes sont calculées sur 20 × 20 (une bordure de plus pour le
    /// scellement), champs lents lus par un treillis 4 × 4 commun. La bordure sert aux pentes et
    /// aux gradients des frontières de biomes.
    fn analyse(&self, x0: i32, z0: i32) -> ChunkColumns {
        let heights = self.hydro.heights();
        let (xa, za) = (x0 - 2, z0 - 2);
        let (xb, zb) = (x0 + CHUNK_SIZE_X as i32 + 1, z0 + CHUNK_SIZE_Z as i32 + 1);
        let grid = FieldGrid::new(heights.macro_fields(), xa, za, xb, zb);
        let lakes = self.hydro.lakes(xa, za, xb, zb);
        let hydro = self.hydro.hydrology();
        let n = (xb - xa + 1) as usize;
        let mut raw = Vec::with_capacity(n * n);
        let mut reliefs = Vec::with_capacity(n * n);
        let mut macros: Vec<MacroSample> = Vec::with_capacity(n * n);
        for wz in za..=zb {
            for wx in xa..=xb {
                let m = grid.at(wx, wz);
                let (x, z) = (f64::from(wx), f64::from(wz));
                let r = heights.relief(x, z, &m);
                raw.push(hydro.column(x, z, &r, &m, &lakes));
                reliefs.push(r);
                macros.push(m);
            }
        }
        let at = |i: usize, j: usize| raw[j * n + i];
        let inner_n = n - 2;
        let mut water = Vec::with_capacity(inner_n * inner_n);
        let mut inner = Vec::with_capacity(inner_n * inner_n);
        let mut inner_macros = Vec::with_capacity(inner_n * inner_n);
        for j in 1..n - 1 {
            for i in 1..n - 1 {
                let nb = [at(i - 1, j), at(i + 1, j), at(i, j - 1), at(i, j + 1)];
                water.push(seal(at(i, j), nb, self.world_height()));
                inner.push(reliefs[j * n + i]);
                inner_macros.push(macros[j * n + i]);
            }
        }
        // Biomes des 16 × 16 colonnes (indices de la grille 20 × 20 : chunk = 2..18).
        let mut biomes = Vec::with_capacity(CHUNK_SIZE_X * CHUNK_SIZE_Z);
        for lz in 0..CHUNK_SIZE_Z {
            for lx in 0..CHUNK_SIZE_X {
                let (i, j) = ((lx + 2) as i32, (lz + 2) as i32);
                let k = |(dx, dz): (i32, i32)| (j + dz) as usize * n + (i + dx) as usize;
                let nb = Neighbourhood {
                    m: NEIGHBOURHOOD.map(|d| &macros[k(d)]),
                    r: NEIGHBOURHOOD.map(|d| &reliefs[k(d)]),
                };
                let w = water[(lz + 1) * inner_n + lx + 1];
                let (wx, wz) = (x0 + lx as i32, z0 + lz as i32);
                biomes.push(
                    self.selector
                        .select(wx, wz, &nb, &w, heights.macro_fields()),
                );
            }
        }
        ChunkColumns {
            water,
            reliefs: inner,
            macros: inner_macros,
            biomes,
        }
    }

    fn remember(&self, cx: i32, cz: i32, sel: &[Selection]) -> ChunkBiomes {
        let mut ids = [0; CHUNK_SIZE_X * CHUNK_SIZE_Z];
        for (id, s) in ids.iter_mut().zip(sel) {
            *id = s.biome.id();
        }
        let mut cache = self.biome_cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= BIOME_CACHE_MAX {
            cache.clear();
        }
        cache.insert((cx, cz), ids);
        ids
    }

    /// Biome de la colonne `(wx, wz)` calculé seul (sans cache, sans chunk) : **exactement** la
    /// sélection du chunk généré. Sert aux cartes et aux tests.
    pub fn column_biome(&self, wx: i32, wz: i32) -> Selection {
        let heights = self.heights();
        let grid = FieldGrid::new(heights.macro_fields(), wx - 1, wz - 1, wx + 1, wz + 1);
        let macros = NEIGHBOURHOOD.map(|(dx, dz)| grid.at(wx + dx, wz + dz));
        let reliefs: [Relief; 5] = std::array::from_fn(|k| {
            let (dx, dz) = NEIGHBOURHOOD[k];
            heights.relief(f64::from(wx + dx), f64::from(wz + dz), &macros[k])
        });
        let nb = Neighbourhood {
            m: std::array::from_fn(|k| &macros[k]),
            r: std::array::from_fn(|k| &reliefs[k]),
        };
        let w = self.hydro.column(wx, wz);
        self.selector
            .select(wx, wz, &nb, &w, heights.macro_fields())
    }

    /// Chenal ouvert dans la banquise.
    fn lead(&self, wx: i32, wz: i32) -> bool {
        let v = self.leads.n(
            f64::from(wx) / LEADS_WAVELENGTH,
            f64::from(wz) / LEADS_WAVELENGTH,
        );
        (v - 0.5).abs() < LEADS_WIDTH
    }

    /// Premier `y` libre au-dessus de la colonne sèche `(wx, wz)` (glace comprise), `None` si la
    /// colonne est sous l'eau.
    fn dry_surface(&self, wx: i32, wz: i32) -> Option<i32> {
        let c = self.hydro.column(wx, wz);
        if c.is_wet() {
            return None;
        }
        let r = self.heights().column(wx, wz);
        Some(ice_top(&r, c.top, self.world_height()).unwrap_or(c.top))
    }

    pub fn subsoil(&self) -> &Subsoil {
        &self.subsoil
    }

    /// Placement des arbres (session 2.3).
    pub fn trees(&self) -> &Trees {
        &self.trees
    }

    /// Sondes de colonne isolée utilisées par le placement des arbres.
    pub fn with_tree_probe<R>(&self, f: impl FnOnce(&TreeProbe) -> R) -> R {
        let column = |x: i32, z: i32| self.tree_column(x, z);
        let water = |x: i32, z: i32| self.hydro.column(x, z);
        let wet_near = |x: i32, z: i32| {
            let lakes = self.hydro.lakes(x - 4, z - 4, x + 4, z + 4);
            WET_RING
                .iter()
                .any(|&(dx, dz)| self.hydro.raw_column(x + dx, z + dz, &lakes).is_wet())
        };
        f(&TreeProbe {
            column: &column,
            water: &water,
            wet_near: &wet_near,
        })
    }

    /// Colonne isolée vue par le placement des arbres : champs, relief, eau scellée et biome
    /// calculés une fois (même résultat que [`Self::column_biome`] et l'eau du chunk généré).
    fn tree_column(&self, wx: i32, wz: i32) -> TreeColumn {
        let heights = self.heights();
        let grid = FieldGrid::new(heights.macro_fields(), wx - 1, wz - 1, wx + 1, wz + 1);
        let macros = NEIGHBOURHOOD.map(|(dx, dz)| grid.at(wx + dx, wz + dz));
        let reliefs: [Relief; 5] = std::array::from_fn(|k| {
            let (dx, dz) = NEIGHBOURHOOD[k];
            heights.relief(f64::from(wx + dx), f64::from(wz + dz), &macros[k])
        });
        let nb = Neighbourhood {
            m: std::array::from_fn(|k| &macros[k]),
            r: std::array::from_fn(|k| &reliefs[k]),
        };
        let water = self.hydro.column(wx, wz);
        let sel = self
            .selector
            .select(wx, wz, &nb, &water, heights.macro_fields());
        let hmax = self.world_height();
        let h0 = reliefs[0].ground_top(hmax);
        let slope = reliefs[1..]
            .iter()
            .map(|r| (r.ground_top(hmax) - h0).abs())
            .max()
            .unwrap_or(0);
        TreeColumn {
            water,
            sel,
            m: macros[0],
            r: reliefs[0],
            slope,
        }
    }

    /// Corps du sous-sol (Malmberg, cuivre, poches, grottes marines, salles) qui touchent le chunk
    /// d'origine `(x0, z0)`.
    pub fn sites(&self, x0: i32, z0: i32) -> Sites {
        self.with_probe(|probe| self.subsoil.sites(Region::chunk(x0, z0), probe))
    }

    /// Corps de la région, décrits (cartes et tests).
    pub fn deposits(&self, reg: Region) -> Vec<DepositInfo> {
        self.with_probe(|probe| self.subsoil.deposits(reg, probe))
    }

    /// Emplacement du Malmberg de la cellule `(i, j)` de pas 512, s'il y en a un.
    pub fn malmberg_at_cell(&self, i: i32, j: i32) -> Option<(i32, i32, i32)> {
        self.with_probe(|probe| self.subsoil.malmberg_at_cell(i, j, probe))
    }

    fn with_probe<R>(&self, f: impl FnOnce(&SiteProbe) -> R) -> R {
        let surface = |x: i32, z: i32| self.hydro.column(x, z).top;
        let water = |x: i32, z: i32| self.hydro.column(x, z);
        let macros = |x: i32, z: i32| self.heights().macro_fields().sample_lattice(x, z);
        let biome = |x: i32, z: i32| self.column_biome(x, z).biome;
        f(&SiteProbe {
            surface: &surface,
            water: &water,
            macros: &macros,
            biome: &biome,
        })
    }
    /// Strate basse (session 2.4), **après** les arbres : une plante par colonne, sur le sol, au
    /// fond de l'eau ou à sa surface. Tout bloc non-air (tronc, feuille) occupe la cellule.
    fn place_understory(&self, chunk: &mut Chunk, cols: &ChunkColumns, x0: i32, z0: i32) {
        let n = CHUNK_SIZE_X + 2;
        let hmax = self.world_height();
        let water = self.blocks.water;
        for lz in 0..CHUNK_SIZE_Z {
            for lx in 0..CHUNK_SIZE_X {
                let (i, j) = (lx + 1, lz + 1);
                let c = cols.water[j * n + i];
                let r = &cols.reliefs[j * n + i];
                let s = &cols.biomes[lz * CHUNK_SIZE_X + lx];
                let h = c.top;
                if s.snow || h < 2 || h >= hmax - 1 || ice_top(r, c.surface(), hmax).is_some() {
                    continue;
                }
                let wet_nb = |k: usize| {
                    let nb = cols.water[k];
                    nb.is_wet()
                        && (matches!(nb.kind, WaterKind::Lake | WaterKind::River)
                            || s.biome == NorrBiome::Bottenhav)
                };
                let (x, z) = (lx as i32, lz as i32);
                let column = Column {
                    wx: x0 + x,
                    wz: z0 + z,
                    biome: s.biome,
                    ground: self.understory.ground_of(chunk.block(x, h - 1, z)),
                    water: c.is_wet().then(|| match c.kind {
                        WaterKind::Sea if s.biome == NorrBiome::Bottenhav => Water::Brackish,
                        WaterKind::Sea => Water::Salt,
                        _ => Water::Fresh,
                    }),
                    depth: (c.level - h).max(0),
                    shore: !c.is_wet()
                        && [j * n + i - 1, j * n + i + 1, (j - 1) * n + i, (j + 1) * n + i]
                            .into_iter()
                            .any(wet_nb),
                };
                if let Some(p) = self.understory.pick(&column) {
                    let y = if p.floating { c.level - 1 } else { h };
                    let here = chunk.block(x, y, z);
                    if here == AIR || here == water {
                        chunk.set_block(x, y, z, p.block);
                    }
                }
            }
        }
    }

}

impl TerrainGenerator for NorrcraftTerrain {
    fn fork(&self) -> Box<dyn TerrainGenerator> {
        Box::new(self.clone())
    }

    /// Premier `y` non solide de la colonne (le fond, sous une nappe d'eau, banquise comprise ; le
    /// dessus de la glace d'un glacier sur la terre sèche).
    fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        self.dry_surface(wx, wz)
            .unwrap_or_else(|| self.hydro.column(wx, wz).top)
    }

    /// Biome de la colonne : lu dans le cache des chunks analysés, sinon le chunk entier est
    /// analysé (la sélection a besoin des voisines et de l'eau scellée).
    fn biome_at(&self, wx: i32, wz: i32) -> BiomeId {
        let (cx, cz) = (
            wx.div_euclid(CHUNK_SIZE_X as i32),
            wz.div_euclid(CHUNK_SIZE_Z as i32),
        );
        let k = (wz.rem_euclid(CHUNK_SIZE_Z as i32) as usize) * CHUNK_SIZE_X
            + wx.rem_euclid(CHUNK_SIZE_X as i32) as usize;
        if let Some(ids) = self
            .biome_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(cx, cz))
        {
            return ids[k];
        }
        let cols = self.analyse(cx * CHUNK_SIZE_X as i32, cz * CHUNK_SIZE_Z as i32);
        self.remember(cx, cz, &cols.biomes)[k]
    }

    fn generate_chunk(&self, chunk: &mut Chunk) {
        let pos = chunk.pos();
        let (x0, z0) = (pos.x * CHUNK_SIZE_X as i32, pos.z * CHUNK_SIZE_Z as i32);
        let cols = self.analyse(x0, z0);
        self.remember(pos.x, pos.z, &cols.biomes);
        let n = CHUNK_SIZE_X + 2;
        let top = |i: usize, j: usize| cols.water[j * n + i].top;
        let sea = self.sea();
        let hmax = self.world_height();
        let b = &self.blocks;
        let sites = self.sites(x0, z0);

        for lz in 0..CHUNK_SIZE_Z {
            for lx in 0..CHUNK_SIZE_X {
                let (i, j) = (lx + 1, lz + 1);
                let c = cols.water[j * n + i];
                let r = &cols.reliefs[j * n + i];
                let m = &cols.macros[j * n + i];
                let s = &cols.biomes[lz * CHUNK_SIZE_X + lx];
                let h = c.top;
                let slope = [top(i - 1, j), top(i + 1, j), top(i, j - 1), top(i, j + 1)]
                    .into_iter()
                    .map(|v| (v - h).abs())
                    .max()
                    .unwrap_or(0);
                let (wx, wz) = (x0 + lx as i32, z0 + lz as i32);
                let cover = b.cover(s, r, &c, slope, sea, self.selector.patch_draw(wx, wz));
                let core_bottom = h - cover.depth - cover.core.map_or(0, |k| k.1);
                let (x, z) = (lx as i32, lz as i32);
                let sc = self.subsoil.column(&ColumnIn {
                    wx,
                    wz,
                    water: c,
                    neighbours: [
                        cols.water[j * n + i - 1],
                        cols.water[j * n + i + 1],
                        cols.water[(j - 1) * n + i],
                        cols.water[(j + 1) * n + i],
                    ],
                    biome: s.biome,
                    fresh_water: s.fresh_water,
                    snow: s.snow,
                    m,
                    province: self.subsoil.province(wx, wz, m),
                    cover_bottom: core_bottom,
                });
                // Roche nue (pentes, barrages, îlots) : celle de la province, sauf si le biome
                // fixe la sienne (calcaire du klint, granit rouge…).
                let cover_block = if cover.block == b.granite {
                    sc.rock()
                } else {
                    cover.block
                };
                for y in 0..h {
                    let (base, bulk) = if y >= h - cover.depth {
                        (cover_block, false)
                    } else if y >= core_bottom {
                        (cover.core.map_or(b.granite, |k| k.0), false)
                    } else {
                        (b.granite, true)
                    };
                    let (base, bulk) = match cover.skin {
                        Some(skin) if y == h - 1 && h >= 2 => (skin, false),
                        _ => (base, bulk),
                    };
                    let id = self.subsoil.cell(&sc, &sites, y, base, bulk);
                    chunk.set_block(x, y, z, id);
                }
                for y in h..c.level {
                    chunk.set_block(x, y, z, b.water);
                }
                // Banquise : la couche d'eau du dessus gèle, hors des chenaux.
                if s.biome == NorrBiome::Packis
                    && c.is_wet()
                    && c.kind == WaterKind::Sea
                    && !self.lead(wx, wz)
                {
                    chunk.set_block(x, c.level - 1, z, b.sea_ice);
                }
                // Glace des glaciers, au-dessus du sol et de l'eau.
                let surface = c.surface();
                let ice = ice_top(r, surface, hmax);
                if let Some(t) = ice {
                    for y in surface..t {
                        // Tunnel de fonte à la base du glacier (grotte de glace, doc 02 §8.3).
                        if !self.subsoil.ice_tunnel(&sc, surface, t, y) {
                            chunk.set_block(x, y, z, b.ice);
                        }
                    }
                }
            }
        }
        // Arbres (session 2.3) : en dernier, les passes de décor suivantes traitent tout bloc
        // non-air comme occupé.
        self.with_tree_probe(|probe| self.trees.pass(chunk, probe));
        self.place_understory(chunk, &cols, x0, z0);
    }

    /// Départ cherché en anneaux autour de l'origine, sur la terre sèche au-dessus du niveau de la
    /// mer **du monde Norrcraft** (la configuration vanilla du monde porte encore son propre
    /// niveau) : jamais dans un lac ni une rivière.
    fn find_spawn(&self, _sea_level: i32) -> [f64; 3] {
        find_spawn_location(self.sea(), |x, z| {
            self.dry_surface(x, z).unwrap_or(i32::MIN)
        })
    }

    fn climate_at(&self, wx: i32, wz: i32) -> Option<Climate> {
        let m = self.heights().macro_fields().sample_lattice(wx, wz);
        Some(Climate {
            temperature: m.t0,
            humidity: m.hm,
            latitude: None,
        })
    }
}
