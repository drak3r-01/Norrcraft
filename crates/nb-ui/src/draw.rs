//! Listes de dessin : sommets 2D (pixels fenêtre, origine en haut à gauche) groupés en lots
//! (texture + scissor). Indépendant de Vulkan ; `nb_gfx::ui_pass` les rend tels quels.
//!
//! Les UV sont en **espace image** (v = 0 en haut du PNG). [`DrawList::region_gl`] convertit
//! les UV « GL » des appels Java (`renderRegion`, textures retournées au chargement).

use bytemuck::{Pod, Zeroable};

use crate::font;
use crate::textures::UiTexture;

/// Couleur RGBA (teinte multipliée par le texel).
pub type Rgba = [f32; 4];

pub const WHITE: Rgba = [1.0, 1.0, 1.0, 1.0];
pub const YELLOW: Rgba = [1.0, 1.0, 0.0, 1.0];

/// Gris opaque `(g, g, g, 1)`.
pub fn gray(g: f32) -> Rgba {
    [g, g, g, 1.0]
}

/// Style d'une icône d'inventaire.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum IconStyle {
    /// Quad plat (comportement d'origine).
    #[default]
    Flat,
    /// Objet pur ou plante : sprite avec ombre portée.
    Item,
    /// Bloc plein : cube isométrique ; `layer` = dessus, ici les deux faces visibles.
    Cube {
        left: u32,
        right: u32,
        tint_top: [f32; 3],
        tint_side: [f32; 3],
    },
}

/// Texture d'un lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureRef {
    Ui(UiTexture),
    /// Couche du tableau de textures de blocs (icônes, `getTextureIndex(0)`).
    BlockLayer(u32),
}

/// Sommet : 32 octets, `#[repr(C)]` (identique à `nb_gfx::ui_pass::UiVertex`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// Rectangle de découpe, pixels fenêtre, origine en haut à gauche (peut déborder : le
/// moteur de rendu le borne à l'écran).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Scissor {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Scissor {
    /// `glScissor(x, y, w, h)` (origine en bas) pour une fenêtre de hauteur `win_h`.
    pub fn from_gl(x: i32, y: i32, w: i32, h: i32, win_h: i32) -> Self {
        Self {
            x,
            y: win_h - y - h,
            w,
            h,
        }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x as f32
            && px < (self.x + self.w) as f32
            && py >= self.y as f32
            && py < (self.y + self.h) as f32
    }
}

/// Lot : `count` sommets (triangles) à partir de `first`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Batch {
    pub texture: TextureRef,
    pub scissor: Option<Scissor>,
    pub first: u32,
    pub count: u32,
}

/// Liste de dessin d'une frame (dans l'ordre de peinture).
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub width: u32,
    pub height: u32,
    vertices: Vec<Vertex>,
    batches: Vec<Batch>,
    scissor: Option<Scissor>,
}

impl DrawList {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            ..Self::default()
        }
    }

    /// Vide la liste et change la taille de la fenêtre.
    pub fn reset(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.vertices.clear();
        self.batches.clear();
        self.scissor = None;
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    pub fn batches(&self) -> &[Batch] {
        &self.batches
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Nombre de quads émis.
    pub fn quad_count(&self) -> usize {
        self.vertices.len() / 6
    }

    /// `glEnable(GL_SCISSOR_TEST)` + `glScissor` (Some) ou `glDisable` (None).
    pub fn set_scissor(&mut self, s: Option<Scissor>) {
        self.scissor = s;
    }

    pub fn scissor(&self) -> Option<Scissor> {
        self.scissor
    }

    /// Quad texturé ; `uv = [u0, v0, u1, v1]` en espace image (coin haut-gauche → bas-droit).
    #[allow(clippy::too_many_arguments)]
    pub fn quad_uv(
        &mut self,
        texture: TextureRef,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        uv: [f32; 4],
        color: Rgba,
    ) {
        let first = self.vertices.len() as u32;
        let [u0, v0, u1, v1] = uv;
        let v = |px, py, u, v| Vertex {
            pos: [px, py],
            uv: [u, v],
            color,
        };
        let (x1, y1) = (x + w, y + h);
        self.vertices.extend_from_slice(&[
            v(x, y, u0, v0),
            v(x, y1, u0, v1),
            v(x1, y1, u1, v1),
            v(x, y, u0, v0),
            v(x1, y1, u1, v1),
            v(x1, y, u1, v0),
        ]);
        match self.batches.last_mut() {
            Some(b) if b.texture == texture && b.scissor == self.scissor => b.count += 6,
            _ => self.batches.push(Batch {
                texture,
                scissor: self.scissor,
                first,
                count: 6,
            }),
        }
    }

    /// `UIRenderer.renderWithTint(tex, x, y, w, h, r, g, b, a)` (texture entière).
    pub fn image(&mut self, tex: UiTexture, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        self.quad_uv(TextureRef::Ui(tex), x, y, w, h, [0.0, 0.0, 1.0, 1.0], color);
    }

    /// Rectangle uni (texture blanche 1×1).
    pub fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        self.image(UiTexture::White, x, y, w, h, color);
    }

    /// `renderRegionWithTint(tex, x, y, w, h, u, v, uw, vh, tint)` avec les UV Java (GL) :
    /// le haut de l'écran échantillonne `v + vh` d'une texture retournée, soit `1 - v - vh`
    /// en espace image.
    #[allow(clippy::too_many_arguments)]
    pub fn region_gl(
        &mut self,
        tex: UiTexture,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        gl_uv: [f32; 4],
        color: Rgba,
    ) {
        let [u, v, uw, vh] = gl_uv;
        self.quad_uv(
            TextureRef::Ui(tex),
            x,
            y,
            w,
            h,
            [u, 1.0 - v - vh, u + uw, 1.0 - v],
            color,
        );
    }

    /// `BlockIconRenderer.render(layer, x, y, size)` : quad plat d'une couche de bloc.
    pub fn block_icon(&mut self, layer: u32, x: f32, y: f32, size: f32) {
        self.quad_uv(
            TextureRef::BlockLayer(layer),
            x,
            y,
            size,
            size,
            [0.0, 0.0, 1.0, 1.0],
            WHITE,
        );
    }

    /// Quadrilatère quelconque (4 coins dans l'ordre horaire, UV par coin).
    pub fn quad_pts(
        &mut self,
        texture: TextureRef,
        pts: [[f32; 2]; 4],
        uvs: [[f32; 2]; 4],
        color: Rgba,
    ) {
        let first = self.vertices.len() as u32;
        let v = |i: usize| Vertex {
            pos: pts[i],
            uv: uvs[i],
            color,
        };
        self.vertices
            .extend_from_slice(&[v(0), v(3), v(2), v(0), v(2), v(1)]);
        match self.batches.last_mut() {
            Some(b) if b.texture == texture && b.scissor == self.scissor => b.count += 6,
            _ => self.batches.push(Batch {
                texture,
                scissor: self.scissor,
                first,
                count: 6,
            }),
        }
    }

    /// Icône d'objet façon Minecraft : cube isométrique (3 faces) pour un bloc plein, sprite
    /// plat avec ombre portée (relief) pour un objet ou une plante.
    /// `layer` = face du dessus (cube) ou texture du sprite.
    pub fn icon(&mut self, layer: u32, style: IconStyle, x: f32, y: f32, size: f32) {
        match style {
            IconStyle::Flat => self.block_icon(layer, x, y, size),
            IconStyle::Item => {
                // Ombre portée puis sprite : donne de l'épaisseur au sprite.
                let o = (size / 16.0).max(1.0);
                self.quad_uv(
                    TextureRef::BlockLayer(layer),
                    x + o,
                    y + o,
                    size,
                    size,
                    [0.0, 0.0, 1.0, 1.0],
                    [0.0, 0.0, 0.0, 0.38],
                );
                self.quad_uv(
                    TextureRef::BlockLayer(layer),
                    x,
                    y,
                    size,
                    size,
                    [0.0, 0.0, 1.0, 1.0],
                    WHITE,
                );
            }
            IconStyle::Cube {
                left,
                right,
                tint_top,
                tint_side,
            } => {
                let s = size;
                let top = [x + 0.5 * s, y];
                let ur = [x + s, y + 0.25 * s];
                let lr = [x + s, y + 0.75 * s];
                let bot = [x + 0.5 * s, y + s];
                let ll = [x, y + 0.75 * s];
                let ul = [x, y + 0.25 * s];
                let c = [x + 0.5 * s, y + 0.5 * s];
                let shade = |t: [f32; 3], k: f32| [t[0] * k, t[1] * k, t[2] * k, 1.0];
                let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                self.quad_pts(
                    TextureRef::BlockLayer(layer),
                    [top, ur, c, ul],
                    uv,
                    shade(tint_top, 1.0),
                );
                self.quad_pts(
                    TextureRef::BlockLayer(left),
                    [ul, c, bot, ll],
                    uv,
                    shade(tint_side, 0.78),
                );
                self.quad_pts(
                    TextureRef::BlockLayer(right),
                    [c, ur, lr, bot],
                    uv,
                    shade(tint_side, 0.58),
                );
            }
        }
    }

    /// `ChatRenderer.renderText(text, x, y, scale, r, g, b, a)` : un quad uni par segment.
    pub fn text(&mut self, text: &str, x: f32, y: f32, scale: f32, color: Rgba) {
        if text.is_empty() {
            return;
        }
        for g in font::print(0.0, 0.0, text) {
            self.fill(
                x + g.x * scale,
                y + g.y * scale,
                g.w * scale,
                g.h * scale,
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quads_batch_by_texture_and_scissor() {
        let mut dl = DrawList::new(100, 100);
        dl.fill(0.0, 0.0, 10.0, 10.0, WHITE);
        dl.fill(5.0, 5.0, 10.0, 10.0, WHITE);
        dl.image(UiTexture::Button, 0.0, 0.0, 1.0, 1.0, WHITE);
        dl.set_scissor(Some(Scissor {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        }));
        dl.image(UiTexture::Button, 0.0, 0.0, 1.0, 1.0, WHITE);
        dl.block_icon(3, 0.0, 0.0, 16.0);
        assert_eq!(dl.quad_count(), 5);
        let b = dl.batches();
        assert_eq!(b.len(), 4);
        assert_eq!((b[0].first, b[0].count), (0, 12));
        assert_eq!(b[1].texture, TextureRef::Ui(UiTexture::Button));
        assert!(b[1].scissor.is_none() && b[2].scissor.is_some());
        assert_eq!(b[3].texture, TextureRef::BlockLayer(3));
        let total: u32 = b.iter().map(|b| b.count).sum();
        assert_eq!(total as usize, dl.vertices().len());
    }

    #[test]
    fn quad_vertices_cover_rect() {
        let mut dl = DrawList::new(10, 10);
        dl.quad_uv(
            TextureRef::Ui(UiTexture::Logo),
            1.0,
            2.0,
            3.0,
            4.0,
            [0.0, 0.0, 1.0, 1.0],
            WHITE,
        );
        let v = dl.vertices();
        assert_eq!(v[0].pos, [1.0, 2.0]);
        assert_eq!(v[2].pos, [4.0, 6.0]);
        assert_eq!(v[2].uv, [1.0, 1.0]);
        assert_eq!(v[5].pos, [4.0, 2.0]);
        assert_eq!(v[5].uv, [1.0, 0.0]);
    }

    #[test]
    fn gl_region_flips_v() {
        // Région d'inventaire Java : (0, 1 - 166/256, 176/256, 166/256) → haut de l'image.
        let mut dl = DrawList::new(10, 10);
        dl.region_gl(
            UiTexture::Inventory,
            0.0,
            0.0,
            616.0,
            581.0,
            [0.0, 1.0 - 166.0 / 256.0, 176.0 / 256.0, 166.0 / 256.0],
            WHITE,
        );
        let v = dl.vertices();
        assert!(v[0].uv[1].abs() < 1e-6);
        assert!((v[2].uv[1] - 166.0 / 256.0).abs() < 1e-6);
        assert!((v[2].uv[0] - 176.0 / 256.0).abs() < 1e-6);
    }

    #[test]
    fn text_scales_glyphs() {
        let mut dl = DrawList::new(10, 10);
        dl.text("!", 10.0, 20.0, 2.0, YELLOW);
        assert_eq!(dl.quad_count(), 2);
        // Point du « ! » : (0, 6) 1×1 → (10, 32) 2×2.
        assert_eq!(dl.vertices()[0].pos, [10.0, 32.0]);
        assert_eq!(dl.vertices()[2].pos, [12.0, 34.0]);
        assert_eq!(dl.vertices()[0].color, YELLOW);
        dl.text("", 0.0, 0.0, 1.0, WHITE);
        assert_eq!(dl.quad_count(), 2);
    }

    #[test]
    fn scissor_from_gl() {
        let s = Scissor::from_gl(0, 80, 1280, 500, 720);
        assert_eq!((s.x, s.y, s.w, s.h), (0, 140, 1280, 500));
        assert!(s.contains(10.0, 140.0) && !s.contains(10.0, 640.0));
    }
}
