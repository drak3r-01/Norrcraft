//! Port exact de `java.util.Random` (LCG 48 bits).
//!
//! Indispensable pour qu'une même graine produise le même monde que la version Java.

const MULTIPLIER: i64 = 0x5DEECE66D;
const ADDEND: i64 = 0xB;
const MASK: i64 = (1_i64 << 48) - 1;
const DOUBLE_UNIT: f64 = 1.0 / (1_u64 << 53) as f64;
const FLOAT_UNIT: f32 = 1.0 / (1_u32 << 24) as f32;

#[derive(Debug, Clone)]
pub struct JavaRandom {
    seed: i64,
    next_next_gaussian: f64,
    have_next_next_gaussian: bool,
}

impl JavaRandom {
    /// `new Random(seed)`.
    pub fn new(seed: i64) -> Self {
        Self {
            seed: (seed ^ MULTIPLIER) & MASK,
            next_next_gaussian: 0.0,
            have_next_next_gaussian: false,
        }
    }

    /// `setSeed(seed)`.
    pub fn set_seed(&mut self, seed: i64) {
        self.seed = (seed ^ MULTIPLIER) & MASK;
        self.have_next_next_gaussian = false;
    }

    /// `protected int next(int bits)`.
    #[inline]
    fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & MASK;
        // `(int)(seed >>> (48 - bits))` : la graine est positive (48 bits), `>>` suffit.
        (self.seed >> (48 - bits)) as i32
    }

    /// `nextInt()`.
    pub fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    /// `nextInt(bound)` ; panique si `bound <= 0` (Java lève `IllegalArgumentException`).
    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        let mut r = self.next(31);
        let m = bound - 1;
        if bound & m == 0 {
            // puissance de deux
            ((i64::from(bound) * i64::from(r)) >> 31) as i32
        } else {
            let mut u = r;
            loop {
                r = u % bound;
                if u.wrapping_sub(r).wrapping_add(m) >= 0 {
                    break;
                }
                u = self.next(31);
            }
            r
        }
    }

    /// `nextLong()`.
    pub fn next_long(&mut self) -> i64 {
        (i64::from(self.next(32)) << 32).wrapping_add(i64::from(self.next(32)))
    }

    /// `nextBoolean()`.
    pub fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }

    /// `nextFloat()`.
    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 * FLOAT_UNIT
    }

    /// `nextDouble()`.
    pub fn next_double(&mut self) -> f64 {
        let hi = i64::from(self.next(26)) << 27;
        let lo = i64::from(self.next(27));
        (hi + lo) as f64 * DOUBLE_UNIT
    }

    /// `nextGaussian()` (méthode polaire, comme `java.util.Random`).
    ///
    /// Java utilise `StrictMath.log/sqrt` ; `f64::ln` peut différer d'un ulp selon la
    /// plate-forme. Le Java ne s'en sert pas pour la génération du monde.
    pub fn next_gaussian(&mut self) -> f64 {
        if self.have_next_next_gaussian {
            self.have_next_next_gaussian = false;
            return self.next_next_gaussian;
        }
        loop {
            let v1 = 2.0 * self.next_double() - 1.0;
            let v2 = 2.0 * self.next_double() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s < 1.0 && s != 0.0 {
                let multiplier = (-2.0 * s.ln() / s).sqrt();
                self.next_next_gaussian = v2 * multiplier;
                self.have_next_next_gaussian = true;
                return v1 * multiplier;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Valeurs de référence du JDK : `new Random(42)`.
    #[test]
    fn seed_42_next_int() {
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int(), -1170105035);
        assert_eq!(r.next_int(), 234785527);
    }

    #[test]
    fn seed_42_bounded() {
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int_bound(10), 0);
        assert_eq!(r.next_int_bound(10), 3);
        assert_eq!(r.next_int_bound(10), 8);
    }

    #[test]
    fn seed_0_next_long_and_double() {
        let mut r = JavaRandom::new(0);
        assert_eq!(r.next_long(), -4962768465676381896);
        let mut r = JavaRandom::new(0);
        assert!((r.next_double() - 0.730_967_787_376_657).abs() < 1e-15);
    }

    #[test]
    fn bounded_stays_in_range() {
        let mut r = JavaRandom::new(7);
        for bound in [1, 2, 3, 16, 100, 256] {
            for _ in 0..200 {
                assert!((0..bound).contains(&r.next_int_bound(bound)));
            }
        }
    }
}
