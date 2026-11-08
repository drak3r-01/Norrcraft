//! Ambiances : lits sonores continus synthétisés en temps réel (vent, grotte, vagues, grillons)
//! et sons ponctuels placés au hasard autour de l'auditeur (oiseaux, grenouilles, gouttes).

use std::f32::consts::TAU;

use crate::dsp::{Biquad, OnePole, Rng, Smooth};
use crate::mixer::{Listener, PlayReq};
use crate::sfx::{Bus, MobCall, Sfx, Voice};

/// Intensités cibles (0..1) des différentes ambiances, fixées par le jeu selon le biome,
/// l'heure et la position.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct AmbienceTargets {
    pub wind: f32,
    pub cave: f32,
    pub water: f32,
    pub birds: f32,
    pub crickets: f32,
    pub frogs: f32,
    pub lava: f32,
}

impl AmbienceTargets {
    fn as_array(&self) -> [f32; 7] {
        [
            self.wind,
            self.cave,
            self.water,
            self.birds,
            self.crickets,
            self.frogs,
            self.lava,
        ]
    }
}

/// Le générateur d'ambiances.
pub struct Ambience {
    rate: f32,
    rng: Rng,
    levels: [Smooth; 7],
    // Vent
    wind_lo: [OnePole; 2],
    wind_band: [Biquad; 2],
    gust: Smooth,
    gust_timer: f32,
    // Grotte
    cave_noise: [OnePole; 2],
    cave_ph: [f32; 3],
    // Vagues
    water_lp: [OnePole; 2],
    water_hp: [OnePole; 2],
    wave_ph: f32,
    // Grillons
    cricket_ph: [f32; 2],
    cricket_gate: f32,
    // Sons ponctuels
    bird_timer: f32,
    drip_timer: f32,
    frog_timer: f32,
    lava_timer: f32,
    /// Sons ponctuels à jouer, vidés par le mixeur après chaque bloc.
    pub pending: Vec<PlayReq>,
}

impl Ambience {
    pub fn new(rate: f32, seed: u64) -> Self {
        Self {
            rate,
            rng: Rng::new(seed),
            levels: [Smooth::new(0.0); 7],
            wind_lo: [OnePole::default(); 2],
            wind_band: [Biquad::bandpass(700.0, 0.6, rate); 2],
            gust: Smooth::new(0.5),
            gust_timer: 0.0,
            cave_noise: [OnePole::default(); 2],
            cave_ph: [0.0; 3],
            water_lp: [OnePole::default(); 2],
            water_hp: [OnePole::default(); 2],
            wave_ph: 0.0,
            cricket_ph: [0.0; 2],
            cricket_gate: 0.0,
            bird_timer: 3.0,
            drip_timer: 2.0,
            frog_timer: 4.0,
            lava_timer: 1.0,
            pending: Vec::with_capacity(8),
        }
    }

    pub fn levels(&self) -> [f32; 7] {
        let mut a = [0.0; 7];
        for (o, l) in a.iter_mut().zip(&self.levels) {
            *o = l.value;
        }
        a
    }

    fn spot(&mut self, l: &Listener, dmin: f32, dmax: f32, up: (f32, f32)) -> [f32; 3] {
        let az = self.rng.range(0.0, TAU);
        let d = self.rng.range(dmin, dmax);
        [
            l.pos[0] + az.cos() * d,
            l.pos[1] + self.rng.range(up.0, up.1),
            l.pos[2] + az.sin() * d,
        ]
    }

    fn schedule(&mut self, dt: f32, l: &Listener) {
        let lv = self.levels();
        let (birds, cave, frogs, lava) = (lv[3], lv[1], lv[5], lv[6]);
        self.bird_timer -= dt;
        if self.bird_timer <= 0.0 {
            if birds > 0.05 {
                let pos = self.spot(l, 6.0, 30.0, (2.0, 12.0));
                self.pending.push(PlayReq {
                    sfx: Sfx::Bird,
                    pos: Some(pos),
                    gain: 0.5 * birds,
                    pitch: self.rng.range(0.85, 1.25),
                    bus: Bus::Ambient,
                    range: 48.0,
                });
            }
            self.bird_timer = self.rng.range(1.5, 7.0) / birds.max(0.2);
        }
        self.drip_timer -= dt;
        if self.drip_timer <= 0.0 {
            if cave > 0.1 {
                let pos = self.spot(l, 2.0, 14.0, (-2.0, 1.0));
                self.pending.push(PlayReq {
                    sfx: Sfx::Drip,
                    pos: Some(pos),
                    gain: 0.6 * cave,
                    pitch: self.rng.range(0.8, 1.3),
                    bus: Bus::Ambient,
                    range: 32.0,
                });
            }
            self.drip_timer = self.rng.range(2.0, 9.0);
        }
        self.frog_timer -= dt;
        if self.frog_timer <= 0.0 {
            if frogs > 0.05 {
                let pos = self.spot(l, 5.0, 24.0, (-1.0, 1.0));
                self.pending.push(PlayReq {
                    sfx: Sfx::Mob(Voice::Croak, MobCall::Idle),
                    pos: Some(pos),
                    gain: 0.45 * frogs,
                    pitch: self.rng.range(0.8, 1.1),
                    bus: Bus::Ambient,
                    range: 36.0,
                });
            }
            self.frog_timer = self.rng.range(2.5, 8.0) / frogs.max(0.2);
        }
        self.lava_timer -= dt;
        if self.lava_timer <= 0.0 {
            if lava > 0.05 {
                let pos = self.spot(l, 2.0, 12.0, (-1.0, 1.0));
                self.pending.push(PlayReq {
                    sfx: Sfx::Crackle,
                    pos: Some(pos),
                    gain: 0.5 * lava,
                    pitch: self.rng.range(0.8, 1.2),
                    bus: Bus::Ambient,
                    range: 24.0,
                });
            }
            self.lava_timer = self.rng.range(0.6, 2.5) / lava.max(0.2);
        }
    }

    /// Ajoute les lits d'ambiance dans `out` (stéréo entrelacé) avec le volume `gain`, et
    /// planifie les sons ponctuels. Renvoie le niveau de réverbération conseillé (grotte).
    pub fn render(
        &mut self,
        out: &mut [f32],
        gain: f32,
        listener: &Listener,
        targets: &AmbienceTargets,
    ) {
        let dt = 1.0 / self.rate;
        let frames = out.len() / 2;
        let block_dt = frames as f32 * dt;
        for (s, t) in self.levels.iter_mut().zip(targets.as_array()) {
            s.target = t.clamp(0.0, 1.0);
            s.step(block_dt, 1.5);
        }
        self.schedule(block_dt, listener);
        // Rafales de vent : nouvelle cible toutes les 2-6 s, lissée.
        self.gust_timer -= block_dt;
        if self.gust_timer <= 0.0 {
            self.gust.target = self.rng.range(0.15, 1.0);
            self.gust_timer = self.rng.range(2.0, 6.0);
        }
        let gust = self.gust.step(block_dt, 1.2);
        let [wind, cave, water, _, crickets, ..] = self.levels();
        if wind + cave + water + crickets < 1e-3 {
            return;
        }
        let a_lo = OnePole::coef(260.0, self.rate);
        let a_w_lp = OnePole::coef(600.0, self.rate);
        let a_w_hp = OnePole::coef(120.0, self.rate);
        let a_cave = OnePole::coef(140.0, self.rate);
        for i in 0..frames {
            let t_wave = {
                self.wave_ph = (self.wave_ph + dt / 7.5).fract();
                0.5 + 0.5 * (self.wave_ph * TAU).sin()
            };
            self.cave_ph[0] = (self.cave_ph[0] + 55.0 * dt).fract();
            self.cave_ph[1] = (self.cave_ph[1] + 55.7 * dt).fract();
            self.cave_ph[2] = (self.cave_ph[2] + 82.4 * dt).fract();
            let drone = (self.cave_ph[0] * TAU).sin() * 0.5
                + (self.cave_ph[1] * TAU).sin() * 0.5
                + (self.cave_ph[2] * TAU).sin() * 0.25;
            // Grillons : trains d'impulsions à 3 Hz, porteuse 4,3 kHz.
            self.cricket_gate = (self.cricket_gate + 3.0 * dt).fract();
            let gate = if self.cricket_gate < 0.35 {
                let k = self.cricket_gate / 0.35;
                ((k * 12.0 * TAU).sin() * 0.5 + 0.5) * (k * std::f32::consts::PI).sin()
            } else {
                0.0
            };
            self.cricket_ph[0] = (self.cricket_ph[0] + 4300.0 * dt).fract();
            self.cricket_ph[1] = (self.cricket_ph[1] + 4560.0 * dt).fract();
            let mut ch = [0.0f32; 2];
            for (c, o) in ch.iter_mut().enumerate() {
                let n = self.rng.noise();
                let mut s = 0.0;
                if wind > 1e-3 {
                    let lo = self.wind_lo[c].lp(n, a_lo);
                    let bp = self.wind_band[c].process(n);
                    s += (lo * 1.6 + bp * 0.7) * wind * (0.35 + 0.65 * gust) * 0.55;
                }
                if cave > 1e-3 {
                    let rumble = self.cave_noise[c].lp(n, a_cave);
                    s += (drone * 0.05 + rumble * 0.5) * cave;
                }
                if water > 1e-3 {
                    let w = self.water_hp[c].hp(self.water_lp[c].lp(n, a_w_lp), a_w_hp);
                    s += w * water * (0.3 + 0.7 * t_wave) * 1.3;
                }
                if crickets > 1e-3 {
                    s += (self.cricket_ph[c] * TAU).sin() * gate * crickets * 0.035;
                }
                *o = s;
            }
            out[2 * i] += ch[0] * gain;
            out[2 * i + 1] += ch[1] * gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(a: &mut Ambience, t: &AmbienceTargets, secs: f32) -> f32 {
        let l = Listener::default();
        let mut e = 0.0;
        let mut buf = vec![0.0f32; 882];
        for _ in 0..(secs * 50.0) as usize {
            buf.iter_mut().for_each(|x| *x = 0.0);
            a.render(&mut buf, 1.0, &l, t);
            assert!(buf.iter().all(|x| x.is_finite()));
            e += buf.iter().map(|x| x * x).sum::<f32>();
        }
        e
    }

    #[test]
    fn silent_without_targets() {
        let mut a = Ambience::new(44_100.0, 1);
        assert_eq!(run(&mut a, &AmbienceTargets::default(), 3.0), 0.0);
        assert!(a.pending.is_empty());
    }

    #[test]
    fn each_bed_produces_sound() {
        for t in [
            AmbienceTargets {
                wind: 1.0,
                ..Default::default()
            },
            AmbienceTargets {
                cave: 1.0,
                ..Default::default()
            },
            AmbienceTargets {
                water: 1.0,
                ..Default::default()
            },
            AmbienceTargets {
                crickets: 1.0,
                ..Default::default()
            },
        ] {
            let mut a = Ambience::new(44_100.0, 2);
            assert!(run(&mut a, &t, 6.0) > 0.01, "{t:?}");
        }
    }

    #[test]
    fn schedules_one_shots_around_the_listener() {
        let mut a = Ambience::new(44_100.0, 3);
        let t = AmbienceTargets {
            birds: 1.0,
            cave: 1.0,
            frogs: 1.0,
            lava: 1.0,
            ..Default::default()
        };
        let mut seen = std::collections::HashSet::new();
        let l = Listener::default();
        let mut buf = vec![0.0f32; 882];
        for _ in 0..(60 * 50) {
            a.render(&mut buf, 1.0, &l, &t);
            for r in a.pending.drain(..) {
                let p = r.pos.unwrap();
                let d = ((p[0] - l.pos[0]).powi(2) + (p[2] - l.pos[2]).powi(2)).sqrt();
                assert!(d < 40.0);
                seen.insert(std::mem::discriminant(&r.sfx));
            }
        }
        assert!(seen.len() >= 3, "{seen:?}");
    }
}
