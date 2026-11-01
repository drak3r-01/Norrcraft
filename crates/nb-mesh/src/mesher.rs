//! Génération du maillage d'un chunk (`ChunkMesher.generateChunkMeshData`, spec §6–11).
//!
//! L'ordre d'émission, le winding et chaque opération flottante reproduisent le Java :
//! tranches Y (dessus puis dessous pour chaque `y`), puis faces +Z, −Z, −X, +X, puis
//! blocs spéciaux (croix, nénuphars) en ordre `lx, y, lz`.

use nb_core::math::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};
use nb_core::ChunkPos;

use crate::biome::{BiomeId, BiomeSource};
use crate::block::{should_render_face, BlockMeshInfo, BlockRole, BlockTable};
use crate::light::{light_scale, LightVolume};
use crate::neighborhood::{BlockCache, Neighborhood};
use crate::vertex::{ChunkVertex, LAYER_CROSS_OFFSET, LAYER_LEAVES_OFFSET};

const SX: i32 = CHUNK_SIZE_X as i32;
const SY: i32 = CHUNK_SIZE_Y as i32;
const SZ: i32 = CHUNK_SIZE_Z as i32;

/// Face d'un cube (indice Java `f`).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    PosZ = 0,
    NegZ = 1,
    NegX = 2,
    PosX = 3,
    PosY = 4,
    NegY = 5,
}

impl Face {
    pub const ALL: [Face; 6] = [
        Face::PosZ,
        Face::NegZ,
        Face::NegX,
        Face::PosX,
        Face::PosY,
        Face::NegY,
    ];

    /// Normale (`OFFSETS[f]`).
    #[inline]
    pub const fn offset(self) -> [i32; 3] {
        OFFSETS[self as usize]
    }

    /// Facteur d'éclairage directionnel.
    #[inline]
    pub const fn dir_light(self) -> f32 {
        DIR_LIGHT[self as usize]
    }
}

/// `OFFSETS` du Java.
pub const OFFSETS: [[i32; 3]; 6] = [
    [0, 0, 1],
    [0, 0, -1],
    [-1, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
];

const DIR_LIGHT: [f32; 6] = [0.8, 0.8, 0.6, 0.6, 1.0, 0.5];

/// Voisins AO / lumière lissée par face et par coin : `[s1, s2, c]`, relatifs à `cellule + normale`.
const CORNER_NEIGHBORS: [[[[i32; 3]; 3]; 4]; 6] = [
    // 0 (+Z)
    [
        [[1, 0, 0], [0, -1, 0], [1, -1, 0]],
        [[1, 0, 0], [0, 1, 0], [1, 1, 0]],
        [[-1, 0, 0], [0, 1, 0], [-1, 1, 0]],
        [[-1, 0, 0], [0, -1, 0], [-1, -1, 0]],
    ],
    // 1 (−Z)
    [
        [[1, 0, 0], [0, -1, 0], [1, -1, 0]],
        [[-1, 0, 0], [0, -1, 0], [-1, -1, 0]],
        [[-1, 0, 0], [0, 1, 0], [-1, 1, 0]],
        [[1, 0, 0], [0, 1, 0], [1, 1, 0]],
    ],
    // 2 (−X)
    [
        [[0, 0, -1], [0, -1, 0], [0, -1, -1]],
        [[0, 0, 1], [0, -1, 0], [0, -1, 1]],
        [[0, 0, 1], [0, 1, 0], [0, 1, 1]],
        [[0, 0, -1], [0, 1, 0], [0, 1, -1]],
    ],
    // 3 (+X)
    [
        [[0, 0, 1], [0, -1, 0], [0, -1, 1]],
        [[0, 0, -1], [0, -1, 0], [0, -1, -1]],
        [[0, 0, -1], [0, 1, 0], [0, 1, -1]],
        [[0, 0, 1], [0, 1, 0], [0, 1, 1]],
    ],
    // 4 (+Y)
    [
        [[-1, 0, 0], [0, 0, 1], [-1, 0, 1]],
        [[1, 0, 0], [0, 0, 1], [1, 0, 1]],
        [[1, 0, 0], [0, 0, -1], [1, 0, -1]],
        [[-1, 0, 0], [0, 0, -1], [-1, 0, -1]],
    ],
    // 5 (−Y)
    [
        [[-1, 0, 0], [0, 0, -1], [-1, 0, -1]],
        [[1, 0, 0], [0, 0, -1], [1, 0, -1]],
        [[1, 0, 0], [0, 0, 1], [1, 0, 1]],
        [[-1, 0, 0], [0, 0, 1], [-1, 0, 1]],
    ],
];

const QUAD_INDICES: [usize; 6] = [0, 1, 2, 2, 3, 0];
const FLUID_DROP: f32 = -0.15;

/// `calcAO(side1, side2, corner)`.
#[inline]
pub fn calc_ao(side1: bool, side2: bool, corner: bool) -> u8 {
    if side1 && side2 {
        return 0;
    }
    3 - (u8::from(side1) + u8::from(side2) + u8::from(corner))
}

/// `getAOScale(v)` : 3 → 1.0, 2 → 0.75, 1 → 0.5, sinon 0.3.
#[inline]
pub fn ao_scale(v: u8) -> f32 {
    match v {
        3 => 1.0,
        2 => 0.75,
        1 => 0.5,
        _ => 0.3,
    }
}

/// Résultat du maillage d'un chunk : triangles non indexés, 6 sommets par quad.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshOutput {
    pub vertices: Vec<ChunkVertex>,
}

impl MeshOutput {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Nombre de sommets (`len / 9`).
    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Vue plate (9 `f32` par sommet), identique au `float[]` du Java.
    #[inline]
    pub fn as_floats(&self) -> &[f32] {
        bytemuck::cast_slice(&self.vertices)
    }

    /// Octets à uploader tels quels dans un vertex buffer.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.vertices)
    }

    /// Sépare (opaque/cutout, fluide) selon le signe du layer en préservant l'ordre relatif.
    pub fn split_fluid(&self) -> (Vec<ChunkVertex>, Vec<ChunkVertex>) {
        self.vertices.iter().partition(|v| !v.is_fluid())
    }

    /// Empreinte FNV-1a 64 des bits des `f32` (comparaison avec les dumps Java).
    pub fn fingerprint(&self) -> u64 {
        fingerprint_floats(self.as_floats())
    }
}

/// FNV-1a 64 sur les octets little-endian de `f32::to_bits`.
pub fn fingerprint_floats(floats: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for f in floats {
        for b in f.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// `generateChunkMeshData` : maillage du chunk central du voisinage. Chunk central absent →
/// maillage vide.
pub fn mesh_chunk(
    pos: ChunkPos,
    n: &Neighborhood,
    blocks: &BlockTable,
    biomes: &dyn BiomeSource,
) -> MeshOutput {
    if n.chunk(4).is_none() {
        return MeshOutput::default();
    }
    let cache = BlockCache::new(*n, blocks);
    let light = LightVolume::compute_cached(&cache);
    let start_x = pos.origin_x();
    let start_z = pos.origin_z();
    let mut column_biomes = [0 as BiomeId; CHUNK_SIZE_X * CHUNK_SIZE_Z];
    for lx in 0..SX {
        for lz in 0..SZ {
            column_biomes[(lx * SZ + lz) as usize] = biomes.biome_at(start_x + lx, start_z + lz);
        }
    }
    let mut m = Mesher {
        cache: &cache,
        table: blocks,
        light: &light,
        biomes,
        column_biomes,
        start_x,
        start_z,
        out: Vec::with_capacity(8192 / 9),
    };
    for y in 0..SY {
        m.slice_y(y, Face::PosY);
        m.slice_y(y, Face::NegY);
    }
    for face in [Face::PosZ, Face::NegZ, Face::NegX, Face::PosX] {
        m.slices_xz(face);
    }
    m.specials();
    MeshOutput { vertices: m.out }
}

#[derive(Clone, Copy, Default)]
struct Cell {
    mask: bool,
    ty: u8,
    biome: BiomeId,
    ao: u32,
    light: u32,
    surface: bool,
}

impl Cell {
    #[inline]
    fn same(&self, o: &Cell) -> bool {
        self.ty == o.ty && self.biome == o.biome && self.ao == o.ao && self.light == o.light
    }
}

struct Mesher<'a> {
    cache: &'a BlockCache<'a>,
    table: &'a BlockTable,
    light: &'a LightVolume,
    biomes: &'a dyn BiomeSource,
    column_biomes: [BiomeId; CHUNK_SIZE_X * CHUNK_SIZE_Z],
    start_x: i32,
    start_z: i32,
    out: Vec<ChunkVertex>,
}

impl Mesher<'_> {
    #[inline]
    fn info(&self, id: u8) -> &BlockMeshInfo {
        self.table.get(id)
    }

    #[inline]
    fn biome(&self, lx: i32, lz: i32) -> BiomeId {
        self.column_biomes[(lx * SZ + lz) as usize]
    }

    #[inline]
    fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        self.info(self.cache.get(x, y, z)).is_ao_solid()
    }

    /// `getFaceAO` : 4 valeurs empaquetées (coin 0 dans les bits 24..31).
    fn face_ao(&self, x: i32, y: i32, z: i32, face: Face) -> u32 {
        let o = face.offset();
        let (nx, ny, nz) = (x + o[0], y + o[1], z + o[2]);
        let mut packed = 0u32;
        for corner in &CORNER_NEIGHBORS[face as usize] {
            let s = |k: usize| {
                let d = corner[k];
                self.solid(nx + d[0], ny + d[1], nz + d[2])
            };
            let a = calc_ao(s(0), s(1), s(2));
            packed = (packed << 8) | u32::from(a);
        }
        packed
    }

    /// `getFaceLightSmooth` : 4 niveaux empaquetés.
    fn face_light(&self, x: i32, y: i32, z: i32, face: Face) -> u32 {
        let o = face.offset();
        let (nx, ny, nz) = (x + o[0], y + o[1], z + o[2]);
        let l = |d: [i32; 3]| self.light.get_clamped(nx + d[0], ny + d[1], nz + d[2]);
        let mut packed = 0u32;
        for corner in &CORNER_NEIGHBORS[face as usize] {
            let lc = l([0, 0, 0]);
            let ls1 = l(corner[0]);
            let ls2 = l(corner[1]);
            let lk = l(corner[2]);
            let v = if ls1 == 0 && ls2 == 0 {
                lc
            } else {
                (lc + ls1 + ls2 + lk) / 4
            };
            packed = (packed << 8) | (v as u32 & 0xFF);
        }
        packed
    }

    /// `generateGreedySliceY(y, face)` pour les faces 4 et 5.
    fn slice_y(&mut self, y: i32, face: Face) {
        let ny = y + face.offset()[1];
        let mut cells = [Cell::default(); CHUNK_SIZE_X * CHUNK_SIZE_Z];
        let at = |lx: i32, lz: i32| (lx * SZ + lz) as usize;

        for lx in 0..SX {
            for lz in 0..SZ {
                let cur_id = self.cache.get(lx, y, lz);
                let cur = self.info(cur_id);
                if cur.skipped_by_greedy() {
                    continue;
                }
                let nb_id = self.cache.get(lx, ny, lz);
                if should_render_face(cur, self.info(nb_id), cur_id == nb_id) {
                    cells[at(lx, lz)] = Cell {
                        mask: true,
                        ty: cur_id,
                        biome: self.biome(lx, lz),
                        ao: self.face_ao(lx, y, lz, face),
                        light: self.face_light(lx, y, lz, face),
                        surface: false,
                    };
                }
            }
        }

        for lx in 0..SX {
            for lz in 0..SZ {
                let c = cells[at(lx, lz)];
                if !c.mask {
                    continue;
                }
                let is_fluid = self.info(c.ty).is_fluid();
                let surface_top = face == Face::PosY && is_fluid;

                let mut width = 1;
                if !surface_top {
                    while lz + width < SZ {
                        let o = &cells[at(lx, lz + width)];
                        if !(o.mask && o.same(&c)) {
                            break;
                        }
                        width += 1;
                    }
                }
                let mut height = 1;
                let mut done = surface_top;
                while lx + height < SX && !done {
                    for w in 0..width {
                        let o = &cells[at(lx + height, lz + w)];
                        if !(o.mask && o.same(&c)) {
                            done = true;
                            break;
                        }
                    }
                    if !done {
                        height += 1;
                    }
                }
                for h in 0..height {
                    for w in 0..width {
                        cells[at(lx + h, lz + w)].mask = false;
                    }
                }
                self.emit_quad(
                    [self.start_x + lx, y, self.start_z + lz],
                    [height, 1, width],
                    face,
                    &c,
                    surface_top,
                );
            }
        }
    }

    /// `generateGreedySliceXZ(face)` pour les faces 0..3 (16 tranches).
    fn slices_xz(&mut self, face: Face) {
        let is_x = matches!(face, Face::NegX | Face::PosX);
        let max_u: i32 = if is_x { SZ } else { SX };
        let o = face.offset();
        let (dir_x, dir_z) = (o[0], o[2]);
        let mu = max_u as usize;
        let mut cells = vec![Cell::default(); CHUNK_SIZE_Y * mu];

        // Borne écrite `SIZE_X` pour les deux orientations (comme le Java).
        for slice in 0..SX {
            cells.fill(Cell::default());
            let at = |y: i32, u: i32| y as usize * mu + u as usize;
            for y in 0..SY {
                for u in 0..max_u {
                    let (lx, lz) = if is_x { (slice, u) } else { (u, slice) };
                    let cur_id = self.cache.get(lx, y, lz);
                    let cur = self.info(cur_id);
                    if cur.skipped_by_greedy() {
                        continue;
                    }
                    let nb_id = self.cache.get(lx + dir_x, y, lz + dir_z);
                    if should_render_face(cur, self.info(nb_id), cur_id == nb_id) {
                        let surface = cur.is_fluid() && self.cache.get(lx, y + 1, lz) != cur_id;
                        cells[at(y, u)] = Cell {
                            mask: true,
                            ty: cur_id,
                            biome: self.biome(lx, lz),
                            ao: self.face_ao(lx, y, lz, face),
                            light: self.face_light(lx, y, lz, face),
                            surface,
                        };
                    }
                }
            }

            for y in 0..SY {
                for u in 0..max_u {
                    let c = cells[at(y, u)];
                    if !c.mask {
                        continue;
                    }
                    let mut width = 1;
                    if !c.surface {
                        while u + width < max_u {
                            let o = &cells[at(y, u + width)];
                            if !(o.mask && o.same(&c) && o.surface == c.surface) {
                                break;
                            }
                            width += 1;
                        }
                    }
                    let mut height = 1;
                    let mut done = false;
                    while y + height < SY && !done {
                        let row_surface = cells[at(y + height, u)].surface;
                        if row_surface != c.surface {
                            break;
                        }
                        for w in 0..width {
                            let o = &cells[at(y + height, u + w)];
                            if !(o.mask && o.same(&c) && o.surface == row_surface) {
                                done = true;
                                break;
                            }
                        }
                        if !done {
                            height += 1;
                        }
                    }
                    for h in 0..height {
                        for w in 0..width {
                            cells[at(y + h, u + w)].mask = false;
                        }
                    }
                    let (lx, lz) = if is_x { (slice, u) } else { (u, slice) };
                    let (size_x, size_z) = if is_x { (1, width) } else { (width, 1) };
                    self.emit_quad(
                        [self.start_x + lx, y, self.start_z + lz],
                        [size_x, height, size_z],
                        face,
                        &c,
                        c.surface,
                    );
                }
            }
        }
    }

    /// `generateSpecials` : croix et nénuphars, ordre `lx, y, lz`.
    fn specials(&mut self) {
        let table = self.table;
        for lx in 0..SX {
            for y in 0..SY {
                for lz in 0..SZ {
                    let id = self.cache.get(lx, y, lz);
                    let info = table.get(id);
                    if info.role == BlockRole::Air {
                        continue;
                    }
                    let light_val = self.light.at(lx, y, lz);
                    let biome = self.biome(lx, lz);
                    let (wx, wz) = (self.start_x + lx, self.start_z + lz);
                    if info.cross_shaped {
                        self.emit_cross(wx, y, wz, info, biome, light_val);
                    } else if info.role == BlockRole::LilyPad {
                        let layer = info.texture_layers[0];
                        self.emit_pad(wx, y, wz, layer, biome, light_val);
                    }
                }
            }
        }
    }

    /// `emitQuad` (§7).
    fn emit_quad(&mut self, p: [i32; 3], size: [i32; 3], face: Face, c: &Cell, surface_top: bool) {
        let info = self.table.get(c.ty);
        let tints = self.biomes.tints(c.biome);
        let f = face as usize;
        let is_fluid = info.is_fluid();

        let mut rgb = [1.0f32; 3];
        if info.role == BlockRole::Grass && face == Face::PosY {
            rgb = tints.grass.map(|t| (t * 2.2) as f32);
        } else if info.leaves_tinted {
            rgb = tints.leaves.map(|t| (t * 2.2) as f32);
        } else if info.role == BlockRole::Water {
            rgb = tints.water.map(|t| t as f32);
        }

        let layer = info.texture_layers[f] as f32;
        let encoded = if is_fluid {
            -layer - 1.0
        } else if info.leaves_tinted {
            layer + LAYER_LEAVES_OFFSET
        } else {
            layer
        };

        let [x, y, z] = p;
        let [w, h, d] = size;
        let u_max = match face {
            Face::PosY | Face::NegY => w,
            Face::NegX | Face::PosX => d,
            _ => w,
        } as f32;
        let v_max = match face {
            Face::PosY | Face::NegY => d,
            _ => h,
        } as f32;

        let mut y_off = [0.0f32; 4];
        if is_fluid && surface_top {
            let lowered: &[usize] = match face {
                Face::PosZ => &[1, 2],
                Face::NegZ | Face::NegX | Face::PosX => &[2, 3],
                Face::PosY => &[0, 1, 2, 3],
                Face::NegY => &[],
            };
            for &i in lowered {
                y_off[i] = FLUID_DROP;
            }
        }

        let positions: [[i32; 3]; 4] = match face {
            Face::PosZ => [
                [x + w, y, z + d],
                [x + w, y + h, z + d],
                [x, y + h, z + d],
                [x, y, z + d],
            ],
            Face::NegZ => [[x + w, y, z], [x, y, z], [x, y + h, z], [x + w, y + h, z]],
            Face::NegX => [[x, y, z], [x, y, z + d], [x, y + h, z + d], [x, y + h, z]],
            Face::PosX => [
                [x + w, y, z + d],
                [x + w, y, z],
                [x + w, y + h, z],
                [x + w, y + h, z + d],
            ],
            Face::PosY => [
                [x, y + h, z + d],
                [x + w, y + h, z + d],
                [x + w, y + h, z],
                [x, y + h, z],
            ],
            Face::NegY => [[x, y, z], [x + w, y, z], [x + w, y, z + d], [x, y, z + d]],
        };
        let uvs: [[f32; 2]; 4] = if face == Face::PosZ {
            [[u_max, 0.0], [u_max, v_max], [0.0, v_max], [0.0, 0.0]]
        } else {
            [[0.0, 0.0], [u_max, 0.0], [u_max, v_max], [0.0, v_max]]
        };

        let dir = face.dir_light();
        let ao_f: [f32; 4] = std::array::from_fn(|i| ao_scale((c.ao >> (24 - 8 * i)) as u8));
        let light_f: [f32; 4] =
            std::array::from_fn(|i| light_scale(((c.light >> (24 - 8 * i)) & 0xFF) as i32));

        for i in QUAD_INDICES {
            let pp = positions[i];
            let (ao, l) = (ao_f[i], light_f[i]);
            self.out.push(ChunkVertex {
                pos: [pp[0] as f32, pp[1] as f32 + y_off[i], pp[2] as f32],
                uv: uvs[i],
                layer: encoded,
                color: [
                    rgb[0] * dir * ao * l,
                    rgb[1] * dir * ao * l,
                    rgb[2] * dir * ao * l,
                ],
            });
        }
    }

    /// `emitCross` (§10).
    fn emit_cross(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        info: &BlockMeshInfo,
        biome: BiomeId,
        light_val: i32,
    ) {
        let mut rgb = [1.0f32; 3];
        if info.grass_tinted {
            rgb = self.biomes.tints(biome).grass.map(|t| (t * 2.2) as f32);
        }
        let encoded = info.texture_layers[0] as f32 + LAYER_CROSS_OFFSET;
        let v_light = light_scale(light_val);
        let dir: f32 = 0.85;
        let color = rgb.map(|c| c * v_light * dir);
        let (x1, z1) = (x + 1, z + 1);
        let y1 = y + 1;
        let faces: [[[i32; 3]; 6]; 4] = [
            [
                [x, y, z],
                [x1, y, z1],
                [x1, y1, z1],
                [x1, y1, z1],
                [x, y1, z],
                [x, y, z],
            ],
            [
                [x1, y, z1],
                [x, y, z],
                [x, y1, z],
                [x, y1, z],
                [x1, y1, z1],
                [x1, y, z1],
            ],
            [
                [x1, y, z],
                [x, y, z1],
                [x, y1, z1],
                [x, y1, z1],
                [x1, y1, z],
                [x1, y, z],
            ],
            [
                [x, y, z1],
                [x1, y, z],
                [x1, y1, z],
                [x1, y1, z],
                [x, y1, z1],
                [x, y, z1],
            ],
        ];
        const UVS: [[f32; 2]; 6] = [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [0.0, 0.0],
        ];
        for f in &faces {
            for (p, uv) in f.iter().zip(UVS) {
                self.out.push(ChunkVertex {
                    pos: [p[0] as f32, p[1] as f32, p[2] as f32],
                    uv,
                    layer: encoded,
                    color,
                });
            }
        }
    }

    /// `emitPad` (§11) : teinte herbe, layer brut, pas de dirLight ni d'AO.
    fn emit_pad(&mut self, x: i32, y: i32, z: i32, layer: u32, biome: BiomeId, light_val: i32) {
        let rgb = self.biomes.tints(biome).grass.map(|t| (t * 2.2) as f32);
        let layer = layer as f32;
        let py = y as f32 + 0.015f32;
        let v_light = light_scale(light_val);
        let color = rgb.map(|c| c * v_light);
        let (fx, fx1, fz, fz1) = (x as f32, (x + 1) as f32, z as f32, (z + 1) as f32);
        let quads: [[[f32; 3]; 4]; 2] = [
            [[fx, py, fz1], [fx1, py, fz1], [fx1, py, fz], [fx, py, fz]],
            [[fx, py, fz], [fx1, py, fz], [fx1, py, fz1], [fx, py, fz1]],
        ];
        const UVS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for q in &quads {
            for i in QUAD_INDICES {
                self.out.push(ChunkVertex {
                    pos: q[i],
                    uv: UVS[i],
                    layer,
                    color,
                });
            }
        }
    }
}
