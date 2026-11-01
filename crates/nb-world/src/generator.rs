//! Générateur de terrain (port exact de `WorldGenerator.java`, spec §7).
//!
//! Le résultat ne dépend que de `(graine, config, position)`. Seul écart volontaire : la passe
//! de lave n'est appliquée que si les grottes sont actives (voir la documentation du crate).

use std::sync::Arc;

use nb_data::{BlockId, BlockIds};

use crate::biome::{Biome, BiomeSystem, TreeType};
use crate::chunk::{Chunk, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};
use crate::config::WorldConfig;
use crate::noise::{hash_position, Noise};

const SY: i32 = CHUNK_SIZE_Y as i32;
const SX: i32 = CHUNK_SIZE_X as i32;
const SZ: i32 = CHUNK_SIZE_Z as i32;

/// `(hash % 10000) / 10000.0`.
#[inline]
fn unit(hash: i32) -> f64 {
    f64::from(hash % 10000) / 10000.0
}

/// `(int) Math.floor(v)`.
#[inline]
fn ifloor(v: f64) -> i32 {
    v.floor() as i32
}

/// Générateur immuable, partageable entre threads.
#[derive(Debug, Clone)]
pub struct WorldGenerator {
    noise: Arc<Noise>,
    biomes: BiomeSystem,
    cfg: Arc<WorldConfig>,
    ids: BlockIds,
    java_lava_pass: bool,
}

/// Données d'une colonne (passe 1).
struct Column {
    biome: Biome,
    height: i32,
    ocean: f64,
    river: f64,
}

impl WorldGenerator {
    pub fn new(seed: i64, cfg: Arc<WorldConfig>, ids: &BlockIds) -> Self {
        let noise = Arc::new(Noise::new(seed));
        let biomes = BiomeSystem::new(noise.clone(), cfg.clone(), ids);
        Self {
            noise,
            biomes,
            cfg,
            ids: *ids,
            java_lava_pass: false,
        }
    }

    /// Réactive la passe de lave inconditionnelle du Java (monde FLAT noyé sous la lave).
    /// Réservé à la vérification contre les vecteurs de référence Java.
    pub fn with_java_lava_pass(mut self, on: bool) -> Self {
        self.java_lava_pass = on;
        self
    }

    pub fn biome_system(&self) -> &BiomeSystem {
        &self.biomes
    }

    pub fn noise(&self) -> &Arc<Noise> {
        &self.noise
    }

    pub fn config(&self) -> &WorldConfig {
        &self.cfg
    }

    fn river_factor(&self, wx: f64, wz: f64) -> f64 {
        let c = &*self.cfg;
        if c.river_width <= 0.0 {
            return 1.0;
        }
        let bank = c.river_width * c.river_bank_width_mult;
        let f = c.river_frequency;
        let r1 = self.noise.evaluate2(wx * f + 300.0, wz * f + 300.0).abs();
        let r2 = self
            .noise
            .evaluate2(wx * f * 1.5 + 700.0, wz * f * 1.5 - 200.0)
            .abs();
        let m = r1.min(r2);
        if m < c.river_width {
            return 0.0;
        }
        if m < bank {
            return (m - c.river_width) / (bank - c.river_width);
        }
        1.0
    }

    fn ocean_factor(&self, wx: f64, wz: f64) -> f64 {
        let c = &*self.cfg;
        let n = self.noise.fbm2d(
            wx * c.ocean_frequency,
            wz * c.ocean_frequency,
            c.ocean_fbm_octaves,
            c.ocean_lacunarity,
            c.ocean_gain,
        );
        if n < c.ocean_threshold {
            return 0.0;
        }
        if n < c.coast_threshold {
            return (n - c.ocean_threshold) / (c.coast_threshold - c.ocean_threshold);
        }
        1.0
    }

    fn column(&self, wx: i32, wz: i32) -> Column {
        let c = &*self.cfg;
        let (x, z) = (f64::from(wx), f64::from(wz));
        let biome = self.biomes.biome(x, z);
        let bc = self.biomes.config(biome);
        let hn = self.noise.fbm2d(
            x * c.height_noise_frequency,
            z * c.height_noise_frequency,
            c.height_noise_octaves,
            c.height_noise_lacunarity,
            c.height_noise_gain,
        );
        let flat = self
            .noise
            .fbm2d(x * 0.005 + 100.0, z * 0.005 + 100.0, 3, 2.0, 0.5);
        let mut height = ifloor(
            f64::from(c.base_height)
                + hn * f64::from(c.height_scale) * bc.height_modifier * (0.5 + flat * 0.8),
        );

        let of = self.ocean_factor(x, z);
        let rf = self.river_factor(x, z);

        if of < 1.0 {
            let ocean_floor = 12i32.wrapping_add(ifloor(of * 10.0));
            height =
                ifloor(f64::from(ocean_floor) + f64::from(height.wrapping_sub(ocean_floor)) * of);
            if of < 0.3 {
                height = height.min(18);
            }
        }

        if rf < 1.0 && height >= c.sea_level.wrapping_sub(2) {
            let carve = ifloor(f64::from(c.river_depth) * (1.0 - rf));
            let river_bed = c.sea_level.wrapping_sub(carve);
            height = height.min(ifloor(
                f64::from(river_bed) + f64::from(height.wrapping_sub(river_bed)) * rf,
            ));
        }

        if biome == Biome::Swamp
            && height > c.sea_level.wrapping_sub(2)
            && height < c.sea_level.wrapping_add(3)
        {
            height = height.min(c.sea_level);
        }

        Column {
            biome,
            height: height.clamp(1, SY - 1),
            ocean: of,
            river: rf,
        }
    }

    /// Hauteur de colonne : premier `y` libre (spec §7.3).
    pub fn compute_surface_height(&self, wx: i32, wz: i32) -> i32 {
        self.column(wx, wz).height
    }

    /// Génère un chunk supposé vide (spec §7.4). Les drapeaux sont à la charge de l'appelant.
    pub fn generate_chunk(&self, chunk: &mut Chunk) {
        let pos = chunk.pos();
        let wx0 = pos.x.wrapping_mul(SX);
        let wz0 = pos.z.wrapping_mul(SZ);
        self.pass_columns(chunk, wx0, wz0);
        if self.cfg.caves_enabled {
            self.pass_caves(chunk, wx0, wz0);
        }
        self.pass_ores(chunk, wx0, wz0);
        // Écart volontaire (décision de Gabriel) : sans grottes, la passe de lave noyait
        // tout le monde FLAT. Avec grottes, elle est identique au Java.
        if self.cfg.caves_enabled || self.java_lava_pass {
            self.pass_lava(chunk);
        }
        self.pass_trees(chunk, wx0, wz0);
        self.pass_vegetation(chunk, wx0, wz0);
    }

    fn pass_columns(&self, chunk: &mut Chunk, wx0: i32, wz0: i32) {
        let ids = &self.ids;
        let sea = self.cfg.sea_level;
        for lx in 0..SX {
            for lz in 0..SZ {
                let wx = wx0.wrapping_add(lx);
                let wz = wz0.wrapping_add(lz);
                let col = self.column(wx, wz);
                let height = col.height;
                let bc = self.biomes.config(col.biome);
                let near_river = col.river < 0.6;
                let ocean_coast = col.ocean < 0.8;
                let surface = self.biomes.surface_block(col.biome, height);
                for y in 0..height.min(SY) {
                    let b = if y == 0 {
                        ids.bedrock
                    } else if y < height - 4 {
                        bc.deep
                    } else if y < height - 1 {
                        if near_river || ocean_coast || height < sea.wrapping_add(2) {
                            ids.sand
                        } else {
                            bc.subsurface
                        }
                    } else if (near_river && height <= sea.wrapping_add(1))
                        || (ocean_coast && height <= sea.wrapping_add(2))
                    {
                        ids.sand
                    } else {
                        surface
                    };
                    chunk.set_block(lx, y, lz, b);
                }
                for y in height..sea {
                    if chunk.block(lx, y, lz) == ids.air {
                        chunk.set_block(lx, y, lz, ids.water);
                    }
                }
            }
        }
    }

    fn pass_caves(&self, chunk: &mut Chunk, wx0: i32, wz0: i32) {
        let ids = &self.ids;
        let c = &*self.cfg;
        let open = |b: BlockId| b == ids.air || b == ids.water || b == ids.bedrock;
        let s1 = 0.035;
        let s2 = 0.04;
        let cavern_max = 30.min(c.cave_depth_max / 2);
        for lx in 0..SX {
            for lz in 0..SZ {
                let wx = f64::from(wx0.wrapping_add(lx));
                let wz = f64::from(wz0.wrapping_add(lz));
                let mut y = 2;
                while y < c.cave_depth_max {
                    let yf = f64::from(y);
                    if open(chunk.block(lx, y, lz)) {
                        y += 2;
                        continue;
                    }
                    let n1 = self.noise.fbm3d(wx * s1, yf * s1, wz * s1, 1);
                    let n2 = self
                        .noise
                        .fbm3d(wx * s2 + 500.0, yf * s2 + 500.0, wz * s2 + 500.0, 1);
                    if n1.abs() < c.cave_threshold && n2.abs() < c.cave_threshold {
                        chunk.set_block(lx, y, lz, ids.air);
                        if y + 1 < c.cave_depth_max && !open(chunk.block(lx, y + 1, lz)) {
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
                        if cav > c.cavern_threshold {
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

        // Champignons (y = 3, 6, …, 48).
        for lx in 1..SX - 1 {
            for lz in 1..SZ - 1 {
                let wx = wx0.wrapping_add(lx);
                let wz = wz0.wrapping_add(lz);
                for y in (3..50).step_by(3) {
                    if chunk.block(lx, y, lz) != ids.air {
                        continue;
                    }
                    let below = chunk.block(lx, y - 1, lz);
                    if below != ids.stone && below != ids.dirt {
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

    fn pass_ores(&self, chunk: &mut Chunk, wx0: i32, wz0: i32) {
        let ids = &self.ids;
        let o = &self.cfg.ores;
        let table = [
            (ids.coal_ore, o.coal, 73),
            (ids.iron_ore, o.iron, 137),
            (ids.gold_ore, o.gold, 251),
            (ids.diamond_ore, o.diamond, 389),
            (ids.emerald_ore, o.emerald, 521),
        ];
        for lx in 0..SX {
            for lz in 0..SZ {
                let wx = wx0.wrapping_add(lx);
                let wz = wz0.wrapping_add(lz);
                for y in 1..SY {
                    let b = chunk.block(lx, y, lz);
                    if b != ids.stone && b != ids.sandstone {
                        continue;
                    }
                    for &(ore, range, seed) in &table {
                        if y < range.min_y || y > range.max_y {
                            continue;
                        }
                        let h = hash_position(
                            wx.wrapping_mul(seed).wrapping_add(y * 31),
                            wz.wrapping_mul(seed).wrapping_add(y * 17),
                        );
                        if unit(h) < range.chance {
                            chunk.set_block(lx, y, lz, ore);
                            break;
                        }
                    }
                }
            }
        }
    }

    fn pass_lava(&self, chunk: &mut Chunk) {
        let ids = &self.ids;
        for lx in 0..SX {
            for lz in 0..SZ {
                for y in 1..12 {
                    if chunk.block(lx, y, lz) == ids.air {
                        chunk.set_block(lx, y, lz, ids.lava);
                    }
                }
            }
        }
    }

    /// Air ou plante basse : la recherche de surface continue à travers.
    fn passable(&self, b: BlockId) -> bool {
        let i = &self.ids;
        b == i.air
            || b == i.tall_grass
            || b == i.flower_red
            || b == i.flower_yellow
            || b == i.fern
            || b == i.dead_bush
    }

    fn pass_trees(&self, chunk: &mut Chunk, wx0: i32, wz0: i32) {
        let ids = &self.ids;
        let c = &*self.cfg;
        for lx in 2..SX - 2 {
            for lz in 2..SZ - 2 {
                let wx = wx0.wrapping_add(lx);
                let wz = wz0.wrapping_add(lz);
                let biome = self.biomes.biome_at(wx, wz);
                let bc = self.biomes.config(biome);
                let ov = c.tree_density_overrides[biome.ordinal()];
                let base = if ov >= 0.0 { ov } else { bc.tree_density };
                let d = base * c.vegetation_density;
                if d <= 0.0 {
                    continue;
                }
                if unit(hash_position(wx, wz)) > d {
                    continue;
                }
                let (x, z) = (f64::from(wx), f64::from(wz));
                if self.river_factor(x, z) < 0.8 || self.ocean_factor(x, z) < 0.7 {
                    continue;
                }
                let mut surface_y = -1;
                for y in (0..SY).rev() {
                    let b = chunk.block(lx, y, lz);
                    if b == ids.grass || b == ids.sand || b == ids.snow {
                        surface_y = y;
                        break;
                    }
                    if !self.passable(b) {
                        break;
                    }
                }
                if surface_y < c.sea_level || surface_y > SY - 15 {
                    continue;
                }
                if biome == Biome::Desert {
                    continue;
                }
                self.place_tree(chunk, bc.tree_type, lx, surface_y + 1, lz);
            }
        }
    }

    fn place_tree(&self, chunk: &mut Chunk, t: TreeType, x: i32, y: i32, z: i32) {
        match t {
            TreeType::Oak => self.place_oak(chunk, x, y, z),
            TreeType::Birch => self.place_birch(chunk, x, y, z),
            TreeType::Spruce => self.place_spruce(chunk, x, y, z),
            TreeType::Jungle => self.place_jungle(chunk, x, y, z),
            TreeType::Acacia => self.place_acacia(chunk, x, y, z),
            TreeType::SwampOak => self.place_swamp_oak(chunk, x, y, z),
        }
    }

    /// `(cwx, cwz)` : origine monde du chunk.
    fn origin(chunk: &Chunk) -> (i32, i32) {
        let p = chunk.pos();
        (p.x.wrapping_mul(SX), p.z.wrapping_mul(SZ))
    }

    /// Feuille posée seulement dans le chunk, sur de l'air.
    fn leaf(&self, chunk: &mut Chunk, lx: i32, y: i32, lz: i32, leaves: BlockId) {
        if (0..SX).contains(&lx)
            && (0..SZ).contains(&lz)
            && y < SY
            && chunk.block(lx, y, lz) == self.ids.air
        {
            chunk.set_block(lx, y, lz, leaves);
        }
    }

    fn trunk(chunk: &mut Chunk, x: i32, y: i32, z: i32, th: i32, wood: BlockId) {
        for dy in 0..th {
            chunk.set_block(x, y + dy, z, wood);
        }
    }

    fn place_oak(&self, chunk: &mut Chunk, x: i32, y: i32, z: i32) {
        let (cwx, cwz) = Self::origin(chunk);
        let th = 4 + hash_position(x.wrapping_add(cwx), z.wrapping_add(cwz)) % 3;
        Self::trunk(chunk, x, y, z, th, self.ids.wood);
        let leaf_start = y + th - 1;
        let leaf_end = y + th + 2;
        for ly in leaf_start..=leaf_end {
            let r: i32 = if ly == leaf_end { 1 } else { 2 };
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx == 0 && dz == 0 && ly < leaf_end {
                        continue;
                    }
                    if dx.abs() == r && dz.abs() == r && ly == leaf_start {
                        continue;
                    }
                    self.leaf(chunk, x + dx, ly, z + dz, self.ids.leaves);
                }
            }
        }
    }

    fn place_birch(&self, chunk: &mut Chunk, x: i32, y: i32, z: i32) {
        let (cwx, cwz) = Self::origin(chunk);
        let th = 5 + hash_position(
            x.wrapping_add(cwx).wrapping_add(11),
            z.wrapping_add(cwz).wrapping_add(13),
        ) % 3;
        Self::trunk(chunk, x, y, z, th, self.ids.birch_wood);
        let leaf_start = y + th - 2;
        let leaf_end = y + th + 1;
        for ly in leaf_start..=leaf_end {
            let r: i32 = if ly == leaf_end || ly == leaf_start {
                1
            } else {
                2
            };
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx == 0 && dz == 0 && ly < leaf_end {
                        continue;
                    }
                    if dx.abs() == r && dz.abs() == r {
                        continue;
                    }
                    self.leaf(chunk, x + dx, ly, z + dz, self.ids.birch_leaves);
                }
            }
        }
    }

    fn place_spruce(&self, chunk: &mut Chunk, x: i32, y: i32, z: i32) {
        let (cwx, cwz) = Self::origin(chunk);
        let th = 6 + hash_position(
            x.wrapping_add(cwx).wrapping_add(23),
            z.wrapping_add(cwz).wrapping_add(29),
        ) % 4;
        let leaves = self.ids.spruce_leaves;
        Self::trunk(chunk, x, y, z, th, self.ids.spruce_wood);
        for d in 2..th + 2 {
            let lft = (th + 1) - d;
            let r = 3.min(ifloor(f64::from(lft) * 0.6) + 1);
            if r < 1 {
                continue;
            }
            let wy = y + d;
            if wy >= SY {
                break;
            }
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    if dx.abs() == r && dz.abs() == r {
                        continue;
                    }
                    self.leaf(chunk, x + dx, wy, z + dz, leaves);
                }
            }
        }
        if y + th + 1 < SY {
            chunk.set_block(x, y + th, z, leaves);
            chunk.set_block(x, y + th + 1, z, leaves);
        }
    }

    fn place_jungle(&self, chunk: &mut Chunk, x: i32, y: i32, z: i32) {
        let (cwx, cwz) = Self::origin(chunk);
        let th = 8 + hash_position(
            x.wrapping_add(cwx).wrapping_add(37),
            z.wrapping_add(cwz).wrapping_add(41),
        ) % 5;
        Self::trunk(chunk, x, y, z, th, self.ids.jungle_wood);
        let leaf_start = y + th - 2;
        let leaf_end = y + th + 2;
        for ly in leaf_start..=leaf_end {
            let dft = leaf_end - ly;
            let r: i32 = if dft >= 2 {
                3
            } else if dft == 1 {
                2
            } else {
                1
            };
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx == 0 && dz == 0 && ly < leaf_end {
                        continue;
                    }
                    if dx.abs() + dz.abs() > r + 1 {
                        continue;
                    }
                    self.leaf(chunk, x + dx, ly, z + dz, self.ids.jungle_leaves);
                }
            }
        }
    }

    fn place_acacia(&self, chunk: &mut Chunk, x: i32, y: i32, z: i32) {
        let (cwx, cwz) = Self::origin(chunk);
        let th = 4 + hash_position(
            x.wrapping_add(cwx).wrapping_add(53),
            z.wrapping_add(cwz).wrapping_add(59),
        ) % 3;
        let lean =
            hash_position(x.wrapping_add(cwx), z.wrapping_add(cwz).wrapping_add(100)) % 3 - 1;
        let bend = (f64::from(th) * 0.6).floor();
        let mut tx = x;
        for d in 0..th {
            if (0..SX).contains(&tx) {
                chunk.set_block(tx, y + d, z, self.ids.acacia_wood);
            }
            if f64::from(d) == bend {
                tx += lean;
            }
        }
        let canopy_y = y + th;
        for d in 0..2 {
            let r: i32 = if d == 0 { 3 } else { 2 };
            let wy = canopy_y + d;
            if wy >= SY {
                break;
            }
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx.abs() == r && dz.abs() == r {
                        continue;
                    }
                    self.leaf(chunk, tx + dx, wy, z + dz, self.ids.acacia_leaves);
                }
            }
        }
    }

    fn place_swamp_oak(&self, chunk: &mut Chunk, x: i32, y: i32, z: i32) {
        let (cwx, cwz) = Self::origin(chunk);
        let th = 4 + hash_position(
            x.wrapping_add(cwx).wrapping_add(67),
            z.wrapping_add(cwz).wrapping_add(71),
        ) % 2;
        Self::trunk(chunk, x, y, z, th, self.ids.wood);
        let leaf_start = y + th - 1;
        let leaf_end = y + th + 2;
        for ly in leaf_start..=leaf_end {
            let r = if ly <= leaf_start + 1 { 3 } else { 1 };
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx == 0 && dz == 0 && ly < leaf_end {
                        continue;
                    }
                    self.leaf(chunk, x + dx, ly, z + dz, self.ids.leaves);
                }
            }
        }
    }

    fn pass_vegetation(&self, chunk: &mut Chunk, wx0: i32, wz0: i32) {
        let ids = &self.ids;
        let c = &*self.cfg;
        for lx in 0..SX {
            for lz in 0..SZ {
                let wx = wx0.wrapping_add(lx);
                let wz = wz0.wrapping_add(lz);
                let biome = self.biomes.biome_at(wx, wz);
                let bc = self.biomes.config(biome);
                let mut y = SY - 1;
                while y >= c.sea_level.wrapping_sub(1) {
                    let b = chunk.block(lx, y, lz);
                    if b == ids.grass || b == ids.sand || b == ids.snow || b == ids.stone {
                        let above = y + 1;
                        if above < SY && chunk.block(lx, above, lz) == ids.air {
                            let veg_noise = self.noise.fbm2d(
                                f64::from(wx) * 0.08 + 200.0,
                                f64::from(wz) * 0.08 + 200.0,
                                2,
                                2.0,
                                0.5,
                            );
                            let hash = hash_position(
                                wx.wrapping_mul(3).wrapping_add(77),
                                wz.wrapping_mul(5).wrapping_add(33),
                            );
                            let rval = unit(hash);
                            for entry in &bc.vegetation {
                                if entry.block == ids.lily_pad {
                                    continue;
                                }
                                let eff = entry.chance * c.vegetation_density;
                                if entry.block == ids.cactus {
                                    if b == ids.sand && rval < eff {
                                        for cy in 0..1 + hash % 3 {
                                            if above + cy < SY {
                                                chunk.set_block(lx, above + cy, lz, ids.cactus);
                                            }
                                        }
                                    }
                                    continue;
                                }
                                if veg_noise > 0.3 && rval < eff {
                                    chunk.set_block(lx, above, lz, entry.block);
                                    break;
                                }
                            }
                        }
                        break;
                    }
                    if b == ids.water && biome == Biome::Swamp {
                        if y == c.sea_level - 1 && chunk.block(lx, y + 1, lz) == ids.air {
                            let h = hash_position(
                                wx.wrapping_mul(11).wrapping_add(99),
                                wz.wrapping_mul(7).wrapping_add(55),
                            );
                            if unit(h) < 0.04 * c.vegetation_density {
                                chunk.set_block(lx, y + 1, lz, ids.lily_pad);
                            }
                        }
                        break;
                    }
                    if !self.passable(b) {
                        break;
                    }
                    y -= 1;
                }
            }
        }
    }
}
