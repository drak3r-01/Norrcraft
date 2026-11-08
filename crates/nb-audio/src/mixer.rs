//! Mixeur temps réel : voix spatialisées (atténuation, panoramique, assombrissement avec la
//! distance), volumes par canal, réverbération d'environnement, effet sous l'eau, ambiances et
//! musique. Pur calcul : il remplit un tampon, la sortie matérielle est ailleurs.

use std::sync::Arc;

use crossbeam_channel::Receiver;

use crate::ambience::{Ambience, AmbienceTargets};
use crate::bank::{Bank, Sample};
use crate::dsp::{soft_clip, OnePole, Reverb, Rng, Smooth};
use crate::music::{Music, MusicMode, Theme};
use crate::sfx::{Bus, Sfx};

/// Nombre maximal de voix simultanées.
pub const MAX_VOICES: usize = 48;
/// Taille d'un bloc de calcul (la spatialisation est recalculée par bloc, gains interpolés).
const CHUNK: usize = 256;

/// Position et orientation de l'auditeur (la tête du joueur).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub pos: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            pos: [0.0; 3],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
        }
    }
}

/// Volumes de 0 à 1 : un général et un par canal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volumes {
    pub master: f32,
    pub music: f32,
    pub ambient: f32,
    pub effects: f32,
    pub creatures: f32,
}

impl Default for Volumes {
    fn default() -> Self {
        Self {
            master: 1.0,
            music: 0.5,
            ambient: 0.8,
            effects: 1.0,
            creatures: 1.0,
        }
    }
}

/// Acoustique du lieu où se trouve l'auditeur.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Environment {
    /// Tête sous l'eau : tout est étouffé.
    pub underwater: bool,
    /// 0 = plein air, 1 = grotte profonde (réverbération forte).
    pub reverb: f32,
}

/// Demande de lecture d'un effet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayReq {
    pub sfx: Sfx,
    /// `None` : son non spatialisé (interface, pas du joueur…).
    pub pos: Option<[f32; 3]>,
    pub gain: f32,
    /// Facteur de vitesse de lecture (1 = nominal ; < 1 plus grave).
    pub pitch: f32,
    pub bus: Bus,
    /// Distance d'audibilité maximale en blocs.
    pub range: f32,
}

impl PlayReq {
    pub fn new(sfx: Sfx, bus: Bus) -> Self {
        Self {
            sfx,
            pos: None,
            gain: 1.0,
            pitch: 1.0,
            bus,
            range: 40.0,
        }
    }

    pub fn at(mut self, pos: [f32; 3]) -> Self {
        self.pos = Some(pos);
        self
    }

    pub fn gain(mut self, g: f32) -> Self {
        self.gain = g;
        self
    }

    pub fn pitch(mut self, p: f32) -> Self {
        self.pitch = p;
        self
    }

    pub fn range(mut self, r: f32) -> Self {
        self.range = r;
        self
    }
}

/// Commandes envoyées au thread audio.
#[derive(Clone, Debug)]
pub enum Cmd {
    Play(PlayReq),
    Listener(Listener),
    Volumes(Volumes),
    Environment(Environment),
    Ambience(AmbienceTargets),
    Theme(Theme),
    MusicMode(MusicMode),
    StopAll,
}

/// Résultat de la spatialisation d'une source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spat {
    pub gl: f32,
    pub gr: f32,
    /// Fréquence de coupure du filtre passe-bas de distance, Hz.
    pub cutoff: f32,
    pub dist: f32,
}

const SQRT_HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l < 1e-6 {
        [0.0; 3]
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

/// Atténuation, panoramique à puissance constante et assombrissement d'une source à `pos`.
pub fn spatialize(l: &Listener, pos: [f32; 3], range: f32) -> Spat {
    let rel = [pos[0] - l.pos[0], pos[1] - l.pos[1], pos[2] - l.pos[2]];
    let d = dot(rel, rel).sqrt();
    let range = range.max(1.0);
    let att = if d <= 1.0 {
        1.0
    } else {
        (1.0 - d / range).max(0.0).powf(1.5) / (1.0 + 0.08 * d)
    };
    let (pan, front) = if d < 1e-3 {
        (0.0, 1.0)
    } else {
        let dir = [rel[0] / d, rel[1] / d, rel[2] / d];
        let right = norm(cross(l.forward, l.up));
        // Les sources très proches restent au centre (pas de bascule brutale en les traversant).
        let near = (d / 2.0).min(1.0);
        (
            dot(dir, right).clamp(-1.0, 1.0) * near,
            dot(dir, norm(l.forward)),
        )
    };
    let ang = (pan * 0.5 + 0.5) * std::f32::consts::FRAC_PI_2;
    let far = (d / range).sqrt().min(1.0);
    let cutoff = (16_000.0 + (1_500.0 - 16_000.0) * far) * (0.6 + 0.4 * (front * 0.5 + 0.5));
    Spat {
        gl: ang.cos() * att,
        gr: ang.sin() * att,
        cutoff,
        dist: d,
    }
}

struct Voice {
    sample: Sample,
    pos: f64,
    step: f64,
    gain: f32,
    bus: Bus,
    spatial: Option<[f32; 3]>,
    range: f32,
    lp: OnePole,
    gl: f32,
    gr: f32,
    fresh: bool,
    age: f32,
    loudness: f32,
}

/// Le mixeur.
pub struct Mixer {
    rate: f32,
    bank: Arc<Bank>,
    rx: Receiver<Cmd>,
    voices: Vec<Voice>,
    listener: Listener,
    volumes: Volumes,
    vol_smooth: [Smooth; 5],
    env: Environment,
    underwater: Smooth,
    uw_lp: [OnePole; 2],
    reverb_amount: Smooth,
    targets: AmbienceTargets,
    ambience: Ambience,
    music: Music,
    fx_reverb: Reverb,
    rng: Rng,
    send: Vec<f32>,
}

impl Mixer {
    pub fn new(rate: f32, bank: Arc<Bank>, rx: Receiver<Cmd>) -> Self {
        let v = Volumes::default();
        Self {
            rate,
            bank,
            rx,
            voices: Vec::with_capacity(MAX_VOICES),
            listener: Listener::default(),
            volumes: v,
            vol_smooth: [
                Smooth::new(v.master),
                Smooth::new(v.music),
                Smooth::new(v.ambient),
                Smooth::new(v.effects),
                Smooth::new(v.creatures),
            ],
            env: Environment::default(),
            underwater: Smooth::new(0.0),
            uw_lp: [OnePole::default(); 2],
            reverb_amount: Smooth::new(0.0),
            targets: AmbienceTargets::default(),
            ambience: Ambience::new(rate, 0xA5A5),
            music: Music::new(rate, 0x5EED),
            fx_reverb: Reverb::new(rate),
            rng: Rng::new(0xC0FFEE),
            send: vec![0.0; CHUNK],
        }
    }

    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    pub fn music(&self) -> &Music {
        &self.music
    }

    pub fn listener(&self) -> &Listener {
        &self.listener
    }

    pub fn volumes(&self) -> Volumes {
        self.volumes
    }

    fn apply(&mut self, c: Cmd) {
        match c {
            Cmd::Play(r) => self.play(r),
            Cmd::Listener(l) => self.listener = l,
            Cmd::Volumes(v) => {
                self.volumes = Volumes {
                    master: v.master.clamp(0.0, 1.0),
                    music: v.music.clamp(0.0, 1.0),
                    ambient: v.ambient.clamp(0.0, 1.0),
                    effects: v.effects.clamp(0.0, 1.0),
                    creatures: v.creatures.clamp(0.0, 1.0),
                };
            }
            Cmd::Environment(e) => self.env = e,
            Cmd::Ambience(a) => self.targets = a,
            Cmd::Theme(t) => self.music.set_theme(t),
            Cmd::MusicMode(m) => self.music.set_mode(m),
            Cmd::StopAll => self.voices.clear(),
        }
    }

    fn play(&mut self, r: PlayReq) {
        if r.gain <= 0.0 {
            return;
        }
        let variant = self.rng.below(r.sfx.variants().max(1));
        let Some(sample) = self.bank.get(r.sfx, variant) else {
            return;
        };
        let loudness = match r.pos {
            Some(p) => {
                let s = spatialize(&self.listener, p, r.range);
                r.gain * s.gl.max(s.gr)
            }
            None => r.gain,
        };
        if loudness < 0.002 {
            return;
        }
        // Évite d'empiler la même source au même instant (plusieurs événements dans un tick).
        let dup = self
            .voices
            .iter()
            .filter(|v| v.age < 0.03 && Arc::ptr_eq(&v.sample, &sample) && v.bus == r.bus)
            .count();
        if dup >= 2 {
            return;
        }
        if self.voices.len() >= MAX_VOICES {
            let (i, quietest) = self
                .voices
                .iter()
                .enumerate()
                .map(|(i, v)| (i, v.loudness))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap_or((0, 0.0));
            if quietest >= loudness {
                return;
            }
            self.voices.swap_remove(i);
        }
        // Légère variation de hauteur pour casser la répétition.
        let jitter = 1.0 + (self.rng.unit() - 0.5) * 0.08;
        self.voices.push(Voice {
            sample,
            pos: 0.0,
            step: f64::from((r.pitch * jitter).clamp(0.25, 4.0)),
            gain: r.gain,
            bus: r.bus,
            spatial: r.pos,
            range: r.range,
            lp: OnePole::default(),
            gl: 0.0,
            gr: 0.0,
            fresh: true,
            age: 0.0,
            loudness,
        });
    }

    /// Remplit `out` (stéréo entrelacé, écrasé) avec le mixage.
    pub fn render(&mut self, out: &mut [f32]) {
        let mut done = 0;
        let total = out.len() / 2;
        while done < total {
            let n = (total - done).min(CHUNK);
            while let Ok(c) = self.rx.try_recv() {
                self.apply(c);
            }
            self.render_chunk(&mut out[done * 2..(done + n) * 2]);
            done += n;
        }
    }

    fn render_chunk(&mut self, out: &mut [f32]) {
        let n = out.len() / 2;
        let dt = n as f32 / self.rate;
        out.fill(0.0);
        let vols = self.volumes;
        let sm = [
            vols.master,
            vols.music,
            vols.ambient,
            vols.effects,
            vols.creatures,
        ];
        let mut v = [0.0f32; 5];
        for (i, s) in self.vol_smooth.iter_mut().enumerate() {
            s.target = sm[i];
            v[i] = s.step(dt, 0.06);
        }
        let master = v[0];
        let bus_vol = |b: Bus| match b {
            Bus::Music => v[1],
            Bus::Ambient => v[2],
            Bus::Effects => v[3],
            Bus::Creatures => v[4],
        };
        self.reverb_amount.target = self.env.reverb.clamp(0.0, 1.0);
        let cave = self.reverb_amount.step(dt, 0.8);
        self.fx_reverb.size = 0.35 + 0.6 * cave;
        let send_base = 0.06 + 0.5 * cave;

        // ── Voix ──
        let send = &mut self.send[..n];
        send.fill(0.0);
        let listener = self.listener;
        let rate = self.rate;
        for voice in &mut self.voices {
            let bv = bus_vol(voice.bus);
            let (tl, tr, fc, dist) = match voice.spatial {
                Some(p) => {
                    let s = spatialize(&listener, p, voice.range);
                    (s.gl, s.gr, s.cutoff, s.dist)
                }
                None => (SQRT_HALF, SQRT_HALF, 20_000.0, 0.0),
            };
            let g = voice.gain * bv;
            let (tl, tr) = (tl * g, tr * g);
            voice.loudness = tl.max(tr);
            if voice.fresh {
                voice.gl = tl;
                voice.gr = tr;
                voice.fresh = false;
            }
            let a = OnePole::coef(fc, rate);
            let (g0l, g0r) = (voice.gl, voice.gr);
            let send_k = send_base * (0.4 + (dist / voice.range.max(1.0)).min(1.0));
            let data = &voice.sample;
            let mut pos = voice.pos;
            let mut ended = false;
            for i in 0..n {
                let ip = pos as usize;
                if ip + 1 >= data.len() {
                    ended = true;
                    break;
                }
                let frac = (pos - ip as f64) as f32;
                let s = data[ip] + (data[ip + 1] - data[ip]) * frac;
                let s = if fc < 19_000.0 { voice.lp.lp(s, a) } else { s };
                let k = (i + 1) as f32 / n as f32;
                let gl = g0l + (tl - g0l) * k;
                let gr = g0r + (tr - g0r) * k;
                out[2 * i] += s * gl;
                out[2 * i + 1] += s * gr;
                send[i] += s * (gl + gr) * 0.5 * send_k;
                pos += voice.step;
            }
            voice.pos = pos;
            voice.gl = tl;
            voice.gr = tr;
            voice.age += dt;
            if ended {
                voice.pos = f64::INFINITY;
            }
        }
        self.voices.retain(|v| v.pos.is_finite());

        // ── Ambiances (volume du canal ambiant) puis sons ponctuels ──
        let amb_gain = bus_vol(Bus::Ambient);
        self.ambience
            .render(out, amb_gain, &listener, &self.targets);
        let mut pend = std::mem::take(&mut self.ambience.pending);
        for r in pend.drain(..) {
            self.play(r);
        }
        self.ambience.pending = pend;

        // ── Réverbération des effets ──
        for i in 0..n {
            let (rl, rr) = self.fx_reverb.process(self.send[i]);
            out[2 * i] += rl * 2.2;
            out[2 * i + 1] += rr * 2.2;
        }

        // ── Musique ──
        self.music.render(out, v[1]);

        // ── Étouffement sous l'eau, volume général, limiteur ──
        self.underwater.target = if self.env.underwater { 1.0 } else { 0.0 };
        let uw = self.underwater.step(dt, 0.15);
        let a_uw = OnePole::coef(700.0, self.rate);
        for i in 0..n {
            for c in 0..2 {
                let x = out[2 * i + c];
                let y = if uw > 0.001 {
                    let f = self.uw_lp[c].lp(x, a_uw);
                    x + (f * 0.85 - x) * uw
                } else {
                    // Garde l'état du filtre à jour pour éviter un saut à la plongée.
                    self.uw_lp[c].lp(x, a_uw);
                    x
                };
                out[2 * i + c] = soft_clip(y * master);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfx::Material;

    fn setup() -> (Mixer, crossbeam_channel::Sender<Cmd>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let bank = Bank::new();
        bank.build(Sfx::Break(Material::Stone), None);
        bank.build(Sfx::UiClick, None);
        bank.build(Sfx::Step(Material::Grass), None);
        (Mixer::new(44_100.0, bank, rx), tx)
    }

    fn render_secs(m: &mut Mixer, secs: f32) -> Vec<f32> {
        let mut out = vec![0.0f32; (secs * 44_100.0) as usize * 2];
        m.render(&mut out);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        out
    }

    fn lr_energy(out: &[f32]) -> (f32, f32) {
        let mut e = (0.0, 0.0);
        for f in out.chunks(2) {
            e.0 += f[0] * f[0];
            e.1 += f[1] * f[1];
        }
        e
    }

    fn silent_music(tx: &crossbeam_channel::Sender<Cmd>) {
        tx.send(Cmd::MusicMode(MusicMode::Off)).unwrap();
    }

    #[test]
    fn silence_without_voices() {
        let (mut m, tx) = setup();
        silent_music(&tx);
        let out = render_secs(&mut m, 0.5);
        assert!(out.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn voice_plays_then_ends() {
        let (mut m, tx) = setup();
        silent_music(&tx);
        tx.send(Cmd::Play(PlayReq::new(Sfx::UiClick, Bus::Effects)))
            .unwrap();
        let out = render_secs(&mut m, 0.02);
        assert!(lr_energy(&out).0 > 0.0);
        assert_eq!(m.voice_count(), 1);
        render_secs(&mut m, 1.0);
        assert_eq!(m.voice_count(), 0);
    }

    #[test]
    fn panning_follows_the_listener_orientation() {
        let (mut m, tx) = setup();
        silent_music(&tx);
        // Auditeur face à -Z : +X est à droite.
        tx.send(Cmd::Play(
            PlayReq::new(Sfx::Break(Material::Stone), Bus::Effects).at([5.0, 0.0, 0.0]),
        ))
        .unwrap();
        let (l, r) = lr_energy(&render_secs(&mut m, 0.4));
        assert!(r > l * 3.0, "l={l} r={r}");
        render_secs(&mut m, 1.0);
        tx.send(Cmd::Listener(Listener {
            forward: [0.0, 0.0, 1.0],
            ..Default::default()
        }))
        .unwrap();
        tx.send(Cmd::Play(
            PlayReq::new(Sfx::Break(Material::Stone), Bus::Effects).at([5.0, 0.0, 0.0]),
        ))
        .unwrap();
        let (l, r) = lr_energy(&render_secs(&mut m, 0.4));
        assert!(l > r * 3.0, "l={l} r={r}");
    }

    #[test]
    fn distance_attenuates_and_range_cuts_off() {
        let l = Listener::default();
        let mut last = f32::MAX;
        for d in [1.0, 3.0, 8.0, 20.0, 35.0] {
            let s = spatialize(&l, [0.0, 0.0, -d], 40.0);
            let loud = s.gl.max(s.gr);
            assert!(loud < last, "d={d}");
            last = loud;
        }
        let s = spatialize(&l, [0.0, 0.0, -50.0], 40.0);
        assert_eq!(s.gl.max(s.gr), 0.0);
        let near = spatialize(&l, [0.0, 0.0, -1.0], 40.0);
        let far = spatialize(&l, [0.0, 0.0, -30.0], 40.0);
        assert!(far.cutoff < near.cutoff);
    }

    #[test]
    fn behind_sources_are_darker() {
        let l = Listener::default();
        let front = spatialize(&l, [0.0, 0.0, -10.0], 40.0);
        let back = spatialize(&l, [0.0, 0.0, 10.0], 40.0);
        assert!(back.cutoff < front.cutoff);
    }

    #[test]
    fn bus_and_master_volumes_apply() {
        let (mut m, tx) = setup();
        silent_music(&tx);
        tx.send(Cmd::Play(PlayReq::new(Sfx::UiClick, Bus::Effects)))
            .unwrap();
        let loud = lr_energy(&render_secs(&mut m, 0.2)).0;
        tx.send(Cmd::Volumes(Volumes {
            effects: 0.0,
            ..Default::default()
        }))
        .unwrap();
        render_secs(&mut m, 0.5);
        tx.send(Cmd::Play(PlayReq::new(Sfx::UiClick, Bus::Effects)))
            .unwrap();
        let quiet = lr_energy(&render_secs(&mut m, 0.2)).0;
        assert!(
            loud > 0.0 && quiet < loud * 1e-3,
            "loud={loud} quiet={quiet}"
        );
        tx.send(Cmd::Volumes(Volumes {
            master: 0.0,
            effects: 1.0,
            ..Default::default()
        }))
        .unwrap();
        render_secs(&mut m, 0.5);
        tx.send(Cmd::Play(PlayReq::new(Sfx::UiClick, Bus::Effects)))
            .unwrap();
        assert!(lr_energy(&render_secs(&mut m, 0.2)).0 < loud * 1e-3);
    }

    #[test]
    fn underwater_muffles_highs() {
        let (mut m, tx) = setup();
        silent_music(&tx);
        let play = |tx: &crossbeam_channel::Sender<Cmd>| {
            tx.send(Cmd::Play(PlayReq::new(Sfx::UiClick, Bus::Effects)))
                .unwrap();
        };
        play(&tx);
        let dry = lr_energy(&render_secs(&mut m, 0.2)).0;
        tx.send(Cmd::Environment(Environment {
            underwater: true,
            reverb: 0.0,
        }))
        .unwrap();
        render_secs(&mut m, 1.0);
        play(&tx);
        let wet = lr_energy(&render_secs(&mut m, 0.2)).0;
        assert!(wet < dry * 0.5, "dry={dry} wet={wet}");
    }

    #[test]
    fn voice_count_is_bounded_and_duplicates_are_dropped() {
        let (mut m, tx) = setup();
        silent_music(&tx);
        for _ in 0..200 {
            tx.send(Cmd::Play(PlayReq::new(
                Sfx::Step(Material::Grass),
                Bus::Effects,
            )))
            .unwrap();
        }
        render_secs(&mut m, 0.01);
        assert!(m.voice_count() <= 10, "{}", m.voice_count());
        // Positions différentes : plafonné à MAX_VOICES.
        for i in 0..200 {
            let mut r = PlayReq::new(Sfx::Break(Material::Stone), Bus::Effects).at([
                i as f32 * 0.1,
                0.0,
                -2.0,
            ]);
            r.gain = 1.0;
            tx.send(Cmd::Play(r)).unwrap();
            render_secs(&mut m, 0.05);
        }
        assert!(m.voice_count() <= MAX_VOICES);
    }

    #[test]
    fn music_and_ambience_mix_without_blowing_up() {
        let (mut m, tx) = setup();
        tx.send(Cmd::MusicMode(MusicMode::Continuous)).unwrap();
        tx.send(Cmd::Ambience(AmbienceTargets {
            wind: 1.0,
            cave: 1.0,
            water: 1.0,
            birds: 1.0,
            crickets: 1.0,
            frogs: 1.0,
            lava: 1.0,
        }))
        .unwrap();
        tx.send(Cmd::Volumes(Volumes {
            master: 1.0,
            music: 1.0,
            ambient: 1.0,
            effects: 1.0,
            creatures: 1.0,
        }))
        .unwrap();
        let out = render_secs(&mut m, 8.0);
        assert!(lr_energy(&out).0 > 0.1);
    }

    #[test]
    fn stop_all_clears_voices() {
        let (mut m, tx) = setup();
        tx.send(Cmd::Play(PlayReq::new(Sfx::UiClick, Bus::Effects)))
            .unwrap();
        render_secs(&mut m, 0.01);
        tx.send(Cmd::StopAll).unwrap();
        render_secs(&mut m, 0.01);
        assert_eq!(m.voice_count(), 0);
    }
}
