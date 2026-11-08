//! Rig compilé : os, clips « cuits » en tables d'échantillons et références résolues. Un
//! `Rig` est immuable et partagé par tous les mobs d'un même modèle ; l'état d'animation de
//! chaque mob (petit) vit dans [`crate::Animator`].

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use glam::{EulerRot, Quat, Vec3};

use crate::def::{ChannelDef, ClipDef, Curve, GaitDef, RigDef, Wave};
use crate::xf::Xf;

/// Nombre maximal d'os d'un rig (tableaux de travail sur la pile, palette de taille bornée).
pub const MAX_BONES: usize = 32;
/// Nombre maximal de pilotes secondaires.
pub const MAX_DRIVERS: usize = 8;
/// Pixels par bloc.
const PX: f32 = 1.0 / 16.0;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RigError {
    #[error("rig vide")]
    Empty,
    #[error("trop d'os : {0} (maximum {MAX_BONES})")]
    TooManyBones(usize),
    #[error("os en double : {0}")]
    DuplicateBone(String),
    #[error("os {bone} : parent {parent} absent ou défini après lui")]
    BadParent { bone: String, parent: String },
    #[error("boîte {index} : os {bone} inexistant")]
    BadBox { index: usize, bone: u16 },
    #[error("{0} : os inconnu")]
    UnknownBone(String),
    #[error("{0} : clip inconnu")]
    UnknownClip(String),
    #[error("clip {0} : durée invalide")]
    BadDuration(String),
    #[error("démarche de vitesse invalide ({0})")]
    BadGait(String),
    #[error("propriété ou source de pilote inconnue : {0}")]
    BadDriver(String),
    #[error("trop de pilotes secondaires")]
    TooManyDrivers,
    #[error("clés de courbe invalides dans le clip {0}")]
    BadKeys(String),
}

/// Un os à l'exécution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bone {
    /// Indice du parent (toujours < indice de l'os), `-1` pour une racine.
    pub parent: i32,
    /// Pivot au repos, espace du modèle, en blocs.
    pub pivot: Vec3,
    /// Position du pivot relative à celui du parent.
    pub rest: Vec3,
}

/// Canal cuit : os + décalage de ses échantillons dans [`Clip::data`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Channel {
    pub bone: u16,
    pub off: u32,
}

/// Clip cuit : `frames` échantillons par boucle (en boucle) ou `frames + 1` (sinon).
#[derive(Clone, Debug)]
pub struct Clip {
    pub duration: f32,
    pub looping: bool,
    pub additive: bool,
    pub fade_in: f32,
    pub fade_out: f32,
    frames: u32,
    channels: Vec<Channel>,
    data: Vec<Xf>,
}

impl Clip {
    /// Nombre de canaux.
    #[inline]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    #[inline]
    pub(crate) fn channel_bone(&self, ch: usize) -> usize {
        self.channels[ch].bone as usize
    }

    /// Échantillon du canal `ch` à la position normalisée `u` (boucle : modulo 1 ; sinon bornée
    /// à [0, 1]).
    #[inline]
    pub(crate) fn sample(&self, ch: usize, u: f32) -> Xf {
        let c = self.channels[ch];
        let n = self.frames as f32;
        let (i0, i1, w) = if self.looping {
            let f = (u - u.floor()) * n;
            let i0 = (f as u32).min(self.frames - 1);
            let i1 = if i0 + 1 >= self.frames { 0 } else { i0 + 1 };
            (i0, i1, f - i0 as f32)
        } else {
            let f = u.clamp(0.0, 1.0) * n;
            let i0 = (f as u32).min(self.frames - 1);
            (i0, i0 + 1, f - i0 as f32)
        };
        let a = &self.data[(c.off + i0) as usize];
        let b = &self.data[(c.off + i1) as usize];
        Xf::lerp(a, b, w)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Gait {
    pub clip: usize,
    pub speed: f32,
    /// Blocs parcourus par boucle.
    pub stride: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverProp {
    Rx,
    Ry,
    Rz,
    Tx,
    Ty,
    Tz,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverSrc {
    Turn,
    Speed,
    VSpeed,
}

#[derive(Clone, Copy, Debug)]
pub struct Driver {
    pub bone: u16,
    pub prop: DriverProp,
    pub src: DriverSrc,
    pub gain: f32,
    pub max: f32,
    pub smooth: f32,
}

#[derive(Clone, Debug)]
pub struct Look {
    pub bones: Vec<(u16, f32)>,
    pub yaw: f32,
    pub pitch: f32,
    pub range: f32,
}

/// Actions déclenchables (`Animator::trigger`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Hurt = 0,
    Attack = 1,
    Jump = 2,
}

impl Action {
    pub const COUNT: usize = 3;

    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "hurt" => Action::Hurt,
            "attack" => Action::Attack,
            "jump" => Action::Jump,
            _ => return None,
        })
    }
}

/// Rig compilé.
#[derive(Clone, Debug)]
pub struct Rig {
    pub bones: Vec<Bone>,
    pub bone_names: Vec<String>,
    /// Os de chaque boîte de la géométrie.
    pub box_bone: Vec<u16>,
    pub clips: Vec<Clip>,
    pub clip_names: Vec<String>,
    pub gaits: Vec<Gait>,
    pub swim: Vec<Gait>,
    pub idle: Option<usize>,
    pub idles: Vec<(usize, f32)>,
    pub idle_gap: [f32; 2],
    pub actions: [Option<usize>; Action::COUNT],
    pub air: Option<usize>,
    pub look: Option<Look>,
    pub drivers: Vec<Driver>,
    /// Vitesse de la démarche la plus rapide (blocs/s).
    pub top_speed: f32,
}

impl Rig {
    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    pub fn clip_index(&self, name: &str) -> Option<usize> {
        self.clip_names.iter().position(|n| n == name)
    }

    /// Compile un rig depuis sa description.
    pub fn compile(def: &RigDef) -> Result<Rig, RigError> {
        if def.bones.is_empty() {
            return Err(RigError::Empty);
        }
        if def.bones.len() > MAX_BONES {
            return Err(RigError::TooManyBones(def.bones.len()));
        }
        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut bones = Vec::with_capacity(def.bones.len());
        for (i, b) in def.bones.iter().enumerate() {
            if index.insert(&b.name, i).is_some() {
                return Err(RigError::DuplicateBone(b.name.clone()));
            }
            let pivot = Vec3::from(b.pivot) * PX;
            let (parent, rest) = match &b.parent {
                None => (-1, pivot),
                Some(p) => {
                    let pi = *index.get(p.as_str()).filter(|&&pi| pi < i).ok_or_else(|| {
                        RigError::BadParent {
                            bone: b.name.clone(),
                            parent: p.clone(),
                        }
                    })?;
                    (pi as i32, pivot - bones_pivot(&bones, pi))
                }
            };
            bones.push(Bone {
                parent,
                pivot,
                rest,
            });
        }
        for (i, &b) in def.boxes.iter().enumerate() {
            if usize::from(b) >= bones.len() {
                return Err(RigError::BadBox { index: i, bone: b });
            }
        }

        let mut clips = Vec::with_capacity(def.clips.len());
        let mut clip_names = Vec::with_capacity(def.clips.len());
        let mut clip_index: HashMap<&str, usize> = HashMap::new();
        for (name, c) in &def.clips {
            clip_index.insert(name, clips.len());
            clip_names.push(name.clone());
            clips.push(bake_clip(name, c, &index)?);
        }
        let clip = |name: &str| -> Result<usize, RigError> {
            clip_index
                .get(name)
                .copied()
                .ok_or_else(|| RigError::UnknownClip(name.to_owned()))
        };
        let gaits = |list: &[GaitDef]| -> Result<Vec<Gait>, RigError> {
            let mut v = Vec::with_capacity(list.len());
            for g in list {
                if !(g.speed > 0.0 && g.speed.is_finite()) {
                    return Err(RigError::BadGait(g.clip.clone()));
                }
                let ci = clip(&g.clip)?;
                v.push(Gait {
                    clip: ci,
                    speed: g.speed,
                    stride: g.speed * clips[ci].duration,
                });
            }
            v.sort_by(|a, b| a.speed.total_cmp(&b.speed));
            Ok(v)
        };
        let ground = gaits(&def.gaits)?;
        let swim = gaits(&def.swim)?;
        let top_speed = ground
            .iter()
            .chain(&swim)
            .map(|g| g.speed)
            .fold(0.0f32, f32::max)
            .max(0.1);
        let idle = def.idle.as_deref().map(clip).transpose()?;
        let mut idles = Vec::new();
        for i in &def.idles {
            idles.push((clip(&i.clip)?, i.weight.max(0.0)));
        }
        let mut actions = [None; Action::COUNT];
        for (k, v) in &def.actions {
            if let Some(a) = Action::from_name(k) {
                actions[a as usize] = Some(clip(v)?);
            }
        }
        let air = def.air.as_deref().map(clip).transpose()?;
        let bone_ix = |name: &str| -> Result<u16, RigError> {
            index
                .get(name)
                .map(|&i| i as u16)
                .ok_or_else(|| RigError::UnknownBone(name.to_owned()))
        };
        let look = match &def.look {
            None => None,
            Some(l) => {
                let mut bs = Vec::new();
                for b in &l.bones {
                    bs.push((bone_ix(&b.bone)?, b.weight));
                }
                Some(Look {
                    bones: bs,
                    yaw: l.yaw.to_radians(),
                    pitch: l.pitch.to_radians(),
                    range: l.range,
                })
            }
        };
        if def.drivers.len() > MAX_DRIVERS {
            return Err(RigError::TooManyDrivers);
        }
        let mut drivers = Vec::new();
        for d in &def.drivers {
            let prop = match d.prop.as_str() {
                "rx" => DriverProp::Rx,
                "ry" => DriverProp::Ry,
                "rz" => DriverProp::Rz,
                "tx" => DriverProp::Tx,
                "ty" => DriverProp::Ty,
                "tz" => DriverProp::Tz,
                other => return Err(RigError::BadDriver(other.to_owned())),
            };
            let src = match d.src.as_str() {
                "turn" => DriverSrc::Turn,
                "speed" => DriverSrc::Speed,
                "vspeed" => DriverSrc::VSpeed,
                other => return Err(RigError::BadDriver(other.to_owned())),
            };
            drivers.push(Driver {
                bone: bone_ix(&d.bone)?,
                prop,
                src,
                gain: d.gain,
                max: d.max,
                smooth: d.smooth.max(0.01),
            });
        }
        Ok(Rig {
            bone_names: def.bones.iter().map(|b| b.name.clone()).collect(),
            bones,
            box_bone: def.boxes.clone(),
            clips,
            clip_names,
            gaits: ground,
            swim,
            idle,
            idles,
            idle_gap: def.idle_gap,
            actions,
            air,
            look,
            drivers,
            top_speed,
        })
    }
}

fn bones_pivot(bones: &[Bone], i: usize) -> Vec3 {
    bones[i].pivot
}

// ------------------------------------------------------------------------------------ courbes

fn eval_wave(w: &Wave, u: f32) -> f32 {
    let main = if w.abs {
        w.a * (PI * (w.n * u + w.p)).sin().abs()
    } else {
        w.a * (TAU * (w.n * u + w.p)).sin()
    };
    w.o + main
        + if w.a2 != 0.0 {
            w.a2 * (TAU * (w.n2 * u + w.p2)).sin()
        } else {
            0.0
        }
}

/// Spline de Catmull-Rom (tangentes par différences centrées, non uniformes) sur des clés
/// `[t, v]` triées. En boucle, la courbe se referme sur elle-même.
fn eval_keys(keys: &[[f32; 2]], u: f32, looping: bool) -> f32 {
    let n = keys.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return keys[0][1];
    }
    // Clé `i` (indices hors bornes : bouclage décalé d'une période, sinon répétition).
    let key = |i: isize| -> [f32; 2] {
        if looping {
            let m = n as isize;
            let w = i.div_euclid(m);
            let k = keys[i.rem_euclid(m) as usize];
            [k[0] + w as f32, k[1]]
        } else if i < 0 {
            keys[0]
        } else if i as usize >= n {
            keys[n - 1]
        } else {
            keys[i as usize]
        }
    };
    let u = if looping {
        u - u.floor()
    } else {
        u.clamp(0.0, 1.0)
    };
    // Segment [i, i+1] contenant u.
    let mut i: isize = -1;
    if looping {
        // Avant la première clé : segment (dernière - 1 période) → première.
        if u >= keys[0][0] {
            i = (0..n).rev().find(|&k| keys[k][0] <= u).unwrap_or(0) as isize;
        }
    } else if u >= keys[n - 1][0] {
        return keys[n - 1][1];
    } else if u <= keys[0][0] {
        return keys[0][1];
    } else {
        i = (0..n - 1).rev().find(|&k| keys[k][0] <= u).unwrap_or(0) as isize;
    }
    let (p0, p1, p2, p3) = (key(i - 1), key(i), key(i + 1), key(i + 2));
    let span = (p2[0] - p1[0]).max(1e-6);
    let t = ((u - p1[0]) / span).clamp(0.0, 1.0);
    let tan = |a: [f32; 2], b: [f32; 2]| (b[1] - a[1]) / (b[0] - a[0]).max(1e-6) * span;
    let (m1, m2) = (tan(p0, p2), tan(p1, p3));
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * p1[1]
        + (t3 - 2.0 * t2 + t) * m1
        + (-2.0 * t3 + 3.0 * t2) * p2[1]
        + (t3 - t2) * m2
}

fn eval_curve(c: &Option<Curve>, u: f32, looping: bool, default: f32) -> f32 {
    match c {
        None => default,
        Some(Curve::Const(v)) => *v,
        Some(Curve::Wave(w)) => eval_wave(w, u),
        Some(Curve::Keys(k)) => eval_keys(k, u, looping),
    }
}

fn check_keys(name: &str, ch: &ChannelDef) -> Result<(), RigError> {
    for c in [
        &ch.rx, &ch.ry, &ch.rz, &ch.tx, &ch.ty, &ch.tz, &ch.s, &ch.sx, &ch.sy, &ch.sz,
    ]
    .into_iter()
    .flatten()
    {
        if let Curve::Keys(k) = c {
            let sorted = k.windows(2).all(|w| w[0][0] < w[1][0]);
            let finite = k.iter().all(|p| p[0].is_finite() && p[1].is_finite());
            if k.is_empty() || !sorted || !finite {
                return Err(RigError::BadKeys(name.to_owned()));
            }
        }
    }
    Ok(())
}

fn bake_clip(name: &str, def: &ClipDef, bones: &HashMap<&str, usize>) -> Result<Clip, RigError> {
    if !(def.duration > 0.0 && def.duration.is_finite()) {
        return Err(RigError::BadDuration(name.to_owned()));
    }
    let frames = ((def.duration * 30.0).ceil() as u32).clamp(16, 96);
    let samples = if def.looping { frames } else { frames + 1 };
    let mut channels = Vec::new();
    let mut data = Vec::new();
    for ch in &def.channels {
        if ch.is_empty() {
            continue;
        }
        check_keys(name, ch)?;
        let bone = *bones
            .get(ch.bone.as_str())
            .ok_or_else(|| RigError::UnknownBone(format!("{name}/{}", ch.bone)))?;
        let off = data.len() as u32;
        for k in 0..samples {
            let u = k as f32 / frames as f32;
            let l = def.looping;
            let rx = eval_curve(&ch.rx, u, l, 0.0).to_radians();
            let ry = eval_curve(&ch.ry, u, l, 0.0).to_radians();
            let rz = eval_curve(&ch.rz, u, l, 0.0).to_radians();
            let t = Vec3::new(
                eval_curve(&ch.tx, u, l, 0.0),
                eval_curve(&ch.ty, u, l, 0.0),
                eval_curve(&ch.tz, u, l, 0.0),
            ) * PX;
            let su = eval_curve(&ch.s, u, l, 1.0);
            let s = Vec3::new(
                su * eval_curve(&ch.sx, u, l, 1.0),
                su * eval_curve(&ch.sy, u, l, 1.0),
                su * eval_curve(&ch.sz, u, l, 1.0),
            );
            data.push(Xf {
                q: Quat::from_euler(EulerRot::YXZ, ry, rx, rz),
                t,
                s,
            });
        }
        channels.push(Channel {
            bone: bone as u16,
            off,
        });
    }
    Ok(Clip {
        duration: def.duration,
        looping: def.looping,
        additive: def.additive,
        fade_in: def.fade[0].max(0.0),
        fade_out: def.fade[1].max(0.0),
        frames,
        channels,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::BoneDef;

    pub(crate) fn two_bone_def() -> RigDef {
        let mut def = RigDef {
            bones: vec![
                BoneDef {
                    name: "root".into(),
                    parent: None,
                    pivot: [0.0, 0.0, 0.0],
                },
                BoneDef {
                    name: "arm".into(),
                    parent: Some("root".into()),
                    pivot: [0.0, 16.0, 0.0],
                },
            ],
            boxes: vec![0, 1],
            ..RigDef::default()
        };
        let mut swing = ChannelDef::new("arm");
        swing.rx = Some(Curve::Wave(Wave::sin(30.0, 1.0, 0.0)));
        def.clips.insert(
            "walk".into(),
            ClipDef {
                duration: 1.0,
                looping: true,
                additive: false,
                fade: [0.1, 0.1],
                channels: vec![swing],
            },
        );
        def.gaits.push(GaitDef {
            clip: "walk".into(),
            speed: 1.0,
        });
        def
    }

    #[test]
    fn compiles_and_resolves() {
        let rig = Rig::compile(&two_bone_def()).unwrap();
        assert_eq!(rig.bone_count(), 2);
        assert_eq!(rig.bones[1].parent, 0);
        assert!((rig.bones[1].rest - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-6);
        assert_eq!(rig.gaits[0].stride, 1.0);
        assert_eq!(rig.top_speed, 1.0);
    }

    #[test]
    fn rejects_bad_references() {
        let mut d = two_bone_def();
        d.boxes.push(5);
        assert!(matches!(Rig::compile(&d), Err(RigError::BadBox { .. })));
        let mut d = two_bone_def();
        d.bones[0].parent = Some("arm".into());
        assert!(matches!(Rig::compile(&d), Err(RigError::BadParent { .. })));
        let mut d = two_bone_def();
        d.gaits[0].clip = "nope".into();
        assert!(matches!(Rig::compile(&d), Err(RigError::UnknownClip(_))));
        let mut d = two_bone_def();
        d.clips.get_mut("walk").unwrap().channels[0].bone = "x".into();
        assert!(matches!(Rig::compile(&d), Err(RigError::UnknownBone(_))));
    }

    #[test]
    fn wave_samples_loop_seamlessly() {
        let rig = Rig::compile(&two_bone_def()).unwrap();
        let c = &rig.clips[0];
        let a = c.sample(0, 0.0);
        let b = c.sample(0, 1.0);
        assert!(a.q.angle_between(b.q) < 1e-4, "boucle continue");
        // Quart de période : 30° autour de X.
        let q = c.sample(0, 0.25).q;
        assert!((q.angle_between(Quat::IDENTITY).to_degrees() - 30.0).abs() < 0.5);
    }

    #[test]
    fn keys_hit_their_values_and_wrap() {
        let keys = [[0.0, 0.0], [0.5, 10.0]];
        assert!((eval_keys(&keys, 0.5, true) - 10.0).abs() < 1e-4);
        assert!(eval_keys(&keys, 0.0, true).abs() < 1e-4);
        let mid = eval_keys(&keys, 0.25, true);
        assert!(mid > 3.0 && mid < 7.0, "{mid}");
        // Non bouclé : bornes tenues.
        let k2 = [[0.2, 1.0], [0.8, 3.0]];
        assert_eq!(eval_keys(&k2, 0.0, false), 1.0);
        assert_eq!(eval_keys(&k2, 1.0, false), 3.0);
        // Valeur finie partout.
        for i in 0..=100 {
            assert!(eval_keys(&keys, i as f32 / 100.0, true).is_finite());
        }
    }

    #[test]
    fn abs_wave_bounces() {
        let w = Wave {
            abs: true,
            ..Wave::sin(10.0, 2.0, 0.0)
        };
        assert!(eval_wave(&w, 0.0).abs() < 1e-5);
        assert!((eval_wave(&w, 0.25) - 10.0).abs() < 1e-4);
        assert!(eval_wave(&w, 0.5).abs() < 1e-4);
        assert!((eval_wave(&w, 0.75) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn rejects_unsorted_keys() {
        let mut d = two_bone_def();
        d.clips.get_mut("walk").unwrap().channels[0].ry =
            Some(Curve::Keys(vec![[0.5, 1.0], [0.2, 2.0]]));
        assert!(matches!(Rig::compile(&d), Err(RigError::BadKeys(_))));
    }
}
