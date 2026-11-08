//! Atlas des particules : les deux PNG historiques (`leaf`, `lava_drop`) et les sprites
//! générés par le code (halo, étincelle, fumée, goutte, flocon, bulle, flamme, filet de pluie,
//! pixel). Les noms et l'ordre viennent de `nb_particles::SPRITE_NAMES`.
//!
//! Les sprites générés sont dessinés orientés « haut = ligne 0 » (le quad des particules lit
//! `v = 0` en haut). Aucun fichier à fournir : tout est calculé au chargement (≈ 1 ms).

use image::{Rgba, RgbaImage};
use nb_particles::SPRITE_NAMES;

use crate::atlas::{decode_rgba_flipped, TextureArrayCpu};

/// Côté des sprites générés (le PNG `leaf` 10×10 et `lava_drop` 16×16 sont agrandis).
pub const SPRITE_TILE: u32 = 32;

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bruit de valeur déterministe (hachage entier).
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(0x27d4_eb2d)
        .wrapping_add((y as u32).wrapping_mul(0x1656_67b1))
        .wrapping_add(seed.wrapping_mul(0x9e37_79b9));
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    (h & 0xffff) as f32 / 65535.0
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (smooth(0.0, 1.0, x - xi), smooth(0.0, 1.0, y - yi));
    let (xi, yi) = (xi as i32, yi as i32);
    let a = hash(xi, yi, seed);
    let b = hash(xi + 1, yi, seed);
    let c = hash(xi, yi + 1, seed);
    let d = hash(xi + 1, yi + 1, seed);
    (a + (b - a) * fx) + ((c + (d - c) * fx) - (a + (b - a) * fx)) * fy
}

/// Distance d'un point au segment `a..b`.
fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
    ((p.0 - a.0 - abx * t).powi(2) + (p.1 - a.1 - aby * t).powi(2)).sqrt()
}

fn paint(f: impl Fn(f32, f32) -> [f32; 4]) -> RgbaImage {
    let n = SPRITE_TILE;
    let mut img = RgbaImage::new(n, n);
    for py in 0..n {
        for px in 0..n {
            // Coordonnées centrées -1..1, y vers le BAS (ligne 0 = haut).
            let x = (px as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let y = (py as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let c = f(x, y);
            let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            img.put_pixel(px, py, Rgba([b(c[0]), b(c[1]), b(c[2]), b(c[3])]));
        }
    }
    img
}

fn soft() -> RgbaImage {
    paint(|x, y| {
        let r = (x * x + y * y).sqrt();
        let a = smooth(1.0, 0.0, r).powf(1.6);
        [1.0, 1.0, 1.0, a]
    })
}

fn spark() -> RgbaImage {
    paint(|x, y| {
        let d = x.abs().sqrt() + y.abs().sqrt();
        let star = smooth(1.0, 0.0, d);
        let r = (x * x + y * y).sqrt();
        let core = smooth(0.4, 0.0, r);
        let a = (star * 0.9 + core).min(1.0);
        [1.0, 1.0, 1.0, a]
    })
}

fn smoke() -> RgbaImage {
    paint(|x, y| {
        let r = (x * x + y * y).sqrt();
        let n = value_noise((x + 2.0) * 3.0, (y + 2.0) * 3.0, 3) * 0.65
            + value_noise((x + 2.0) * 7.0, (y + 2.0) * 7.0, 9) * 0.35;
        let a = smooth(1.0, 0.1, r + (n - 0.5) * 0.55).powf(1.2) * (0.5 + 0.5 * n);
        let shade = 0.78 + 0.22 * n;
        [shade, shade, shade, a.min(1.0)]
    })
}

fn drop() -> RgbaImage {
    paint(|x, y| {
        // Corps : disque en bas ; pointe effilée vers le haut.
        let body = ((x / 0.42).powi(2) + ((y - 0.35) / 0.5).powi(2)).sqrt();
        let tip = if (-0.95..0.2).contains(&y) {
            (x.abs() / (0.42 * ((y + 0.95) / 1.15))).max(0.0)
        } else {
            9.0
        };
        let d = body.min(tip);
        let a = smooth(1.05, 0.8, d);
        let hi = smooth(0.35, 0.0, ((x + 0.15).powi(2) + (y - 0.2).powi(2)).sqrt()) * 0.5;
        [0.8 + hi, 0.9 + hi * 0.5, 1.0, a]
    })
}

fn snow() -> RgbaImage {
    paint(|x, y| {
        let mut d: f32 = 9.0;
        for k in 0..3 {
            let a = k as f32 * std::f32::consts::PI / 3.0;
            let (s, c) = a.sin_cos();
            d = d.min(seg_dist((x, y), (-c * 0.92, -s * 0.92), (c * 0.92, s * 0.92)));
        }
        // Petites barbes sur chaque branche.
        for k in 0..6 {
            let a = k as f32 * std::f32::consts::PI / 3.0;
            let (s, c) = a.sin_cos();
            let m = (c * 0.55, s * 0.55);
            for sgn in [-1.0f32, 1.0] {
                let b = a + sgn * 0.9;
                let (bs, bc) = b.sin_cos();
                d = d.min(seg_dist((x, y), m, (m.0 + bc * 0.28, m.1 + bs * 0.28)));
            }
        }
        let a = smooth(0.17, 0.05, d);
        [1.0, 1.0, 1.0, a]
    })
}

fn bubble() -> RgbaImage {
    paint(|x, y| {
        let r = (x * x + y * y).sqrt();
        let ring = smooth(0.2, 0.0, (r - 0.78).abs());
        let fill = smooth(0.8, 0.0, r) * 0.12;
        let hl = smooth(0.3, 0.0, ((x + 0.38).powi(2) + (y + 0.4).powi(2)).sqrt()) * 0.9;
        [1.0, 1.0, 1.0, (ring * 0.85 + fill + hl).min(1.0)]
    })
}

fn flame() -> RgbaImage {
    paint(|x, y| {
        // Pointe en haut (y = −1), ventre en bas.
        let t = (y + 1.0) * 0.5;
        let w = 0.78 * (std::f32::consts::PI * t.powf(0.8)).sin().max(0.0);
        let d = if w > 0.0 { x.abs() / w } else { 9.0 };
        let a = smooth(1.0, 0.45, d) * smooth(0.0, 0.12, t);
        let core = smooth(0.7, 0.0, d) * smooth(0.25, 0.8, t);
        [1.0, 0.55 + 0.45 * core, 0.1 + 0.7 * core, a]
    })
}

fn streak() -> RgbaImage {
    paint(|x, y| {
        let a = (1.0 - x.abs() * 3.2).clamp(0.0, 1.0).powi(2) * smooth(-1.0, -0.3, y) * smooth(1.0, 0.85, y);
        [1.0, 1.0, 1.0, a]
    })
}

fn pixel() -> RgbaImage {
    RgbaImage::from_pixel(SPRITE_TILE, SPRITE_TILE, Rgba([255; 4]))
}

/// Atlas complet, dans l'ordre de `SPRITE_NAMES`.
pub fn build_atlas(leaf_png: &[u8], lava_drop_png: &[u8]) -> TextureArrayCpu {
    let entries = SPRITE_NAMES
        .iter()
        .map(|&name| {
            let img = match name {
                "leaf" => decode_rgba_flipped(leaf_png),
                "lava_drop" => decode_rgba_flipped(lava_drop_png),
                "soft" => Some(soft()),
                "spark" => Some(spark()),
                "smoke" => Some(smoke()),
                "drop" => Some(drop()),
                "snow" => Some(snow()),
                "bubble" => Some(bubble()),
                "flame" => Some(flame()),
                "streak" => Some(streak()),
                "pixel" => Some(pixel()),
                other => unreachable!("sprite inconnu {other}"),
            };
            (name.to_string(), img)
        })
        .collect();
    TextureArrayCpu::from_entries(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_layers_follow_sprite_names() {
        let a = build_atlas(
            include_bytes!("../../../assets/textures/particle/leaf.png"),
            include_bytes!("../../../assets/textures/particle/lava_drop.png"),
        );
        for (i, n) in SPRITE_NAMES.iter().enumerate() {
            assert_eq!(a.layer(n), i as u32, "{n}");
        }
        assert_eq!(a.tile, SPRITE_TILE);
    }

    #[test]
    fn sprites_have_visible_pixels_and_transparent_corners() {
        for (n, img) in [
            ("soft", soft()),
            ("spark", spark()),
            ("smoke", smoke()),
            ("drop", drop()),
            ("snow", snow()),
            ("bubble", bubble()),
            ("flame", flame()),
            ("streak", streak()),
        ] {
            let visible = img.pixels().filter(|p| p.0[3] > 64).count();
            assert!(visible > 12, "{n}: {visible} pixels visibles");
            assert!(img.get_pixel(0, 0).0[3] < 8, "{n}: coin opaque");
        }
    }
}
