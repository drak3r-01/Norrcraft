//! Terrain du générateur « Norrblock » : climat, relief continu et choix du biome.
//!
//! Contrairement au générateur Java (biome tiré d'abord, hauteur modulée ensuite par un facteur
//! qui change brutalement à la frontière), tout est ici une fonction **continue** de quelques
//! champs de bruit 2D ; le biome n'est que l'étiquette posée en dernier sur ces champs. Le relief
//! se raccorde donc sans falaise artificielle entre deux biomes.
//!
//! Champs : température, humidité, « étrangeté » (variante au sein d'une même zone climatique),
//! érosion (plaine ↔ relief marqué), masse montagneuse, continentalité (océan, issue de la
//! configuration), rivières (zéros du bruit), lacs, marais, mesas, dunes et volcans (cônes posés
//! sur une grille de cellules). Tout est pur : le résultat ne dépend que de
//! `(graine, configuration, x, z)`, pour que le mailleur et le générateur voient le même biome.
//!
//! Aucune fonction transcendante (`exp`, `powf`…) n'est utilisée : seules des opérations dont le
//! résultat est identique sur toutes les plateformes.

use std::sync::Arc;

use crate::biome::Biome;
use crate::config::WorldConfig;
use crate::noise::{hash_position, Noise};

/// Écarts types mesurés (voir `examples/worldmap.rs`) servant à étaler les champs de bruit
/// fBm — très concentrés autour de 0,5 — sur [0, 1] de façon quasi uniforme.
const SIG_TEMP: f64 = 0.088;
const SIG_HUMID: f64 = 0.088;
const SIG_WEIRD: f64 = 0.092;
const SIG_EROSION: f64 = 0.092;
const SIG_MOUNT: f64 = 0.098;
const SIG_MUSH: f64 = 0.11;

/// Facteur appliqué aux échelles de température / humidité de la configuration.
const CLIMATE_SCALE: f64 = 0.55;
/// Amplitude (blocs) de la déformation des coordonnées climatiques.
const CLIMATE_WARP: f64 = 90.0;

/// Taille d'une cellule de la grille des volcans (blocs).
const VOLCANO_CELL: f64 = 704.0;
const VOLCANO_CHANCE: f64 = 0.42;
/// Rayon du cratère, en fraction du rayon du volcan.
const CRATER: f64 = 0.16;

#[inline]
pub fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Approximation (sans fonction transcendante) de la fonction de répartition d'un fBm centré en
/// 0,5 d'écart type `sigma` : étale les valeurs sur [0, 1].
#[inline]
fn calib(v: f64, sigma: f64) -> f64 {
    smoothstep(0.5 - 2.2 * sigma, 0.5 + 2.2 * sigma, v)
}

#[inline]
fn unit(h: i32) -> f64 {
    f64::from(h % 10000) / 10000.0
}

/// Une colonne du monde : tout ce que le mailleur et les passes de génération ont besoin de
/// savoir sans refaire les calculs de bruit.
#[derive(Clone, Copy, Debug)]
pub struct NbColumn {
    pub biome: Biome,
    /// Premier `y` libre au-dessus du sol (les blocs `0..height` sont pleins).
    pub height: i32,
    pub temp: f64,
    pub humid: f64,
    pub weird: f64,
    /// Facteur de continentalité (0 = océan profond, 1 = terre ferme).
    pub land: f64,
    /// Facteur rivière (0 = lit de la rivière, 1 = pas de rivière).
    pub river: f64,
    /// Masse montagneuse lissée (0..1).
    pub mount: f64,
    /// Masque désert (dunes) et mesa (0..1).
    pub desert: f64,
    pub mesa: f64,
    /// Masque de marais (0..1).
    pub swamp: f64,
    /// Vrai au fond d'un lac d'eau douce.
    pub lake: bool,
    /// Distance normalisée au centre du volcan le plus proche (`> 1` : aucun).
    pub volcano_t: f64,
    /// Niveau (exclu) du lac de lave du cratère ; 0 hors cratère.
    pub lava_y: i32,
}

/// Relief avant volcan, plus les masques qui le composent.
#[derive(Clone, Copy)]
struct Shape {
    h: f64,
    temp: f64,
    humid: f64,
    weird: f64,
    land: f64,
    river: f64,
    mount: f64,
    desert: f64,
    mesa: f64,
    swamp: f64,
    lake: f64,
    mush: f64,
}

struct Volcano {
    t: f64,
    center_h: f64,
    scale: f64,
}

/// Échantillonneur de terrain, partageable entre threads.
#[derive(Debug, Clone)]
pub struct Terrain {
    noise: Arc<Noise>,
    cfg: Arc<WorldConfig>,
}

impl Terrain {
    pub fn new(noise: Arc<Noise>, cfg: Arc<WorldConfig>) -> Self {
        Self { noise, cfg }
    }

    fn is_flat(&self) -> bool {
        self.cfg.height_scale <= 2
    }

    /// Bruit brut d'océan (même formule que le Java : le préréglage ISLAND, les curseurs de
    /// l'interface et les seuils gardent leur sens).
    fn ocean_noise(&self, x: f64, z: f64) -> f64 {
        let c = &*self.cfg;
        self.noise.fbm2d(
            x * c.ocean_frequency,
            z * c.ocean_frequency,
            c.ocean_fbm_octaves,
            c.ocean_lacunarity,
            c.ocean_gain,
        )
    }

    /// Facteur rivière : 0 au lit, 1 hors des berges. Zéros de deux bruits (comme le Java) mais
    /// avec un domaine **tordu** par un bruit lent — sans cela, les zéros du bruit de Perlin à
    /// gradients limités forment de longs segments rectilignes alignés sur les axes — et une
    /// largeur modulée régionalement : certaines régions n'ont que de fines rivières.
    fn river_factor(&self, x: f64, z: f64) -> f64 {
        let c = &*self.cfg;
        if c.river_width <= 0.0 {
            return 1.0;
        }
        let n = &*self.noise;
        let wx = x + 55.0 * n.evaluate2(x * 0.011 + 17.0, z * 0.011 - 3.0);
        let wz = z + 55.0 * n.evaluate2(x * 0.011 - 41.0, z * 0.011 + 29.0);
        let regional = smoothstep(
            0.38,
            0.62,
            n.fbm2d(x * 0.0016 + 6000.0, z * 0.0016 + 70.0, 2, 2.0, 0.5),
        );
        let scale = 0.30 + 0.45 * regional;
        let width = c.river_width * scale;
        let bank = width * c.river_bank_width_mult * 1.5;
        let f = c.river_frequency;
        let r1 = n.evaluate2(wx * f + 300.0, wz * f + 300.0).abs();
        let r2 = n
            .evaluate2(wx * f * 1.5 + 700.0, wz * f * 1.5 - 200.0)
            .abs();
        // Le second réseau (affluents) n'existe que dans les régions humides en rivières.
        let m = if regional > 0.5 { r1.min(r2 * 1.2) } else { r1 };
        if m < width {
            return 0.0;
        }
        if m < bank {
            return smoothstep(width, bank, m);
        }
        1.0
    }

    /// Hauteur (réelle, avant volcan) et masques d'une colonne.
    fn shape(&self, x: f64, z: f64) -> Shape {
        let c = &*self.cfg;
        let n = &*self.noise;
        let sea = f64::from(c.sea_level);
        let base = f64::from(c.base_height);
        let sc = f64::from(c.height_scale) / 38.0;

        // Climat. Les coordonnées sont déformées par un bruit lent (frontières organiques au
        // lieu de blobs) et l'échelle est élargie : des régions de plusieurs centaines de blocs.
        let cxw = x + CLIMATE_WARP * n.evaluate2(x * 0.0055 + 1.3, z * 0.0055 + 8.1);
        let czw = z + CLIMATE_WARP * n.evaluate2(x * 0.0055 - 6.2, z * 0.0055 + 3.4);
        let ts = c.temp_scale * CLIMATE_SCALE;
        let hs = c.humid_scale * CLIMATE_SCALE;
        let temp = calib(
            n.fbm2d((cxw + 500.0) * ts, (czw + 500.0) * ts, 3, 2.0, 0.45),
            SIG_TEMP,
        );
        let humid = calib(
            n.fbm2d((cxw - 300.0) * hs, (czw + 800.0) * hs, 3, 2.0, 0.45),
            SIG_HUMID,
        );
        let weird = calib(
            n.fbm2d(cxw * 0.0030 - 777.0, czw * 0.0030 + 333.0, 3, 2.0, 0.5),
            SIG_WEIRD,
        );
        let erosion = calib(
            n.fbm2d(x * 0.0026 + 2100.0, z * 0.0026 - 1500.0, 3, 2.0, 0.5),
            SIG_EROSION,
        );
        let mush = calib(
            n.fbm2d(cxw * 0.0028 + 9100.0, czw * 0.0028 + 500.0, 2, 2.0, 0.5),
            SIG_MUSH,
        );

        // Relief de base.
        let hn = n.fbm2d(
            x * c.height_noise_frequency,
            z * c.height_noise_frequency,
            c.height_noise_octaves,
            c.height_noise_lacunarity,
            c.height_noise_gain,
        );
        let detail = hn - 0.5;
        let rough = 0.35 + 0.9 * (1.0 - erosion);

        // Coordonnée côtière unifiée : 0 au large, 1 à l'intérieur des terres. Elle commence
        // 0,14 sous le seuil d'océan pour que le plateau continental ait la même largeur quel
        // que soit le préréglage (le Java avait un fond plat partout où le bruit < seuil).
        let on = self.ocean_noise(x, z);
        let lo = c.ocean_threshold - 0.10;
        let hi = c.coast_threshold.max(c.ocean_threshold + 0.05) + 0.05;
        let of = ((on - lo) / (hi - lo)).clamp(0.0, 1.0);

        if self.is_flat() {
            let flat_h = base + if hn > 0.56 { 1.0 } else { 0.0 };
            return Shape {
                h: flat_h,
                temp,
                humid,
                weird,
                land: 1.0,
                river: 1.0,
                mount: 0.0,
                desert: smoothstep(0.2, 0.55, desert_raw(temp, humid)),
                mesa: 0.0,
                swamp: 0.0,
                lake: 0.0,
                mush: 0.0,
            };
        }

        // Montagnes : part du monde réglée par `mountain_threshold`.
        let mu = calib(
            n.fbm2d(
                x * c.mountain_frequency * 0.75 + 1000.0,
                z * c.mountain_frequency * 0.75 + 1000.0,
                4,
                2.0,
                0.5,
            ),
            SIG_MOUNT,
        );
        let share = ((1.0 - c.mountain_threshold) * 0.7).clamp(0.04, 0.6);
        let a = 1.0 - share;
        let m_land = smoothstep(a - 0.10, a + 0.12, mu);
        // Les montagnes ne se forment pas au large.
        let w = smoothstep(0.35, 0.85, of);
        let m = m_land * smoothstep(0.7, 0.95, w);

        let r1 = 1.0 - n.evaluate2(x * 0.012 + 55.0, z * 0.012 - 35.0).abs();
        let r2 = 1.0 - n.evaluate2(x * 0.027 - 400.0, z * 0.027 + 90.0).abs();
        let peaks = r1 * r1 * 0.75 + r2 * r2 * r2 * 0.25;

        // Terres : plaine ondulée, érosion modulant l'amplitude, montée vers l'intérieur.
        let inland = smoothstep(hi, hi + 0.18, on);
        let mut land_h = base + sc * (10.0 + 38.0 * detail * rough + 6.0 * inland);
        land_h += sc * (12.0 * m + 58.0 * m * m * peaks);
        // Collines arrondies dans les zones de moyenne montagne.
        let hill = smoothstep(0.1, 0.45, m_land) * (1.0 - smoothstep(0.55, 0.9, m_land));
        land_h += sc * 9.0 * hill * (n.evaluate2(x * 0.02 + 12.0, z * 0.02 - 70.0) + 0.4);

        // Désert (dunes) et mesa (étages).
        let desert = smoothstep(0.2, 0.55, desert_raw(temp, humid)) * (1.0 - m);
        let mesa = desert * smoothstep(0.5, 0.62, weird);
        let dunes = 1.0 - n.evaluate2(x * 0.021 + 140.0, z * 0.013 - 60.0).abs();
        land_h += sc.min(1.5) * (1.0 - mesa) * desert * 7.0 * dunes * dunes;
        if mesa > 0.0 {
            let plateau = sc.min(1.5) * (8.0 + 16.0 * smoothstep(0.35, 0.8, 1.0 - erosion));
            let raised = land_h + plateau * mesa;
            let step = 6.0;
            let s = (raised - sea) / step;
            let f = s.floor();
            let terrace = sea + (f + smoothstep(0.55, 0.9, s - f)) * step;
            land_h = lerp(raised, terrace, smoothstep(0.1, 0.5, mesa));
        }

        // Marais : très plat, autour du niveau de la mer.
        let tmid = smoothstep(0.34, 0.44, temp) * (1.0 - smoothstep(0.56, 0.66, temp));
        let swamp = smoothstep(0.64, 0.76, humid) * tmid * smoothstep(0.7, 0.95, w) * (1.0 - m);
        if swamp > 0.0 {
            let marsh = sea + (hn - 0.5) * 9.0 - 0.2;
            land_h = lerp(land_h, marsh, 0.9 * swamp);
        }

        // Fond marin : plateau continental puis abysses.
        let s = 1.0 - of;
        let ocean_h = sea - 3.0 - 24.0 * (s * (1.6 - 0.6 * s)) + detail * 8.0;
        let mut h = lerp(ocean_h, land_h, w);

        // Rivières : creusées sur la terre, estompées en montagne.
        let rf0 = self.river_factor(x, z);
        let rf = rf0 + (1.0 - rf0) * smoothstep(0.2, 0.6, m);
        if rf < 1.0 && h >= sea - 2.0 {
            let carve = f64::from(c.river_depth) * (1.0 - rf);
            let bed = sea - carve.floor();
            h = h.min(bed + (h - bed) * rf);
        }

        // Lacs d'eau douce : cuvettes sur les basses terres.
        let ln = n.fbm2d(x * 0.018 + 4000.0, z * 0.018 - 900.0, 3, 2.0, 0.5);
        let lake_w = smoothstep(0.60, 0.68, ln) * w * (1.0 - smoothstep(0.2, 0.5, m));
        let fade = 1.0 - smoothstep(sea + 4.0, sea + 12.0, h);
        if lake_w > 0.0 && fade > 0.0 {
            let bowl = sea - 2.0 - 5.0 * lake_w;
            if h > bowl {
                h = lerp(h, bowl, lake_w * fade);
            }
        }

        Shape {
            h,
            temp,
            humid,
            weird,
            land: w,
            river: rf,
            mount: m_land,
            desert,
            mesa,
            swamp,
            lake: lake_w * fade,
            mush,
        }
    }

    /// Volcan couvrant `(x, z)`, s'il y en a un.
    fn volcano_at(&self, x: f64, z: f64) -> Option<Volcano> {
        if self.is_flat() {
            return None;
        }
        let sea = f64::from(self.cfg.sea_level);
        let cx0 = (x / VOLCANO_CELL).floor() as i32;
        let cz0 = (z / VOLCANO_CELL).floor() as i32;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let (cx, cz) = (cx0 + dx, cz0 + dz);
                let h1 = hash_position(
                    cx.wrapping_mul(31).wrapping_add(7),
                    cz.wrapping_mul(17).wrapping_add(13),
                );
                if unit(h1) >= VOLCANO_CHANCE {
                    continue;
                }
                let h2 = hash_position(h1 ^ 0x5bd1, cz.wrapping_add(cx.wrapping_mul(3)));
                let px = f64::from(cx) * VOLCANO_CELL + 120.0 + f64::from(h1 / 7 % 460);
                let pz = f64::from(cz) * VOLCANO_CELL + 120.0 + f64::from(h2 % 460);
                let r = 80.0 + f64::from(h2 / 11 % 45);
                let d = ((x - px) * (x - px) + (z - pz) * (z - pz)).sqrt();
                if d >= r {
                    continue;
                }
                let center = self.shape(px, pz);
                if center.h < sea + 4.0 || center.land < 0.95 || center.mount > 0.5 {
                    continue;
                }
                let scale = (f64::from(self.cfg.height_scale) / 38.0)
                    .sqrt()
                    .clamp(0.5, 1.5);
                return Some(Volcano {
                    t: d / r,
                    center_h: center.h,
                    scale,
                });
            }
        }
        None
    }

    /// Profil du cône (hauteur au-dessus du sol du centre) hors cratère.
    fn cone(t: f64, scale: f64) -> f64 {
        let u = 1.0 - t;
        54.0 * scale * u * u.sqrt()
    }

    /// Colonne complète : hauteur, masques et biome.
    pub fn column(&self, wx: i32, wz: i32) -> NbColumn {
        let (x, z) = (f64::from(wx), f64::from(wz));
        let c = &*self.cfg;
        let sea = c.sea_level;
        let sh = self.shape(x, z);
        let mut h = sh.h;
        let mut volcano_t = 9.0;
        let mut lava_y = 0;
        if let Some(v) = self.volcano_at(x, z) {
            let profile = if v.t < CRATER {
                let depth = 16.0 * v.scale;
                Self::cone(CRATER, v.scale) - (1.0 - (v.t / CRATER) * (v.t / CRATER)) * depth
            } else {
                Self::cone(v.t, v.scale)
            };
            let w = 1.0 - smoothstep(0.5, 1.0, v.t);
            h = lerp(h, v.center_h + profile, w);
            // Frontière irrégulière pour la nature du sol (biome, cendres) : le cône, lui,
            // reste parfaitement radial.
            volcano_t = v.t
                + 0.14 * self.noise.evaluate2(x * 0.045 + 210.0, z * 0.045 - 330.0)
                + 0.06 * self.noise.evaluate2(x * 0.11 - 20.0, z * 0.11 + 70.0);
            if v.t < CRATER {
                let floor_h = v.center_h + Self::cone(CRATER, v.scale) - 16.0 * v.scale;
                lava_y = floor_h.floor() as i32 + 5;
            }
        }
        let height = (h.floor() as i32).clamp(1, crate::chunk::CHUNK_SIZE_Y as i32 - 2);
        let lake = sh.lake > 0.5 && height < sea - 1;
        let biome = self.pick_biome(&sh, height, lake, volcano_t);
        NbColumn {
            biome,
            height,
            temp: sh.temp,
            humid: sh.humid,
            weird: sh.weird,
            land: sh.land,
            river: sh.river,
            mount: sh.mount,
            desert: sh.desert,
            mesa: sh.mesa,
            swamp: sh.swamp,
            lake,
            volcano_t,
            lava_y,
        }
    }

    fn pick_biome(&self, s: &Shape, height: i32, lake: bool, volcano_t: f64) -> Biome {
        let c = &*self.cfg;
        let sea = c.sea_level;
        let t = s.temp;
        let hm = s.humid;
        let w = s.weird;
        if self.is_flat() {
            return land_biome(s, t, hm, w);
        }
        // Rivières.
        if s.river < 0.3 && height <= sea + 1 && s.land > 0.8 && !lake {
            return Biome::River;
        }
        // Sous l'eau : lac (biome terrestre) ou océan.
        if height < sea - 1 {
            if lake {
                return land_biome(s, t, hm, w);
            }
            if t < 0.15 {
                return Biome::FrozenOcean;
            }
            if t > 0.66 && height >= sea - 14 {
                return Biome::WarmOcean;
            }
            if height < sea - 16 {
                return Biome::DeepOcean;
            }
            return Biome::Ocean;
        }
        if volcano_t < 0.5 {
            return Biome::Volcano;
        }
        let m = s.mount;
        let t_eff = t - f64::from((height - sea - 18).max(0)) * 0.0035;
        // Plages et rivages rocheux.
        if height <= sea + 3 && s.land < 0.985 && m < 0.5 {
            return if m > 0.15 {
                Biome::StonyShore
            } else {
                Biome::Beach
            };
        }
        if m >= 0.5 {
            if t_eff < 0.2 || height >= c.snow_line_mountains + 34 {
                return Biome::SnowyPeaks;
            }
            return Biome::Mountains;
        }
        if s.mesa > 0.5 {
            return Biome::Badlands;
        }
        if m >= 0.22 && t_eff > 0.15 {
            return Biome::Hills;
        }
        if s.mush > 0.993 && s.land > 0.95 {
            return Biome::MushroomFields;
        }
        land_biome(s, t, hm, w)
    }

    /// Niveau de sol (premier `y` libre) seul : utile aux recherches de point d'apparition.
    pub fn height(&self, wx: i32, wz: i32) -> i32 {
        self.column(wx, wz).height
    }
}

fn desert_raw(temp: f64, humid: f64) -> f64 {
    ((temp - 0.60) / 0.2).clamp(0.0, 1.0) * ((0.42 - humid) / 0.2).clamp(0.0, 1.0)
}

/// Biome terrestre selon la zone climatique et la variante (« étrangeté »).
fn land_biome(s: &Shape, t: f64, hm: f64, w: f64) -> Biome {
    if s.desert > 0.5 {
        return Biome::Desert;
    }
    if s.swamp > 0.5 {
        return Biome::Swamp;
    }
    if t < 0.12 {
        if hm < 0.45 {
            return if w > 0.8 {
                Biome::IceSpikes
            } else {
                Biome::SnowyPlains
            };
        }
        return Biome::SnowyTaiga;
    }
    if t < 0.27 {
        if hm < 0.30 {
            return Biome::Heath;
        }
        if hm >= 0.62 && w > 0.5 {
            return Biome::MegaTaiga;
        }
        return Biome::Taiga;
    }
    if t < 0.60 {
        if hm < 0.27 {
            return if w > 0.62 {
                Biome::FlowerMeadow
            } else {
                Biome::Plains
            };
        }
        if hm < 0.46 {
            return if w < 0.5 {
                Biome::Forest
            } else {
                Biome::BirchForest
            };
        }
        return if w < 0.30 {
            Biome::DarkForest
        } else if w < 0.58 {
            Biome::Forest
        } else if w < 0.78 {
            Biome::AutumnForest
        } else {
            Biome::CherryGrove
        };
    }
    if t < 0.78 {
        return if hm < 0.68 {
            Biome::Savanna
        } else {
            Biome::Jungle
        };
    }
    if hm < 0.62 {
        Biome::Savanna
    } else {
        Biome::Jungle
    }
}
