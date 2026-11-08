//! État d'animation d'un mob et évaluation de sa pose.
//!
//! Chaque mob possède un [`Animator`] minuscule (quelques dizaines d'octets) ; le [`Rig`]
//! compilé, lui, est partagé. Une frame se déroule en deux temps :
//!
//! 1. [`Animator::update`] fait avancer l'état à partir du mouvement observé ([`AnimInput`]) :
//!    vitesse lissée, phase de marche synchronisée sur la distance parcourue (pas de patinage),
//!    variantes d'idle aléatoires, actions (blessure, attaque…), regard, pilotes secondaires.
//! 2. [`Animator::evaluate`] produit la palette de matrices de peau : mélange des clips de la
//!    couche de base (idle ↔ démarche lente ↔ démarche rapide, sol ↔ nage), couches
//!    superposées (additives ou de remplacement), regard, pilotes, puis hiérarchie. Aucune
//!    allocation : tout vit sur la pile ou dans le [`Scratch`] réutilisé.
//!
//! Le premier temps est quasi gratuit ; le second ne coûte que quelques dizaines de
//! multiplications par os, et le gameplay peut l'omettre (mob hors champ, très loin) sans
//! perdre la continuité de l'animation.

use glam::{Affine3A, EulerRot, Quat, Vec3, Vec4};

use crate::rig::{Action, Clip, DriverProp, DriverSrc, Gait, Rig, MAX_BONES, MAX_DRIVERS};
use crate::xf::{Mat34, Xf};

/// Mouvement observé d'un mob, calculé par le gameplay.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimInput {
    /// Vitesse horizontale (blocs/s).
    pub speed: f32,
    /// Vitesse verticale (blocs/s).
    pub vspeed: f32,
    /// Vitesse de rotation (rad/s, signée).
    pub turn: f32,
    pub grounded: bool,
    pub in_water: bool,
    /// Cible du regard `(lacet, hauteur)` en radians, relative au cap du mob (hauteur > 0 :
    /// vers le haut). `None` : regarde droit devant.
    pub look: Option<[f32; 2]>,
}

#[derive(Clone, Copy, Debug)]
struct Oneshot {
    clip: u32,
    t: f32,
    /// Atténuation supplémentaire (annulation d'une variante quand le mob repart).
    fade: f32,
    cancel: bool,
}

/// État d'animation d'un mob.
#[derive(Clone, Debug)]
pub struct Animator {
    speed: f32,
    turn: f32,
    vspeed: f32,
    swim: f32,
    air: f32,
    phase: f32,
    idle_t: f32,
    still_for: f32,
    next_variant: f32,
    variant: Option<Oneshot>,
    actions: [Option<Oneshot>; Action::COUNT],
    look: [f32; 2],
    drivers: [f32; MAX_DRIVERS],
    rng: u32,
}

/// Tampons de travail d'`evaluate` (à créer une fois et réutiliser).
pub struct Scratch {
    acc: [Acc; MAX_BONES],
    local: [Xf; MAX_BONES],
    world: [Affine3A; MAX_BONES],
}

impl Default for Scratch {
    fn default() -> Self {
        Self {
            acc: [Acc::ZERO; MAX_BONES],
            local: [Xf::IDENTITY; MAX_BONES],
            world: [Affine3A::IDENTITY; MAX_BONES],
        }
    }
}

impl Scratch {
    pub fn new() -> Box<Self> {
        Box::default()
    }
}

#[derive(Clone, Copy)]
struct Acc {
    q: Vec4,
    t: Vec3,
    s: Vec3,
    w: f32,
}

impl Acc {
    const ZERO: Acc = Acc {
        q: Vec4::ZERO,
        t: Vec3::ZERO,
        s: Vec3::ZERO,
        w: 0.0,
    };
}

#[inline]
fn approach(cur: f32, target: f32, tau: f32, dt: f32) -> f32 {
    cur + (target - cur) * (1.0 - (-dt / tau).exp())
}

#[inline]
fn smooth01(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Enveloppe d'une couche jouée une fois : montée, plateau, descente.
fn envelope(t: f32, clip: &Clip) -> f32 {
    let up = if clip.fade_in > 1e-4 {
        t / clip.fade_in
    } else {
        1.0
    };
    let down = if clip.fade_out > 1e-4 {
        (clip.duration - t) / clip.fade_out
    } else {
        1.0
    };
    smooth01(up.min(down))
}

/// Poids de mélange d'un jeu de démarches à la vitesse `s` : jusqu'à deux démarches voisines et
/// le poids de l'idle.
fn gait_blend(gaits: &[Gait], s: f32) -> ([(usize, f32); 2], usize, f32) {
    let first = &gaits[0];
    if s < first.speed {
        let w = smooth01(s / first.speed);
        return ([(0, w), (0, 0.0)], 1, 1.0 - w);
    }
    for i in 0..gaits.len() - 1 {
        if s < gaits[i + 1].speed {
            let t = (s - gaits[i].speed) / (gaits[i + 1].speed - gaits[i].speed);
            return ([(i, 1.0 - t), (i + 1, t)], 2, 0.0);
        }
    }
    ([(gaits.len() - 1, 1.0), (0, 0.0)], 1, 0.0)
}

impl Animator {
    /// `seed` décorrèle les mobs (par exemple l'identifiant de l'entité) : phases et délais
    /// d'idle différents, donc pas de troupeau qui marche au pas.
    pub fn new(seed: u32) -> Self {
        let mut a = Self {
            speed: 0.0,
            turn: 0.0,
            vspeed: 0.0,
            swim: 0.0,
            air: 0.0,
            phase: 0.0,
            idle_t: 0.0,
            still_for: 0.0,
            next_variant: 0.0,
            variant: None,
            actions: [None; Action::COUNT],
            look: [0.0; 2],
            drivers: [0.0; MAX_DRIVERS],
            rng: seed.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B,
        };
        if a.rng == 0 {
            a.rng = 0x1234_5679;
        }
        a.phase = a.rand();
        a.idle_t = a.rand() * 20.0;
        a.next_variant = 1.0 + a.rand() * 3.0;
        a
    }

    fn rand(&mut self) -> f32 {
        // xorshift32
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Vitesse lissée courante (blocs/s).
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Phase de marche (tours).
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Vrai si une variante d'idle ou une action est en cours.
    pub fn is_playing_oneshot(&self) -> bool {
        self.variant.is_some() || self.actions.iter().any(Option::is_some)
    }

    /// Déclenche une action (si le rig en a une pour elle) ; la relance si elle jouait déjà.
    pub fn trigger(&mut self, rig: &Rig, action: Action) {
        if let Some(c) = rig.actions[action as usize] {
            self.actions[action as usize] = Some(Oneshot {
                clip: c as u32,
                t: 0.0,
                fade: 1.0,
                cancel: false,
            });
        }
    }

    /// Joue immédiatement la variante d'idle `clip` (tests, outils).
    pub fn play_variant(&mut self, clip: usize) {
        self.variant = Some(Oneshot {
            clip: clip as u32,
            t: 0.0,
            fade: 1.0,
            cancel: false,
        });
    }

    /// Fait avancer l'état de `dt` secondes.
    pub fn update(&mut self, rig: &Rig, inp: &AnimInput, dt: f32) {
        let dt = dt.clamp(0.0, 0.25);
        if dt <= 0.0 {
            return;
        }
        // Grandeurs lissées.
        let tau = if inp.speed > self.speed { 0.08 } else { 0.14 };
        self.speed = approach(self.speed, inp.speed.max(0.0), tau, dt);
        if self.speed < 0.01 && inp.speed < 0.01 {
            self.speed = 0.0;
        }
        self.turn = approach(self.turn, inp.turn, 0.12, dt);
        self.vspeed = approach(self.vspeed, inp.vspeed, 0.1, dt);
        self.swim = approach(self.swim, f32::from(inp.in_water), 0.15, dt);
        let air_target = f32::from(!inp.grounded && !inp.in_water);
        self.air = approach(self.air, air_target, 0.07, dt);

        // Phase de marche : une boucle de la démarche = `stride` blocs parcourus.
        let gaits = self.active_gaits(rig);
        if !gaits.is_empty() && self.speed > 0.02 {
            let (pair, n, _) = gait_blend(gaits, self.speed);
            let stride: f32 = pair[..n].iter().map(|&(i, w)| gaits[i].stride * w).sum::<f32>()
                / pair[..n].iter().map(|&(_, w)| w).sum::<f32>().max(1e-4);
            self.phase = (self.phase + self.speed / stride.max(0.05) * dt).fract();
        }
        // Idle : le temps avance toujours.
        if let Some(i) = rig.idle {
            let d = rig.clips[i].duration;
            self.idle_t = (self.idle_t + dt) % (d * 64.0);
        }

        // Variantes d'idle.
        let still = self.speed < 0.05 && (inp.grounded || inp.in_water);
        if let Some(v) = &mut self.variant {
            let clip = &rig.clips[v.clip as usize];
            v.t += dt;
            if !still && !v.cancel {
                v.cancel = true;
            }
            if v.cancel {
                v.fade -= dt / 0.18;
            }
            if v.t >= clip.duration || v.fade <= 0.0 {
                self.variant = None;
            }
        }
        if still && self.variant.is_none() && !rig.idles.is_empty() {
            self.still_for += dt;
            if self.still_for >= self.next_variant {
                if let Some(c) = self.pick_variant(rig) {
                    self.variant = Some(Oneshot {
                        clip: c as u32,
                        t: 0.0,
                        fade: 1.0,
                        cancel: false,
                    });
                }
                self.still_for = 0.0;
                let [lo, hi] = rig.idle_gap;
                self.next_variant = lo + self.rand() * (hi - lo).max(0.0);
            }
        } else if !still {
            self.still_for = 0.0;
        }

        // Actions.
        for a in &mut self.actions {
            if let Some(o) = a.as_mut() {
                o.t += dt;
                if o.t >= rig.clips[o.clip as usize].duration {
                    *a = None;
                }
            }
        }

        // Regard.
        if let Some(l) = &rig.look {
            let target = inp.look.unwrap_or([0.0, 0.0]);
            let t = [
                target[0].clamp(-l.yaw, l.yaw),
                target[1].clamp(-l.pitch, l.pitch),
            ];
            let tau = if inp.look.is_some() { 0.16 } else { 0.30 };
            self.look[0] = approach(self.look[0], t[0], tau, dt);
            self.look[1] = approach(self.look[1], t[1], tau, dt);
        }

        // Pilotes secondaires.
        for (i, d) in rig.drivers.iter().enumerate() {
            let src = match d.src {
                DriverSrc::Turn => self.turn,
                DriverSrc::Speed => (self.speed / rig.top_speed).min(1.5),
                DriverSrc::VSpeed => self.vspeed,
            };
            let target = (src * d.gain).clamp(-d.max, d.max);
            self.drivers[i] = approach(self.drivers[i], target, d.smooth, dt);
        }
    }

    fn pick_variant(&mut self, rig: &Rig) -> Option<usize> {
        let total: f32 = rig.idles.iter().map(|&(_, w)| w).sum();
        if total <= 0.0 {
            return None;
        }
        let mut r = self.rand() * total;
        for &(c, w) in &rig.idles {
            if r < w {
                return Some(c);
            }
            r -= w;
        }
        rig.idles.last().map(|&(c, _)| c)
    }

    /// Jeu de démarches actif (sol ou nage).
    fn active_gaits<'a>(&self, rig: &'a Rig) -> &'a [Gait] {
        match (rig.gaits.is_empty(), rig.swim.is_empty()) {
            (false, true) => &rig.gaits,
            (true, false) => &rig.swim,
            (true, true) => &[],
            (false, false) => {
                if self.swim > 0.5 {
                    &rig.swim
                } else {
                    &rig.gaits
                }
            }
        }
    }

    /// Calcule la palette de matrices de peau (`out.len() >= rig.bone_count()`).
    pub fn evaluate(&self, rig: &Rig, sc: &mut Scratch, out: &mut [Mat34]) {
        let nb = rig.bones.len();
        debug_assert!(out.len() >= nb);
        for a in &mut sc.acc[..nb] {
            *a = Acc::ZERO;
        }

        // 1. Couche de base : idle + démarches (sol et/ou nage).
        let (gw, sw) = match (rig.gaits.is_empty(), rig.swim.is_empty()) {
            (false, true) => (1.0, 0.0),
            (true, false) => (0.0, 1.0),
            (true, true) => (0.0, 0.0),
            (false, false) => (1.0 - self.swim, self.swim),
        };
        let mut idle_w = if rig.gaits.is_empty() && rig.swim.is_empty() {
            1.0
        } else {
            0.0
        };
        for (gaits, set_w) in [(&rig.gaits, gw), (&rig.swim, sw)] {
            if gaits.is_empty() || set_w <= 0.0 {
                continue;
            }
            let (pair, n, iw) = gait_blend(gaits, self.speed);
            idle_w += iw * set_w;
            for &(gi, w) in &pair[..n] {
                accumulate(&mut sc.acc[..nb], &rig.clips[gaits[gi].clip], self.phase, w * set_w);
            }
        }
        if idle_w > 1e-4 {
            if let Some(i) = rig.idle {
                let c = &rig.clips[i];
                accumulate(&mut sc.acc[..nb], c, self.idle_t / c.duration, idle_w);
            }
        }
        for (a, l) in sc.acc[..nb].iter().zip(sc.local[..nb].iter_mut()) {
            let norm = a.w.max(1.0);
            let rem = (1.0 - a.w).max(0.0);
            let q = a.q + Vec4::new(0.0, 0.0, 0.0, rem);
            *l = Xf {
                q: Quat::from_vec4(q).normalize(),
                t: a.t / norm,
                s: (a.s + Vec3::splat(rem)) / norm,
            };
        }

        // 2. Couches superposées.
        if let Some(v) = &self.variant {
            let c = &rig.clips[v.clip as usize];
            overlay(&mut sc.local[..nb], c, v.t / c.duration, envelope(v.t, c) * v.fade.clamp(0.0, 1.0));
        }
        for o in self.actions.iter().flatten() {
            let c = &rig.clips[o.clip as usize];
            overlay(&mut sc.local[..nb], c, o.t / c.duration, envelope(o.t, c));
        }
        if let (Some(i), true) = (rig.air, self.air > 0.01) {
            let c = &rig.clips[i];
            overlay(&mut sc.local[..nb], c, self.idle_t / c.duration, self.air);
        }

        // 3. Regard.
        if let Some(l) = &rig.look {
            if self.look[0].abs() > 1e-4 || self.look[1].abs() > 1e-4 {
                for &(b, w) in &l.bones {
                    let q = Quat::from_euler(EulerRot::YXZ, self.look[0] * w, -self.look[1] * w, 0.0);
                    let x = &mut sc.local[b as usize];
                    x.q = (x.q * q).normalize();
                }
            }
        }

        // 4. Pilotes secondaires.
        for (d, &v) in rig.drivers.iter().zip(&self.drivers) {
            if v.abs() < 1e-5 {
                continue;
            }
            let x = &mut sc.local[d.bone as usize];
            let r = v.to_radians();
            match d.prop {
                DriverProp::Rx => x.q = (x.q * Quat::from_rotation_x(r)).normalize(),
                DriverProp::Ry => x.q = (x.q * Quat::from_rotation_y(r)).normalize(),
                DriverProp::Rz => x.q = (x.q * Quat::from_rotation_z(r)).normalize(),
                DriverProp::Tx => x.t.x += v / 16.0,
                DriverProp::Ty => x.t.y += v / 16.0,
                DriverProp::Tz => x.t.z += v / 16.0,
            }
        }

        // 5. Hiérarchie → matrices de peau.
        for i in 0..nb {
            let b = &rig.bones[i];
            let l = &sc.local[i];
            let local = Affine3A::from_scale_rotation_translation(l.s, l.q, b.rest + l.t);
            let world = if b.parent >= 0 {
                sc.world[b.parent as usize] * local
            } else {
                local
            };
            sc.world[i] = world;
            out[i] = Mat34::from_affine(&(world * Affine3A::from_translation(-b.pivot)));
        }
    }
}

/// Ajoute la contribution pondérée d'un clip à la couche de base.
fn accumulate(acc: &mut [Acc], clip: &Clip, u: f32, w: f32) {
    if w <= 1e-4 {
        return;
    }
    for ch in 0..clip.channel_count() {
        let b = clip.channel_bone(ch);
        let x = clip.sample(ch, u);
        let a = &mut acc[b];
        // Hémisphère de l'identité (w ≥ 0), pour que la moyenne ne s'annule pas.
        let q = if x.q.w < 0.0 { -x.q } else { x.q };
        a.q += Vec4::from(q) * w;
        a.t += x.t * w;
        a.s += x.s * w;
        a.w += w;
    }
    // Un os absent du clip ne reçoit pas son poids : `evaluate` complète par la pose de repos.
}

/// Applique un clip par-dessus la pose locale (os pilotés par le clip uniquement).
fn overlay(local: &mut [Xf], clip: &Clip, u: f32, w: f32) {
    if w <= 1e-4 {
        return;
    }
    for ch in 0..clip.channel_count() {
        let b = clip.channel_bone(ch);
        let x = clip.sample(ch, u);
        let l = &mut local[b];
        if clip.additive {
            let qa = if x.q.w < 0.0 { -x.q } else { x.q };
            l.q = (l.q * Quat::IDENTITY.lerp(qa, w).normalize()).normalize();
            l.t += x.t * w;
            l.s *= Vec3::ONE.lerp(x.s, w);
        } else {
            *l = Xf::lerp(l, &x, w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::*;

    fn rig_def() -> RigDef {
        // root(0) ─ body(1) ─ head(2) ; legs: leg_l(3), leg_r(4) attachées à root.
        let bone = |n: &str, p: Option<&str>, pv: [f32; 3]| BoneDef {
            name: n.into(),
            parent: p.map(Into::into),
            pivot: pv,
        };
        let mut d = RigDef {
            bones: vec![
                bone("root", None, [0.0, 0.0, 0.0]),
                bone("body", Some("root"), [0.0, 8.0, 0.0]),
                bone("head", Some("body"), [0.0, 12.0, 4.0]),
                bone("leg_l", Some("root"), [-2.0, 8.0, 0.0]),
                bone("leg_r", Some("root"), [2.0, 8.0, 0.0]),
            ],
            boxes: vec![1, 2, 3, 4],
            ..RigDef::default()
        };
        let wave = |a: f32, p: f32| Some(Curve::Wave(Wave::sin(a, 1.0, p)));
        let ch = |b: &str, a: f32, p: f32| {
            let mut c = ChannelDef::new(b);
            c.rx = wave(a, p);
            c
        };
        let clip = |dur: f32, looping: bool, additive: bool, chans: Vec<ChannelDef>| ClipDef {
            duration: dur,
            looping,
            additive,
            fade: [0.1, 0.1],
            channels: chans,
        };
        d.clips.insert(
            "walk".into(),
            clip(0.8, true, false, vec![ch("leg_l", 35.0, 0.0), ch("leg_r", 35.0, 0.5)]),
        );
        d.clips.insert(
            "run".into(),
            clip(0.5, true, false, vec![ch("leg_l", 55.0, 0.0), ch("leg_r", 55.0, 0.5)]),
        );
        let mut breathe = ChannelDef::new("body");
        breathe.sy = Some(Curve::Wave(Wave {
            o: 1.0,
            ..Wave::sin(0.02, 1.0, 0.0)
        }));
        d.clips.insert("idle".into(), clip(3.0, true, false, vec![breathe]));
        let mut bow = ChannelDef::new("head");
        bow.rx = Some(Curve::Keys(vec![[0.0, 0.0], [0.5, 40.0], [1.0, 0.0]]));
        d.clips.insert("graze".into(), clip(2.0, false, false, vec![bow]));
        let mut flinch = ChannelDef::new("body");
        flinch.rx = Some(Curve::Keys(vec![[0.0, 0.0], [0.3, -15.0], [1.0, 0.0]]));
        d.clips.insert("hurt".into(), clip(0.4, false, true, vec![flinch]));
        d.gaits = vec![
            GaitDef {
                clip: "walk".into(),
                speed: 1.0,
            },
            GaitDef {
                clip: "run".into(),
                speed: 3.0,
            },
        ];
        d.idle = Some("idle".into());
        d.idles = vec![IdleDef {
            clip: "graze".into(),
            weight: 1.0,
        }];
        d.idle_gap = [1.0, 2.0];
        d.actions.insert("hurt".into(), "hurt".into());
        d.look = Some(LookDef {
            bones: vec![LookBone {
                bone: "head".into(),
                weight: 1.0,
            }],
            yaw: 60.0,
            pitch: 30.0,
            range: 8.0,
        });
        d.drivers = vec![DriverDef {
            bone: "body".into(),
            prop: "rz".into(),
            src: "turn".into(),
            gain: -10.0,
            max: 20.0,
            smooth: 0.1,
        }];
        d
    }

    fn setup() -> (Rig, Animator, Box<Scratch>, Vec<Mat34>) {
        let rig = Rig::compile(&rig_def()).unwrap();
        let n = rig.bone_count();
        (rig, Animator::new(7), Scratch::new(), vec![Mat34::IDENTITY; n])
    }

    fn run(a: &mut Animator, rig: &Rig, inp: &AnimInput, secs: f32) {
        let steps = (secs * 60.0) as usize;
        for _ in 0..steps {
            a.update(rig, inp, 1.0 / 60.0);
        }
    }

    fn mv(speed: f32) -> AnimInput {
        AnimInput {
            speed,
            grounded: true,
            ..AnimInput::default()
        }
    }

    /// Angle (°) de rotation de la peau d'un os autour de X, via l'image du vecteur +Y.
    fn tilt(m: &Mat34) -> f32 {
        let up = m.transform_point(Vec3::Y) - m.transform_point(Vec3::ZERO);
        up.z.atan2(up.y).to_degrees()
    }

    #[test]
    fn rest_pose_is_identity_skinning() {
        // Un rig sans clip donne des matrices identité (os à leur place).
        let mut d = rig_def();
        d.clips.clear();
        d.gaits.clear();
        d.idle = None;
        d.idles.clear();
        d.actions.clear();
        d.look = None;
        d.drivers.clear();
        let rig = Rig::compile(&d).unwrap();
        let mut out = vec![Mat34::IDENTITY; rig.bone_count()];
        Animator::new(1).evaluate(&rig, &mut Scratch::new(), &mut out);
        for m in out {
            for (a, b) in m.0.iter().zip(Mat34::IDENTITY.0) {
                assert!((a - b).abs() < 1e-5, "{m:?}");
            }
        }
    }

    #[test]
    fn standing_still_uses_idle_only() {
        let (rig, mut a, mut sc, mut out) = setup();
        run(&mut a, &rig, &mv(0.0), 0.5);
        a.evaluate(&rig, &mut sc, &mut out);
        assert!(tilt(&out[3]).abs() < 0.5, "pas de balancement des jambes à l'arrêt");
        assert!(tilt(&out[4]).abs() < 0.5);
    }

    #[test]
    fn walking_swings_legs_in_opposition() {
        let (rig, mut a, mut sc, mut out) = setup();
        run(&mut a, &rig, &mv(1.0), 1.0);
        let mut seen_l = (f32::MAX, f32::MIN);
        for _ in 0..60 {
            a.update(&rig, &mv(1.0), 1.0 / 60.0);
            a.evaluate(&rig, &mut sc, &mut out);
            let (l, r) = (tilt(&out[3]), tilt(&out[4]));
            assert!((l + r).abs() < 1.5, "jambes en opposition de phase : {l} {r}");
            seen_l = (seen_l.0.min(l), seen_l.1.max(l));
        }
        assert!(seen_l.1 > 30.0 && seen_l.0 < -30.0, "amplitude ≈ 35° : {seen_l:?}");
    }

    #[test]
    fn stride_is_locked_to_distance() {
        // À vitesse constante, une boucle de marche = `stride` blocs, que l'on aille vite ou non.
        let (rig, ..) = setup();
        for speed in [1.0f32, 2.0, 3.0] {
            let mut a = Animator::new(3);
            run(&mut a, &rig, &mv(speed), 1.0); // montée en vitesse
            let p0 = a.phase();
            let secs = 2.0;
            run(&mut a, &rig, &mv(speed), secs);
            let cycles = (a.phase() - p0).rem_euclid(1.0);
            // Partie fractionnaire du nombre de cycles attendu :
            let stride = if speed <= 1.0 {
                0.8
            } else if speed >= 3.0 {
                1.5
            } else {
                0.8 + (1.5 - 0.8) * (speed - 1.0) / 2.0
            };
            let expect = (speed * secs / stride).fract();
            let diff = ((cycles - expect + 0.5).rem_euclid(1.0) - 0.5).abs();
            assert!(diff < 0.08, "vitesse {speed} : {cycles} vs {expect}");
        }
    }

    #[test]
    fn blends_walk_to_run_continuously() {
        let (rig, mut a, mut sc, mut out) = setup();
        let mut prev: Option<f32> = None;
        let mut speed = 0.0;
        for _ in 0..600 {
            speed += 0.006; // 0 → 3,6 blocs/s sur 10 s
            a.update(&rig, &mv(speed), 1.0 / 60.0);
            a.evaluate(&rig, &mut sc, &mut out);
            let t = tilt(&out[3]);
            assert!(t.is_finite());
            if let Some(p) = prev {
                assert!((t - p).abs() < 20.0, "saut d'amplitude {p} → {t} à {speed}");
            }
            prev = Some(t);
        }
    }

    #[test]
    fn idle_variant_plays_after_delay_and_ends() {
        let (rig, mut a, mut sc, mut out) = setup();
        run(&mut a, &rig, &mv(0.0), 0.2);
        let mut max_head = 0.0f32;
        let mut played = false;
        for _ in 0..(8 * 60) {
            a.update(&rig, &mv(0.0), 1.0 / 60.0);
            if a.is_playing_oneshot() {
                played = true;
            }
            a.evaluate(&rig, &mut sc, &mut out);
            max_head = max_head.max(tilt(&out[2]).abs());
        }
        assert!(played, "une variante doit se déclencher à l'arrêt");
        assert!(max_head > 25.0, "tête baissée : {max_head}");
    }

    #[test]
    fn moving_cancels_variant() {
        let (rig, mut a, ..) = setup();
        a.play_variant(rig.clip_index("graze").unwrap());
        assert!(a.is_playing_oneshot());
        run(&mut a, &rig, &mv(2.0), 0.4);
        assert!(!a.is_playing_oneshot(), "repartir interrompt la variante");
    }

    #[test]
    fn hurt_action_is_additive_and_finishes() {
        let (rig, mut a, mut sc, mut out) = setup();
        run(&mut a, &rig, &mv(0.0), 0.1);
        a.trigger(&rig, Action::Hurt);
        let mut worst = 0.0f32;
        for _ in 0..12 {
            a.update(&rig, &mv(0.0), 1.0 / 60.0);
            a.evaluate(&rig, &mut sc, &mut out);
            worst = worst.max(tilt(&out[1]).abs());
        }
        assert!(worst > 5.0, "{worst}");
        run(&mut a, &rig, &mv(0.0), 0.6);
        assert!(!a.is_playing_oneshot());
        a.evaluate(&rig, &mut sc, &mut out);
        assert!(tilt(&out[1]).abs() < 1.0);
    }

    #[test]
    fn look_turns_head_within_limits() {
        let (rig, mut a, mut sc, mut out) = setup();
        let inp = AnimInput {
            look: Some([2.0, 0.0]), // bien au-delà du maximum (60°)
            ..mv(0.0)
        };
        run(&mut a, &rig, &inp, 2.0);
        a.evaluate(&rig, &mut sc, &mut out);
        // Image du vecteur avant (+Z) de la tête : tourné de 60° en lacet.
        let f = out[2].transform_point(Vec3::Z) - out[2].transform_point(Vec3::ZERO);
        let yaw = f.x.atan2(f.z).to_degrees();
        assert!((yaw - 60.0).abs() < 2.0, "{yaw}");
        // Sans cible, la tête revient.
        run(&mut a, &rig, &mv(0.0), 3.0);
        a.evaluate(&rig, &mut sc, &mut out);
        let f = out[2].transform_point(Vec3::Z) - out[2].transform_point(Vec3::ZERO);
        assert!(f.x.abs() < 0.05);
    }

    #[test]
    fn turning_drives_secondary_bone() {
        let (rig, mut a, mut sc, mut out) = setup();
        let inp = AnimInput {
            turn: 2.0,
            ..mv(0.0)
        };
        run(&mut a, &rig, &inp, 1.0);
        a.evaluate(&rig, &mut sc, &mut out);
        let x = out[1].transform_point(Vec3::X) - out[1].transform_point(Vec3::ZERO);
        let roll = x.y.atan2(x.x).to_degrees();
        assert!((roll - -20.0).abs() < 2.0, "{roll}"); // gain -10 × 2 rad/s = -20°, max 20
    }

    #[test]
    fn evaluation_is_finite_and_deterministic() {
        let (rig, ..) = setup();
        let mut sc = Scratch::new();
        let mut o1 = vec![Mat34::IDENTITY; rig.bone_count()];
        let mut o2 = o1.clone();
        let (mut a, mut b) = (Animator::new(9), Animator::new(9));
        for i in 0..400 {
            let inp = AnimInput {
                speed: (i as f32 * 0.05).sin().abs() * 4.0,
                turn: (i as f32 * 0.1).cos(),
                vspeed: (i as f32 * 0.03).sin() * 3.0,
                grounded: i % 50 > 10,
                in_water: i % 120 > 100,
                look: (i % 90 < 45).then_some([0.5, -0.2]),
            };
            a.update(&rig, &inp, 1.0 / 60.0);
            b.update(&rig, &inp, 1.0 / 60.0);
            a.evaluate(&rig, &mut sc, &mut o1);
            b.evaluate(&rig, &mut sc, &mut o2);
            assert_eq!(o1, o2);
            assert!(o1.iter().all(|m| m.0.iter().all(|v| v.is_finite())));
        }
    }

    #[test]
    fn different_seeds_desynchronise() {
        let (rig, ..) = setup();
        let (mut a, mut b) = (Animator::new(1), Animator::new(2));
        run(&mut a, &rig, &mv(1.0), 1.0);
        run(&mut b, &rig, &mv(1.0), 1.0);
        assert!((a.phase() - b.phase()).abs() > 1e-3);
    }

    #[test]
    fn large_dt_is_safe() {
        let (rig, mut a, mut sc, mut out) = setup();
        a.update(&rig, &mv(2.0), 30.0);
        a.evaluate(&rig, &mut sc, &mut out);
        assert!(out.iter().all(|m| m.0.iter().all(|v| v.is_finite())));
    }
}
