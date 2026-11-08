//! Synthèse procédurale des effets sonores (placeholders libres de droits : rien n'est échantillonné).
//!
//! Chaque son est un `Vec<f32>` mono normalisé en crête ; la variante `v` change la graine et
//! quelques paramètres pour éviter l'effet « mitraillette » des pas et des casses de blocs.
//! Un vrai fichier `.wav` posé dans le dossier des sons remplace n'importe lequel (voir
//! [`crate::bank`]).

use std::f32::consts::TAU;

use crate::dsp::{Biquad, Rng};
use crate::sfx::{Material, MobCall, Sfx, Voice};

/// Fréquence d'échantillonnage de la banque et du mixeur.
pub const RATE: f32 = 44_100.0;

fn samples(seconds: f32) -> usize {
    (seconds * RATE).max(1.0) as usize
}

/// Normalise la crête à `peak` et applique un fondu de sortie de 5 ms (pas de clic final).
fn finish(mut s: Vec<f32>, peak: f32) -> Vec<f32> {
    let m = s.iter().fold(0.0f32, |a, &x| a.max(x.abs()));
    if m > 1e-6 {
        let g = peak / m;
        for x in &mut s {
            *x *= g;
        }
    }
    let fade = samples(0.005).min(s.len());
    let n = s.len();
    for i in 0..fade {
        s[n - 1 - i] *= i as f32 / fade as f32;
    }
    s
}

/// Ajoute `src` dans `dst` à partir de `at` (en échantillons), avec gain.
fn mix_into(dst: &mut [f32], src: &[f32], at: usize, gain: f32) {
    for (i, &x) in src.iter().enumerate() {
        if let Some(d) = dst.get_mut(at + i) {
            *d += x * gain;
        }
    }
}

/// Rafale de bruit filtré (passe-haut `hp`, passe-bas `lp`) à décroissance exponentielle.
fn noise_burst(rng: &mut Rng, secs: f32, hp: f32, lp: f32, decay: f32, attack: f32) -> Vec<f32> {
    let n = samples(secs);
    let mut h = Biquad::highpass(hp, 0.7, RATE);
    let mut l = Biquad::lowpass(lp, 0.8, RATE);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let env = (-t / decay.max(1e-3)).exp() * (t / attack.max(1e-4)).min(1.0);
            l.process(h.process(rng.noise())) * env
        })
        .collect()
}

/// Sinus à décroissance exponentielle, avec glissando de `f0` vers `f1` sur la durée.
fn tone(secs: f32, f0: f32, f1: f32, decay: f32, harmonics: &[f32]) -> Vec<f32> {
    let n = samples(secs);
    let mut ph = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let k = i as f32 / n as f32;
            let f = f0 + (f1 - f0) * k;
            ph += TAU * f / RATE;
            let mut s = ph.sin();
            for (h, &a) in harmonics.iter().enumerate() {
                s += a * (ph * (h + 2) as f32).sin();
            }
            s * (-t / decay.max(1e-3)).exp() * (t / 0.002).min(1.0)
        })
        .collect()
}

/// Profil sonore d'un matériau.
struct Profile {
    lp: f32,
    hp: f32,
    decay: f32,
    tone: f32,
    tone_mix: f32,
    tone_decay: f32,
    /// Nombre de petits claquements (froissement) ; 0 = bruit continu.
    grains: u32,
    /// Résonance métallique / vitreuse inharmonique.
    ring: f32,
}

fn profile(m: Material) -> Profile {
    let p = |lp, hp, decay, tone, tone_mix, tone_decay, grains, ring| Profile {
        lp,
        hp,
        decay,
        tone,
        tone_mix,
        tone_decay,
        grains,
        ring,
    };
    match m {
        Material::Stone => p(5200.0, 180.0, 0.07, 210.0, 0.55, 0.07, 0, 0.0),
        Material::Dirt => p(1700.0, 70.0, 0.10, 95.0, 0.7, 0.07, 0, 0.0),
        Material::Grass => p(3800.0, 500.0, 0.09, 0.0, 0.0, 0.0, 7, 0.0),
        Material::Gravel => p(6000.0, 350.0, 0.12, 120.0, 0.25, 0.05, 16, 0.0),
        Material::Sand => p(6800.0, 1400.0, 0.12, 0.0, 0.0, 0.0, 0, 0.0),
        Material::Snow => p(5800.0, 1100.0, 0.10, 0.0, 0.0, 0.0, 9, 0.0),
        Material::Wood => p(2600.0, 160.0, 0.07, 330.0, 0.9, 0.08, 0, 0.0),
        Material::Leaves => p(8500.0, 2400.0, 0.09, 0.0, 0.0, 0.0, 11, 0.0),
        Material::Plant => p(6500.0, 1700.0, 0.06, 0.0, 0.0, 0.0, 5, 0.0),
        Material::Glass => p(12000.0, 1800.0, 0.05, 2700.0, 0.7, 0.22, 3, 1.0),
        Material::Cloth => p(1500.0, 90.0, 0.08, 0.0, 0.0, 0.0, 0, 0.0),
        Material::Metal => p(9000.0, 500.0, 0.05, 880.0, 0.8, 0.35, 0, 1.0),
        Material::Ice => p(9500.0, 2200.0, 0.06, 3100.0, 0.5, 0.16, 4, 0.5),
        Material::Water => p(3200.0, 300.0, 0.15, 520.0, 0.5, 0.06, 0, 0.0),
    }
}

/// Un « impact » de matériau : `scale` règle durée et brillance, `weight` le grave.
fn impact(m: Material, rng: &mut Rng, scale: f32, weight: f32, bright: f32) -> Vec<f32> {
    let p = profile(m);
    let secs = (p.decay * 4.0 * scale).max(0.06) + p.tone_decay * 2.0 * p.tone_mix;
    let n = samples(secs);
    let mut out = vec![0.0f32; n];
    let lp = p.lp * bright;
    if p.grains == 0 {
        let b = noise_burst(rng, secs, p.hp, lp, p.decay * scale, 0.002);
        mix_into(&mut out, &b, 0, 1.0 - p.tone_mix * 0.4);
    } else {
        for g in 0..p.grains {
            let at = samples(rng.range(0.0, p.decay * 3.5 * scale));
            let amp = rng.range(0.4, 1.0) * (1.0 - g as f32 / (p.grains as f32 * 1.4));
            let f = rng.range(0.7, 1.4);
            let b = noise_burst(rng, 0.03, p.hp * f, lp * f, 0.007, 0.0005);
            mix_into(&mut out, &b, at, amp);
        }
    }
    if p.tone > 0.0 {
        let f = p.tone * weight * rng.range(0.92, 1.08);
        let h: &[f32] = if p.ring > 0.9 {
            &[0.0, 0.0, 0.0]
        } else {
            &[0.3]
        };
        let t = tone(secs, f * 1.15, f * 0.9, p.tone_decay * scale, h);
        mix_into(&mut out, &t, 0, p.tone_mix);
        if p.ring > 0.0 {
            // Partiels inharmoniques de verre / métal.
            for (ratio, a) in [(1.52, 0.5), (2.31, 0.35), (3.07, 0.2)] {
                let t = tone(secs, f * ratio, f * ratio, p.tone_decay * scale * 0.8, &[]);
                mix_into(&mut out, &t, 0, p.tone_mix * a * p.ring);
            }
        }
    }
    out
}

/// Sons de blocs : casse, pose, pas, atterrissage.
fn block_sound(sfx: Sfx, v: usize, rng: &mut Rng) -> Vec<f32> {
    match sfx {
        Sfx::Break(m) => {
            let mut s = impact(m, rng, 1.0, 1.0, 1.0);
            // Débris qui retombent.
            let tail = impact(m, rng, 0.6, 1.2, 0.8);
            let at = samples(rng.range(0.06, 0.1));
            s.resize(s.len().max(at + tail.len()), 0.0);
            mix_into(&mut s, &tail, at, 0.35);
            finish(s, 0.9)
        }
        Sfx::Place(m) => finish(impact(m, rng, 0.7, 0.85, 0.7), 0.7),
        Sfx::Step(m) => {
            let w = 0.9 + 0.05 * (v % 5) as f32;
            finish(impact(m, rng, 0.45, w, 0.55 + 0.04 * (v % 3) as f32), 0.35)
        }
        Sfx::Land(m) => finish(impact(m, rng, 0.8, 0.7, 0.5), 0.6),
        _ => unreachable!(),
    }
}

/// Paramètres d'une voix « formantique ».
struct VoiceParams {
    f0: [f32; 3],
    dur: f32,
    /// `(fréquence, q, gain)` des formants.
    formants: [(f32, f32, f32); 3],
    vib_rate: f32,
    vib_depth: f32,
    noise: f32,
    am_rate: f32,
    am_depth: f32,
    /// 0 = dent de scie, 1 = quasi sinus.
    purity: f32,
    attack: f32,
    /// Nombre d'impulsions (gloussements, coassements en rafale).
    pulses: u32,
}

fn voice_params(v: Voice) -> VoiceParams {
    let z = (0.0, 1.0, 0.0);
    let vp = |f0: [f32; 3],
              dur,
              formants,
              vib_rate,
              vib_depth,
              noise,
              am_rate,
              am_depth,
              purity,
              attack,
              pulses| {
        VoiceParams {
            f0,
            dur,
            formants,
            vib_rate,
            vib_depth,
            noise,
            am_rate,
            am_depth,
            purity,
            attack,
            pulses,
        }
    };
    match v {
        Voice::Grunt => vp(
            [95.0, 85.0, 70.0],
            0.38,
            [(500.0, 6.0, 1.0), (1100.0, 6.0, 0.6), z],
            0.0,
            0.0,
            0.22,
            24.0,
            0.45,
            0.0,
            0.03,
            1,
        ),
        Voice::Moo => vp(
            [105.0, 120.0, 92.0],
            1.0,
            [(420.0, 9.0, 1.0), (900.0, 9.0, 0.5), z],
            5.0,
            0.02,
            0.06,
            0.0,
            0.0,
            0.1,
            0.12,
            1,
        ),
        Voice::Bleat => vp(
            [330.0, 360.0, 300.0],
            0.7,
            [(700.0, 8.0, 1.0), (1500.0, 9.0, 0.7), (2600.0, 8.0, 0.3)],
            0.0,
            0.0,
            0.08,
            32.0,
            0.6,
            0.0,
            0.05,
            1,
        ),
        Voice::Cluck => vp(
            [620.0, 700.0, 430.0],
            0.1,
            [(900.0, 7.0, 1.0), (2200.0, 8.0, 0.6), z],
            0.0,
            0.0,
            0.1,
            0.0,
            0.0,
            0.0,
            0.01,
            3,
        ),
        Voice::Squawk => vp(
            [850.0, 1500.0, 1100.0],
            0.4,
            [(1500.0, 7.0, 1.0), (3000.0, 8.0, 0.7), z],
            14.0,
            0.03,
            0.2,
            0.0,
            0.0,
            0.0,
            0.02,
            1,
        ),
        Voice::Growl => vp(
            [72.0, 78.0, 60.0],
            0.85,
            [(300.0, 5.0, 1.0), (800.0, 6.0, 0.7), z],
            0.0,
            0.0,
            0.35,
            42.0,
            0.8,
            0.0,
            0.1,
            1,
        ),
        Voice::Squeak => vp(
            [1500.0, 2300.0, 1800.0],
            0.14,
            [(2500.0, 12.0, 1.0), z, z],
            0.0,
            0.0,
            0.02,
            0.0,
            0.0,
            0.9,
            0.01,
            1,
        ),
        Voice::Croak => vp(
            [190.0, 140.0, 120.0],
            0.12,
            [(500.0, 8.0, 1.0), (1500.0, 9.0, 0.5), z],
            0.0,
            0.0,
            0.1,
            48.0,
            0.6,
            0.0,
            0.01,
            2,
        ),
        Voice::Hiss => vp(
            [0.0, 0.0, 0.0],
            0.7,
            [(6500.0, 1.5, 1.0), z, z],
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.1,
            1,
        ),
        Voice::Chitter => vp(
            [0.0, 0.0, 0.0],
            0.05,
            [(1800.0, 4.0, 1.0), (4200.0, 5.0, 0.6), z],
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.002,
            5,
        ),
        Voice::Whale => vp(
            [420.0, 1100.0, 620.0],
            1.1,
            [(900.0, 6.0, 1.0), z, z],
            6.0,
            0.03,
            0.01,
            0.0,
            0.0,
            1.0,
            0.1,
            1,
        ),
        Voice::Bubble => vp(
            [450.0, 700.0, 900.0],
            0.07,
            [z, z, z],
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.003,
            3,
        ),
        Voice::Squelch => vp(
            [0.0, 0.0, 0.0],
            0.4,
            [(500.0, 3.0, 1.0), (1000.0, 3.0, 0.6), z],
            0.0,
            0.0,
            1.0,
            11.0,
            0.7,
            0.0,
            0.03,
            1,
        ),
        Voice::Whinny => vp(
            [520.0, 900.0, 560.0],
            0.85,
            [(800.0, 7.0, 1.0), (2000.0, 8.0, 0.7), (3200.0, 8.0, 0.3)],
            8.0,
            0.025,
            0.1,
            0.0,
            0.0,
            0.0,
            0.04,
            1,
        ),
    }
}

/// Une impulsion vocale de `dur` secondes.
fn vocal_pulse(vp: &VoiceParams, rng: &mut Rng, dur: f32, pitch: f32, tilt: f32) -> Vec<f32> {
    let n = samples(dur);
    let mut forms: Vec<(Biquad, f32)> = vp
        .formants
        .iter()
        .filter(|f| f.2 > 0.0)
        .map(|&(f, q, g)| (Biquad::bandpass(f * pitch.powf(0.3), q, RATE), g))
        .collect();
    let mut phase = 0.0f32;
    let mut vib_ph = rng.range(0.0, TAU);
    let mut am_ph = rng.range(0.0, TAU);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let k = i as f32 / n as f32;
        let t = i as f32 / RATE;
        // Contour de hauteur : départ → pic → fin, modulé par `tilt` (plaintif / mourant).
        let contour = if k < 0.3 {
            vp.f0[0] + (vp.f0[1] - vp.f0[0]) * (k / 0.3)
        } else {
            vp.f0[1] + (vp.f0[2] - vp.f0[1]) * ((k - 0.3) / 0.7)
        };
        vib_ph += TAU * vp.vib_rate / RATE;
        let f0 = contour * pitch * (1.0 + vp.vib_depth * vib_ph.sin()) * (1.0 + tilt * (0.5 - k));
        phase = (phase + f0 / RATE).fract();
        let saw = phase * 2.0 - 1.0;
        let sine = (phase * TAU).sin();
        let src = saw * (1.0 - vp.purity) + sine * vp.purity;
        let breath = rng.noise() * vp.noise;
        let excitation = if vp.f0[0] > 0.0 {
            src * (1.0 - vp.noise * 0.5) + breath
        } else {
            breath
        };
        let mut y = 0.0;
        if forms.is_empty() {
            y = excitation;
        } else {
            for (f, g) in &mut forms {
                y += f.process(excitation) * *g;
            }
        }
        am_ph += TAU * vp.am_rate / RATE;
        let am = if vp.am_rate > 0.0 {
            1.0 - vp.am_depth * (0.5 + 0.5 * am_ph.sin())
        } else {
            1.0
        };
        let att = (t / vp.attack.max(1e-3)).min(1.0);
        let rel = ((1.0 - k) / 0.35).min(1.0);
        out.push(y * am * att * rel);
    }
    out
}

fn mob_sound(v: Voice, call: MobCall, var: usize, rng: &mut Rng) -> Vec<f32> {
    let vp = voice_params(v);
    let (pitch, dur_scale, tilt) = match call {
        MobCall::Idle => (1.0 + 0.04 * var as f32, 1.0, 0.0),
        MobCall::Hurt => (1.35, 0.55, 0.5),
        MobCall::Death => (0.85, 1.6, -0.9),
    };
    // Whale : deux appels enchaînés pour les dauphins ; Bubble : bulles ascendantes.
    if v == Voice::Bubble {
        let mut out = vec![0.0; samples(0.45)];
        for b in 0..vp.pulses {
            let f = vp.f0[0] * rng.range(0.9, 1.3) * (1.0 + 0.3 * b as f32) * pitch;
            let t = tone(0.07, f, f * 1.8, 0.03, &[]);
            mix_into(
                &mut out,
                &t,
                samples(0.08 * b as f32 + rng.range(0.0, 0.03)),
                0.8,
            );
        }
        return finish(out, 0.5);
    }
    let gap = vp.dur * 0.35;
    let total = (vp.dur * dur_scale + gap) * vp.pulses as f32;
    let mut out = vec![0.0; samples(total)];
    for p in 0..vp.pulses {
        let dur = vp.dur * dur_scale * rng.range(0.9, 1.1);
        let pj = pitch * rng.range(0.96, 1.04);
        let s = vocal_pulse(&vp, rng, dur, pj, tilt);
        let at = samples((vp.dur * dur_scale + gap) * p as f32);
        mix_into(&mut out, &s, at, 1.0);
    }
    let peak = match call {
        MobCall::Idle => 0.5,
        MobCall::Hurt => 0.7,
        MobCall::Death => 0.7,
    };
    finish(out, peak)
}

fn bird(var: usize, rng: &mut Rng) -> Vec<f32> {
    let notes = 2 + var % 4;
    let base = 2400.0 + 450.0 * (var % 3) as f32;
    let mut out = vec![0.0; samples(0.12 * notes as f32 + 0.1)];
    for n in 0..notes {
        let up = (var + n) & 1 == 0;
        let f0 = base * rng.range(0.85, 1.25);
        let (a, b) = if up {
            (f0, f0 * 1.5)
        } else {
            (f0 * 1.4, f0 * 0.8)
        };
        let t = tone(0.09, a, b, 0.05, &[0.15]);
        mix_into(&mut out, &t, samples(0.115 * n as f32), 0.8);
    }
    finish(out, 0.35)
}

/// Synthétise le son `sfx`, variante `variant` (graine déterministe).
pub fn render(sfx: Sfx, variant: usize) -> Vec<f32> {
    let seed = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        sfx.hash(&mut h);
        variant.hash(&mut h);
        h.finish()
    };
    let mut rng = Rng::new(seed);
    let rng = &mut rng;
    match sfx {
        Sfx::Break(_) | Sfx::Place(_) | Sfx::Step(_) | Sfx::Land(_) => {
            block_sound(sfx, variant, rng)
        }
        Sfx::Mob(v, c) => mob_sound(v, c, variant, rng),
        Sfx::Splash => {
            let mut out = noise_burst(
                rng,
                0.55,
                250.0,
                3200.0 - 400.0 * variant as f32,
                0.16,
                0.004,
            );
            let b = noise_burst(rng, 0.25, 1500.0, 7000.0, 0.05, 0.001);
            mix_into(&mut out, &b, 0, 0.6);
            for k in 0..4 {
                let f = rng.range(350.0, 800.0);
                let t = tone(0.08, f, f * 2.0, 0.03, &[]);
                mix_into(&mut out, &t, samples(0.05 + 0.07 * k as f32), 0.25);
            }
            finish(out, 0.75)
        }
        Sfx::Swim => {
            let out = noise_burst(rng, 0.3, 300.0, 2000.0 + 300.0 * variant as f32, 0.09, 0.03);
            finish(out, 0.3)
        }
        Sfx::Hit => {
            let mut out = tone(0.14, 150.0 + 20.0 * variant as f32, 70.0, 0.045, &[0.2]);
            let b = noise_burst(rng, 0.08, 400.0, 3500.0, 0.025, 0.0005);
            mix_into(&mut out, &b, 0, 0.7);
            finish(out, 0.8)
        }
        Sfx::PlayerHurt => {
            let vp = VoiceParams {
                f0: [210.0, 180.0, 130.0],
                dur: 0.28,
                formants: [(650.0, 6.0, 1.0), (1100.0, 7.0, 0.7), (2400.0, 8.0, 0.2)],
                vib_rate: 0.0,
                vib_depth: 0.0,
                noise: 0.15,
                am_rate: 0.0,
                am_depth: 0.0,
                purity: 0.0,
                attack: 0.01,
                pulses: 1,
            };
            finish(vocal_pulse(&vp, rng, 0.28, 1.0, 0.3), 0.7)
        }
        Sfx::PlayerDeath => {
            let vp = VoiceParams {
                f0: [200.0, 170.0, 70.0],
                dur: 0.9,
                formants: [(600.0, 6.0, 1.0), (1000.0, 7.0, 0.7), (2300.0, 8.0, 0.2)],
                vib_rate: 5.0,
                vib_depth: 0.03,
                noise: 0.25,
                am_rate: 0.0,
                am_depth: 0.0,
                purity: 0.0,
                attack: 0.02,
                pulses: 1,
            };
            finish(vocal_pulse(&vp, rng, 0.9, 1.0, -0.8), 0.75)
        }
        Sfx::Respawn => {
            let mut out = vec![0.0; samples(1.1)];
            for (i, f) in [523.25f32, 659.25, 783.99].iter().enumerate() {
                let t = tone(0.9, *f, *f, 0.35, &[0.3, 0.1]);
                mix_into(&mut out, &t, samples(0.12 * i as f32), 0.5);
            }
            finish(out, 0.5)
        }
        Sfx::Pickup => finish(tone(0.12, 520.0, 1250.0, 0.06, &[0.2]), 0.4),
        Sfx::Craft => {
            let mut out = vec![0.0; samples(0.6)];
            for k in 0..2 {
                let t = impact(Material::Wood, rng, 0.6, 1.0, 0.8);
                mix_into(&mut out, &t, samples(0.13 * k as f32), 0.8);
            }
            let c = tone(0.45, 1318.5, 1318.5, 0.2, &[0.2]);
            mix_into(&mut out, &c, samples(0.26), 0.35);
            finish(out, 0.6)
        }
        Sfx::UiClick => {
            let mut out = tone(0.05, 1500.0, 1100.0, 0.012, &[]);
            let n = noise_burst(rng, 0.02, 2000.0, 8000.0, 0.004, 0.0003);
            mix_into(&mut out, &n, 0, 0.3);
            finish(out, 0.35)
        }
        Sfx::UiOpen => {
            let mut out = tone(0.2, 520.0, 520.0, 0.07, &[0.2]);
            let b = tone(0.2, 780.0, 780.0, 0.08, &[0.2]);
            mix_into(&mut out, &b, samples(0.06), 0.8);
            finish(out, 0.3)
        }
        Sfx::UiClose => {
            let mut out = tone(0.2, 780.0, 780.0, 0.06, &[0.2]);
            let b = tone(0.2, 520.0, 520.0, 0.07, &[0.2]);
            mix_into(&mut out, &b, samples(0.06), 0.8);
            finish(out, 0.28)
        }
        Sfx::UiSlider => finish(tone(0.03, 1250.0, 1250.0, 0.008, &[]), 0.2),
        Sfx::Drip => {
            let f = 1300.0 + 250.0 * variant as f32;
            finish(tone(0.22, f * 1.3, f * 0.7, 0.05, &[0.1]), 0.4)
        }
        Sfx::Bird => bird(variant, rng),
        Sfx::Crackle => {
            let mut out = vec![0.0; samples(0.5)];
            for _ in 0..(5 + variant * 2) {
                let b = noise_burst(rng, 0.03, 500.0, 5000.0, 0.008, 0.0005);
                mix_into(
                    &mut out,
                    &b,
                    samples(rng.range(0.0, 0.45)),
                    rng.range(0.3, 1.0),
                );
            }
            finish(out, 0.4)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(sfx: Sfx) {
        for v in 0..sfx.variants() {
            let s = render(sfx, v);
            assert!(!s.is_empty(), "{sfx:?}");
            assert!(
                s.iter().all(|x| x.is_finite() && x.abs() <= 1.01),
                "{sfx:?}"
            );
            let peak = s.iter().fold(0.0f32, |a, &x| a.max(x.abs()));
            assert!(peak > 0.1, "{sfx:?} v{v} silencieux ({peak})");
            assert!(s.len() < samples(3.0), "{sfx:?} trop long");
        }
    }

    #[test]
    fn every_material_sound_renders() {
        for m in Material::ALL {
            for s in [Sfx::Break(m), Sfx::Place(m), Sfx::Step(m), Sfx::Land(m)] {
                check(s);
            }
        }
    }

    #[test]
    fn every_voice_renders() {
        let voices = [
            Voice::Grunt,
            Voice::Moo,
            Voice::Bleat,
            Voice::Cluck,
            Voice::Squawk,
            Voice::Growl,
            Voice::Squeak,
            Voice::Croak,
            Voice::Hiss,
            Voice::Chitter,
            Voice::Whale,
            Voice::Bubble,
            Voice::Squelch,
            Voice::Whinny,
        ];
        for v in voices {
            for c in [MobCall::Idle, MobCall::Hurt, MobCall::Death] {
                check(Sfx::Mob(v, c));
            }
        }
    }

    #[test]
    fn misc_sounds_render() {
        for s in [
            Sfx::Splash,
            Sfx::Swim,
            Sfx::Hit,
            Sfx::PlayerHurt,
            Sfx::PlayerDeath,
            Sfx::Respawn,
            Sfx::Pickup,
            Sfx::Craft,
            Sfx::UiClick,
            Sfx::UiOpen,
            Sfx::UiClose,
            Sfx::UiSlider,
            Sfx::Drip,
            Sfx::Bird,
            Sfx::Crackle,
        ] {
            check(s);
        }
    }

    #[test]
    fn rendering_is_deterministic_and_variants_differ() {
        let a = render(Sfx::Step(Material::Stone), 0);
        assert_eq!(a, render(Sfx::Step(Material::Stone), 0));
        assert_ne!(a, render(Sfx::Step(Material::Stone), 1));
    }
}
