//! Tableaux de textures CPU (« atlas » de blocs et de particules, spec §5.1, §5.4) et textures
//! auxiliaires (nuages, bruit, damier). Pur CPU : l'upload Vulkan est dans `vk::texture`.

use std::collections::HashMap;
use std::path::Path;

use image::imageops::{self, FilterType};
use image::RgbaImage;
use nb_core::JavaRandom;

/// Magenta de la couche manquante / de repli.
pub const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// Décode un PNG en RGBA8 **retourné verticalement** (`stbi_set_flip_vertically_on_load`).
pub fn decode_rgba_flipped(bytes: &[u8]) -> Option<RgbaImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(imageops::flip_vertical(&img))
}

/// Tableau de textures CPU : `layers[i]` = tuile RGBA8 `tile × tile` (déjà retournée).
#[derive(Clone, Debug, PartialEq)]
pub struct TextureArrayCpu {
    pub tile: u32,
    pub layers: Vec<Vec<u8>>,
    /// Clé (minuscules) → couche.
    pub names: HashMap<String, u32>,
    pub missing_layer: u32,
    /// Faux pour la texture de repli 1×1 (NEAREST/NEAREST, sans mip).
    pub mipmapped: bool,
}

impl TextureArrayCpu {
    /// Texture de repli : 1×1×1 magenta, couche manquante 0 (§5.1 étape 2).
    pub fn fallback() -> Self {
        Self {
            tile: 1,
            layers: vec![MAGENTA.to_vec()],
            names: HashMap::new(),
            missing_layer: 0,
            mipmapped: false,
        }
    }

    /// `TextureRegistry.loadDirectory` (§5.1). Fichiers triés par nom (ordinal en majuscules).
    pub fn from_dir(dir: &Path) -> Self {
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| n.to_lowercase().ends_with(".png"))
                    .collect()
            })
            .unwrap_or_default();
        if files.is_empty() {
            return Self::fallback();
        }
        files.sort_by_key(|n| n.to_uppercase());
        let entries: Vec<(String, Option<RgbaImage>)> = files
            .iter()
            .map(|n| {
                // `replace(".png", "")` sensible à la casse puis `toLowerCase` ([JAVA-BUG]).
                let key = n.replace(".png", "").to_lowercase();
                let img = std::fs::read(dir.join(n))
                    .ok()
                    .and_then(|b| decode_rgba_flipped(&b));
                (key, img)
            })
            .collect();
        Self::build(entries)
    }

    /// `loadFromClasspath` (§5.4) : liste ordonnée de noms ; `None` = ressource absente.
    pub fn from_named(files: &[(&str, Option<&[u8]>)]) -> Self {
        let entries = files
            .iter()
            .map(|(n, b)| (n.to_lowercase(), b.and_then(decode_rgba_flipped)))
            .collect();
        Self::build(entries)
    }

    fn build(entries: Vec<(String, Option<RgbaImage>)>) -> Self {
        let tile = entries
            .iter()
            .filter_map(|(_, i)| i.as_ref().map(|i| i.width()))
            .max()
            .unwrap_or(16);
        let mut names = HashMap::new();
        let mut layers = Vec::with_capacity(entries.len() + 1);
        for (i, (key, img)) in entries.into_iter().enumerate() {
            match img {
                Some(img) => {
                    layers.push(fit_tile(&img, tile));
                    names.insert(key, i as u32);
                }
                None => layers.push(vec![0; (tile * tile * 4) as usize]),
            }
        }
        let missing_layer = layers.len() as u32;
        layers.push(checkerboard(tile, (tile / 4).max(1)));
        Self {
            tile,
            layers,
            names,
            missing_layer,
            mipmapped: true,
        }
    }

    /// `getLayer(nom)` : insensible à la casse, absent → couche manquante.
    pub fn layer(&self, name: &str) -> u32 {
        self.names
            .get(&name.to_lowercase())
            .copied()
            .unwrap_or(self.missing_layer)
    }

    pub fn layer_count(&self) -> u32 {
        self.layers.len() as u32
    }

    /// Nombre de niveaux de mip (chaîne complète si [`Self::mipmapped`]).
    pub fn mip_levels(&self) -> u32 {
        if self.mipmapped {
            mip_level_count(self.tile, self.tile)
        } else {
            1
        }
    }
}

/// Copie / redimensionnement d'une image retournée vers une tuile `tile × tile` (§5.1 étape 6).
fn fit_tile(img: &RgbaImage, tile: u32) -> Vec<u8> {
    let (w, h) = img.dimensions();
    if w == tile && h >= tile {
        // Premières lignes du buffer retourné = bas du PNG (dernière frame d'une bande animée).
        return img.as_raw()[..(tile * tile * 4) as usize].to_vec();
    }
    let src_h = h.min(w);
    let src = imageops::crop_imm(img, 0, 0, w, src_h).to_image();
    // [DÉVIATION] sous-pixel acceptée : Catmull-Rom dans les deux sens (stb : Mitchell en
    // réduction).
    imageops::resize(&src, tile, tile, FilterType::CatmullRom).into_raw()
}

/// Damier magenta/noir `size × size`, cases de `cell` texels (magenta si `(x/c + y/c)` pair).
pub fn checkerboard(size: u32, cell: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            out.extend_from_slice(if (x / cell + y / cell) % 2 == 0 {
                &MAGENTA
            } else {
                &BLACK
            });
        }
    }
    out
}

/// `floor(log2(max(w, h))) + 1`.
pub fn mip_level_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// Chaîne de mips RGBA8 par moyenne de boîtes 2×2 (équivalent `glGenerateMipmap`,
/// [DÉVIATION] mineure acceptée §5.1). `out[0]` = niveau 0 (copie).
pub fn mip_chain(level0: &[u8], w: u32, h: u32) -> Vec<(u32, u32, Vec<u8>)> {
    let mut out = vec![(w, h, level0.to_vec())];
    let (mut cw, mut ch) = (w, h);
    while cw > 1 || ch > 1 {
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let prev = &out.last().expect("niveau précédent").2;
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let sx = [(2 * x).min(cw - 1), (2 * x + 1).min(cw - 1)];
                let sy = [(2 * y).min(ch - 1), (2 * y + 1).min(ch - 1)];
                for c in 0..4 {
                    let mut sum = 0u32;
                    for yy in sy {
                        for xx in sx {
                            sum += u32::from(prev[((yy * cw + xx) * 4 + c) as usize]);
                        }
                    }
                    next[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
                }
            }
        }
        out.push((nw, nh, next));
        cw = nw;
        ch = nh;
    }
    out
}

/// Texture de bruit 256×256 R8 (§4) : octets `floor(random·255)`.
/// [DÉVIATION] graine fixe (Java : `Math.random()`, non déterministe) pour des rendus
/// reproductibles.
pub fn noise_texture(seed: i64) -> Vec<u8> {
    let mut rng = JavaRandom::new(seed);
    (0..256 * 256)
        .map(|_| (rng.next_double() * 255.0) as u8)
        .collect()
}

/// Graine de la texture de bruit.
pub const NOISE_SEED: i64 = 0x4E42_4E4F;

/// Texture de nuages RGBA8 retournée (`clouds.png`, §4, §5.3). `None` si illisible.
pub fn cloud_texture(bytes: &[u8]) -> Option<RgbaImage> {
    decode_rgba_flipped(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn default_blocks() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../resource_packs/Default/assets/minecraft/textures/block")
    }

    fn png(img: &RgbaImage) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn default_pack_atlas() {
        let a = TextureArrayCpu::from_dir(&default_blocks());
        assert_eq!(a.tile, 32);
        assert_eq!(a.layer_count(), 46);
        assert_eq!(a.missing_layer, 45);
        assert_eq!(a.layer("STONE"), a.layer("stone"));
        assert_ne!(a.layer("stone"), a.missing_layer);
        assert_eq!(a.layer("nope"), a.missing_layer);
        assert_eq!(a.mip_levels(), 6);
        // Ordre trié : acacia_leaves en premier.
        assert_eq!(a.layer("acacia_leaves"), 0);
        // water_still (32×1024) : la couche est la dernière frame (bas) du PNG.
        let raw = std::fs::read(default_blocks().join("water_still.png")).unwrap();
        let png_img = image::load_from_memory(&raw).unwrap().to_rgba8();
        let layer = &a.layers[a.layer("water_still") as usize];
        let h = png_img.height();
        for y in 0..32u32 {
            for x in 0..32u32 {
                let expect = png_img.get_pixel(x, h - 1 - y).0;
                let i = ((y * 32 + x) * 4) as usize;
                assert_eq!(&layer[i..i + 4], &expect);
            }
        }
        // Couche manquante : damier de cases de 8.
        let m = &a.layers[45];
        assert_eq!(&m[0..4], &MAGENTA);
        assert_eq!(&m[8 * 4..8 * 4 + 4], &BLACK);
    }

    #[test]
    fn empty_dir_gives_fallback() {
        let a = TextureArrayCpu::from_dir(Path::new("/definitely/not/here"));
        assert_eq!(a, TextureArrayCpu::fallback());
        assert_eq!(a.mip_levels(), 1);
        assert_eq!(a.layer("stone"), 0);
    }

    #[test]
    fn named_particles_and_failures() {
        let leaf = RgbaImage::from_pixel(10, 10, image::Rgba([10, 200, 10, 255]));
        let lava = RgbaImage::from_pixel(16, 16, image::Rgba([250, 60, 0, 255]));
        let (lb, vb) = (png(&leaf), png(&lava));
        let a = TextureArrayCpu::from_named(&[
            ("leaf", Some(&lb)),
            ("lava_drop", Some(&vb)),
            ("broken", Some(b"pas un png")),
        ]);
        assert_eq!(a.tile, 16);
        assert_eq!(a.layer("leaf"), 0);
        assert_eq!(a.layer("lava_drop"), 1);
        assert_eq!(a.layer("broken"), 3);
        assert_eq!(a.missing_layer, 3);
        assert!(a.layers[2].iter().all(|&b| b == 0));
        // Agrandissement 10 → 16 d'une couleur unie : reste unie.
        assert!(a.layers[0].chunks(4).all(|p| p == [10, 200, 10, 255]));
    }

    #[test]
    fn uppercase_extension_key_bug() {
        let dir = std::env::temp_dir().join(format!("nbgfx-atlas-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let img = RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]));
        std::fs::write(dir.join("Foo.PNG"), png(&img)).unwrap();
        let a = TextureArrayCpu::from_dir(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(a.layer("foo.png"), 0);
        assert_eq!(a.layer("foo"), a.missing_layer);
    }

    #[test]
    fn mips_average_boxes() {
        let mut l0 = vec![0u8; 4 * 4 * 4];
        for (i, p) in l0.chunks_mut(4).enumerate() {
            p.copy_from_slice(&[if i % 2 == 0 { 255 } else { 0 }, 0, 0, 255]);
        }
        let chain = mip_chain(&l0, 4, 4);
        assert_eq!(chain.len(), 3);
        assert_eq!(mip_level_count(4, 4), 3);
        assert_eq!(mip_level_count(32, 32), 6);
        assert_eq!((chain[1].0, chain[1].1), (2, 2));
        assert!(chain[1].2.chunks(4).all(|p| p == [128, 0, 0, 255]));
        assert_eq!(chain[2].2, vec![128, 0, 0, 255]);
    }

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let a = noise_texture(NOISE_SEED);
        assert_eq!(a.len(), 65536);
        assert_eq!(a, noise_texture(NOISE_SEED));
        assert!(a.iter().all(|&b| b < 255));
        let mean = a.iter().map(|&b| f64::from(b)).sum::<f64>() / 65536.0;
        assert!((mean - 127.0).abs() < 2.0);
    }
}
