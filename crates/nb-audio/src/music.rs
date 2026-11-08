//! Musique générative en temps réel : nappes (pads), cloche de boîte à musique et basse, dont
//! la gamme, le tempo et la densité dépendent du thème (biome, nuit, grotte, menu…).
//!
//! Rien n'est pré-enregistré : chaque note est synthétisée dans le callback audio, sans
//! allocation. En jeu la musique joue par phrases entrecoupées de longs silences, comme un
//! fond discret ; dans les menus elle est continue.

use std::f32::consts::TAU;

use crate::dsp::{midi_to_hz, Reverb, Rng, Smooth};

/// Ambiance musicale courante.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Theme {
    #[default]
    Menu,
    Meadow,
    Forest,
    Jungle,
    Desert,
    Snow,
    Mountain,
    Ocean,
    Swamp,
    Cave,
    Night,
    Volcano,
    Mystic,
}

/// Comment la musique se déclenche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MusicMode {
    /// Pas de musique (le volume « Music » à 0 ne change pas ce mode).
    Off,
    /// Continue (menus).
    Continuous,
    /// Phrases séparées par de longs silences (en jeu).
    #[default]
    Sparse,
}

struct ThemeParams {
    root: f32,
    scale: &'static [i32],
    /// Degrés (indices de gamme) des fondamentales d'accords possibles.
    chord_roots: &'static [usize],
    /// Battements par seconde.
    tempo: f32,
    /// Probabilité d'une note de cloche par temps.
    note_prob: f32,
    pad: f32,
    bell: f32,
    bass: f32,
    /// Décalage d'octave de la mélodie (en demi-tons).
    melody_shift: f32,
    /// Harmoniques des nappes (0 = doux, 1 = riche).
    brightness: f32,
    reverb: f32,
}

const PENTA_MAJ: &[i32] = &[0, 2, 4, 7, 9];
const PENTA_MIN: &[i32] = &[0, 3, 5, 7, 10];
const DORIAN: &[i32] = &[0, 2, 3, 5, 7, 9, 10];
const LYDIAN: &[i32] = &[0, 2, 4, 6, 7, 9, 11];
const PHRYGIAN_DOM: &[i32] = &[0, 1, 4, 5, 7, 8, 10];
const AEOLIAN: &[i32] = &[0, 2, 3, 5, 7, 8, 10];
const WHOLE_TONE: &[i32] = &[0, 2, 4, 6, 8, 10];

fn params(t: Theme) -> ThemeParams {
    let p = |root,
             scale,
             chord_roots,
             tempo,
             note_prob,
             pad,
             bell,
             bass,
             melody_shift,
             brightness,
             reverb| ThemeParams {
        root,
        scale,
        chord_roots,
        tempo,
        note_prob,
        pad,
        bell,
        bass,
        melody_shift,
        brightness,
        reverb,
    };
    match t {
        Theme::Menu => p(
            57.0,
            LYDIAN,
            &[0, 3, 4, 5],
            1.0,
            0.55,
            0.9,
            0.7,
            0.5,
            12.0,
            0.4,
            0.8,
        ),
        Theme::Meadow => p(
            60.0,
            PENTA_MAJ,
            &[0, 2, 3, 1],
            1.05,
            0.5,
            0.7,
            0.8,
            0.35,
            12.0,
            0.3,
            0.6,
        ),
        Theme::Forest => p(
            55.0,
            DORIAN,
            &[0, 3, 4, 2],
            0.8,
            0.35,
            0.9,
            0.55,
            0.4,
            12.0,
            0.3,
            0.75,
        ),
        Theme::Jungle => p(
            62.0,
            PENTA_MAJ,
            &[0, 1, 3, 4],
            1.3,
            0.55,
            0.5,
            0.8,
            0.4,
            12.0,
            0.5,
            0.55,
        ),
        Theme::Desert => p(
            52.0,
            PHRYGIAN_DOM,
            &[0, 0, 4, 3],
            0.75,
            0.4,
            0.8,
            0.5,
            0.5,
            12.0,
            0.2,
            0.7,
        ),
        Theme::Snow => p(
            64.0,
            LYDIAN,
            &[0, 4, 2, 5],
            0.6,
            0.3,
            0.9,
            0.8,
            0.15,
            12.0,
            0.15,
            0.9,
        ),
        Theme::Mountain => p(
            53.0,
            PENTA_MIN,
            &[0, 3, 2, 4],
            0.7,
            0.3,
            1.0,
            0.45,
            0.5,
            12.0,
            0.35,
            0.9,
        ),
        Theme::Ocean => p(
            58.0,
            WHOLE_TONE,
            &[0, 2, 4, 1],
            0.55,
            0.28,
            1.0,
            0.55,
            0.3,
            12.0,
            0.1,
            1.0,
        ),
        Theme::Swamp => p(
            50.0,
            DORIAN,
            &[0, 5, 3, 0],
            0.65,
            0.32,
            0.9,
            0.4,
            0.5,
            12.0,
            0.25,
            0.8,
        ),
        Theme::Cave => p(
            45.0,
            AEOLIAN,
            &[0, 0, 5, 3],
            0.45,
            0.12,
            1.0,
            0.35,
            0.7,
            12.0,
            0.1,
            1.0,
        ),
        Theme::Night => p(
            57.0,
            PENTA_MIN,
            &[0, 3, 4, 2],
            0.6,
            0.3,
            0.85,
            0.7,
            0.35,
            12.0,
            0.15,
            0.9,
        ),
        Theme::Volcano => p(
            46.0,
            PHRYGIAN_DOM,
            &[0, 1, 0, 5],
            0.8,
            0.22,
            0.9,
            0.3,
            0.9,
            12.0,
            0.45,
            0.8,
        ),
        Theme::Mystic => p(
            59.0,
            LYDIAN,
            &[0, 5, 2, 4],
            0.7,
            0.4,
            0.8,
            0.9,
            0.3,
            12.0,
            0.2,
            0.9,
        ),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Pad,
    Bell,
    Bass,
}

#[derive(Clone, Copy)]
struct Note {
    kind: Kind,
    active: bool,
    freq: f32,
    ph: f32,
    age: f32,
    /// Durée tenue avant relâchement (nappes), secondes.
    hold: f32,
    amp: f32,
    pan: f32,
    bright: f32,
}

const MAX_NOTES: usize = 28;

/// Le générateur de musique.
pub struct Music {
    rate: f32,
    rng: Rng,
    notes: [Note; MAX_NOTES],
    theme: Theme,
    mode: MusicMode,
    /// Niveau global (fondu d'entrée / de sortie des phrases).
    level: Smooth,
    /// Phrase en cours (vrai) ou silence (faux), et temps restant.
    phrase_on: bool,
    timer: f32,
    beat_clock: f32,
    beat: u32,
    chord_root: usize,
    melody_idx: i32,
    reverb: Reverb,
}

impl Music {
    pub fn new(rate: f32, seed: u64) -> Self {
        let dead = Note {
            kind: Kind::Pad,
            active: false,
            freq: 0.0,
            ph: 0.0,
            age: 0.0,
            hold: 0.0,
            amp: 0.0,
            pan: 0.0,
            bright: 0.0,
        };
        let mut reverb = Reverb::new(rate);
        reverb.size = 0.95;
        reverb.damp = 0.45;
        Self {
            rate,
            rng: Rng::new(seed),
            notes: [dead; MAX_NOTES],
            theme: Theme::Menu,
            mode: MusicMode::Sparse,
            level: Smooth::new(0.0),
            phrase_on: false,
            timer: 20.0,
            beat_clock: 0.0,
            beat: 0,
            chord_root: 0,
            melody_idx: 5,
            reverb,
        }
    }

    pub fn set_theme(&mut self, t: Theme) {
        self.theme = t;
    }

    pub fn set_mode(&mut self, m: MusicMode) {
        if m != self.mode {
            self.mode = m;
            match m {
                MusicMode::Continuous => self.phrase_on = true,
                MusicMode::Off => self.phrase_on = false,
                MusicMode::Sparse => {
                    self.phrase_on = false;
                    self.timer = self.rng.range(8.0, 25.0);
                }
            }
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// La musique est-elle audible (phrase en cours ou queue de réverbération) ?
    pub fn is_playing(&self) -> bool {
        self.phrase_on && self.mode != MusicMode::Off
    }

    fn spawn(&mut self, n: Note) {
        let slot = self
            .notes
            .iter()
            .position(|x| !x.active)
            .unwrap_or_else(|| {
                self.notes
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.age.total_cmp(&b.1.age))
                    .map_or(0, |(i, _)| i)
            });
        self.notes[slot] = n;
    }

    fn scale_note(p: &ThemeParams, idx: i32) -> f32 {
        let len = p.scale.len() as i32;
        let oct = idx.div_euclid(len);
        let step = idx.rem_euclid(len) as usize;
        p.root + (oct * 12 + p.scale[step]) as f32
    }

    fn on_bar(&mut self, p: &ThemeParams) {
        self.chord_root = p.chord_roots[self.rng.below(p.chord_roots.len())];
        let r = self.chord_root as i32;
        if p.pad > 0.0 {
            for (k, off) in [0, 2, 4].iter().enumerate() {
                let midi = Self::scale_note(p, r + off) - 12.0;
                let ph0 = self.rng.range(0.0, 1.0);
                self.spawn(Note {
                    kind: Kind::Pad,
                    active: true,
                    freq: midi_to_hz(midi),
                    ph: ph0,
                    age: 0.0,
                    hold: 8.0 / p.tempo,
                    amp: 0.11 * p.pad,
                    pan: (k as f32 - 1.0) * 0.45,
                    bright: p.brightness,
                });
            }
        }
        if p.bass > 0.0 {
            let midi = Self::scale_note(p, r) - 24.0;
            self.spawn(Note {
                kind: Kind::Bass,
                active: true,
                freq: midi_to_hz(midi),
                ph: 0.0,
                age: 0.0,
                hold: 5.0 / p.tempo,
                amp: 0.16 * p.bass,
                pan: 0.0,
                bright: 0.0,
            });
        }
    }

    fn on_beat(&mut self, p: &ThemeParams) {
        if self.rng.unit() < p.note_prob {
            // Marche aléatoire dans la gamme, avec rappel vers le centre.
            let len = p.scale.len() as i32;
            let step = [-2, -1, -1, 1, 1, 2, 3][self.rng.below(7)];
            self.melody_idx = (self.melody_idx + step).clamp(len - 2, len * 2 + 1);
            let midi = Self::scale_note(p, self.melody_idx) + p.melody_shift - 12.0;
            let vel = self.rng.range(0.7, 1.0);
            let pan0 = self.rng.range(-0.7, 0.7);
            self.spawn(Note {
                kind: Kind::Bell,
                active: true,
                freq: midi_to_hz(midi),
                ph: 0.0,
                age: 0.0,
                hold: 0.0,
                amp: 0.10 * p.bell * vel,
                pan: pan0,
                bright: p.brightness,
            });
        }
    }

    /// Ajoute `frames` échantillons stéréo de musique dans `out` (entrelacé L/R) et renvoie l'envoi
    /// mono à ne pas retraiter. `gain` est le volume combiné (maître × musique).
    pub fn render(&mut self, out: &mut [f32], gain: f32) {
        let dt = 1.0 / self.rate;
        let p = params(self.theme);
        let frames = out.len() / 2;
        // Machine à phrases : calculée une fois par bloc (le bloc ≈ 10 ms).
        let block_dt = frames as f32 * dt;
        if self.mode == MusicMode::Sparse {
            self.timer -= block_dt;
            if self.timer <= 0.0 {
                self.phrase_on = !self.phrase_on;
                self.timer = if self.phrase_on {
                    self.rng.range(60.0, 130.0)
                } else {
                    self.rng.range(70.0, 200.0)
                };
            }
        }
        self.level.target = if self.is_playing() { 1.0 } else { 0.0 };
        let tau = if self.level.target > 0.5 { 3.0 } else { 5.0 };
        self.level.step(block_dt, tau);
        let fading = self.level.value > 0.002 || self.level.target > 0.0;
        // Les nouvelles notes ne démarrent que pendant une phrase.
        if self.is_playing() {
            self.beat_clock += block_dt * p.tempo;
            while self.beat_clock >= 1.0 {
                self.beat_clock -= 1.0;
                if self.beat & 7 == 0 {
                    self.on_bar(&p);
                }
                self.on_beat(&p);
                self.beat = self.beat.wrapping_add(1);
            }
        }
        self.reverb.size = 0.7 + 0.28 * p.reverb;
        let lvl = self.level.value * gain;
        for i in 0..frames {
            let (mut l, mut r) = (0.0f32, 0.0f32);
            if fading {
                for n in self.notes.iter_mut().filter(|n| n.active) {
                    n.age += dt;
                    n.ph = (n.ph + n.freq * dt).fract();
                    let (s, done) = match n.kind {
                        Kind::Pad => {
                            let att = (n.age / 2.0).min(1.0);
                            let rel = if n.age > n.hold {
                                (-(n.age - n.hold) / 1.4).exp()
                            } else {
                                1.0
                            };
                            let a = att * att * rel;
                            let w = n.ph * TAU;
                            let s = w.sin()
                                + 0.5 * (w * 1.004).sin()
                                + 0.5 * (w * 0.996).sin()
                                + n.bright * 0.45 * (w * 2.0).sin()
                                + n.bright * 0.2 * (w * 3.0).sin();
                            (s * a * 0.45, n.age > n.hold + 8.0)
                        }
                        Kind::Bell => {
                            let w = n.ph * TAU;
                            let idx = 2.2 * (-n.age / 0.35).exp() * (0.4 + n.bright);
                            let s = (w + idx * (w * 2.0).sin()).sin();
                            let a = (n.age / 0.003).min(1.0) * (-n.age / 1.7).exp();
                            (s * a, n.age > 7.0)
                        }
                        Kind::Bass => {
                            let w = n.ph * TAU;
                            let s = w.sin() + 0.25 * (w * 2.0).sin();
                            let att = (n.age / 0.35).min(1.0);
                            let rel = if n.age > n.hold {
                                (-(n.age - n.hold) / 1.2).exp()
                            } else {
                                1.0
                            };
                            (s * att * rel * 0.8, n.age > n.hold + 7.0)
                        }
                    };
                    let v = s * n.amp;
                    let gl = ((1.0 - n.pan) * 0.5).sqrt();
                    let gr = ((1.0 + n.pan) * 0.5).sqrt();
                    l += v * gl;
                    r += v * gr;
                    if done {
                        n.active = false;
                    }
                }
            }
            let send = (l + r) * 0.5 * (0.35 + 0.65 * p.reverb);
            let (rl, rr) = self.reverb.process(send);
            out[2 * i] += (l + rl * 1.6) * lvl;
            out[2 * i + 1] += (r + rr * 1.6) * lvl;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(m: &mut Music, secs: f32) -> f32 {
        let mut e = 0.0;
        let mut buf = vec![0.0f32; 882];
        for _ in 0..(secs * 50.0) as usize {
            buf.iter_mut().for_each(|x| *x = 0.0);
            m.render(&mut buf, 1.0);
            assert!(buf.iter().all(|x| x.is_finite()));
            e += buf.iter().map(|x| x * x).sum::<f32>();
        }
        e
    }

    #[test]
    fn continuous_music_makes_sound_and_stays_bounded() {
        let mut m = Music::new(44_100.0, 1);
        m.set_mode(MusicMode::Continuous);
        let e = energy(&mut m, 12.0);
        assert!(e > 1.0, "musique silencieuse ({e})");
        let mut buf = vec![0.0f32; 882];
        for _ in 0..500 {
            buf.iter_mut().for_each(|x| *x = 0.0);
            m.render(&mut buf, 1.0);
            assert!(buf.iter().all(|x| x.abs() < 1.5));
        }
    }

    #[test]
    fn off_mode_fades_to_silence() {
        let mut m = Music::new(44_100.0, 2);
        m.set_mode(MusicMode::Continuous);
        let _ = energy(&mut m, 8.0);
        m.set_mode(MusicMode::Off);
        let _ = energy(&mut m, 40.0);
        assert!(energy(&mut m, 2.0) < 1e-4);
    }

    #[test]
    fn every_theme_is_playable() {
        for t in [
            Theme::Menu,
            Theme::Meadow,
            Theme::Forest,
            Theme::Jungle,
            Theme::Desert,
            Theme::Snow,
            Theme::Mountain,
            Theme::Ocean,
            Theme::Swamp,
            Theme::Cave,
            Theme::Night,
            Theme::Volcano,
            Theme::Mystic,
        ] {
            let mut m = Music::new(44_100.0, 3);
            m.set_theme(t);
            m.set_mode(MusicMode::Continuous);
            assert!(energy(&mut m, 15.0) > 0.01, "{t:?}");
        }
    }

    #[test]
    fn sparse_mode_starts_silent() {
        let mut m = Music::new(44_100.0, 4);
        m.set_mode(MusicMode::Continuous);
        m.set_mode(MusicMode::Sparse);
        assert!(!m.is_playing());
    }
}
