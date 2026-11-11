//! Générateur de terrain Norrcraft (sessions 1.2 et 1.4) : le relief de [`HeightFields`] rempli de
//! roche, de moraine et de gravier, et l'eau de l'hydrographie ([`Hydrography`] : mer, lacs,
//! rivières, toutes étanches).
//!
//! Les blocs de surface définitifs viennent avec les biomes (session 2.1) ; d'ici là la règle est
//! minimale et lisible : granit partout, moraine (`moran`) sur les pentes douces des terres basses,
//! gravier (`grus`) sous l'eau et sur les rives, granit nu sur les parois, les barrages et en
//! altitude.
//!
//! Session 1.3 : les formes de relief ont leurs blocs, provisoires eux aussi (textures empruntées) :
//! basalte du champ de lave, glace bleue (`blais`) des glaciers, gravier des moraines et des
//! eskers, cœur de `permafrost` des palses, tourbe (`torv`) des tourbières, sable des dunes,
//! galets (`klappersten`) des côtes soulevées, granit nu des îlots de l'archipel. La glace est
//! posée au-dessus du sol et de l'eau, jusqu'à `plancher(h + épaisseur)`.

use nb_data::{BlockId, BlockRegistry};

use super::config::NorrConfig;
use super::fields::FieldGrid;
use super::height::{HeightFields, Relief};
use super::hydro::{seal, Hydrography, WaterColumn};
use super::landforms::Coast;
use crate::chunk::{Chunk, CHUNK_SIZE_X, CHUNK_SIZE_Z};
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
        }
    }

    /// Couverture meuble d'une colonne d'eau `c` (sol `c.top`) de relief `r` et de pente `slope`.
    fn cover(&self, r: &Relief, c: &WaterColumn, slope: i32, sea: i32) -> Cover {
        let f = &r.forms;
        let h = c.top;
        let plain = |block: BlockId, depth: i32| Cover {
            block,
            depth,
            core: None,
        };
        if f.lava > 0.5 {
            return plain(self.basalt, 4);
        }
        if c.dam {
            return plain(self.granite, 0);
        }
        if f.moraine > 1.0 {
            return plain(self.gravel, 3);
        }
        if f.ice > 0.5 && slope <= BARE_SLOPE {
            return plain(self.gravel, 1);
        }
        if f.palsa > 1.0 && !c.is_wet() {
            // Croûte de tourbe d'un bloc sur un cœur de glace du sol.
            let core = (f.palsa.floor() as i32).max(1);
            return Cover {
                block: self.peat,
                depth: 1,
                core: Some((self.permafrost, core)),
            };
        }
        if f.mire > 0.5 && slope <= BARE_SLOPE {
            return plain(self.peat, 3);
        }
        if (f.esker > 1.0 || f.drumlin > 1.0) && !c.is_wet() {
            return plain(self.gravel, 4);
        }
        if slope > BARE_SLOPE {
            return plain(self.granite, 0);
        }
        match f.coast {
            Coast::Skargard if !c.is_wet() => return plain(self.granite, 0),
            Coast::Dunes if h <= sea + 6 => return plain(self.sand, 3),
            Coast::Raised if !c.is_wet() => return plain(self.cobbles, 1),
            _ => {}
        }
        if c.is_wet() || h <= c.level + 1 {
            plain(self.gravel, 2)
        } else if h < sea + TILL_MAX_ABOVE_SEA {
            plain(self.till, 3 - slope)
        } else {
            plain(self.granite, 0)
        }
    }
}

/// Couverture d'une colonne : `depth` blocs de `block` sous la surface, puis, dessous, `core`
/// (bloc, épaisseur) s'il y en a un.
struct Cover {
    block: BlockId,
    depth: i32,
    core: Option<(BlockId, i32)>,
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
/// Au-dessus de S + 50, la moraine laisse place à la roche (en attendant l'étagement de la 2.1).
const TILL_MAX_ABOVE_SEA: i32 = 50;

/// Générateur Norrcraft : fonction pure de (graine, configuration, colonne).
#[derive(Debug, Clone)]
pub struct NorrcraftTerrain {
    hydro: Hydrography,
    blocks: NorrBlocks,
}

impl NorrcraftTerrain {
    pub fn new(seed: i64, cfg: NorrConfig, blocks: NorrBlocks) -> Self {
        Self {
            hydro: Hydrography::new(seed, cfg),
            blocks,
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

    /// Colonnes d'eau scellées du chunk et de sa bordure (`x0 − 1 ..= x0 + 16`, idem en z) : les
    /// colonnes brutes sont calculées sur 20 × 20 (une bordure de plus pour le scellement), champs
    /// lents lus par un treillis 4 × 4 commun. La bordure scellée sert aux pentes. Renvoie aussi le
    /// relief des colonnes du chunk et de sa bordure (formes de la 1.3).
    fn water_grid(&self, x0: i32, z0: i32) -> (Vec<WaterColumn>, Vec<Relief>) {
        let heights = self.hydro.heights();
        let (xa, za) = (x0 - 2, z0 - 2);
        let (xb, zb) = (x0 + CHUNK_SIZE_X as i32 + 1, z0 + CHUNK_SIZE_Z as i32 + 1);
        let grid = FieldGrid::new(heights.macro_fields(), xa, za, xb, zb);
        let lakes = self.hydro.lakes(xa, za, xb, zb);
        let hydro = self.hydro.hydrology();
        let n = (xb - xa + 1) as usize;
        let mut raw = Vec::with_capacity(n * n);
        let mut reliefs = Vec::with_capacity(n * n);
        for wz in za..=zb {
            for wx in xa..=xb {
                let m = grid.at(wx, wz);
                let (x, z) = (f64::from(wx), f64::from(wz));
                let r = heights.relief(x, z, &m);
                raw.push(hydro.column(x, z, &r, &m, &lakes));
                reliefs.push(r);
            }
        }
        let at = |i: usize, j: usize| raw[j * n + i];
        let mut out = Vec::with_capacity((n - 2) * (n - 2));
        let mut inner = Vec::with_capacity((n - 2) * (n - 2));
        for j in 1..n - 1 {
            for i in 1..n - 1 {
                let nb = [at(i - 1, j), at(i + 1, j), at(i, j - 1), at(i, j + 1)];
                out.push(seal(at(i, j), nb, self.world_height()));
                inner.push(reliefs[j * n + i]);
            }
        }
        (out, inner)
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
}

impl TerrainGenerator for NorrcraftTerrain {
    fn fork(&self) -> Box<dyn TerrainGenerator> {
        Box::new(self.clone())
    }

    /// Premier `y` non solide de la colonne (le fond, sous une nappe d'eau ; le dessus de la glace
    /// d'un glacier sur la terre sèche).
    fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        self.dry_surface(wx, wz)
            .unwrap_or_else(|| self.hydro.column(wx, wz).top)
    }

    fn biome_at(&self, _wx: i32, _wz: i32) -> BiomeId {
        0
    }

    fn generate_chunk(&self, chunk: &mut Chunk) {
        let pos = chunk.pos();
        let (x0, z0) = (pos.x * CHUNK_SIZE_X as i32, pos.z * CHUNK_SIZE_Z as i32);
        let (cols, reliefs) = self.water_grid(x0, z0);
        let n = CHUNK_SIZE_X + 2;
        let top = |i: usize, j: usize| cols[j * n + i].top;
        let sea = self.sea();
        let hmax = self.world_height();
        let b = self.blocks;

        for lz in 0..CHUNK_SIZE_Z {
            for lx in 0..CHUNK_SIZE_X {
                let (i, j) = (lx + 1, lz + 1);
                let c = cols[j * n + i];
                let r = &reliefs[j * n + i];
                let h = c.top;
                let slope = [top(i - 1, j), top(i + 1, j), top(i, j - 1), top(i, j + 1)]
                    .into_iter()
                    .map(|v| (v - h).abs())
                    .max()
                    .unwrap_or(0);
                // Couverture meuble selon la forme de relief (voir `NorrBlocks::cover`) ; par
                // défaut gravier sous l'eau et sur les rives, moraine sur les terres basses, rien
                // sur les parois, les barrages ni en altitude.
                let cover = b.cover(r, &c, slope, sea);
                let core_bottom = h - cover.depth - cover.core.map_or(0, |k| k.1);
                let (x, z) = (lx as i32, lz as i32);
                for y in 0..h {
                    let id = if y == 0 {
                        b.bedrock
                    } else if y >= h - cover.depth {
                        cover.block
                    } else if y >= core_bottom {
                        cover.core.map_or(b.granite, |k| k.0)
                    } else {
                        b.granite
                    };
                    chunk.set_block(x, y, z, id);
                }
                for y in h..c.level {
                    chunk.set_block(x, y, z, b.water);
                }
                // Glace des glaciers, au-dessus du sol et de l'eau.
                let surface = c.surface();
                if let Some(t) = ice_top(r, surface, hmax) {
                    for y in surface..t {
                        chunk.set_block(x, y, z, b.ice);
                    }
                }
            }
        }
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
