//! Modèles de mobs (spec §14.2–§14.4, `MobModelLoader` / `MobRenderer`) : format de sommet,
//! boîtes texturées et colorées, formes vanilla codées en dur, cube manquant, recherche des
//! textures d'entité et ordre de résolution JEM → vanilla → géométrie → boîte texturée.
//! Pur CPU (les fichiers sont lus ici, l'upload GPU est dans `vk::mobs`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3};
use image::RgbaImage;
use nb_data::{MobConfig, MobRegistry};

use crate::atlas::{checkerboard, decode_rgba_flipped};
use crate::jem;
use crate::packs::DEFAULT_PACK;

/// Sommet de mob (10 floats = 40 o, §9) : loc0 `aPos`, loc1 `aUV`, loc2 `aTint`,
/// loc3 `aBrightness`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct MobVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub tint: [f32; 4],
    pub brightness: f32,
}

const _: () = assert!(std::mem::size_of::<MobVertex>() == 40);

/// Luminosités par face (`BR_*`).
pub const BR_TOP: f32 = 1.0;
pub const BR_FRONT: f32 = 0.85;
pub const BR_SIDE: f32 = 0.70;
pub const BR_BOTTOM: f32 = 0.50;

/// Boîte colorée `[bx,by,bz,bw,bh,bd,r,g,b,a]` (pixels MC) fournie par la config du mob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorBox {
    pub min_px: [f32; 3],
    pub size_px: [f32; 3],
    pub rgba: [f32; 4],
}

impl ColorBox {
    /// Depuis une ligne de `mob_geometry.json`.
    pub fn from_row(r: &[f32; 10]) -> Self {
        Self {
            min_px: [r[0], r[1], r[2]],
            size_px: [r[3], r[4], r[5]],
            rgba: [r[6], r[7], r[8], r[9]],
        }
    }
}

/// Description d'un mob nécessaire à la construction de son modèle (contrat §19).
#[derive(Clone, Debug, PartialEq)]
pub struct MobModelDesc {
    pub id: String,
    /// Mob intégré (renne, poisson, orque) : pas de boîte texturée de dernier recours.
    pub built_in: bool,
    /// Géométrie couleur (`cfg.geometry`).
    pub geometry: Option<Vec<ColorBox>>,
}

impl MobModelDesc {
    pub fn from_config(cfg: &MobConfig) -> Self {
        Self {
            id: cfg.id.clone(),
            built_in: MobRegistry::is_built_in(&cfg.id),
            geometry: cfg
                .geometry
                .as_ref()
                .map(|g| g.boxes.iter().map(ColorBox::from_row).collect()),
        }
    }

    /// Tous les mobs du registre, dans son ordre.
    pub fn from_registry(reg: &MobRegistry) -> Vec<Self> {
        reg.all().iter().map(Self::from_config).collect()
    }
}

/// Modèle CPU : sommets opaques puis transparents ; `texture = None` → mode couleur (texture
/// blanche 1×1). La texture est déjà retournée verticalement.
#[derive(Clone, Debug, PartialEq)]
pub struct MobModelCpu {
    pub vertices: Vec<MobVertex>,
    pub opaque_count: u32,
    pub transparent_count: u32,
    pub texture: Option<RgbaImage>,
}

impl MobModelCpu {
    /// Opaques puis transparents ; `None` si aucun sommet.
    pub fn from_lists(
        mut opaque: Vec<MobVertex>,
        trans: Vec<MobVertex>,
        texture: Option<RgbaImage>,
    ) -> Option<Self> {
        if opaque.is_empty() && trans.is_empty() {
            return None;
        }
        let (o, t) = (opaque.len() as u32, trans.len() as u32);
        opaque.extend(trans);
        Some(Self {
            vertices: opaque,
            opaque_count: o,
            transparent_count: t,
            texture,
        })
    }
}

/// Coins des 6 faces d'une boîte (ordre de `addTexBox` / `addColorBox`) :
/// +Z, −Z, −X, +X, +Y, −Y.
fn box_faces(x0: f32, x1: f32, y0: f32, y1: f32, z0: f32, z1: f32) -> [[[f32; 3]; 4]; 6] {
    [
        [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
        [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
        [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
        [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
        [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
    ]
}

const FACE_BRIGHTNESS: [f32; 6] = [BR_FRONT, BR_FRONT, BR_SIDE, BR_SIDE, BR_TOP, BR_BOTTOM];

/// Deux triangles `(0,1,2)(2,3,0)`.
fn push_quad(
    out: &mut Vec<MobVertex>,
    corners: &[[f32; 3]; 4],
    uvs: &[[f32; 2]; 4],
    tint: [f32; 4],
    brightness: f32,
) {
    for i in [0, 1, 2, 2, 3, 0] {
        out.push(MobVertex {
            pos: corners[i],
            uv: uvs[i],
            tint,
            brightness,
        });
    }
}

fn px_bounds(b: &[f32]) -> (f32, f32, f32, f32, f32, f32) {
    (
        b[0] / 16.0,
        (b[0] + b[3]) / 16.0,
        b[1] / 16.0,
        (b[1] + b[4]) / 16.0,
        b[2] / 16.0,
        (b[2] + b[5]) / 16.0,
    )
}

/// `addTexBox` (§14.3) : `b = [bx,by,bz,bw,bh,bd,u0,v0]` en pixels MC, texture `tw × th`.
pub fn add_tex_box(out: &mut Vec<MobVertex>, b: &[f32; 8], tw: f32, th: f32) {
    let (x0, x1, y0, y1, z0, z1) = px_bounds(b);
    let (bw, bh, bd, u0, v0) = (b[3], b[4], b[5], b[6], b[7]);
    let ua = u0 / tw;
    let ub = (u0 + bd) / tw;
    let uc = (u0 + bd + bw) / tw;
    let ud = (u0 + 2.0 * bd + bw) / tw;
    let ue = (u0 + 2.0 * bd + 2.0 * bw) / tw;
    let vt = 1.0 - v0 / th;
    let vm = 1.0 - (v0 + bd) / th;
    let vb = 1.0 - (v0 + bd + bh) / th;
    let uvs: [[[f32; 2]; 4]; 6] = [
        [[ub, vb], [uc, vb], [uc, vm], [ub, vm]],
        [[ud, vb], [ue, vb], [ue, vm], [ud, vm]],
        [[ua, vb], [ub, vb], [ub, vm], [ua, vm]],
        [[uc, vb], [ud, vb], [ud, vm], [uc, vm]],
        [[ub, vm], [uc, vm], [uc, vt], [ub, vt]],
        [[ud, vt], [uc, vt], [uc, vm], [ud, vm]],
    ];
    for (f, corners) in box_faces(x0, x1, y0, y1, z0, z1).iter().enumerate() {
        push_quad(out, corners, &uvs[f], [1.0; 4], FACE_BRIGHTNESS[f]);
    }
}

/// `addColorBox` : `b = [bx,by,bz,bw,bh,bd]`, UV `(0,0)`, teinte `rgba`.
pub fn add_color_box(out: &mut Vec<MobVertex>, b: &[f32; 6], rgba: [f32; 4]) {
    let (x0, x1, y0, y1, z0, z1) = px_bounds(b);
    for (f, corners) in box_faces(x0, x1, y0, y1, z0, z1).iter().enumerate() {
        push_quad(out, corners, &[[0.0; 2]; 4], rgba, FACE_BRIGHTNESS[f]);
    }
}

/// Forme vanilla codée en dur : taille de texture (pixels MC) et boîtes
/// `[bx,by,bz,bw,bh,bd,u0,v0]`.
#[derive(Clone, Copy, Debug)]
pub struct VanillaShape {
    pub tex_size: [f32; 2],
    pub boxes: &'static [[f32; 8]],
}

macro_rules! shape {
    ($tw:expr, $th:expr; $($b:expr),* $(,)?) => {
        VanillaShape { tex_size: [$tw as f32, $th as f32], boxes: &[$(bx($b)),*] }
    };
}

const fn bx(v: [i32; 8]) -> [f32; 8] {
    [
        v[0] as f32,
        v[1] as f32,
        v[2] as f32,
        v[3] as f32,
        v[4] as f32,
        v[5] as f32,
        v[6] as f32,
        v[7] as f32,
    ]
}

const COW: VanillaShape = shape!(64, 32;
    [-6, 10, -5, 12, 9, 10, 18, 4], [-4, 16, 6, 8, 8, 6, 0, 0],
    [-6, 0, 3, 4, 12, 4, 0, 16], [2, 0, 3, 4, 12, 4, 0, 16],
    [-6, 0, -7, 4, 12, 4, 0, 16], [2, 0, -7, 4, 12, 4, 0, 16]);
const SHEEP: VanillaShape = shape!(64, 32;
    [-4, 9, -5, 8, 10, 10, 28, 8], [-3, 16, 6, 6, 6, 4, 0, 0],
    [-5, 0, 3, 4, 12, 4, 0, 16], [1, 0, 3, 4, 12, 4, 0, 16],
    [-5, 0, -7, 4, 12, 4, 0, 16], [1, 0, -7, 4, 12, 4, 0, 16]);
const PIG: VanillaShape = shape!(64, 32;
    [-5, 7, -5, 10, 8, 10, 28, 8], [-4, 12, 5, 8, 8, 8, 0, 0],
    [-5, 0, 2, 4, 8, 4, 0, 16], [1, 0, 2, 4, 8, 4, 0, 16],
    [-5, 0, -6, 4, 8, 4, 0, 16], [1, 0, -6, 4, 8, 4, 0, 16]);
const CHICKEN: VanillaShape = shape!(64, 32;
    [-3, 4, -3, 6, 8, 6, 0, 9], [-2, 11, 3, 4, 6, 4, 0, 0],
    [-4, 4, -3, 1, 8, 6, 24, 13], [3, 4, -3, 1, 8, 6, 24, 13],
    [-3, 0, -1, 3, 4, 3, 26, 0], [0, 0, -1, 3, 4, 3, 26, 0]);
const RABBIT: VanillaShape = shape!(64, 32;
    [-3, 6, -3, 6, 6, 6, 16, 0], [-2, 11, 1, 4, 5, 4, 0, 0],
    [-1, 10, 4, 2, 8, 1, 0, 16], [1, 10, 4, 2, 8, 1, 10, 16],
    [-2, 0, -2, 4, 6, 4, 0, 16], [2, 0, -2, 4, 6, 4, 0, 16]);
const POLARBEAR: VanillaShape = shape!(64, 64;
    [-7, 14, -6, 14, 10, 11, 18, 4], [-5, 21, 5, 10, 9, 7, 0, 0],
    [-7, 0, 4, 4, 16, 4, 0, 16], [3, 0, 4, 4, 16, 4, 0, 16],
    [-7, 0, -8, 4, 16, 4, 0, 16], [3, 0, -8, 4, 16, 4, 0, 16]);
const WOLF: VanillaShape = shape!(64, 32;
    [-4, 10, -4, 8, 8, 8, 18, 4], [-3, 16, 4, 6, 6, 4, 0, 0],
    [-4, 0, 3, 3, 8, 3, 0, 16], [1, 0, 3, 3, 8, 3, 0, 16],
    [-4, 0, -6, 3, 8, 3, 0, 16], [1, 0, -6, 3, 8, 3, 0, 16]);
const FOX: VanillaShape = shape!(64, 32;
    [-4, 8, -4, 8, 6, 8, 24, 0], [-3, 13, 3, 6, 5, 5, 0, 0],
    [-4, 0, 2, 3, 6, 3, 0, 16], [1, 0, 2, 3, 6, 3, 0, 16],
    [-4, 0, -5, 3, 6, 3, 0, 16], [1, 0, -5, 3, 6, 3, 0, 16]);
const HORSE: VanillaShape = shape!(64, 64;
    [-5, 11, -6, 10, 10, 12, 18, 4], [-3, 20, 4, 6, 7, 5, 0, 0],
    [-2, 26, 6, 2, 7, 2, 56, 0],
    [-5, 0, 4, 4, 14, 4, 0, 16], [1, 0, 4, 4, 14, 4, 0, 16],
    [-5, 0, -8, 4, 14, 4, 0, 16], [1, 0, -8, 4, 14, 4, 0, 16]);
const LLAMA: VanillaShape = shape!(64, 64;
    [-6, 12, -5, 12, 14, 10, 18, 4], [-4, 25, 3, 8, 8, 5, 0, 0],
    [-6, 0, 4, 4, 14, 4, 0, 16], [2, 0, 4, 4, 14, 4, 0, 16],
    [-6, 0, -9, 4, 14, 4, 0, 16], [2, 0, -9, 4, 14, 4, 0, 16]);
const HUMANOID: VanillaShape = shape!(64, 64;
    [-4, 24, -4, 8, 8, 8, 0, 0], [-4, 12, -2, 8, 12, 4, 16, 16],
    [-8, 12, -2, 4, 12, 4, 40, 16], [4, 12, -2, 4, 12, 4, 32, 48],
    [-4, 0, -2, 4, 12, 4, 0, 16], [0, 0, -2, 4, 12, 4, 16, 48]);
const CREEPER: VanillaShape = shape!(64, 32;
    [-4, 18, -4, 8, 8, 8, 0, 0], [-4, 6, -2, 8, 12, 4, 16, 16],
    [-4, 0, 2, 4, 6, 4, 0, 16], [0, 0, 2, 4, 6, 4, 0, 16],
    [-4, 0, -6, 4, 6, 4, 0, 16], [0, 0, -6, 4, 6, 4, 0, 16]);
const SPIDER: VanillaShape = shape!(64, 32;
    [-7, 5, -4, 14, 6, 8, 18, 4], [-4, 5, 3, 8, 8, 8, 0, 0],
    [-8, 5, 1, 4, 1, 12, 0, 16], [4, 5, 1, 4, 1, 12, 0, 16],
    [-8, 5, -3, 4, 1, 12, 0, 16], [4, 5, -3, 4, 1, 12, 0, 16]);
const ENDERMAN: VanillaShape = shape!(64, 32;
    [-2, 24, -2, 4, 4, 4, 0, 0], [-2, 12, -1, 4, 12, 2, 10, 0],
    [-4, 12, -1, 2, 12, 2, 16, 0], [2, 12, -1, 2, 12, 2, 16, 0],
    [-2, 0, -1, 2, 12, 2, 0, 16], [0, 0, -1, 2, 12, 2, 8, 16]);
const SLIME: VanillaShape = shape!(64, 32; [-4, 0, -4, 8, 8, 8, 0, 0]);
const SQUID: VanillaShape = shape!(64, 32;
    [-4, 4, -4, 8, 8, 8, 0, 0], [-1, 0, -2, 2, 4, 2, 16, 16],
    [2, 0, 1, 2, 4, 2, 16, 16], [-3, 0, 2, 2, 4, 2, 16, 16],
    [-1, 0, -4, 2, 4, 2, 16, 16]);
const BAT: VanillaShape = shape!(64, 64;
    [-2, 8, -2, 4, 6, 4, 0, 0], [-2, 2, -1, 4, 8, 2, 16, 16],
    [-8, 3, 0, 6, 8, 1, 32, 0], [2, 3, 0, 6, 8, 1, 32, 0]);
const PHANTOM: VanillaShape = shape!(64, 64;
    [-2, 8, -2, 4, 4, 4, 0, 0], [-4, 4, -2, 8, 4, 4, 16, 8],
    [-12, 6, -1, 8, 2, 3, 0, 16], [4, 6, -1, 8, 2, 3, 0, 16]);
const BEE: VanillaShape = shape!(64, 64;
    [-3, 6, -3, 6, 6, 6, 0, 0], [-3, 3, -3, 6, 4, 8, 24, 0],
    [-8, 7, 0, 6, 1, 6, 24, 16], [2, 7, 0, 6, 1, 6, 24, 16]);
const ARMADILLO: VanillaShape = shape!(64, 64;
    [-4, 4, -3, 8, 8, 8, 0, 0], [-2, 9, 4, 4, 4, 2, 24, 0],
    [-4, 0, 2, 3, 4, 3, 0, 16], [1, 0, 2, 3, 4, 3, 0, 16],
    [-4, 0, -5, 3, 4, 3, 0, 16], [1, 0, -5, 3, 4, 3, 0, 16]);

/// `VANILLA_SHAPES.get(id)` (§14.4), alias compris.
pub fn vanilla_shape(id: &str) -> Option<VanillaShape> {
    Some(match id {
        "cow" => COW,
        "sheep" => SHEEP,
        "pig" => PIG,
        "chicken" => CHICKEN,
        "rabbit" => RABBIT,
        "polarbear" => POLARBEAR,
        "wolf" => WOLF,
        "fox" => FOX,
        "horse" | "donkey" | "mule" => HORSE,
        "llama" => LLAMA,
        "zombie" | "skeleton" | "husk" | "drowned" | "piglin" | "villager" | "pillager" => HUMANOID,
        "creeper" => CREEPER,
        "spider" => SPIDER,
        "enderman" => ENDERMAN,
        "slime" | "magma_cube" => SLIME,
        "squid" => SQUID,
        "bat" => BAT,
        "phantom" => PHANTOM,
        "bee" => BEE,
        "armadillo" => ARMADILLO,
        _ => return None,
    })
}

/// `buildFromVanillaShape` : correction de hauteur (`nativeTh = texH / (texW / tw)`, appliquée
/// si l'écart dépasse 0.5), tout opaque.
pub fn build_vanilla(shape: &VanillaShape, texture: RgbaImage) -> MobModelCpu {
    let [tw, mut th] = shape.tex_size;
    let (w, h) = texture.dimensions();
    if w > 0 && h > 0 {
        let scale = w as f32 / tw;
        let native_th = h as f32 / scale;
        if (native_th - th).abs() > 0.5 {
            th = native_th;
        }
    }
    let mut out = Vec::with_capacity(shape.boxes.len() * 36);
    for b in shape.boxes {
        add_tex_box(&mut out, b, tw, th);
    }
    MobModelCpu::from_lists(out, Vec::new(), Some(texture)).expect("forme vanilla non vide")
}

/// `buildFromGeometry` (mode couleur) ; `None` si aucune boîte.
pub fn build_geometry(boxes: &[ColorBox]) -> Option<MobModelCpu> {
    let mut opaque = Vec::new();
    let mut trans = Vec::new();
    for b in boxes {
        let dims = [
            b.min_px[0],
            b.min_px[1],
            b.min_px[2],
            b.size_px[0],
            b.size_px[1],
            b.size_px[2],
        ];
        let out = if b.rgba[3] < 1.0 {
            &mut trans
        } else {
            &mut opaque
        };
        add_color_box(out, &dims, b.rgba);
    }
    MobModelCpu::from_lists(opaque, trans, None)
}

/// `buildTexturedBox` : boîte `(-8,0,-8, 16,16,16)` sur un « atlas » 1×1. [JAVA-BUG] reproduit :
/// le commentaire Java annonce des UV pleines par face, mais `addTexBox` divise des tailles de
/// 16 pixels par 1 → UV de 0 à 64 (texture répétée 16 fois par face en REPEAT, spec §14.2
/// étape 4).
pub fn build_textured_box(texture: RgbaImage) -> MobModelCpu {
    let mut out = Vec::with_capacity(36);
    add_tex_box(
        &mut out,
        &[-8.0, 0.0, -8.0, 16.0, 16.0, 16.0, 0.0, 0.0],
        1.0,
        1.0,
    );
    MobModelCpu::from_lists(out, Vec::new(), Some(texture)).expect("boîte non vide")
}

/// Damier 16×16 (cases de 4, magenta si pair) des modèles et textures manquants, écrit tel quel
/// en mémoire (pas de retournement : ligne 0 = `v = 0`).
pub fn missing_texture() -> RgbaImage {
    RgbaImage::from_raw(16, 16, checkerboard(16, 4)).expect("damier 16×16")
}

/// Cube manquant de `MobRenderer` : `[-0.5,0.5]×[0,1]×[-0.5,0.5]`, UV 0..1 par face, mis à
/// l'échelle `(widthX, height, widthZ)` au rendu.
pub fn missing_cube() -> Vec<MobVertex> {
    let mut out = Vec::with_capacity(36);
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for (f, corners) in box_faces(-0.5, 0.5, 0.0, 1.0, -0.5, 0.5).iter().enumerate() {
        push_quad(&mut out, corners, &uvs, [1.0; 4], FACE_BRIGHTNESS[f]);
    }
    out
}

/// Matrice modèle d'un mob : `T(p) · Ry(yaw) · Rx(pitch)` (radians) `[· S(missing_scale)]`.
pub fn mob_model_matrix(position: Vec3, yaw_rad: f32, pitch_rad: f32, scale: Option<Vec3>) -> Mat4 {
    let m = Mat4::from_translation(position)
        * Mat4::from_rotation_y(yaw_rad)
        * Mat4::from_rotation_x(pitch_rad);
    match scale {
        Some(s) => m * Mat4::from_scale(s),
        None => m,
    }
}

/// Billboard sphérique des particules (§15) : `T(p) · R_vᵀ · S(s)` où `R_v` est le bloc 3×3 de
/// la vue (`model[c][r] = view[r][c]`).
pub fn particle_model_matrix(position: Vec3, view: &Mat4, scale: f32) -> Mat4 {
    let r = Mat3::from_mat4(*view).transpose();
    let m = Mat4::from_cols(
        r.x_axis.extend(0.0),
        r.y_axis.extend(0.0),
        r.z_axis.extend(0.0),
        position.extend(1.0),
    );
    m * Mat4::from_scale(Vec3::splat(scale))
}

/// Quad unité des particules `[x,y,z,u,v]` : `TRIANGLE_FAN (0,1,2,3)` → triangles
/// `(0,1,2)(0,2,3)`. `v` inversé : l'image apparaît tête en bas (Java, à reproduire).
pub const PARTICLE_QUAD: [[f32; 5]; 6] = {
    const V: [[f32; 5]; 4] = [
        [-0.5, -0.5, 0.0, 0.0, 1.0],
        [0.5, -0.5, 0.0, 1.0, 1.0],
        [0.5, 0.5, 0.0, 1.0, 0.0],
        [-0.5, 0.5, 0.0, 0.0, 0.0],
    ];
    [V[0], V[1], V[2], V[0], V[2], V[3]]
};

// ------------------------------------------------------------------------ fichiers

/// Racines de recherche des modèles : pack actif puis Default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobRoots {
    pub pack_root: PathBuf,
    pub default_root: PathBuf,
}

impl MobRoots {
    /// `activePackRoot = packsRoot/<nom>` (nom vide → « Default », même si le pack n'existe
    /// pas ou n'a pas de textures de blocs), `activeDefaultRoot = packsRoot/Default`.
    pub fn for_pack(packs_root: &Path, name: &str) -> Self {
        let name = if name.is_empty() { DEFAULT_PACK } else { name };
        Self {
            pack_root: packs_root.join(name),
            default_root: packs_root.join(DEFAULT_PACK),
        }
    }

    fn roots(&self) -> [&Path; 2] {
        [&self.pack_root, &self.default_root]
    }
}

/// `safeLoad` : PNG RGBA8 retourné ; `None` si illisible.
fn load_texture(path: &Path) -> Option<RgbaImage> {
    std::fs::read(path)
        .ok()
        .and_then(|b| decode_rgba_flipped(&b))
}

/// `findEntityTexture` (§14.2) : `id/id.png`, `id.png`, `id/id_temperate.png` dans
/// `assets/minecraft/textures/entity/` de chaque racine ; le **premier existant** est retourné
/// même si son chargement échoue (`None`, sans essayer la racine suivante) ; sinon premier `.png`
/// du dossier `id/` (tri insensible à la casse, comme `File.compareTo` sous Windows) s'il se
/// charge.
pub fn find_entity_texture(id: &str, roots: &MobRoots) -> Option<RgbaImage> {
    let base = Path::new("assets/minecraft/textures/entity");
    for root in roots.roots() {
        let entity = root.join(base);
        for p in [
            entity.join(id).join(format!("{id}.png")),
            entity.join(format!("{id}.png")),
            entity.join(id).join(format!("{id}_temperate.png")),
        ] {
            if p.exists() {
                return load_texture(&p);
            }
        }
        let dir = entity.join(id);
        if dir.is_dir() {
            let mut pngs: Vec<PathBuf> = std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| {
                            p.file_name()
                                .and_then(|n| n.to_str())
                                .is_some_and(|n| n.to_lowercase().ends_with(".png"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            pngs.sort_by_key(|p| p.to_string_lossy().to_lowercase());
            if let Some(t) = pngs.first().and_then(|p| load_texture(p)) {
                return Some(t);
            }
        }
    }
    None
}

/// `resolveJemTexture` : `<racine>/assets/minecraft/<chemin sans préfixe>` (premier existant,
/// même illisible), sinon id deviné → [`find_entity_texture`].
fn resolve_jem_texture(path: &str, roots: &MobRoots) -> Option<RgbaImage> {
    let norm = path.strip_prefix("assets/minecraft/").unwrap_or(path);
    for root in roots.roots() {
        let f = root.join("assets/minecraft").join(norm);
        if f.exists() {
            return load_texture(&f);
        }
    }
    jem::guess_mob_id(norm).and_then(|id| find_entity_texture(&id, roots))
}

/// Lecture UTF-8 « ligne par ligne » Java (`readLine` + `\n`).
fn read_lines_joined(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut out = String::with_capacity(text.len() + 1);
    for line in text.replace("\r\n", "\n").replace('\r', "\n").lines() {
        out.push_str(line);
        out.push('\n');
    }
    Some(out)
}

/// `tryLoadJem` : `<racine>/assets/minecraft/optifine/cem/<id>.jem`, premier fichier existant
/// **et** parsable.
pub fn try_load_jem(id: &str, roots: &MobRoots) -> Option<MobModelCpu> {
    for root in roots.roots() {
        let f = root.join(format!("assets/minecraft/optifine/cem/{id}.jem"));
        if f.exists() {
            if let Some(m) = read_lines_joined(&f)
                .and_then(|json| jem::parse_jem(&json, &mut |p| resolve_jem_texture(p, roots)))
            {
                return Some(m);
            }
        }
    }
    None
}

/// Ordre de résolution d'un modèle (§14.2, `loadAll` / `loadMob`) : JEM, forme vanilla
/// (texture absente → damier), géométrie couleur, boîte texturée (mobs non intégrés), sinon
/// `None` (cube manquant au rendu).
pub fn load_mob_model(desc: &MobModelDesc, roots: &MobRoots) -> Option<MobModelCpu> {
    if let Some(m) = try_load_jem(&desc.id, roots) {
        return Some(m);
    }
    if let Some(shape) = vanilla_shape(&desc.id) {
        let tex = find_entity_texture(&desc.id, roots).unwrap_or_else(missing_texture);
        return Some(build_vanilla(&shape, tex));
    }
    if let Some(m) = desc.geometry.as_deref().and_then(build_geometry) {
        return Some(m);
    }
    if !desc.built_in {
        return find_entity_texture(&desc.id, roots).map(build_textured_box);
    }
    None
}

/// `loadAll` : un modèle par id (doublons ignorés, le premier gagne).
pub fn load_all_mob_models(
    descs: &[MobModelDesc],
    roots: &MobRoots,
) -> HashMap<String, MobModelCpu> {
    let mut done = HashSet::new();
    let mut out = HashMap::new();
    for d in descs {
        if !done.insert(d.id.as_str()) {
            continue;
        }
        if let Some(m) = load_mob_model(d, roots) {
            out.insert(d.id.clone(), m);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec4;

    fn approx(a: Mat4, b: Mat4) -> bool {
        a.abs_diff_eq(b, 1e-5)
    }

    #[test]
    fn cow_vertex_count_and_head_uvs() {
        let m = build_vanilla(&COW, RgbaImage::new(64, 32));
        assert_eq!(m.vertices.len(), 216);
        assert_eq!((m.opaque_count, m.transparent_count), (216, 0));
        // Tête (boîte 1) : face +Z, offset (0,0), 8×8×6 → uB = 6/64, uC = 14/64,
        // vM = 1 − 6/32, vB = 1 − 14/32.
        let head = &m.vertices[36..42];
        let (ub, uc, vm, vb) = (6.0 / 64.0, 14.0 / 64.0, 1.0 - 6.0 / 32.0, 1.0 - 14.0 / 32.0);
        assert_eq!(head[0].uv, [ub, vb]);
        assert_eq!(head[1].uv, [uc, vb]);
        assert_eq!(head[2].uv, [uc, vm]);
        assert_eq!(head[4].uv, [ub, vm]);
        assert_eq!(head[0].pos, [-0.25, 1.0, 0.75]);
        assert_eq!(head[2].pos, [0.25, 1.5, 0.75]);
        assert!(head
            .iter()
            .all(|v| v.brightness == BR_FRONT && v.tint == [1.0; 4]));
        // Triangles (0,1,2)(2,3,0).
        assert_eq!(head[3], head[2]);
        assert_eq!(head[5], head[0]);
    }

    #[test]
    fn faces_are_ccw_from_outside() {
        let mut v = Vec::new();
        add_color_box(&mut v, &[-8.0, 0.0, -8.0, 16.0, 16.0, 16.0], [1.0; 4]);
        let center = Vec3::new(0.0, 0.5, 0.0);
        let brightness: Vec<f32> = v.chunks(6).map(|f| f[0].brightness).collect();
        assert_eq!(brightness, FACE_BRIGHTNESS);
        for tri in v.chunks(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|t| Vec3::from(t.pos));
            let n = (b - a).cross(c - a);
            assert!(n.dot(a - center) > 0.0, "triangle orienté vers l'intérieur");
        }
        assert!(v.iter().all(|x| x.uv == [0.0, 0.0]));
    }

    #[test]
    fn height_correction() {
        // Damier 16×16 sur une forme 64×32 : scale = 0.25 → th natif = 64.
        let m = build_vanilla(&COW, missing_texture());
        let vb_body = m.vertices[0].uv[1];
        assert_eq!(vb_body, 1.0 - (4.0 + 10.0 + 9.0) / 64.0);
        // Texture 128×64 pour 64×32 : même rapport, pas de correction.
        let m = build_vanilla(&COW, RgbaImage::new(128, 64));
        assert_eq!(m.vertices[0].uv[1], 1.0 - 23.0 / 32.0);
        // Variante 1.21 carrée 64×64 → th = 64.
        let m = build_vanilla(&COW, RgbaImage::new(64, 64));
        assert_eq!(m.vertices[0].uv[1], 1.0 - 23.0 / 64.0);
    }

    #[test]
    fn vanilla_table() {
        let n = |id| vanilla_shape(id).map(|s| s.boxes.len());
        assert_eq!(n("cow"), Some(6));
        assert_eq!(n("mule"), Some(7));
        assert_eq!(n("pillager"), Some(6));
        assert_eq!(n("magma_cube"), Some(1));
        assert_eq!(n("squid"), Some(5));
        assert_eq!(n("bat"), Some(4));
        assert_eq!(n("reindeer"), None);
        assert_eq!(vanilla_shape("polarbear").unwrap().tex_size, [64.0, 64.0]);
        assert_eq!(
            vanilla_shape("chicken").unwrap().boxes[2],
            [-4.0, 4.0, -3.0, 1.0, 8.0, 6.0, 24.0, 13.0]
        );
    }

    #[test]
    fn geometry_and_builtin_descs() {
        let reg = MobRegistry::builtin();
        let descs = MobModelDesc::from_registry(&reg);
        // Trois mobs d'origine + les animaux de la phase 9, tous intégrés et tous dessinables.
        assert_eq!(descs.len(), reg.all().len());
        assert!(descs.len() >= 40);
        assert!(descs.iter().all(|d| d.built_in));
        for d in &descs {
            let m = build_geometry(d.geometry.as_ref().unwrap())
                .unwrap_or_else(|| panic!("{} sans géométrie", d.id));
            assert!(m.vertices.len() >= 36 * 4, "{} trop simple", d.id);
            // Un identifiant de forme vanilla (`sheep`, `wolf`…) donnerait un damier sans texture.
            if !MobRegistry::builtin()
                .get(&d.id)
                .is_some_and(|c| c.id == "reindeer")
            {
                assert!(
                    vanilla_shape(&d.id).is_none(),
                    "{} entre en collision avec une forme vanilla",
                    d.id
                );
            }
        }
        let reindeer = build_geometry(descs[0].geometry.as_ref().unwrap()).unwrap();
        assert_eq!(reindeer.vertices.len(), 17 * 36);
        assert!(reindeer.texture.is_none());
        // Poisson : 4 boîtes à alpha 0.7 → plage transparente en fin de tampon.
        let fish = build_geometry(descs[1].geometry.as_ref().unwrap()).unwrap();
        assert_eq!(
            (fish.opaque_count, fish.transparent_count),
            (4 * 36, 4 * 36)
        );
        assert!(fish.vertices[4 * 36..].iter().all(|v| v.tint[3] < 1.0));
        assert_eq!(fish.vertices[0].tint, [0.40, 0.55, 0.68, 1.0]);
        assert!(build_geometry(&[]).is_none());
    }

    #[test]
    fn missing_cube_and_texture() {
        let c = missing_cube();
        assert_eq!(c.len(), 36);
        for axis in 0..3 {
            let min = c.iter().map(|v| v.pos[axis]).fold(f32::MAX, f32::min);
            let max = c.iter().map(|v| v.pos[axis]).fold(f32::MIN, f32::max);
            assert_eq!((min, max), [(-0.5, 0.5), (0.0, 1.0), (-0.5, 0.5)][axis]);
        }
        let uv: Vec<_> = c[..6].iter().map(|v| v.uv).collect();
        assert_eq!(
            uv,
            [[0., 0.], [1., 0.], [1., 1.], [1., 1.], [0., 1.], [0., 0.]]
        );
        let t = missing_texture();
        assert_eq!(t.get_pixel(0, 0).0, [255, 0, 255, 255]);
        assert_eq!(t.get_pixel(4, 0).0, [0, 0, 0, 255]);
        assert_eq!(t.get_pixel(4, 4).0, [255, 0, 255, 255]);
        assert_eq!(t.get_pixel(15, 12).0, [255, 0, 255, 255]);
    }

    #[test]
    fn mob_matrix_is_t_ry_rx() {
        let p = Vec3::new(1.0, 2.0, 3.0);
        let m = mob_model_matrix(p, std::f32::consts::FRAC_PI_2, 0.0, None);
        // yaw = π/2 : l'avant (+Z) passe vers +X.
        let front = m * Vec4::new(0.0, 0.0, 1.0, 1.0);
        assert!((front.truncate() - Vec3::new(2.0, 2.0, 3.0)).length() < 1e-5);
        // Pitch appliqué avant le yaw (dans le repère du mob).
        let m = mob_model_matrix(Vec3::ZERO, std::f32::consts::FRAC_PI_2, 0.3, None);
        let expect =
            Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2) * Mat4::from_rotation_x(0.3);
        assert!(approx(m, expect));
        let s = mob_model_matrix(p, 0.0, 0.0, Some(Vec3::new(0.75, 2.0, 1.375)));
        let top = s * Vec4::new(0.5, 1.0, 0.5, 1.0);
        assert!((top.truncate() - Vec3::new(1.375, 4.0, 3.6875)).length() < 1e-5);
    }

    #[test]
    fn particle_billboard_faces_camera() {
        let cam = crate::Camera {
            position: Vec3::new(3.0, 70.0, -2.0),
            pitch_deg: 25.0,
            yaw_deg: 130.0,
            ..crate::Camera::default()
        };
        let view = cam.view();
        let p = Vec3::new(5.0, 71.0, 1.0);
        let m = particle_model_matrix(p, &view, 0.15);
        // Dans l'espace vue, le quad reste parallèle à l'écran, à l'échelle s.
        let mv = view * m;
        let r = Mat3::from_mat4(mv);
        assert!(r.abs_diff_eq(Mat3::from_diagonal(Vec3::splat(0.15)), 1e-5));
        assert!(((m * Vec4::new(0.0, 0.0, 0.0, 1.0)).truncate() - p).length() < 1e-6);
        // Sa normale (+Z modèle) pointe vers la caméra (la face CCW est visible).
        let n = (m * Vec4::new(0.0, 0.0, 1.0, 0.0)).truncate();
        assert!(n.dot(cam.position - p) > 0.0);
        let q = PARTICLE_QUAD;
        assert_eq!(q[0][3..], [0.0, 1.0]);
        assert_eq!(q[5][..2], [-0.5, 0.5]);
    }

    /// Dossier temporaire unique (supprimé à la fin du test).
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "nb-gfx-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_png(path: &Path, w: u32, h: u32, rgba: [u8; 4]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        RgbaImage::from_pixel(w, h, image::Rgba(rgba))
            .save(path)
            .unwrap();
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn entity_texture_lookup_order() {
        let tmp = TempDir::new("entity");
        let roots = MobRoots::for_pack(&tmp.0, "Pack");
        assert_eq!(roots.pack_root, tmp.0.join("Pack"));
        assert_eq!(
            MobRoots::for_pack(&tmp.0, "").pack_root,
            tmp.0.join("Default")
        );
        let ent = |root: &str| tmp.0.join(root).join("assets/minecraft/textures/entity");
        assert!(find_entity_texture("cow", &roots).is_none());
        // Repli Default : premier PNG du dossier (tri sans casse).
        write_png(&ent("Default").join("cow/b.png"), 2, 2, [2, 0, 0, 255]);
        write_png(&ent("Default").join("cow/A.png"), 1, 1, [1, 0, 0, 255]);
        assert_eq!(find_entity_texture("cow", &roots).unwrap().width(), 1);
        // Le pack actif passe avant le Default.
        write_png(
            &ent("Pack").join("cow/cow_temperate.png"),
            3,
            3,
            [3, 0, 0, 255],
        );
        assert_eq!(find_entity_texture("cow", &roots).unwrap().width(), 3);
        write_png(&ent("Pack").join("cow.png"), 4, 4, [4, 0, 0, 255]);
        assert_eq!(find_entity_texture("cow", &roots).unwrap().width(), 4);
        write_png(&ent("Pack").join("cow/cow.png"), 5, 5, [5, 0, 0, 255]);
        assert_eq!(find_entity_texture("cow", &roots).unwrap().width(), 5);
        // Premier existant illisible : `None` sans essayer le Default.
        write(&ent("Pack").join("pig/pig.png"), "pas un png");
        write_png(&ent("Default").join("pig/pig.png"), 2, 2, [0; 4]);
        assert!(find_entity_texture("pig", &roots).is_none());
        // Texture retournée verticalement (ligne 0 = bas du PNG).
        let mut img = RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]));
        img.put_pixel(0, 1, image::Rgba([255, 255, 255, 255]));
        std::fs::create_dir_all(ent("Default").join("fox")).unwrap();
        img.save(ent("Default").join("fox/fox.png")).unwrap();
        let t = find_entity_texture("fox", &roots).unwrap();
        assert_eq!(t.get_pixel(0, 0).0, [255, 255, 255, 255]);
    }

    #[test]
    fn resolution_order() {
        let tmp = TempDir::new("resolve");
        let roots = MobRoots::for_pack(&tmp.0, "Default");
        let ent = tmp.0.join("Default/assets/minecraft/textures/entity");
        let cem = tmp.0.join("Default/assets/minecraft/optifine/cem");
        let desc = |id: &str, built_in, geometry: Option<Vec<ColorBox>>| MobModelDesc {
            id: id.into(),
            built_in,
            geometry,
        };
        let red = ColorBox::from_row(&[0., 0., 0., 16., 16., 16., 1., 0., 0., 1.]);
        // Vanilla sans texture → damier (texture 16×16, hauteur corrigée).
        let cow = load_mob_model(&desc("cow", false, Some(vec![red])), &roots).unwrap();
        assert_eq!(cow.texture, Some(missing_texture()));
        assert_eq!(cow.vertices.len(), 216);
        // Géométrie couleur.
        let r = load_mob_model(&desc("reindeer", true, Some(vec![red])), &roots).unwrap();
        assert!(r.texture.is_none() && r.vertices.len() == 36);
        // Intégré sans géométrie : aucun modèle.
        assert!(load_mob_model(&desc("orca", true, None), &roots).is_none());
        // Personnalisé avec texture : boîte texturée sur « atlas » 1×1 ([JAVA-BUG] : UV en
        // pixels non normalisés → texture répétée 16 fois par face, REPEAT).
        write_png(&ent.join("ghost.png"), 8, 8, [9, 9, 9, 255]);
        let g = load_mob_model(&desc("ghost", false, None), &roots).unwrap();
        assert_eq!(g.vertices.len(), 36);
        assert_eq!(g.vertices[0].uv, [16.0, -31.0]);
        assert_eq!(g.vertices[1].uv, [32.0, -31.0]);
        assert_eq!(g.vertices[0].pos, [-0.5, 0.0, 0.5]);
        assert!(load_mob_model(&desc("ghost", true, None), &roots).is_none());
        // JEM prioritaire, y compris sur la géométrie ; texture devinée depuis le chemin.
        write(
            &cem.join("ghost.jem"),
            "{\r\n\"texture\": \"textures/entity/ghost_baby.png\",\r\n\"textureSize\": [8, 8],\r\n\"models\": [{\"boxes\": [{\"coordinates\": [0, 0, 0, 2, 2, 2], \"textureOffset\": [0, 0]}]}]\r\n}",
        );
        let j = load_mob_model(&desc("ghost", true, Some(vec![red])), &roots).unwrap();
        assert_eq!(j.vertices.len(), 36);
        assert_eq!(j.vertices[0].uv, [2.0 / 8.0, 1.0 - 4.0 / 8.0]);
        assert_eq!(j.texture.as_ref().unwrap().width(), 8);
        // JEM illisible → étape suivante.
        write(&cem.join("cow.jem"), "{\"textureSize\": [a, b]}");
        assert_eq!(
            load_mob_model(&desc("cow", false, None), &roots)
                .unwrap()
                .vertices
                .len(),
            216
        );
        // loadAll : doublons ignorés (le premier gagne).
        let all = load_all_mob_models(
            &[
                desc("reindeer", true, Some(vec![red])),
                desc("reindeer", true, None),
                desc("orca", true, None),
            ],
            &roots,
        );
        assert_eq!(all.len(), 1);
        assert_eq!(all["reindeer"].vertices.len(), 36);
    }
}
