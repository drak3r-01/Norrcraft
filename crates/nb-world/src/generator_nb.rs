//! Passes du générateur « Norrblock » (colonnes, strates, minerais, arbres, décors, végétation).
//!
//! Le terrain lui-même (hauteur, biome) vient de [`crate::terrain::Terrain`] ; ce module le met
//! en blocs. Tout est déterministe en `(graine, configuration, position)` et **indépendant de
//! l'ordre de génération des chunks** : les arbres et les décors qui chevauchent une frontière
//! sont recalculés à l'identique par chaque chunk voisin (positions tirées d'une grille jitterée
//! par hachage, hauteur du sol redemandée au terrain), sans jamais dépendre du contenu des
//! chunks voisins.

use nb_data::{BlockId, BlockIds};

use crate::biome::{Biome, TreeType};
use crate::chunk::{Chunk, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};
use crate::generator::WorldGenerator;
use crate::noise::hash_position;
use crate::terrain::NbColumn;

const SX: i32 = CHUNK_SIZE_X as i32;
const SY: i32 = CHUNK_SIZE_Y as i32;
const SZ: i32 = CHUNK_SIZE_Z as i32;
/// Côté de la grille de colonnes avec une bordure d'un bloc (pentes, rivages).
const GRID: i32 = SX + 2;

#[inline]
fn gi(lx: i32, lz: i32) -> usize {
    ((lz + 1) * GRID + (lx + 1)) as usize
}

#[inline]
fn unit(h: i32) -> f64 {
    f64::from(h % 10000) / 10000.0
}

#[inline]
fn floor_div(a: i32, b: i32) -> i32 {
    a.div_euclid(b)
}

/// Générateur pseudo-aléatoire de décor (SplitMix64) semé par la position.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(a: i32, b: i32, salt: u32) -> Self {
        let s = (u64::from(a as u32) << 32 | u64::from(b as u32))
            ^ u64::from(salt).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        Rng(s)
    }
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_9B09);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Entier dans `[lo, hi]`.
    pub(crate) fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1) as u64) as i32
    }
    pub(crate) fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub(crate) fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }
}

/// Écrit des blocs en coordonnées monde dans un chunk (hors chunk : ignoré).
pub(crate) struct Painter<'a> {
    pub chunk: &'a mut Chunk,
    x0: i32,
    z0: i32,
    ids: &'a BlockIds,
}

impl<'a> Painter<'a> {
    pub(crate) fn new(chunk: &'a mut Chunk, ids: &'a BlockIds) -> Self {
        let p = chunk.pos();
        let (x0, z0) = (p.x.wrapping_mul(SX), p.z.wrapping_mul(SZ));
        Self { chunk, x0, z0, ids }
    }

    fn local(&self, wx: i32, y: i32, wz: i32) -> Option<(i32, i32, i32)> {
        let (lx, lz) = (wx.wrapping_sub(self.x0), wz.wrapping_sub(self.z0));
        ((0..SX).contains(&lx) && (0..SZ).contains(&lz) && (0..SY).contains(&y))
            .then_some((lx, y, lz))
    }

    fn is_leaf(&self, b: BlockId) -> bool {
        let i = self.ids;
        b == i.leaves
            || b == i.birch_leaves
            || b == i.spruce_leaves
            || b == i.jungle_leaves
            || b == i.acacia_leaves
    }

    fn is_plant(&self, b: BlockId) -> bool {
        let i = self.ids;
        b == i.tall_grass
            || b == i.flower_red
            || b == i.flower_yellow
            || b == i.flower_blue
            || b == i.flower_pink
            || b == i.flower_white
            || b == i.flower_orange
            || b == i.lavender
            || b == i.reeds
            || b == i.fern
            || b == i.dead_bush
            || b == i.mushroom_red
            || b == i.mushroom_brown
    }

    /// Tronc / bloc plein : remplace l'air, les feuilles et les plantes.
    pub(crate) fn solid(&mut self, wx: i32, y: i32, wz: i32, b: BlockId) {
        if let Some((lx, y, lz)) = self.local(wx, y, wz) {
            let cur = self.chunk.block(lx, y, lz);
            if cur == self.ids.air || self.is_leaf(cur) || self.is_plant(cur) {
                self.chunk.set_block(lx, y, lz, b);
            }
        }
    }

    /// Feuille : seulement sur l'air ou une plante.
    pub(crate) fn leaf(&mut self, wx: i32, y: i32, wz: i32, b: BlockId) {
        if let Some((lx, y, lz)) = self.local(wx, y, wz) {
            let cur = self.chunk.block(lx, y, lz);
            if cur == self.ids.air || self.is_plant(cur) {
                self.chunk.set_block(lx, y, lz, b);
            }
        }
    }

    /// Écrit `b` sur de l'eau ou du sol meuble (coraux, icebergs).
    pub(crate) fn wet(&mut self, wx: i32, y: i32, wz: i32, b: BlockId) {
        if let Some((lx, y, lz)) = self.local(wx, y, wz) {
            let cur = self.chunk.block(lx, y, lz);
            if cur == self.ids.water
                || cur == self.ids.sand
                || cur == self.ids.gravel
                || cur == self.ids.air
            {
                self.chunk.set_block(lx, y, lz, b);
            }
        }
    }

    /// Ellipsoïde de feuilles (légèrement irrégulier).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn blob(
        &mut self,
        c: (i32, i32, i32),
        r: (f64, f64, f64),
        b: BlockId,
        rng: &mut Rng,
        ragged: f64,
    ) {
        let (rx, ry, rz) = r;
        let (ix, iy, iz) = (rx.ceil() as i32, ry.ceil() as i32, rz.ceil() as i32);
        for dy in -iy..=iy {
            for dz in -iz..=iz {
                for dx in -ix..=ix {
                    let d = (f64::from(dx) / rx).powi(2)
                        + (f64::from(dy) / ry).powi(2)
                        + (f64::from(dz) / rz).powi(2);
                    if d <= 1.0 && !(d > 0.72 && rng.chance(ragged)) {
                        self.leaf(c.0 + dx, c.1 + dy, c.2 + dz, b);
                    }
                }
            }
        }
    }

    /// Segment de tronc (pas de 1 bloc, de `a` vers `b`).
    pub(crate) fn line(&mut self, a: (i32, i32, i32), b: (i32, i32, i32), block: BlockId) {
        let n = (b.0 - a.0)
            .abs()
            .max((b.1 - a.1).abs())
            .max((b.2 - a.2).abs())
            .max(1);
        for i in 0..=n {
            let t = f64::from(i) / f64::from(n);
            let x = f64::from(a.0) + f64::from(b.0 - a.0) * t;
            let y = f64::from(a.1) + f64::from(b.1 - a.1) * t;
            let z = f64::from(a.2) + f64::from(b.2 - a.2) * t;
            self.solid(x.round() as i32, y.round() as i32, z.round() as i32, block);
        }
    }
}

/// Bandes de terracotta des mesas, de bas en haut (période de 24 blocs).
fn strata_block(ids: &BlockIds, y: i32) -> BlockId {
    let t = [
        ids.terracotta,
        ids.terracotta_orange,
        ids.terracotta,
        ids.terracotta_white,
        ids.terracotta,
        ids.terracotta,
        ids.terracotta_red,
        ids.terracotta_orange,
        ids.terracotta,
        ids.terracotta_yellow,
        ids.terracotta,
        ids.terracotta_brown,
        ids.terracotta,
        ids.terracotta_white,
        ids.terracotta_white,
        ids.terracotta,
        ids.terracotta_orange,
        ids.terracotta_orange,
        ids.terracotta,
        ids.terracotta_red,
        ids.terracotta,
        ids.terracotta_brown,
        ids.terracotta,
        ids.terracotta_yellow,
    ];
    t[(y + 40).rem_euclid(24) as usize]
}

/// Couches de surface d'une colonne.
struct Plan {
    top: BlockId,
    fill: BlockId,
    fill_depth: i32,
    under: BlockId,
    under_depth: i32,
    strata: bool,
}

impl WorldGenerator {
    pub(crate) fn generate_chunk_nb(&self, chunk: &mut Chunk) {
        let pos = chunk.pos();
        let wx0 = pos.x.wrapping_mul(SX);
        let wz0 = pos.z.wrapping_mul(SZ);
        let mut grid: Vec<NbColumn> = Vec::with_capacity((GRID * GRID) as usize);
        for gz in -1..=SZ {
            for gx in -1..=SX {
                grid.push(
                    self.terrain
                        .column(wx0.wrapping_add(gx), wz0.wrapping_add(gz)),
                );
            }
        }
        self.nb_columns(chunk, wx0, wz0, &grid);
        if self.cfg.caves_enabled {
            self.nb_caves(chunk, wx0, wz0, &grid);
        }
        self.nb_ores(chunk, wx0, wz0);
        if self.cfg.caves_enabled {
            self.pass_lava(chunk);
        }
        self.nb_features(chunk, wx0, wz0);
        self.nb_vegetation(chunk, wx0, wz0, &grid);
    }

    fn rock_at(&self, wx: i32, y: i32, wz: i32, deep_y: i32) -> BlockId {
        let ids = &self.ids;
        if y < deep_y {
            return ids.slate;
        }
        let (x, yf, z) = (f64::from(wx), f64::from(y), f64::from(wz));
        if y > 18
            && y < 100
            && self
                .noise
                .evaluate3(x * 0.045 + 11.0, yf * 0.09, z * 0.045 - 5.0)
                > 0.40
        {
            return ids.marble;
        }
        if y < 56
            && self
                .noise
                .evaluate3(x * 0.06 - 90.0, yf * 0.1 + 20.0, z * 0.06 + 40.0)
                > 0.44
        {
            return ids.basalt;
        }
        ids.stone
    }

    /// Matériau d'un fond d'eau (océan, rivière, lac).
    fn floor_block(&self, col: &NbColumn, patch: f64) -> BlockId {
        let ids = &self.ids;
        let depth = self.cfg.sea_level - col.height;
        match col.biome {
            Biome::WarmOcean => ids.sand,
            Biome::FrozenOcean | Biome::DeepOcean => {
                if patch < 0.55 {
                    ids.gravel
                } else if patch < 0.75 {
                    ids.clay
                } else {
                    ids.sand
                }
            }
            Biome::River => {
                if patch < 0.35 {
                    ids.gravel
                } else if patch < 0.52 {
                    ids.clay
                } else {
                    ids.sand
                }
            }
            Biome::Swamp => {
                if patch < 0.5 {
                    ids.clay
                } else {
                    ids.dirt
                }
            }
            _ => {
                if patch < 0.26 {
                    ids.gravel
                } else if patch < 0.42 && depth <= 12 {
                    ids.clay
                } else {
                    ids.sand
                }
            }
        }
    }

    fn plan(&self, col: &NbColumn, slope: i32, patch: f64, rnd: i32, wet_nb: bool) -> Plan {
        let ids = &self.ids;
        let c = &*self.cfg;
        let sea = c.sea_level;
        let h = col.height;
        let mk = |top, fill, fd, under, ud| Plan {
            top,
            fill,
            fill_depth: fd,
            under,
            under_depth: ud,
            strata: false,
        };
        // Sous l'eau : fond marin, lit de rivière, cuvette de lac.
        if h < sea {
            let f = self.floor_block(col, patch);
            return mk(f, f, 2, ids.sand, 2);
        }
        let steep = slope >= 4;
        let grass_like = |top: BlockId| -> Plan {
            if steep {
                let t = if patch < 0.3 { ids.gravel } else { ids.stone };
                return mk(t, ids.stone, 2, ids.stone, 0);
            }
            mk(top, ids.dirt, 3, ids.stone, 0)
        };
        // Rivage : sable sur la bande qui touche l'eau.
        let shoreline = h <= sea + 1 && (h <= sea || wet_nb);
        match col.biome {
            Biome::Beach => {
                if col.temp < 0.3 {
                    mk(ids.gravel, ids.gravel, 2, ids.stone, 0)
                } else {
                    mk(ids.sand, ids.sand, 3, ids.sandstone, 4)
                }
            }
            Biome::StonyShore => mk(ids.gravel, ids.stone, 1, ids.stone, 0),
            Biome::Desert => mk(ids.sand, ids.sand, 3, ids.sandstone, 9),
            Biome::Badlands => {
                let mut p = mk(ids.terracotta, ids.terracotta, 0, ids.stone, 0);
                p.strata = true;
                if patch > 0.58 && slope <= 2 {
                    p.top = ids.red_sand;
                    p.fill = ids.red_sand;
                    p.fill_depth = 1;
                    p.strata = false;
                    p.under = ids.terracotta_orange;
                    p.under_depth = 4;
                }
                p
            }
            Biome::Volcano => {
                let r = unit(rnd);
                let t = if patch > 0.93 && col.volcano_t < 0.45 && r < 0.5 {
                    ids.magma
                } else if patch < 0.40 {
                    ids.ash
                } else {
                    ids.basalt
                };
                mk(t, ids.basalt, 3, ids.stone, 0)
            }
            Biome::Mountains => {
                let snow_line = c.snow_line_mountains + 10 + (patch * 10.0) as i32;
                if h > snow_line && slope <= 3 {
                    mk(ids.snow, ids.stone, 1, ids.stone, 0)
                } else if steep || h > 70 {
                    let t = if patch < 0.22 { ids.gravel } else { ids.stone };
                    mk(t, ids.stone, 2, ids.stone, 0)
                } else if patch > 0.4 && slope <= 2 {
                    mk(ids.grass, ids.dirt, 2, ids.stone, 0)
                } else {
                    mk(ids.stone, ids.stone, 2, ids.stone, 0)
                }
            }
            Biome::SnowyPeaks => {
                if slope <= 3 {
                    let t = if patch > 0.8 {
                        ids.packed_ice
                    } else {
                        ids.snow
                    };
                    mk(t, ids.snow, 2, ids.stone, 0)
                } else {
                    mk(ids.stone, ids.stone, 2, ids.stone, 0)
                }
            }
            Biome::SnowyPlains | Biome::SnowyTaiga => {
                if steep {
                    mk(ids.stone, ids.stone, 2, ids.stone, 0)
                } else {
                    mk(ids.snow, ids.dirt, 3, ids.stone, 0)
                }
            }
            Biome::IceSpikes => mk(ids.snow, ids.packed_ice, 3, ids.stone, 0),
            Biome::MushroomFields => mk(ids.mycelium, ids.dirt, 3, ids.stone, 0),
            Biome::Swamp => {
                if shoreline {
                    mk(ids.dirt, ids.clay, 3, ids.stone, 0)
                } else {
                    grass_like(ids.grass)
                }
            }
            Biome::Taiga => {
                if h > c.snow_line_taiga + (patch * 6.0) as i32 && !steep {
                    mk(ids.snow, ids.dirt, 3, ids.stone, 0)
                } else if shoreline {
                    mk(ids.sand, ids.sand, 2, ids.stone, 0)
                } else if patch > 0.62 {
                    grass_like(ids.podzol)
                } else {
                    grass_like(ids.grass)
                }
            }
            Biome::MegaTaiga => {
                if shoreline {
                    mk(ids.sand, ids.sand, 2, ids.stone, 0)
                } else if patch > 0.4 {
                    grass_like(ids.podzol)
                } else {
                    grass_like(ids.grass)
                }
            }
            Biome::DarkForest => {
                if shoreline {
                    mk(ids.sand, ids.sand, 2, ids.stone, 0)
                } else if patch > 0.84 {
                    grass_like(ids.moss)
                } else if patch > 0.66 {
                    grass_like(ids.podzol)
                } else {
                    grass_like(ids.grass)
                }
            }
            Biome::Heath => {
                if shoreline {
                    mk(ids.sand, ids.sand, 2, ids.stone, 0)
                } else if patch > 0.7 {
                    grass_like(ids.moss)
                } else {
                    grass_like(ids.grass)
                }
            }
            // Ocean / River à ras de l'eau (berges) et biomes herbeux.
            _ => {
                if shoreline {
                    mk(ids.sand, ids.sand, 2, ids.stone, 0)
                } else {
                    grass_like(ids.grass)
                }
            }
        }
    }

    fn nb_columns(&self, chunk: &mut Chunk, wx0: i32, wz0: i32, grid: &[NbColumn]) {
        let ids = &self.ids;
        let sea = self.cfg.sea_level;
        for lx in 0..SX {
            for lz in 0..SZ {
                let wx = wx0.wrapping_add(lx);
                let wz = wz0.wrapping_add(lz);
                let col = grid[gi(lx, lz)];
                let h = col.height;
                let slope = [
                    grid[gi(lx - 1, lz)].height,
                    grid[gi(lx + 1, lz)].height,
                    grid[gi(lx, lz - 1)].height,
                    grid[gi(lx, lz + 1)].height,
                ]
                .iter()
                .map(|n| (h - n).abs())
                .max()
                .unwrap_or(0);
                let (xf, zf) = (f64::from(wx), f64::from(wz));
                let patch = 0.5 + 0.5 * self.noise.evaluate2(xf * 0.06 + 31.7, zf * 0.06 - 12.9);
                let rnd = hash_position(
                    wx.wrapping_mul(7).wrapping_add(3),
                    wz.wrapping_mul(13).wrapping_add(5),
                );
                let deep_y = 9
                    + (self.noise.evaluate2(xf * 0.03 + 5.0, zf * 0.03 + 9.0) * 3.0).floor() as i32;
                let wet_nb =
                    (-1..=1).any(|dz| (-1..=1).any(|dx| grid[gi(lx + dx, lz + dz)].height < sea));
                let plan = self.plan(&col, slope, patch, rnd, wet_nb);
                for y in 0..h.min(SY) {
                    let d = h - 1 - y;
                    let b = if y == 0
                        || (y <= 2 && unit(hash_position(wx ^ (y * 977), wz)) < 0.45 && y < 3)
                    {
                        ids.bedrock
                    } else if plan.strata && y >= sea - 8 && d < 60 {
                        strata_block(ids, y)
                    } else if d == 0 {
                        plan.top
                    } else if d <= plan.fill_depth {
                        plan.fill
                    } else if d <= plan.fill_depth + plan.under_depth {
                        plan.under
                    } else {
                        self.rock_at(wx, y, wz, deep_y)
                    };
                    chunk.set_block(lx, y, lz, b);
                }
                // Lac de lave du cratère, puis eau.
                if col.lava_y > h {
                    for y in h..col.lava_y.min(SY) {
                        chunk.set_block(lx, y, lz, ids.lava);
                    }
                }
                for y in h..sea {
                    if chunk.block(lx, y, lz) == ids.air {
                        chunk.set_block(lx, y, lz, ids.water);
                    }
                }
            }
        }
    }

    /// Grottes : tunnels sinueux (intersection de deux surfaces de bruit centrées sur 0) et
    /// salles profondes. Contrairement au Java, où près d'un tiers des colonnes s'ouvraient en
    /// puits sur le ciel, le sol garde une croûte de 5 blocs ; seules quelques entrées, placées
    /// par un bruit lent, percent la surface (et jamais sous l'eau).
    fn nb_caves(&self, chunk: &mut Chunk, wx0: i32, wz0: i32, grid: &[NbColumn]) {
        let ids = &self.ids;
        let c = &*self.cfg;
        let sea = c.sea_level;
        let open = |b: BlockId| b == ids.air || b == ids.water || b == ids.bedrock || b == ids.lava;
        let (s1, s2) = (0.035, 0.04);
        let width = 0.075 + (c.cave_threshold - 0.42) * 0.2;
        let cavern_max = 30.min(c.cave_depth_max / 2);
        for lx in 0..SX {
            for lz in 0..SZ {
                let wx = f64::from(wx0.wrapping_add(lx));
                let wz = f64::from(wz0.wrapping_add(lz));
                let col = grid[gi(lx, lz)];
                let entrance = col.height > sea + 2
                    && col.lava_y == 0
                    && self.noise.evaluate2(wx * 0.017 + 400.0, wz * 0.017 - 90.0) > 0.52;
                let crust = if entrance { 1 } else { 5 };
                let top = c.cave_depth_max.min(col.height - crust);
                let mut y = 2;
                while y < top {
                    let yf = f64::from(y);
                    if open(chunk.block(lx, y, lz)) {
                        y += 2;
                        continue;
                    }
                    let n1 = (self.noise.fbm3d(wx * s1, yf * s1, wz * s1, 1) - 0.5) * 2.0;
                    let n2 =
                        (self
                            .noise
                            .fbm3d(wx * s2 + 500.0, yf * s2 + 500.0, wz * s2 + 500.0, 1)
                            - 0.5)
                            * 2.0;
                    if n1.abs() < width && n2.abs() < width {
                        chunk.set_block(lx, y, lz, ids.air);
                        if y + 1 < top && !open(chunk.block(lx, y + 1, lz)) {
                            chunk.set_block(lx, y + 1, lz, ids.air);
                        }
                    }
                    if y < cavern_max {
                        let cav = self.noise.fbm3d(
                            wx * 0.015 + 1000.0,
                            yf * 0.02 + 1000.0,
                            wz * 0.015 + 1000.0,
                            1,
                        );
                        if cav > c.cavern_threshold + 0.07 {
                            chunk.set_block(lx, y, lz, ids.air);
                            if y + 1 < cavern_max {
                                chunk.set_block(lx, y + 1, lz, ids.air);
                            }
                        }
                    }
                    y += 2;
                }
            }
        }
        // Champignons de grotte (y = 3, 6, …, 48).
        for lx in 1..SX - 1 {
            for lz in 1..SZ - 1 {
                let wx = wx0.wrapping_add(lx);
                let wz = wz0.wrapping_add(lz);
                for y in (3..50).step_by(3) {
                    if chunk.block(lx, y, lz) != ids.air
                        || !self.is_ore_host(chunk.block(lx, y - 1, lz))
                    {
                        continue;
                    }
                    let h = hash_position(
                        wx.wrapping_mul(7).wrapping_add(y * 31),
                        wz.wrapping_mul(13).wrapping_add(y * 17),
                    );
                    if h % 1000 < 8 {
                        let m = if h % 2 == 0 {
                            ids.mushroom_red
                        } else {
                            ids.mushroom_brown
                        };
                        chunk.set_block(lx, y, lz, m);
                    }
                }
            }
        }
    }

    fn is_ore_host(&self, b: BlockId) -> bool {
        let i = &self.ids;
        b == i.stone || b == i.sandstone || b == i.slate || b == i.basalt || b == i.marble
    }

    /// Minerais en petits amas (cellules de 2×2×2) : même quantité moyenne qu'avant, mais
    /// regroupés en filons au lieu d'être des blocs isolés.
    fn nb_ores(&self, chunk: &mut Chunk, wx0: i32, wz0: i32) {
        let ids = &self.ids;
        let o = &self.cfg.ores;
        let table = [
            (ids.coal_ore, o.coal, 73),
            (ids.iron_ore, o.iron, 137),
            (ids.gold_ore, o.gold, 251),
            (ids.diamond_ore, o.diamond, 389),
            (ids.emerald_ore, o.emerald, 521),
        ];
        const FILL: f64 = 0.6;
        for lx in 0..SX {
            for lz in 0..SZ {
                let wx = wx0.wrapping_add(lx);
                let wz = wz0.wrapping_add(lz);
                for y in 1..SY {
                    if !self.is_ore_host(chunk.block(lx, y, lz)) {
                        continue;
                    }
                    for &(ore, range, seed) in &table {
                        if y < range.min_y || y > range.max_y {
                            continue;
                        }
                        let (cx, cy, cz) = (wx >> 1, y >> 1, wz >> 1);
                        let hc = hash_position(
                            cx.wrapping_mul(seed).wrapping_add(cy * 31),
                            cz.wrapping_mul(seed).wrapping_add(cy * 17),
                        );
                        if unit(hc) >= range.chance / FILL {
                            continue;
                        }
                        let hv = hash_position(
                            wx.wrapping_mul(seed).wrapping_add(y * 5 + 1),
                            wz.wrapping_mul(seed).wrapping_add(y * 7 + 3),
                        );
                        if unit(hv) < FILL {
                            chunk.set_block(lx, y, lz, ore);
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Positions de décor sur une grille jitterée de cellules `cell × cell` couvrant le chunk
    /// élargi de `margin` blocs : `(x, z, hash)`.
    fn cells(wx0: i32, wz0: i32, cell: i32, margin: i32, salt: i32) -> Vec<(i32, i32, i32)> {
        let mut out = Vec::new();
        let (cx0, cx1) = (
            floor_div(wx0 - margin, cell),
            floor_div(wx0 + SX - 1 + margin, cell),
        );
        let (cz0, cz1) = (
            floor_div(wz0 - margin, cell),
            floor_div(wz0 + SZ - 1 + margin, cell),
        );
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                let h = hash_position(
                    cx.wrapping_mul(0x2545_F491u32 as i32).wrapping_add(salt),
                    cz.wrapping_mul(0x1B87_3593u32 as i32).wrapping_sub(salt),
                );
                let px = cx * cell + h % cell;
                let pz = cz * cell + (h / 64) % cell;
                out.push((px, pz, h));
            }
        }
        out
    }

    fn tree_density(&self, biome: Biome) -> f64 {
        let c = &*self.cfg;
        let ov = c.tree_density_overrides[biome.family().ordinal()];
        let base = if ov >= 0.0 {
            ov
        } else {
            self.biomes.config(biome).tree_density
        };
        base * c.vegetation_density
    }

    fn nb_features(&self, chunk: &mut Chunk, wx0: i32, wz0: i32) {
        let ids = self.ids;
        let sea = self.cfg.sea_level;
        let mut painter = Painter::new(chunk, &ids);

        // Arbres.
        for (px, pz, h) in Self::cells(wx0, wz0, 4, 7, 11) {
            let col = self.terrain.column(px, pz);
            let d = self.tree_density(col.biome);
            if d <= 0.0
                || col.height < sea
                || col.height > SY - 40
                || col.river < 0.7
                || col.lava_y > 0
            {
                continue;
            }
            // Bosquets : la densité module selon un bruit lent.
            let grove = self.noise.fbm2d(
                f64::from(px) * 0.014 + 60.0,
                f64::from(pz) * 0.014 - 20.0,
                2,
                2.0,
                0.5,
            );
            let local = (0.35 + 1.5 * ((grove - 0.5) * 2.2 + 0.5)).clamp(0.1, 1.9);
            let prob = (d * 16.0 * local).min(0.97);
            let mut rng = Rng::new(px, pz, 0x7ee);
            if unit(h) / 1.0 >= prob || !rng.chance(1.0) {
                continue;
            }
            let bc = self.biomes.config(col.biome);
            let kind = if bc.tree_mix.is_empty() {
                bc.tree_type
            } else {
                let total: u32 = bc.tree_mix.iter().map(|m| m.1).sum();
                let mut r = rng.next() as u32 % total;
                let mut k = bc.tree_mix[0].0;
                for &(t, w) in &bc.tree_mix {
                    if r < w {
                        k = t;
                        break;
                    }
                    r -= w;
                }
                k
            };
            if kind == TreeType::Palm && col.temp < 0.42 {
                continue;
            }
            if col.biome == Biome::Desert {
                continue;
            }
            self.place_tree_nb(&mut painter, kind, px, col.height, pz, &mut rng);
        }

        // Pics de glace.
        for (px, pz, h) in Self::cells(wx0, wz0, 7, 9, 29) {
            if unit(h) > 0.42 {
                continue;
            }
            let col = self.terrain.column(px, pz);
            if col.biome != Biome::IceSpikes {
                continue;
            }
            let mut rng = Rng::new(px, pz, 0x1ce);
            let tall = rng.chance(0.2);
            let height = if tall {
                rng.range(16, 26)
            } else {
                rng.range(6, 15)
            };
            let base_r = if tall {
                1.6
            } else {
                f64::from(rng.range(1, 3)) + 0.4
            };
            for i in 0..height {
                let r = base_r * (1.0 - f64::from(i) / f64::from(height));
                let ri = r.ceil() as i32;
                for dz in -ri..=ri {
                    for dx in -ri..=ri {
                        if f64::from(dx * dx + dz * dz) <= r * r + 0.3 {
                            let b = if (i + dx + dz) % 5 == 0 {
                                ids.ice
                            } else {
                                ids.packed_ice
                            };
                            painter.solid(px + dx, col.height - 2 + i, pz + dz, b);
                        }
                    }
                }
            }
        }

        // Rochers moussus.
        for (px, pz, h) in Self::cells(wx0, wz0, 11, 5, 47) {
            let col = self.terrain.column(px, pz);
            let chance = match col.biome {
                Biome::MegaTaiga => 0.40,
                Biome::Heath => 0.30,
                Biome::Hills => 0.22,
                Biome::Taiga | Biome::SnowyTaiga => 0.14,
                Biome::Forest | Biome::DarkForest | Biome::Mountains => 0.05,
                _ => 0.0,
            };
            if chance <= 0.0 || unit(h) >= chance || col.height < sea {
                continue;
            }
            let mut rng = Rng::new(px, pz, 0xb01d);
            let r = 1.3 + rng.f64() * 1.4;
            let ri = r.ceil() as i32;
            let cy = col.height;
            for dy in -ri..=ri {
                for dz in -ri..=ri {
                    for dx in -ri..=ri {
                        let dd = f64::from(dx * dx + dz * dz) + f64::from(dy * dy) * 1.4;
                        if dd <= r * r && !(dd > r * r * 0.7 && rng.chance(0.3)) {
                            let b = if rng.chance(0.4) { ids.moss } else { ids.stone };
                            painter.solid(px + dx, cy + dy, pz + dz, b);
                        }
                    }
                }
            }
        }

        // Troncs tombés.
        for (px, pz, h) in Self::cells(wx0, wz0, 10, 7, 61) {
            if unit(h) >= 0.10 {
                continue;
            }
            let col = self.terrain.column(px, pz);
            let wood = match col.biome {
                Biome::Forest | Biome::AutumnForest => ids.wood,
                Biome::BirchForest => ids.birch_wood,
                Biome::DarkForest => ids.dark_oak_wood,
                Biome::Taiga | Biome::MegaTaiga | Biome::SnowyTaiga => ids.spruce_wood,
                Biome::Jungle => ids.jungle_wood,
                _ => continue,
            };
            if col.height < sea {
                continue;
            }
            let mut rng = Rng::new(px, pz, 0xfa11);
            let len = rng.range(4, 7);
            let along_x = rng.chance(0.5);
            for i in 0..len {
                let (x, z) = if along_x { (px + i, pz) } else { (px, pz + i) };
                let g = self.terrain.column(x, z);
                if g.height < sea || (g.height - col.height).abs() > 1 {
                    continue;
                }
                painter.solid(x, g.height, z, wood);
                if rng.chance(0.25) {
                    painter.leaf(x, g.height + 1, z, ids.moss);
                }
            }
        }

        // Récifs de corail (océans chauds).
        for (px, pz, h) in Self::cells(wx0, wz0, 5, 4, 83) {
            if unit(h) >= 0.45 {
                continue;
            }
            let col = self.terrain.column(px, pz);
            if col.biome != Biome::WarmOcean || col.height > sea - 2 || col.height < sea - 14 {
                continue;
            }
            let mut rng = Rng::new(px, pz, 0xc0a1);
            let coral = [
                ids.coral_pink,
                ids.coral_purple,
                ids.coral_yellow,
                ids.coral_blue,
            ][rng.range(0, 3) as usize];
            let r = 1.2 + rng.f64() * 1.5;
            let ri = r.ceil() as i32;
            for dy in -1..=ri + 1 {
                for dz in -ri..=ri {
                    for dx in -ri..=ri {
                        let dd = f64::from(dx * dx + dz * dz) + f64::from(dy * dy) * 1.6;
                        let y = col.height + dy;
                        if dd <= r * r && y <= sea - 2 && !rng.chance(0.15) {
                            painter.wet(px + dx, y, pz + dz, coral);
                        }
                    }
                }
            }
            // Quelques colonnes plus hautes.
            for _ in 0..rng.range(1, 3) {
                let (cx, cz) = (px + rng.range(-ri, ri), pz + rng.range(-ri, ri));
                let g = self.terrain.column(cx, cz);
                for k in 0..rng.range(2, 4) {
                    let y = g.height + k;
                    if y <= sea - 2 {
                        painter.wet(cx, y, cz, coral);
                    }
                }
            }
        }

        // Icebergs.
        for (px, pz, h) in Self::cells(wx0, wz0, 28, 9, 97) {
            if unit(h) >= 0.30 {
                continue;
            }
            let col = self.terrain.column(px, pz);
            if col.biome != Biome::FrozenOcean {
                continue;
            }
            let mut rng = Rng::new(px, pz, 0x1ceb);
            let r = 3.0 + rng.f64() * 3.5;
            let up = rng.range(3, 6);
            let ri = r.ceil() as i32;
            for dy in -4..=up {
                let shrink = if dy > 0 {
                    1.0 - f64::from(dy) / f64::from(up + 1)
                } else {
                    1.0
                };
                let rr = r * shrink;
                for dz in -ri..=ri {
                    for dx in -ri..=ri {
                        if f64::from(dx * dx + dz * dz) <= rr * rr && !(dy > 0 && rng.chance(0.12))
                        {
                            let b = if dy < 0 || rng.chance(0.3) {
                                ids.ice
                            } else {
                                ids.packed_ice
                            };
                            painter.wet(px + dx, sea - 1 + dy, pz + dz, b);
                        }
                    }
                }
            }
        }
    }

    fn place_tree_nb(
        &self,
        p: &mut Painter,
        kind: TreeType,
        x: i32,
        y: i32,
        z: i32,
        rng: &mut Rng,
    ) {
        let ids = self.ids;
        match kind {
            TreeType::Oak => {
                let th = rng.range(4, 6);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.wood);
                }
                p.blob((x, y + th - 1, z), (2.7, 2.3, 2.7), ids.leaves, rng, 0.35);
                p.blob((x, y + th + 1, z), (1.7, 1.5, 1.7), ids.leaves, rng, 0.2);
            }
            TreeType::Fancy => {
                let th = rng.range(7, 11);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.wood);
                }
                let branches = rng.range(3, 5);
                for i in 0..branches {
                    let ang =
                        f64::from(i) * std::f64::consts::TAU / f64::from(branches) + rng.f64();
                    let len = rng.range(3, 4);
                    let by = y + (f64::from(th) * (0.5 + rng.f64() * 0.25)) as i32;
                    let ex = x + (ang.cos() * f64::from(len)).round() as i32;
                    let ez = z + (ang.sin() * f64::from(len)).round() as i32;
                    p.line((x, by, z), (ex, by + len - 1, ez), ids.wood);
                    p.blob((ex, by + len, ez), (2.6, 2.0, 2.6), ids.leaves, rng, 0.3);
                }
                p.blob((x, y + th, z), (3.4, 2.6, 3.4), ids.leaves, rng, 0.3);
            }
            TreeType::Birch => {
                let th = rng.range(5, 8);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.birch_wood);
                }
                p.blob(
                    (x, y + th - 1, z),
                    (2.3, 2.8, 2.3),
                    ids.birch_leaves,
                    rng,
                    0.35,
                );
                p.blob(
                    (x, y + th + 1, z),
                    (1.5, 1.5, 1.5),
                    ids.birch_leaves,
                    rng,
                    0.1,
                );
            }
            TreeType::Spruce => {
                let th = rng.range(6, 11);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.spruce_wood);
                }
                for d in 2..th + 2 {
                    let lft = th + 1 - d;
                    let r = 3.min((f64::from(lft) * 0.6).floor() as i32 + 1);
                    if r < 1 {
                        continue;
                    }
                    for dz in -r..=r {
                        for dx in -r..=r {
                            if (dx == 0 && dz == 0) || (dx.abs() == r && dz.abs() == r) {
                                continue;
                            }
                            p.leaf(x + dx, y + d, z + dz, ids.spruce_leaves);
                        }
                    }
                }
                p.leaf(x, y + th, z, ids.spruce_leaves);
                p.leaf(x, y + th + 1, z, ids.spruce_leaves);
            }
            TreeType::MegaSpruce => {
                let th = rng.range(15, 25);
                for k in 0..th {
                    for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        p.solid(x + dx, y + k, z + dz, ids.spruce_wood);
                    }
                }
                for d in 4..th + 3 {
                    let lft = th + 2 - d;
                    let r = 5.min((f64::from(lft) * 0.45).floor() as i32 + 1);
                    // Couches alternées : le feuillage forme des étages.
                    let r = if d % 3 == 0 && r > 1 { r - 1 } else { r };
                    for dz in -r..=r + 1 {
                        for dx in -r..=r + 1 {
                            let ax = if dx > 0 { dx - 1 } else { dx };
                            let az = if dz > 0 { dz - 1 } else { dz };
                            if ax.abs() + az.abs() > r + r / 2 {
                                continue;
                            }
                            p.leaf(x + dx, y + d, z + dz, ids.spruce_leaves);
                        }
                    }
                }
                p.leaf(x, y + th + 2, z, ids.spruce_leaves);
                p.leaf(x, y + th + 3, z, ids.spruce_leaves);
            }
            TreeType::Jungle => {
                let th = rng.range(9, 14);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.jungle_wood);
                }
                p.blob((x, y + th, z), (3.6, 2.4, 3.6), ids.jungle_leaves, rng, 0.3);
                p.blob(
                    (x, y + th - 3, z),
                    (2.6, 1.5, 2.6),
                    ids.jungle_leaves,
                    rng,
                    0.4,
                );
            }
            TreeType::MegaJungle => {
                let th = rng.range(18, 26);
                for k in 0..th {
                    for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        p.solid(x + dx, y + k, z + dz, ids.jungle_wood);
                    }
                }
                p.blob((x, y + th, z), (6.0, 3.0, 6.0), ids.jungle_leaves, rng, 0.3);
                for i in 0..4 {
                    let ang = f64::from(i) * 1.57 + rng.f64();
                    let len = rng.range(4, 6);
                    let by = y + th - rng.range(4, 8);
                    let ex = x + (ang.cos() * f64::from(len)).round() as i32;
                    let ez = z + (ang.sin() * f64::from(len)).round() as i32;
                    p.line((x, by, z), (ex, by + 2, ez), ids.jungle_wood);
                    p.blob(
                        (ex, by + 3, ez),
                        (3.6, 2.0, 3.6),
                        ids.jungle_leaves,
                        rng,
                        0.3,
                    );
                }
            }
            TreeType::Acacia => {
                let th = rng.range(4, 6);
                let lean = rng.range(-1, 1);
                let bend = (f64::from(th) * 0.6).floor() as i32;
                let mut tx = x;
                for k in 0..th {
                    p.solid(tx, y + k, z, ids.acacia_wood);
                    if k == bend {
                        tx += lean;
                    }
                }
                for (r, dy) in [(3i32, 0), (2, 1)] {
                    for dz in -r..=r {
                        for dx in -r..=r {
                            if dx.abs() == r && dz.abs() == r {
                                continue;
                            }
                            p.leaf(tx + dx, y + th + dy, z + dz, ids.acacia_leaves);
                        }
                    }
                }
                // Seconde branche.
                if rng.chance(0.5) {
                    let (bx, by) = (tx - lean.signum() * 2, y + th - 2);
                    p.line((tx, by, z), (bx, by + 1, z), ids.acacia_wood);
                    for dz in -2..=2 {
                        for dx in -2..=2 {
                            if dx * dx + dz * dz <= 5 {
                                p.leaf(bx + dx, by + 2, z + dz, ids.acacia_leaves);
                            }
                        }
                    }
                }
            }
            TreeType::SwampOak => {
                let th = rng.range(4, 5);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.wood);
                }
                p.blob((x, y + th - 1, z), (3.5, 1.9, 3.5), ids.leaves, rng, 0.35);
            }
            TreeType::DarkOak => {
                let th = rng.range(6, 9);
                for k in 0..th {
                    for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        p.solid(x + dx, y + k, z + dz, ids.dark_oak_wood);
                    }
                }
                p.blob((x, y + th, z), (4.8, 2.8, 4.8), ids.leaves, rng, 0.25);
                p.blob(
                    (x + 1, y + th + 1, z + 1),
                    (3.0, 2.0, 3.0),
                    ids.leaves,
                    rng,
                    0.1,
                );
            }
            TreeType::Cherry => {
                let th = rng.range(4, 6);
                let lean = (rng.range(-1, 1), rng.range(-1, 1));
                let (mut tx, mut tz) = (x, z);
                for k in 0..th {
                    p.solid(tx, y + k, tz, ids.cherry_wood);
                    if k == 2 {
                        tx += lean.0;
                        tz += lean.1;
                    }
                }
                let top = y + th;
                for i in 0..rng.range(2, 3) {
                    let ang = f64::from(i) * 2.4 + rng.f64() * 2.0;
                    let ex = tx + (ang.cos() * 3.0).round() as i32;
                    let ez = tz + (ang.sin() * 3.0).round() as i32;
                    p.line((tx, top - 2, tz), (ex, top, ez), ids.cherry_wood);
                    p.blob((ex, top + 1, ez), (2.5, 1.8, 2.5), ids.leaves, rng, 0.3);
                }
                p.blob((tx, top + 1, tz), (3.3, 2.2, 3.3), ids.leaves, rng, 0.3);
            }
            TreeType::Palm => {
                let th = rng.range(7, 10);
                let (dx, dz) = [(1, 0), (-1, 0), (0, 1), (0, -1)][rng.range(0, 3) as usize];
                let (mut tx, mut tz) = (x, z);
                for k in 0..th {
                    p.solid(tx, y + k, tz, ids.jungle_wood);
                    if k >= 2 && k % 3 == 2 {
                        tx += dx;
                        tz += dz;
                    }
                }
                let top = y + th;
                p.leaf(tx, top, tz, ids.jungle_leaves);
                p.leaf(tx, top + 1, tz, ids.jungle_leaves);
                for (fx, fz) in [
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                    (1, 1),
                    (-1, 1),
                    (1, -1),
                    (-1, -1),
                ] {
                    for i in 1..=4 {
                        let drop = if i <= 2 { 0 } else { i - 2 };
                        p.leaf(tx + fx * i, top - drop, tz + fz * i, ids.jungle_leaves);
                    }
                }
            }
            TreeType::Dead => {
                let th = rng.range(4, 7);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.wood);
                }
                for _ in 0..rng.range(1, 3) {
                    let by = y + rng.range(th / 2, th - 1);
                    let (dx, dz) = [(1, 0), (-1, 0), (0, 1), (0, -1)][rng.range(0, 3) as usize];
                    p.solid(x + dx, by, z + dz, ids.wood);
                    if rng.chance(0.6) {
                        p.solid(x + dx * 2, by + 1, z + dz * 2, ids.wood);
                    }
                }
            }
            TreeType::Bush => {
                p.solid(x, y, z, ids.wood);
                p.blob((x, y + 1, z), (1.9, 1.5, 1.9), ids.leaves, rng, 0.3);
            }
            TreeType::HugeRedMushroom | TreeType::HugeBrownMushroom => {
                let red = kind == TreeType::HugeRedMushroom;
                let th = rng.range(5, 8);
                for k in 0..th {
                    p.solid(x, y + k, z, ids.mushroom_stem);
                }
                let cap = if red {
                    ids.mushroom_cap_red
                } else {
                    ids.mushroom_cap_brown
                };
                let top = y + th;
                if red {
                    for (dy, r, hollow) in [
                        (0, 2i32, false),
                        (-1, 3, false),
                        (-2, 4, true),
                        (-3, 4, true),
                    ] {
                        for dz in -r..=r {
                            for dx in -r..=r {
                                let d2 = dx * dx + dz * dz;
                                let inside = d2 <= r * r + 1;
                                let rim = d2 > (r - 1) * (r - 1);
                                if inside && (!hollow || rim) {
                                    p.solid(x + dx, top + dy, z + dz, cap);
                                }
                            }
                        }
                    }
                } else {
                    for (dy, r, hollow) in [(0, 5i32, false), (-1, 5, true)] {
                        for dz in -r..=r {
                            for dx in -r..=r {
                                let d2 = dx * dx + dz * dz;
                                if d2 <= r * r + 2 && (!hollow || d2 > (r - 1) * (r - 1)) {
                                    p.solid(x + dx, top + dy, z + dz, cap);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn nb_vegetation(&self, chunk: &mut Chunk, wx0: i32, wz0: i32, grid: &[NbColumn]) {
        let ids = &self.ids;
        let c = &*self.cfg;
        let sea = c.sea_level;
        for lx in 0..SX {
            for lz in 0..SZ {
                let wx = wx0.wrapping_add(lx);
                let wz = wz0.wrapping_add(lz);
                let col = grid[gi(lx, lz)];
                let h = col.height;
                let biome = col.biome;
                if h >= SY - 4 {
                    continue;
                }
                // Nénuphars des marais (et des lacs de marais).
                if h < sea {
                    if biome == Biome::Swamp
                        && chunk.block(lx, sea, lz) == ids.air
                        && chunk.block(lx, sea - 1, lz) == ids.water
                    {
                        let hh = hash_position(
                            wx.wrapping_mul(11).wrapping_add(99),
                            wz.wrapping_mul(7).wrapping_add(55),
                        );
                        if unit(hh) < 0.06 * c.vegetation_density {
                            chunk.set_block(lx, sea, lz, ids.lily_pad);
                        }
                    }
                    continue;
                }
                if chunk.block(lx, h, lz) != ids.air {
                    continue;
                }
                let below = chunk.block(lx, h - 1, lz);
                let grassy = below == ids.grass || below == ids.podzol || below == ids.moss;
                let sandy = below == ids.sand || below == ids.red_sand;
                let hash = hash_position(
                    wx.wrapping_mul(3).wrapping_add(77),
                    wz.wrapping_mul(5).wrapping_add(33),
                );
                let r = unit(hash);

                // Roseaux au bord de l'eau.
                if (grassy || sandy) && h <= sea + 1 {
                    let wet_neighbour = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                        .iter()
                        .any(|&(dx, dz)| grid[gi(lx + dx, lz + dz)].height < sea);
                    if wet_neighbour
                        && biome != Biome::Swamp
                        && col.temp > 0.3
                        && r < 0.14 * c.vegetation_density
                    {
                        let n = 1 + hash % 3;
                        for k in 0..n {
                            if h + k < SY {
                                chunk.set_block(lx, h + k, lz, ids.reeds);
                            }
                        }
                        continue;
                    }
                }

                let vn = self.noise.fbm2d(
                    f64::from(wx) * 0.08 + 200.0,
                    f64::from(wz) * 0.08 + 200.0,
                    2,
                    2.0,
                    0.5,
                );
                let bc = self.biomes.config(biome);
                if sandy || below == ids.terracotta || below == ids.red_sand {
                    for e in &bc.vegetation {
                        let eff = e.chance * c.vegetation_density;
                        if e.block == ids.cactus && sandy && r < eff {
                            for k in 0..1 + hash % 3 {
                                chunk.set_block(lx, h + k, lz, ids.cactus);
                            }
                            break;
                        }
                        if e.block == ids.dead_bush
                            && (sandy || below == ids.terracotta)
                            && unit(hash / 7) < eff
                        {
                            chunk.set_block(lx, h, lz, ids.dead_bush);
                            break;
                        }
                    }
                    continue;
                }
                let on_myc = below == ids.mycelium;
                if !grassy && !on_myc {
                    continue;
                }
                if vn <= 0.3 {
                    continue;
                }
                // Parterres monochromes : la fleur dépend d'un bruit lent, pas du hasard.
                let patch_idx = (self.noise.fbm2d(
                    f64::from(wx) * 0.045 + 800.0,
                    f64::from(wz) * 0.045 + 90.0,
                    2,
                    2.0,
                    0.5,
                ) * 12.0) as usize;
                let mut acc = 0.0;
                for e in &bc.vegetation {
                    let mut block = e.block;
                    if block == ids.lily_pad
                        || block == ids.cactus
                        || block == ids.dead_bush
                        || block == ids.reeds
                    {
                        continue;
                    }
                    let is_flower = block == ids.flower_red
                        || block == ids.flower_yellow
                        || block == ids.flower_blue
                        || block == ids.flower_pink
                        || block == ids.flower_white
                        || block == ids.flower_orange;
                    if is_flower && biome == Biome::FlowerMeadow {
                        let pool = [
                            ids.flower_red,
                            ids.flower_yellow,
                            ids.flower_blue,
                            ids.flower_pink,
                            ids.flower_white,
                            ids.flower_orange,
                        ];
                        block = pool[patch_idx % pool.len()];
                        if e.block != ids.flower_red {
                            continue; // une seule entrée « fleur » compte : la teinte du parterre
                        }
                        acc += 0.14 * c.vegetation_density;
                    } else if block == ids.lavender {
                        // Landes : lavande par plaques.
                        acc += e.chance
                            * c.vegetation_density
                            * if patch_idx.is_multiple_of(3) {
                                3.0
                            } else {
                                0.3
                            };
                    } else {
                        acc += e.chance * c.vegetation_density;
                    }
                    if on_myc && block != ids.mushroom_red && block != ids.mushroom_brown {
                        continue;
                    }
                    if r < acc {
                        chunk.set_block(lx, h, lz, block);
                        break;
                    }
                }
            }
        }
    }
}
