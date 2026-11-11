//! Briques mathématiques du worldgen Norrcraft (GDD doc 02 §3).
//!
//! # Règles
//!
//! * **Aucune fonction transcendante** (ni `exp`, ni `sin`, ni `ln`, ni `powf`) : seules `+ − × ÷`,
//!   `sqrt`, `floor` et les comparaisons, toutes exactes selon IEEE 754. Le résultat est donc
//!   identique bit à bit sur toutes les plateformes. Les sigmoïdes sont rationnelles. Un test
//!   d'intégration (`tests/norrcraft_fields.rs`) refuse ces appels dans tout le dossier
//!   `norrcraft/`.
//! * **Fonctions pures** : rien ne dépend de l'ordre dans lequel les chunks sont demandés.
//! * Le bruit de gradient est celui de [`crate::noise::Noise`] ; chaque champ a sa propre
//!   instance, semée par `graine XOR constante_du_champ`, et son propre décalage d'origine (pour
//!   que le point (0, 0) n'ait aucun statut particulier).

use crate::noise::Noise;

// ---------------------------------------------------------------------------------------------
// Hachage entier (type SplitMix64)
// ---------------------------------------------------------------------------------------------

/// Mélangeur SplitMix64 (bijection de `u64`).
#[inline]
pub fn mix64(z: u64) -> u64 {
    let z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `h(graine, i, j, k)` : hachage 64 bits de trois entiers (cellule de grille, par exemple).
#[inline]
pub fn hash64(seed: u64, i: i32, j: i32, k: i32) -> u64 {
    let h = mix64(seed);
    let h = mix64(h ^ u64::from(i as u32));
    let h = mix64(h ^ u64::from(j as u32));
    mix64(h ^ u64::from(k as u32))
}

/// `h(graine, i, j, k)` ramené à un flottant uniforme dans [0, 1).
#[inline]
pub fn hash01(seed: u64, i: i32, j: i32, k: i32) -> f64 {
    ((hash64(seed, i, j, k) >> 11) as f64) * (1.0 / 9_007_199_254_740_992.0)
}

// ---------------------------------------------------------------------------------------------
// Utilitaires scalaires
// ---------------------------------------------------------------------------------------------

#[inline]
pub fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Sigmoïde rationnelle `s(t) = t / (1 + |t|)`, dans ]−1, 1[.
#[inline]
pub fn sigmoid(t: f64) -> f64 {
    t / (1.0 + t.abs())
}

/// Étalement `q(n; k) = 0,5 + 0,5 · s(k (2n − 1)) / s(k)` : étale un fBm concentré autour de 0,5
/// sur tout [0, 1] (`k` = 3 par défaut, voir [`spread`]). Bijection croissante de [0, 1] sur [0, 1].
#[inline]
pub fn spread_k(n: f64, k: f64) -> f64 {
    0.5 + 0.5 * sigmoid(k * (2.0 * n - 1.0)) / sigmoid(k)
}

/// [`spread_k`] avec `k` = 3.
#[inline]
pub fn spread(n: f64) -> f64 {
    spread_k(n, 3.0)
}

/// Seuil lisse `sm(a, b, v) = u² (3 − 2u)`, `u = clamp((v − a) / (b − a), 0, 1)`. Fonctionne aussi
/// quand `b < a` (seuil décroissant).
#[inline]
pub fn smooth(a: f64, b: f64, v: f64) -> f64 {
    let u = clamp01((v - a) / (b - a));
    u * u * (3.0 - 2.0 * u)
}

/// Bosse `bump(v; c, w) = max(0, 1 − ((v − c) / w)²)²`.
#[inline]
pub fn bump(v: f64, c: f64, w: f64) -> f64 {
    let t = (v - c) / w;
    let b = (1.0 - t * t).max(0.0);
    b * b
}

/// Plafond doux : pour `h > h0`, `h0 + (h − h0) · m / (m + h − h0)` (asymptote à `h0 + m`).
#[inline]
pub fn soft_cap(h: f64, h0: f64, m: f64) -> f64 {
    if h > h0 {
        let d = h - h0;
        h0 + d * m / (m + d)
    } else {
        h
    }
}

// ---------------------------------------------------------------------------------------------
// Bruit de gradient : N, fBm, crêtes, ligne de zéro, déformation de domaine
// ---------------------------------------------------------------------------------------------

/// Décalage d'une octave à la suivante (évite que les octaves s'annulent toutes aux mêmes nœuds).
const OCTAVE_SHIFT_X: f64 = 31.7;
const OCTAVE_SHIFT_Z: f64 = 17.3;

/// Une instance de bruit de gradient 2D : `N(p)` dans [0, 1] et ses compositions.
///
/// Les coordonnées passées aux méthodes sont des coordonnées **de bruit** : l'appelant divise
/// déjà par la longueur d'onde (`F(p / 2600; 5)` s'écrit `f.fbm(x / 2600.0, z / 2600.0, 5)`).
#[derive(Debug, Clone)]
pub struct GradField {
    noise: Noise,
    ox: f64,
    oz: f64,
}

impl GradField {
    /// Instance semée par `graine XOR constante` (la constante identifie le champ).
    pub fn new(seed: i64, constant: u64) -> Self {
        let s = (seed as u64) ^ constant;
        Self {
            noise: Noise::new(s as i64),
            ox: 16.0 + 224.0 * hash01(s, 1, 0, 0),
            oz: 16.0 + 224.0 * hash01(s, 2, 0, 0),
        }
    }

    /// `N(p)` : bruit de gradient dans [0, 1] (0,5 aux nœuds du réseau).
    #[inline]
    pub fn n(&self, x: f64, z: f64) -> f64 {
        clamp01((self.noise.evaluate2(x + self.ox, z + self.oz) + 1.0) * 0.5)
    }

    /// `F(p; o)` : fBm de `octaves` octaves, lacunarité 2, gain 0,5, dans [0, 1].
    #[inline]
    pub fn fbm(&self, x: f64, z: f64, octaves: u32) -> f64 {
        self.fbm_lg(x, z, octaves, 2.0, 0.5)
    }

    /// `F(p; o, ℓ, g) = Σ gⁱ N(p ℓⁱ) / Σ gⁱ`.
    pub fn fbm_lg(&self, x: f64, z: f64, octaves: u32, lacunarity: f64, gain: f64) -> f64 {
        let (mut sum, mut norm) = (0.0, 0.0);
        let (mut amp, mut freq, mut shift) = (1.0, 1.0, 0.0);
        for _ in 0..octaves {
            sum += amp
                * self.n(
                    x * freq + shift * OCTAVE_SHIFT_X,
                    z * freq + shift * OCTAVE_SHIFT_Z,
                );
            norm += amp;
            amp *= gain;
            freq *= lacunarity;
            shift += 1.0;
        }
        if norm > 0.0 {
            sum / norm
        } else {
            0.5
        }
    }

    /// Crête `Rg(p; o) = Σ gⁱ wᵢ (1 − |2 Nᵢ − 1|)²`, `w₀ = 1`, `wᵢ = clamp(2 · terme précédent, 0, 1)`
    /// (crêtes vives, vallées douces). Normalisée par `Σ gⁱ` : le résultat est dans [0, 1].
    pub fn ridged(&self, x: f64, z: f64, octaves: u32) -> f64 {
        let (mut sum, mut norm) = (0.0, 0.0);
        let (mut amp, mut freq, mut shift) = (1.0, 1.0, 0.0);
        let mut weight = 1.0;
        for _ in 0..octaves {
            let n = self.n(
                x * freq + shift * OCTAVE_SHIFT_X,
                z * freq + shift * OCTAVE_SHIFT_Z,
            );
            let r = 1.0 - (2.0 * n - 1.0).abs();
            let term = r * r;
            sum += amp * weight * term;
            norm += amp;
            weight = clamp01(2.0 * term);
            amp *= 0.5;
            freq *= 2.0;
            shift += 1.0;
        }
        if norm > 0.0 {
            sum / norm
        } else {
            0.0
        }
    }

    /// Ligne de zéro `Z(p) = |2 F(p) − 1|` : vaut 0 sur des courbes continues sinueuses (rivières,
    /// vallées, eskers, failles).
    #[inline]
    pub fn zero_line(&self, x: f64, z: f64, octaves: u32) -> f64 {
        (2.0 * self.fbm(x, z, octaves) - 1.0).abs()
    }

    /// Déformation de domaine `warp(p; A, L) = p + A · (F(p / L + o₁) − 0,5 ; F(p / L + o₂) − 0,5) · 2`.
    /// Ici `p` est en blocs. Le résultat est borné par `A` en valeur absolue sur chaque axe.
    pub fn warp(&self, x: f64, z: f64, amplitude: f64, wavelength: f64) -> (f64, f64) {
        let (u, v) = (x / wavelength, z / wavelength);
        let dx = (self.fbm(u + 5.2, v + 1.3, 3) - 0.5) * 2.0 * amplitude;
        let dz = (self.fbm(u + 9.7, v + 8.3, 3) - 0.5) * 2.0 * amplitude;
        (x + dx, z + dz)
    }
}

/// Une instance de bruit de gradient **3D** : `N₃(P)` dans [0, 1] (doc 02 §3, filons et cavités).
///
/// Comme [`GradField`], les coordonnées sont des coordonnées de bruit (l'appelant divise déjà par
/// la longueur d'onde : `N₃(P / 40)` s'écrit `f.n(x / 40.0, y / 40.0, z / 40.0)`).
#[derive(Debug, Clone)]
pub struct GradField3 {
    noise: Noise,
    o: [f64; 3],
}

impl GradField3 {
    /// Instance semée par `graine XOR constante`.
    pub fn new(seed: i64, constant: u64) -> Self {
        let s = (seed as u64) ^ constant;
        Self {
            noise: Noise::new(s as i64),
            o: [
                16.0 + 224.0 * hash01(s, 1, 0, 0),
                16.0 + 224.0 * hash01(s, 2, 0, 0),
                16.0 + 224.0 * hash01(s, 3, 0, 0),
            ],
        }
    }

    /// `N₃(P)` : bruit de gradient dans [0, 1] (0,5 aux nœuds du réseau).
    #[inline]
    pub fn n(&self, x: f64, y: f64, z: f64) -> f64 {
        clamp01(
            (self
                .noise
                .evaluate3(x + self.o[0], y + self.o[1], z + self.o[2])
                + 1.0)
                * 0.5,
        )
    }

    /// Variante « ligne de zéro » : `|2 N₃ − 1|`, nulle sur des nappes continues (filons, parois).
    #[inline]
    pub fn sheet(&self, x: f64, y: f64, z: f64) -> f64 {
        (2.0 * self.n(x, y, z) - 1.0).abs()
    }
}

// ---------------------------------------------------------------------------------------------
// Worley
// ---------------------------------------------------------------------------------------------

/// Résultat d'une évaluation de Worley.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorleyHit {
    /// Distance au point le plus proche.
    pub f1: f64,
    /// Distance au deuxième point le plus proche.
    pub f2: f64,
    /// Cellule du point le plus proche (son `id`).
    pub cell: (i32, i32),
    /// Position (en blocs) du point le plus proche.
    pub center: [f64; 2],
    /// Cellule et position du deuxième point le plus proche.
    pub cell2: (i32, i32),
    pub center2: [f64; 2],
}

/// Bruit cellulaire : une grille de pas `pitch` blocs, un point jitteré par cellule (par hachage).
#[derive(Debug, Clone, Copy)]
pub struct Worley {
    seed: u64,
    pitch: f64,
}

impl Worley {
    pub fn new(seed: i64, constant: u64, pitch: f64) -> Self {
        Self {
            seed: (seed as u64) ^ constant,
            pitch,
        }
    }

    pub fn pitch(&self) -> f64 {
        self.pitch
    }

    /// Point de la cellule `(i, j)`, en blocs. Jitter dans [0,1 ; 0,9] de la cellule : les points
    /// ne se collent jamais aux bords, donc le voisinage 3 × 3 donne F1 exact (et F2 en pratique).
    pub fn point(&self, i: i32, j: i32) -> [f64; 2] {
        let jx = 0.1 + 0.8 * hash01(self.seed, i, j, 0);
        let jz = 0.1 + 0.8 * hash01(self.seed, i, j, 1);
        [
            (f64::from(i) + jx) * self.pitch,
            (f64::from(j) + jz) * self.pitch,
        ]
    }

    /// Nombre pseudo-aléatoire de la cellule dans [0, 1) ; `salt` distingue plusieurs usages.
    pub fn cell_rand(&self, i: i32, j: i32, salt: i32) -> f64 {
        hash01(self.seed, i, j, 16 + salt)
    }

    pub fn eval(&self, x: f64, z: f64) -> WorleyHit {
        let ci = (x / self.pitch).floor() as i32;
        let cj = (z / self.pitch).floor() as i32;
        let mut best = (f64::INFINITY, (0, 0), [0.0; 2]);
        let mut second = (f64::INFINITY, (0, 0), [0.0; 2]);
        for dj in -1..=1 {
            for di in -1..=1 {
                let cell = (ci + di, cj + dj);
                let p = self.point(cell.0, cell.1);
                let (dx, dz) = (p[0] - x, p[1] - z);
                let d2 = dx * dx + dz * dz;
                if d2 < best.0 {
                    second = best;
                    best = (d2, cell, p);
                } else if d2 < second.0 {
                    second = (d2, cell, p);
                }
            }
        }
        WorleyHit {
            f1: best.0.sqrt(),
            f2: second.0.sqrt(),
            cell: best.1,
            center: best.2,
            cell2: second.1,
            center2: second.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(p: [f64; 2], x: f64, z: f64) -> f64 {
        ((p[0] - x) * (p[0] - x) + (p[1] - z) * (p[1] - z)).sqrt()
    }

    #[test]
    fn hash_is_uniform_and_deterministic() {
        assert_eq!(hash64(7, 1, 2, 3), hash64(7, 1, 2, 3));
        assert_ne!(hash64(7, 1, 2, 3), hash64(7, 2, 1, 3));
        assert_ne!(hash64(7, 1, 2, 3), hash64(8, 1, 2, 3));
        let n = 200_000;
        let (mut sum, mut min, mut max) = (0.0, 1.0f64, 0.0f64);
        let mut deciles = [0u32; 10];
        for i in 0..n {
            let h = hash01(99, i % 500 - 250, i / 500 - 200, 0);
            sum += h;
            min = min.min(h);
            max = max.max(h);
            deciles[((h * 10.0) as usize).min(9)] += 1;
        }
        assert!((sum / f64::from(n) - 0.5).abs() < 0.005);
        assert!((0.0..1.0).contains(&min) && max < 1.0);
        for d in deciles {
            assert!((d as f64 / f64::from(n) - 0.1).abs() < 0.005, "{deciles:?}");
        }
    }

    #[test]
    fn scalar_helpers_match_their_definitions() {
        assert_eq!(sigmoid(0.0), 0.0);
        assert_eq!(sigmoid(1.0), 0.5);
        assert_eq!(sigmoid(-3.0), -0.75);
        assert_eq!(spread(0.5), 0.5);
        assert_eq!(spread(0.0), 0.0);
        assert_eq!(spread(1.0), 1.0);
        assert!(
            spread(0.6) > 0.6 && spread(0.4) < 0.4,
            "q étale vers les bords"
        );
        assert_eq!(smooth(0.0, 1.0, -1.0), 0.0);
        assert_eq!(smooth(0.0, 1.0, 2.0), 1.0);
        assert_eq!(smooth(0.0, 1.0, 0.5), 0.5);
        assert_eq!(smooth(1.0, 0.0, 1.0), 0.0, "seuil décroissant");
        assert_eq!(smooth(1.0, 0.0, 0.0), 1.0);
        assert_eq!(bump(5.0, 5.0, 2.0), 1.0);
        assert_eq!(bump(7.0, 5.0, 2.0), 0.0);
        assert_eq!(bump(9.0, 5.0, 2.0), 0.0);
        assert_eq!(bump(6.0, 5.0, 2.0), 0.75 * 0.75);
        assert_eq!(soft_cap(10.0, 20.0, 5.0), 10.0);
        assert_eq!(soft_cap(25.0, 20.0, 5.0), 22.5);
        assert!(soft_cap(1.0e9, 20.0, 5.0) < 25.0, "asymptote à h0 + m");
        assert!(soft_cap(1000.0, 62.0, 22.0) < 84.0);
    }

    #[test]
    fn spread_is_monotonic() {
        let mut prev = -1.0;
        for i in 0..=1000 {
            let v = spread(f64::from(i) / 1000.0);
            assert!(v > prev || i == 0);
            prev = v;
        }
    }

    #[test]
    fn fbm_and_ridged_stay_in_unit_range_and_are_pure() {
        let f = GradField::new(12345, 0xABCD);
        for i in -50..50 {
            for j in -50..50 {
                let (x, z) = (f64::from(i) * 0.37, f64::from(j) * 0.41);
                for o in 1..=6 {
                    let v = f.fbm(x, z, o);
                    assert!((0.0..=1.0).contains(&v));
                    let r = f.ridged(x, z, o);
                    assert!((0.0..=1.0).contains(&r), "{r}");
                }
                assert!((0.0..=1.0).contains(&f.zero_line(x, z, 3)));
            }
        }
        let g = GradField::new(12345, 0xABCD);
        assert_eq!(f.fbm(3.3, -8.1, 4).to_bits(), g.fbm(3.3, -8.1, 4).to_bits());
        let other = GradField::new(12345, 0xABCE);
        assert_ne!(
            f.fbm(3.3, -8.1, 4).to_bits(),
            other.fbm(3.3, -8.1, 4).to_bits()
        );
    }

    #[test]
    fn fbm_has_no_special_origin() {
        let f = GradField::new(5, 1);
        assert_ne!(f.n(0.0, 0.0), 0.5, "décalage d'origine");
    }

    #[test]
    fn warp_displacement_is_bounded() {
        let f = GradField::new(3, 9);
        let mut max_shift = 0.0f64;
        for i in -40..40 {
            for j in -40..40 {
                let (x, z) = (f64::from(i) * 97.0, f64::from(j) * 53.0);
                let (wx, wz) = f.warp(x, z, 120.0, 700.0);
                max_shift = max_shift.max((wx - x).abs()).max((wz - z).abs());
            }
        }
        assert!(max_shift <= 120.0, "{max_shift}");
        assert!(
            max_shift > 10.0,
            "la déformation doit être sensible : {max_shift}"
        );
    }

    #[test]
    fn worley_properties() {
        let w = Worley::new(11, 0x77, 100.0);
        let mut ids = std::collections::HashSet::new();
        for i in -60..60 {
            for j in -60..60 {
                let (x, z) = (f64::from(i) * 13.7, f64::from(j) * 11.3);
                let h = w.eval(x, z);
                assert!(h.f1 <= h.f2);
                assert!(h.f1 < 100.0 * 1.5);
                ids.insert(h.cell);
                // Le point le plus proche annoncé est bien à la distance f1.
                assert!((dist(h.center, x, z) - h.f1).abs() < 1e-9);
            }
        }
        assert!(ids.len() > 50, "beaucoup de cellules touchées");
        // F1 exact : comparer à une recherche exhaustive 7 × 7.
        for k in 0..400 {
            let (x, z) = (
                f64::from(k) * 31.3 - 4000.0,
                f64::from(k % 37) * 77.7 - 1500.0,
            );
            let h = w.eval(x, z);
            let (ci, cj) = ((x / 100.0).floor() as i32, (z / 100.0).floor() as i32);
            let mut best = f64::INFINITY;
            for dj in -3..=3 {
                for di in -3..=3 {
                    let p = w.point(ci + di, cj + dj);
                    best = best.min(dist(p, x, z));
                }
            }
            assert!((best - h.f1).abs() < 1e-9);
        }
    }

    #[test]
    fn worley_is_continuous() {
        let w = Worley::new(2, 5, 200.0);
        let mut prev = w.eval(-5000.0, 321.0);
        for k in 1..20_000 {
            let x = -5000.0 + f64::from(k) * 0.5;
            let h = w.eval(x, 321.0);
            assert!((h.f1 - prev.f1).abs() <= 0.5 + 1e-9);
            assert!((h.f2 - prev.f2).abs() <= 0.5 + 1e-9);
            prev = h;
        }
    }
}
