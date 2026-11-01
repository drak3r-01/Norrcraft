//! Bruit de Perlin « improved » (port exact de `Noise.java`) et hachage entier du générateur.
//!
//! Toutes les expressions flottantes suivent l'ordre d'évaluation du Java (pas de FMA, pas de
//! réordonnancement) pour garantir l'égalité bit à bit.

use nb_core::JavaRandom;

/// Table de permutation dérivée de la graine (spec §4).
#[derive(Debug, Clone)]
pub struct Noise {
    p: [u8; 512],
}

#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

#[inline]
fn grad2(hash: u8, x: f64, y: f64) -> f64 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        0.0
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

#[inline]
fn grad3(hash: u8, x: f64, y: f64, z: f64) -> f64 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

/// `(int) Math.floor(v) & 255`.
#[inline]
fn lattice(v: f64) -> usize {
    ((v.floor() as i32) & 255) as usize
}

impl Noise {
    /// `new Noise(seed)` : 255 tirages `nextInt(i + 1)` pour `i` = 255 → 1.
    pub fn new(seed: i64) -> Self {
        let mut perm = [0u8; 256];
        for (i, v) in perm.iter_mut().enumerate() {
            *v = i as u8;
        }
        let mut rnd = JavaRandom::new(seed);
        for i in (1..=255usize).rev() {
            let index = rnd.next_int_bound(i as i32 + 1) as usize;
            perm.swap(index, i);
        }
        let mut p = [0u8; 512];
        p[..256].copy_from_slice(&perm);
        p[256..].copy_from_slice(&perm);
        Self { p }
    }

    /// Les 256 premières entrées de la table (pour les tests).
    pub fn permutation(&self) -> &[u8] {
        &self.p[..256]
    }

    #[inline]
    fn p(&self, i: usize) -> usize {
        usize::from(self.p[i])
    }

    /// `evaluate(x, y)` — bruit 2D dans ≈ [-1, 1].
    pub fn evaluate2(&self, x: f64, y: f64) -> f64 {
        let xi = lattice(x);
        let yi = lattice(y);
        let x = x - x.floor();
        let y = y - y.floor();
        let u = fade(x);
        let v = fade(y);
        let a = self.p(xi) + yi;
        let b = self.p(xi + 1) + yi;
        lerp(
            v,
            lerp(u, grad2(self.p[a], x, y), grad2(self.p[b], x - 1.0, y)),
            lerp(
                u,
                grad2(self.p[a + 1], x, y - 1.0),
                grad2(self.p[b + 1], x - 1.0, y - 1.0),
            ),
        )
    }

    /// `evaluate(x, y, z)` — bruit 3D dans ≈ [-1, 1].
    pub fn evaluate3(&self, x: f64, y: f64, z: f64) -> f64 {
        let xi = lattice(x);
        let yi = lattice(y);
        let zi = lattice(z);
        let x = x - x.floor();
        let y = y - y.floor();
        let z = z - z.floor();
        let u = fade(x);
        let v = fade(y);
        let w = fade(z);
        let a = self.p(xi) + yi;
        let aa = self.p(a) + zi;
        let ab = self.p(a + 1) + zi;
        let b = self.p(xi + 1) + yi;
        let ba = self.p(b) + zi;
        let bb = self.p(b + 1) + zi;
        let p = &self.p;
        lerp(
            w,
            lerp(
                v,
                lerp(u, grad3(p[aa], x, y, z), grad3(p[ba], x - 1.0, y, z)),
                lerp(
                    u,
                    grad3(p[ab], x, y - 1.0, z),
                    grad3(p[bb], x - 1.0, y - 1.0, z),
                ),
            ),
            lerp(
                v,
                lerp(
                    u,
                    grad3(p[aa + 1], x, y, z - 1.0),
                    grad3(p[ba + 1], x - 1.0, y, z - 1.0),
                ),
                lerp(
                    u,
                    grad3(p[ab + 1], x, y - 1.0, z - 1.0),
                    grad3(p[bb + 1], x - 1.0, y - 1.0, z - 1.0),
                ),
            ),
        )
    }

    /// fBm 2D, résultat renormalisé dans [0, 1].
    pub fn fbm2d(&self, x: f64, y: f64, octaves: i32, lacunarity: f64, gain: f64) -> f64 {
        let mut sum = 0.0;
        let mut amp = 1.0;
        let mut freq = 1.0;
        let mut max_amp = 0.0;
        for _ in 0..octaves {
            sum += self.evaluate2(x * freq, y * freq) * amp;
            max_amp += amp;
            amp *= gain;
            freq *= lacunarity;
        }
        (sum / max_amp + 1.0) * 0.5
    }

    /// fBm 3D (gain 0,5 et lacunarité 2,0 fixes), résultat dans [0, 1].
    pub fn fbm3d(&self, x: f64, y: f64, z: f64, octaves: i32) -> f64 {
        let mut sum = 0.0;
        let mut amp = 1.0;
        let mut freq = 1.0;
        let mut max_amp = 0.0;
        for _ in 0..octaves {
            sum += self.evaluate3(x * freq, y * freq, z * freq) * amp;
            max_amp += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        (sum / max_amp + 1.0) * 0.5
    }

    /// `octaveEvaluate` (inutilisé par le Java, conservé pour l'API).
    pub fn octave_evaluate(
        &self,
        x: f64,
        y: f64,
        octaves: i32,
        persistence: f64,
        lacunarity: f64,
    ) -> f64 {
        self.fbm2d(x, y, octaves, lacunarity, persistence)
    }
}

/// `WorldGenerator.hashPosition` : hachage entier enveloppant, toujours ≥ 0.
#[inline]
pub fn hash_position(x: i32, z: i32) -> i32 {
    let h = x
        .wrapping_mul(374_761_393)
        .wrapping_add(z.wrapping_mul(668_265_263))
        .wrapping_add(1_234_567);
    let h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    (h ^ (h >> 16)) & 0x7FFF_FFFF
}

/// `String.hashCode()` Java (unités UTF-16, `int` enveloppant) — utilisé pour les graines textuelles.
pub fn java_string_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permutation_seed_12345() {
        let n = Noise::new(12345);
        assert_eq!(
            &n.permutation()[..16],
            &[200, 42, 20, 121, 182, 178, 7, 39, 194, 118, 164, 160, 251, 166, 254, 53]
        );
    }

    #[test]
    fn noise_reference_values() {
        let n = Noise::new(12345);
        assert_eq!(n.evaluate2(0.5, 0.5).to_bits(), 0.0f64.to_bits());
        assert_eq!(n.evaluate2(12.3, -4.7), -0.11912406912000098);
        assert_eq!(n.evaluate3(1.1, 2.2, 3.3), 0.17309989174425583);
        assert_eq!(n.evaluate3(-7.25, 0.5, 100.75), -0.18932104110717773);
        assert_eq!(n.fbm2d(1.37, -2.11, 5, 2.0, 0.45), 0.49202660249155644);
        assert_eq!(n.fbm3d(3.5, 1.25, -9.75, 1), 0.5923972129821777);
    }

    #[test]
    fn lattice_points_are_zero() {
        let n = Noise::new(7);
        assert_eq!(n.evaluate2(3.0, -5.0), 0.0);
        assert_eq!(n.evaluate3(3.0, -5.0, 10.0), 0.0);
    }

    #[test]
    fn hash_reference_values() {
        assert_eq!(hash_position(0, 0), 991465577);
        assert_eq!(hash_position(1, 0), 1329105385);
        assert_eq!(hash_position(0, 1), 212133164);
        assert_eq!(hash_position(-1, -1), 753772374);
        assert_eq!(hash_position(123456, -654321), 112537465);
        assert_eq!(hash_position(2147483647, -2147483648), 835488806);
    }

    #[test]
    fn string_hash() {
        assert_eq!(java_string_hash("Norrblock"), -1910315700);
        assert_eq!(java_string_hash(""), 0);
    }
}
