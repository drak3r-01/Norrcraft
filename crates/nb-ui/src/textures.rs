//! Textures de l'UI (`UITextureLoader`) : PNG intégrés (ressources « classpath » Java),
//! `dirt.png` du pack Default, blanc 1×1 et icônes procédurales de vie / faim.
//!
//! Toutes les images sont en RGBA8 **à l'endroit** (ligne 0 = haut), l'espace UV des
//! [`DrawList`](crate::DrawList).

use image::RgbaImage;
use nb_core::{Error, Result};

macro_rules! asset {
    ($p:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $p))
    };
}

/// Identifiant d'une texture UI ; la valeur numérique est l'index dans [`load_all`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum UiTexture {
    White,
    Crosshair,
    Hotbar,
    Selector,
    Inventory,
    Button,
    Logo,
    Dirt,
    HeartFull,
    HeartHalf,
    HeartEmpty,
    HungerFull,
    HungerHalf,
    HungerEmpty,
    Workbench,
    Forge,
}

impl UiTexture {
    pub const ALL: [UiTexture; 16] = [
        UiTexture::White,
        UiTexture::Crosshair,
        UiTexture::Hotbar,
        UiTexture::Selector,
        UiTexture::Inventory,
        UiTexture::Button,
        UiTexture::Logo,
        UiTexture::Dirt,
        UiTexture::HeartFull,
        UiTexture::HeartHalf,
        UiTexture::HeartEmpty,
        UiTexture::HungerFull,
        UiTexture::HungerHalf,
        UiTexture::HungerEmpty,
        UiTexture::Workbench,
        UiTexture::Forge,
    ];

    pub fn index(self) -> u32 {
        self as u32
    }

    /// PNG intégré (`None` pour les textures générées).
    pub fn png(self) -> Option<&'static [u8]> {
        Some(match self {
            UiTexture::Crosshair => asset!("assets/textures/hud/crosshair.png"),
            UiTexture::Hotbar => asset!("assets/textures/hud/hotbar.png"),
            UiTexture::Selector => asset!("assets/textures/hud/hotbar_selector.png"),
            UiTexture::Inventory => asset!("assets/textures/hud/inventory.png"),
            UiTexture::Workbench => asset!("assets/textures/hud/workbench.png"),
            UiTexture::Forge => asset!("assets/textures/hud/forge.png"),
            UiTexture::Button => asset!("assets/textures/ui/button.png"),
            UiTexture::Logo => asset!("assets/textures/ui/logo.png"),
            // `defaultPackBlockPath("dirt.png")` : toujours le pack Default.
            UiTexture::Dirt => {
                asset!("resource_packs/Default/assets/minecraft/textures/block/dirt.png")
            }
            _ => return None,
        })
    }
}

#[rustfmt::skip]
const HEART_MASK: [u8; 256] = [
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,1,1,0,0,0,0,0,0,1,1,0,0,0,
    0,0,1,1,1,1,0,0,0,0,1,1,1,1,0,0,
    0,1,1,1,1,1,1,1,1,1,1,1,1,1,1,0,
    0,1,1,1,1,1,1,1,1,1,1,1,1,1,1,0,
    0,1,1,1,1,1,1,1,1,1,1,1,1,1,1,0,
    0,0,1,1,1,1,1,1,1,1,1,1,1,1,0,0,
    0,0,0,1,1,1,1,1,1,1,1,1,1,0,0,0,
    0,0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,
    0,0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,
    0,0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,
    0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

#[rustfmt::skip]
const FOOD_MASK: [u8; 256] = [
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,
    0,0,0,0,0,1,1,1,1,1,1,1,0,0,0,0,
    0,0,0,0,1,1,1,1,1,1,1,1,1,0,0,0,
    0,0,0,0,1,1,1,1,1,1,1,1,1,0,0,0,
    0,0,0,0,1,1,1,1,1,1,1,1,1,0,0,0,
    0,0,0,0,0,1,1,1,1,1,1,1,0,0,0,0,
    0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,
    0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,
    0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,0,
    0,0,0,0,1,1,0,0,0,0,0,0,0,0,0,0,
    0,0,0,1,1,0,0,0,0,0,0,0,0,0,0,0,
    0,0,1,1,1,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

const HEART: [u8; 3] = [210, 30, 30];
const EMPTY: [u8; 3] = [75, 75, 75];
const FOOD: [u8; 3] = [185, 115, 40];

/// `buildIconTexture` : masque 16×16, bord (voisin hors masque ou bord d'image) assombri à
/// 0,55 (troncature `(byte)(c × 0.55f)`), colonnes 8..15 en `empty` si `split`.
/// Java remplit le tampon GL de bas en haut (ligne 15 d'abord) : l'image à l'endroit a donc la
/// ligne 0 du masque en haut.
fn icon(mask: &[u8; 256], full: [u8; 3], empty: [u8; 3], split: bool) -> RgbaImage {
    const S: usize = 16;
    let mut img = RgbaImage::new(16, 16);
    for row in 0..S {
        for col in 0..S {
            if mask[row * S + col] == 0 {
                continue;
            }
            let edge = row == 0
                || mask[(row - 1) * S + col] == 0
                || row == S - 1
                || mask[(row + 1) * S + col] == 0
                || col == 0
                || mask[row * S + col - 1] == 0
                || col == S - 1
                || mask[row * S + col + 1] == 0;
            let c = if split && col >= S / 2 { empty } else { full };
            let shade = if edge { 0.55f32 } else { 1.0 };
            let px = c.map(|v| (v as f32 * shade) as u8);
            img.put_pixel(
                col as u32,
                row as u32,
                image::Rgba([px[0], px[1], px[2], 255]),
            );
        }
    }
    img
}

/// Image RGBA8 à l'endroit d'une texture UI.
pub fn load(tex: UiTexture) -> Result<RgbaImage> {
    Ok(match tex {
        UiTexture::White => RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])),
        UiTexture::HeartFull => icon(&HEART_MASK, HEART, HEART, false),
        UiTexture::HeartHalf => icon(&HEART_MASK, HEART, EMPTY, true),
        UiTexture::HeartEmpty => icon(&HEART_MASK, EMPTY, EMPTY, false),
        UiTexture::HungerFull => icon(&FOOD_MASK, FOOD, FOOD, false),
        UiTexture::HungerHalf => icon(&FOOD_MASK, FOOD, EMPTY, true),
        UiTexture::HungerEmpty => icon(&FOOD_MASK, EMPTY, EMPTY, false),
        _ => {
            let bytes = tex.png().expect("texture PNG");
            image::load_from_memory(bytes)
                .map_err(|e| Error::data(format!("{tex:?}.png"), e))?
                .to_rgba8()
        }
    })
}

/// Toutes les textures, dans l'ordre de [`UiTexture::ALL`] (index = [`UiTexture::index`]).
pub fn load_all() -> Result<Vec<RgbaImage>> {
    UiTexture::ALL.iter().map(|&t| load(t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_follow_all() {
        for (i, t) in UiTexture::ALL.iter().enumerate() {
            assert_eq!(t.index() as usize, i);
        }
    }

    #[test]
    fn all_textures_decode() {
        let all = load_all().unwrap();
        assert_eq!(all.len(), UiTexture::ALL.len());
        assert_eq!(all[0].dimensions(), (1, 1));
        let inv = &all[UiTexture::Inventory.index() as usize];
        assert!(inv.width() >= 176 && inv.height() >= 166);
    }

    #[test]
    fn heart_icons() {
        let full = load(UiTexture::HeartFull).unwrap();
        // Bosse gauche ligne 1 colonnes 3-4 : bord → 210 × 0,55 = 115.
        assert_eq!(full.get_pixel(3, 1).0, [115, 16, 16, 255]);
        // Intérieur (ligne 4, colonne 5).
        assert_eq!(full.get_pixel(5, 4).0, [210, 30, 30, 255]);
        // Hors masque.
        assert_eq!(full.get_pixel(0, 0).0[3], 0);
        let half = load(UiTexture::HeartHalf).unwrap();
        assert_eq!(half.get_pixel(5, 4).0, [210, 30, 30, 255]);
        assert_eq!(half.get_pixel(10, 4).0, [75, 75, 75, 255]);
        let food = load(UiTexture::HungerEmpty).unwrap();
        assert_eq!(food.get_pixel(7, 4).0, [75, 75, 75, 255]);
    }
}
