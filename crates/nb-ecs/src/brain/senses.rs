//! Perception (doc 03 §2.2) : vue (cône, lumière, brouillard, forêt), ouïe (pas du joueur,
//! pluie et vent), odorat sous le vent. Fonctions pures, sauf la ligne de vue qui lit les blocs.

use nb_core::glam::DVec3;
use nb_data::behavior::Senses;
use nb_data::BlockRegistry;
use nb_world::weather::Environment;

use super::clock::light_factor;
use crate::blocks::BlockAccess;
use crate::jmath::floor_i;

/// Densité de brouillard de l'air clair (celle du rendu).
pub const AIR_FOG: f32 = 0.002;
/// Transmission en deçà de laquelle la vue est coupée.
const OPAQUE: f32 = 0.05;
/// Vitesse de vent à partir de laquelle l'odorat est pleinement orienté.
const FULL_WIND: f32 = 0.2;

/// Sens qui a détecté.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sense {
    Sight,
    Hearing,
    Smell,
}

/// Facteur de bruit d'une source : joueur accroupi ÷ 3, qui court × 2 (doc 03 §2.2).
pub fn noise_factor(horizontal_speed: f32, crouching: bool) -> f32 {
    if crouching {
        1.0 / 3.0
    } else if horizontal_speed > 4.5 {
        2.0
    } else {
        1.0
    }
}

/// Portée de vue que laisse le brouillard (transmission 0,2 : `ln 5 / densité`).
pub fn fog_range(fog: f32) -> f32 {
    1.609 / (AIR_FOG + fog.max(0.0))
}

/// Multiplicateur de l'odorat : × 2 quand la source est au vent de l'animal (le vent porte son
/// odeur vers lui), ÷ 2 sous le vent, 1 par calme. Continu : `2^(cos α · min(1, v / 0,2))`.
pub fn wind_smell_factor(env: Option<&Environment>, source: DVec3, me: DVec3) -> f32 {
    let Some(e) = env else {
        return 1.0;
    };
    let (dx, dz) = ((me.x - source.x) as f32, (me.z - source.z) as f32);
    let len = (dx * dx + dz * dz).sqrt();
    if len < 1e-3 {
        return 1.0;
    }
    let c = (e.wind.dir[0] * dx + e.wind.dir[1] * dz) / len;
    let s = (e.wind.speed / FULL_WIND).min(1.0);
    (c * s).exp2()
}

/// Ouïe couverte par la pluie et le vent.
pub fn hearing_cover(env: Option<&Environment>) -> f32 {
    env.map_or(1.0, |e| {
        1.0 - 0.4 * e.rain.max(e.wind.speed).clamp(0.0, 1.0)
    })
}

/// Ce que les sens d'un animal donnent dans les conditions du moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ranges {
    pub sight: f32,
    /// cos(demi-ouverture du cône).
    pub cone_cos: f32,
    pub hearing: f32,
    pub smell: f32,
}

impl Ranges {
    pub fn new(s: &Senses, night_vision: f32, sun: f32, env: Option<&Environment>) -> Self {
        let fog = env.map_or(0.0, |e| e.fog);
        Self {
            sight: (s.sight * light_factor(night_vision, sun)).min(fog_range(fog)),
            cone_cos: (s.fov.clamp(0.0, 360.0).to_radians() * 0.5).cos(),
            hearing: s.hearing * hearing_cover(env),
            smell: s.smell,
        }
    }
}

/// Transmission de la vue par bloc : air 1, plantes et eau presque transparentes, feuillages
/// 0,5, verre 0,9, blocs pleins 0.
pub fn sight_table(reg: &BlockRegistry) -> [f32; 256] {
    let mut t = [0.0f32; 256];
    for (i, slot) in t.iter_mut().enumerate() {
        let b = reg.from_id(i as u8);
        *slot = if !b.solid {
            if b.cross_shaped || i as u8 == reg.ids().air {
                if i as u8 == reg.ids().air {
                    1.0
                } else {
                    0.85
                }
            } else {
                0.9
            }
        } else if b.transparent {
            if b.leaves_tinted || b.seasonal {
                0.5
            } else {
                0.9
            }
        } else {
            0.0
        };
    }
    t
}

/// Transmission de la ligne de vue de `from` à `to` (pas d'un bloc, extrémités exclues).
pub fn line_of_sight(blocks: &dyn BlockAccess, table: &[f32; 256], from: DVec3, to: DVec3) -> f32 {
    let d = to - from;
    let len = d.length();
    let steps = len.floor() as i32;
    if steps < 2 {
        return 1.0;
    }
    let step = d / len;
    let mut trans = 1.0f32;
    let mut last = (i32::MIN, i32::MIN, i32::MIN);
    for k in 1..steps.min(64) {
        let p = from + step * f64::from(k);
        let cell = (floor_i(p.x), floor_i(p.y), floor_i(p.z));
        if cell == last {
            continue;
        }
        last = cell;
        trans *= table[usize::from(blocks.block(cell.0, cell.1, cell.2))];
        if trans < OPAQUE {
            return 0.0;
        }
    }
    trans
}

/// Une source perçue : position et facteurs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Source {
    pub pos: DVec3,
    /// Bruit (voir [`noise_factor`]).
    pub noise: f32,
    /// Odeur (1 : normale ; un joueur au surströmming sent bien plus fort).
    pub scent: f32,
    /// Hauteur des yeux ou du centre au-dessus des pieds.
    pub height: f32,
}

/// Perception d'une source par un animal en `me` (pieds), hauteur des yeux `eye`, lacet `yaw`.
/// Renvoie l'éveil (0 à 1, 1 au contact) et le sens qui l'emporte.
#[allow(clippy::too_many_arguments)]
pub fn perceive(
    r: &Ranges,
    me: DVec3,
    eye: f32,
    yaw: f32,
    src: &Source,
    env: Option<&Environment>,
    blocks: &dyn BlockAccess,
    table: &[f32; 256],
) -> Option<(f32, Sense)> {
    let d = src.pos - me;
    let dist = d.length() as f32;
    let mut best: Option<(f32, Sense)> = None;
    let mut offer = |aw: f32, s: Sense| {
        if aw > 0.0 && best.is_none_or(|(b, _)| aw > b) {
            best = Some((aw, s));
        }
    };
    // Odorat sous le vent.
    let smell = r.smell * src.scent * wind_smell_factor(env, src.pos, me);
    offer(1.0 - dist / smell.max(1e-3), Sense::Smell);
    // Ouïe.
    let hearing = r.hearing * src.noise;
    offer(1.0 - dist / hearing.max(1e-3), Sense::Hearing);
    // Vue : cône, puis ligne de vue (feuillages, relief).
    if dist < r.sight {
        let (fx, fz) = (yaw.sin(), yaw.cos());
        let h = ((d.x * d.x + d.z * d.z) as f32).sqrt();
        let in_cone = h < 1e-3 || (fx * d.x as f32 + fz * d.z as f32) / h >= r.cone_cos;
        if in_cone {
            let from = me + DVec3::new(0.0, f64::from(eye), 0.0);
            let to = src.pos + DVec3::new(0.0, f64::from(src.height), 0.0);
            let t = line_of_sight(blocks, table, from, to);
            offer(1.0 - dist / (r.sight * t).max(1e-3), Sense::Sight);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_world::weather::Wind;

    fn env(dir: [f32; 2], speed: f32) -> Environment {
        Environment {
            air: 0.5,
            wind: Wind { dir, speed },
            chill: 1.0,
            rain: 0.0,
            snow: 0.0,
            fog: 0.0,
            sun: 1.0,
        }
    }

    #[test]
    fn upwind_doubles_and_downwind_halves_smell() {
        // Vent d'ouest (souffle vers +x) ; l'animal est en x = 0.
        let e = env([1.0, 0.0], 0.6);
        let me = DVec3::ZERO;
        let upwind = DVec3::new(-30.0, 0.0, 0.0);
        let downwind = DVec3::new(30.0, 0.0, 0.0);
        assert!((wind_smell_factor(Some(&e), upwind, me) - 2.0).abs() < 1e-5);
        assert!((wind_smell_factor(Some(&e), downwind, me) - 0.5).abs() < 1e-5);
        // De côté, ou par calme : inchangé.
        assert!((wind_smell_factor(Some(&e), DVec3::new(0.0, 0.0, 30.0), me) - 1.0).abs() < 1e-5);
        assert_eq!(
            wind_smell_factor(Some(&env([1.0, 0.0], 0.0)), upwind, me),
            1.0
        );
        assert_eq!(wind_smell_factor(None, upwind, me), 1.0);
    }

    #[test]
    fn steps_and_weather_change_hearing() {
        assert!((noise_factor(1.0, true) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(noise_factor(4.0, false), 1.0);
        assert_eq!(noise_factor(5.6, false), 2.0);
        let mut storm = env([1.0, 0.0], 0.1);
        storm.rain = 1.0;
        assert!((hearing_cover(Some(&storm)) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn fog_and_night_shorten_sight() {
        let s = Senses {
            sight: 40.0,
            fov: 300.0,
            hearing: 10.0,
            smell: 10.0,
        };
        let day = Ranges::new(&s, 0.4, 1.0, None);
        assert_eq!(day.sight, 40.0);
        let night = Ranges::new(&s, 0.4, -1.0, None);
        assert!((night.sight - 16.0).abs() < 1e-4);
        let mut blizzard = env([1.0, 0.0], 0.5);
        blizzard.fog = 0.12;
        let r = Ranges::new(&s, 0.4, 1.0, Some(&blizzard));
        assert!(r.sight < 14.0, "{}", r.sight);
    }
}
