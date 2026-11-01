//! Lumière locale recalculée par le mesher (spec §5) : volume 20×130×20 couvrant le chunk et
//! 2 blocs de bordure, passe colonne puis au plus 14 balayages Gauss-Seidel en place.

use crate::block::BlockTable;
use crate::neighborhood::{BlockCache, Neighborhood};

/// Dimensions du volume (`[lx+2][y+1][lz+2]`).
pub const LIGHT_W: usize = 20;
pub const LIGHT_H: usize = 130;
pub const LIGHT_D: usize = 20;
/// Nombre maximal de balayages de diffusion.
pub const LIGHT_MAX_ITERS: u32 = 14;

/// Carte de lumière (`lightMap`) et d'atténuation (`attenuationMap`) du mesher.
#[derive(Clone)]
pub struct LightVolume {
    light: Vec<i32>,
    atten: Vec<u8>,
}

impl std::fmt::Debug for LightVolume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LightVolume(..)")
    }
}

#[inline]
const fn idx(ax: usize, ay: usize, az: usize) -> usize {
    (ax * LIGHT_H + ay) * LIGHT_D + az
}

impl LightVolume {
    /// Calcule la lumière du voisinage (§5.2 puis §5.3, ordre exact).
    pub fn compute(n: &Neighborhood, blocks: &BlockTable) -> Self {
        Self::compute_cached(&BlockCache::new(*n, blocks))
    }

    pub(crate) fn compute_cached(cache: &BlockCache) -> Self {
        let table = cache.table();
        let mut light = vec![0i32; LIGHT_W * LIGHT_H * LIGHT_D];
        let mut atten = vec![0u8; LIGHT_W * LIGHT_H * LIGHT_D];

        // Passe colonne (ciel descendant).
        for lx in -2..=17i32 {
            for lz in -2..=17i32 {
                let mut cur: i32 = 15;
                for y in (-1..=128i32).rev() {
                    let mut a: i32 = 1;
                    let mut emission: i32 = 0;
                    if (0..128).contains(&y) {
                        let b = table.get(cache.get(lx, y, lz));
                        emission = b.light_emission;
                        a = if b.is_light_blocking() {
                            15
                        } else {
                            b.light_attenuation.max(1)
                        };
                    } else if y < 0 {
                        a = 15;
                    }
                    let i = idx((lx + 2) as usize, (y + 1) as usize, (lz + 2) as usize);
                    // `(byte) atten` : relu comme byte signé lors de la diffusion.
                    atten[i] = a as u8;
                    if y < 128 {
                        if a == 1 && cur == 15 {
                            cur = 15;
                        } else {
                            cur = (cur - a).max(0);
                        }
                        cur = cur.max(emission);
                    }
                    light[i] = cur;
                }
            }
        }

        // Diffusion Gauss-Seidel en place, ordre x → y → z.
        let mut changed = true;
        let mut iters = LIGHT_MAX_ITERS;
        while changed && iters > 0 {
            iters -= 1;
            changed = false;
            for x in 0..LIGHT_W {
                for y in 0..LIGHT_H {
                    for z in 0..LIGHT_D {
                        let i = idx(x, y, z);
                        let a = i32::from(atten[i] as i8);
                        if a >= 15 {
                            continue;
                        }
                        let prev = light[i];
                        let mut m = prev;
                        if x > 0 {
                            m = m.max(light[idx(x - 1, y, z)] - a);
                        }
                        if x < LIGHT_W - 1 {
                            m = m.max(light[idx(x + 1, y, z)] - a);
                        }
                        if y > 0 {
                            m = m.max(light[idx(x, y - 1, z)] - a);
                        }
                        if y < LIGHT_H - 1 {
                            let up = light[idx(x, y + 1, z)];
                            if up == 15 && a == 1 {
                                m = m.max(15);
                            } else {
                                m = m.max(up - a);
                            }
                        }
                        if z > 0 {
                            m = m.max(light[idx(x, y, z - 1)] - a);
                        }
                        if z < LIGHT_D - 1 {
                            m = m.max(light[idx(x, y, z + 1)] - a);
                        }
                        if m > prev {
                            light[i] = m;
                            changed = true;
                        }
                    }
                }
            }
        }

        Self { light, atten }
    }

    /// Valeur brute aux indices du tableau (`lightMap[ax][ay][az]`).
    #[inline]
    pub fn raw(&self, ax: usize, ay: usize, az: usize) -> i32 {
        self.light[idx(ax, ay, az)]
    }

    /// Atténuation brute aux indices du tableau.
    #[inline]
    pub fn attenuation(&self, ax: usize, ay: usize, az: usize) -> u8 {
        self.atten[idx(ax, ay, az)]
    }

    /// Lumière de la cellule locale `(lx, y, lz)` (doit être dans le volume).
    #[inline]
    pub fn at(&self, lx: i32, y: i32, lz: i32) -> i32 {
        self.raw((lx + 2) as usize, (y + 1) as usize, (lz + 2) as usize)
    }

    /// `getL` : lecture clampée aux bornes du volume.
    #[inline]
    pub fn get_clamped(&self, lx: i32, y: i32, lz: i32) -> i32 {
        let ax = (lx + 2).clamp(0, LIGHT_W as i32 - 1) as usize;
        let ay = (y + 1).clamp(0, LIGHT_H as i32 - 1) as usize;
        let az = (lz + 2).clamp(0, LIGHT_D as i32 - 1) as usize;
        self.raw(ax, ay, az)
    }

    /// Empreinte FNV-1a 64 du volume (tests de non-régression).
    pub fn fingerprint(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &v in &self.light {
            for b in v.to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// `getLightScale(l) = max(0.08f, l / 15.0f)`.
#[inline]
pub fn light_scale(l: i32) -> f32 {
    f32::max(0.08, l as f32 / 15.0)
}
