//! Briques de traitement du signal : générateur pseudo-aléatoire, filtres, enveloppes, réverbération.

use std::f32::consts::TAU;

/// xorshift64* : suffisant pour du bruit audio, déterministe et sans allocation.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniforme dans `[0, 1)`.
    #[inline]
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Bruit blanc dans `[-1, 1)`.
    #[inline]
    pub fn noise(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    /// Uniforme dans `[a, b)`.
    #[inline]
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.unit()
    }

    /// Entier dans `[0, n)` (`n > 0`).
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

/// Filtre passe-bas à un pôle.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    y: f32,
}

impl OnePole {
    /// Coefficient pour une fréquence de coupure `fc` (Hz).
    #[inline]
    pub fn coef(fc: f32, rate: f32) -> f32 {
        1.0 - (-TAU * fc.clamp(10.0, rate * 0.45) / rate).exp()
    }

    #[inline]
    pub fn lp(&mut self, x: f32, a: f32) -> f32 {
        self.y += a * (x - self.y);
        self.y
    }

    /// Passe-haut complémentaire.
    #[inline]
    pub fn hp(&mut self, x: f32, a: f32) -> f32 {
        x - self.lp(x, a)
    }
}

/// Filtre biquad (formes RBJ).
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn from_raw(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Passe-bande à gain de crête constant, de fréquence centrale `f` et de facteur `q`.
    pub fn bandpass(f: f32, q: f32, rate: f32) -> Self {
        let w = TAU * f.clamp(20.0, rate * 0.45) / rate;
        let alpha = w.sin() / (2.0 * q.max(0.1));
        Self::from_raw(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * w.cos(), 1.0 - alpha)
    }

    pub fn lowpass(f: f32, q: f32, rate: f32) -> Self {
        let w = TAU * f.clamp(20.0, rate * 0.45) / rate;
        let alpha = w.sin() / (2.0 * q.max(0.1));
        let c = w.cos();
        Self::from_raw(
            (1.0 - c) / 2.0,
            1.0 - c,
            (1.0 - c) / 2.0,
            1.0 + alpha,
            -2.0 * c,
            1.0 - alpha,
        )
    }

    pub fn highpass(f: f32, q: f32, rate: f32) -> Self {
        let w = TAU * f.clamp(20.0, rate * 0.45) / rate;
        let alpha = w.sin() / (2.0 * q.max(0.1));
        let c = w.cos();
        Self::from_raw(
            (1.0 + c) / 2.0,
            -(1.0 + c),
            (1.0 + c) / 2.0,
            1.0 + alpha,
            -2.0 * c,
            1.0 - alpha,
        )
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Lissage exponentiel d'une valeur vers sa cible.
#[derive(Clone, Copy, Debug)]
pub struct Smooth {
    pub value: f32,
    pub target: f32,
}

impl Smooth {
    pub fn new(v: f32) -> Self {
        Self {
            value: v,
            target: v,
        }
    }

    /// Avance de `dt` secondes avec la constante de temps `tau`.
    #[inline]
    pub fn step(&mut self, dt: f32, tau: f32) -> f32 {
        let k = 1.0 - (-dt / tau.max(1e-4)).exp();
        self.value += (self.target - self.value) * k;
        self.value
    }
}

/// Convertit un numéro de note MIDI en fréquence.
#[inline]
pub fn midi_to_hz(n: f32) -> f32 {
    440.0 * ((n - 69.0) / 12.0).exp2()
}

/// Saturation douce : garde le mixage dans `[-1, 1]` sans écrêtage dur.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a < 0.6 {
        x
    } else {
        x.signum() * (0.6 + 0.4 * ((a - 0.6) / 0.4).tanh())
    }
}

/// Peigne à rétroaction avec amortissement (réverbération de Schroeder/Moorer).
#[derive(Clone, Debug)]
struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn new(n: usize) -> Self {
        Self {
            buf: vec![0.0; n.max(1)],
            i: 0,
            store: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.i];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * feedback;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        out
    }
}

#[derive(Clone, Debug)]
struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn new(n: usize) -> Self {
        Self {
            buf: vec![0.0; n.max(1)],
            i: 0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        let out = b - x;
        self.buf[self.i] = x + b * 0.5;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        out
    }
}

/// Réverbération stéréo (4 peignes + 2 passe-tout par canal), allouée une fois.
#[derive(Clone, Debug)]
pub struct Reverb {
    combs: [[Comb; 4]; 2],
    allpass: [[Allpass; 2]; 2],
    /// Durée de la queue : 0 (court) … 1 (long).
    pub size: f32,
    pub damp: f32,
}

impl Reverb {
    pub fn new(rate: f32) -> Self {
        let k = rate / 44_100.0;
        let tune = |n: usize, spread: usize| (((n + spread) as f32) * k) as usize;
        let mk = |spread: usize| {
            [
                Comb::new(tune(1116, spread)),
                Comb::new(tune(1188, spread)),
                Comb::new(tune(1277, spread)),
                Comb::new(tune(1356, spread)),
            ]
        };
        let ap = |spread: usize| {
            [
                Allpass::new(tune(556, spread)),
                Allpass::new(tune(441, spread)),
            ]
        };
        Self {
            combs: [mk(0), mk(23)],
            allpass: [ap(0), ap(23)],
            size: 0.55,
            damp: 0.35,
        }
    }

    /// Traite un échantillon mono d'entrée, renvoie `(gauche, droite)` entièrement mouillés.
    #[inline]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        let feedback = 0.70 + 0.27 * self.size.clamp(0.0, 1.0);
        let input = x * 0.015;
        let mut out = [0.0f32; 2];
        for (ch, o) in out.iter_mut().enumerate() {
            let mut acc = 0.0;
            for c in &mut self.combs[ch] {
                acc += c.process(input, feedback, self.damp);
            }
            for a in &mut self.allpass[ch] {
                acc = a.process(acc);
            }
            *o = acc;
        }
        (out[0], out[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_in_range_and_deterministic() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..1000 {
            let x = a.unit();
            assert!((0.0..1.0).contains(&x));
            assert_eq!(x, b.unit());
        }
    }

    #[test]
    fn lowpass_attenuates_highs() {
        let rate = 44_100.0;
        let mut f = Biquad::lowpass(500.0, 0.7, rate);
        let mut peak = 0.0f32;
        for n in 0..4000 {
            let x = (TAU * 8000.0 * n as f32 / rate).sin();
            let y = f.process(x).abs();
            if n > 500 {
                peak = peak.max(y);
            }
        }
        assert!(peak < 0.1, "peak {peak}");
    }

    #[test]
    fn reverb_decays() {
        let mut r = Reverb::new(44_100.0);
        let _ = r.process(1.0);
        let mut early = 0.0;
        let mut late = 0.0;
        for n in 0..220_500 {
            let (l, _) = r.process(0.0);
            if n < 22_050 {
                early += l * l;
            } else if n >= 198_450 {
                late += l * l;
            }
        }
        assert!(early > 0.0 && late < early);
    }

    #[test]
    fn soft_clip_bounded() {
        for i in -100..100 {
            assert!(soft_clip(i as f32).abs() <= 1.0 + 1e-6);
        }
    }
}
