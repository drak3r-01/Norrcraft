//! Horloge jour/nuit et paramètres de ciel (spec §7, §8.2). Fonctions pures, sans GPU.

use glam::Vec3;

/// Durée d'un cycle complet en « ticks » de `timeOfDay`.
pub const DAY_TICKS: f32 = 1200.0;

/// Horloge jour/nuit (`MasterRenderer.timeOfDay`, `auroraPhase`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyClock {
    /// `[0, 1200)`, défaut 900 (midi).
    pub time_of_day: f32,
    /// Durée réelle d'un jour en secondes, défaut 1200, au moins 1.
    pub day_length_seconds: f32,
    /// Phase des aurores, jamais bornée.
    pub aurora_phase: f32,
}

impl Default for SkyClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SkyClock {
    pub fn new() -> Self {
        Self {
            time_of_day: 900.0,
            day_length_seconds: 1200.0,
            aurora_phase: 0.0,
        }
    }

    /// `updateTime(dt)`, appelé à chaque tick fixe (§7.1).
    pub fn update(&mut self, dt: f64) {
        let dt = dt as f32;
        self.time_of_day =
            (self.time_of_day + dt * DAY_TICKS / self.day_length_seconds) % DAY_TICKS;
        self.aurora_phase += dt * 0.015;
    }

    /// `setTime(t)` : `t % 1200`, puis `+1200` si négatif.
    pub fn set_time(&mut self, t: f32) {
        let mut v = t % DAY_TICKS;
        if v < 0.0 {
            v += DAY_TICKS;
        }
        self.time_of_day = v;
    }

    /// `setDayLengthSeconds(s) = max(1, s)`.
    pub fn set_day_length_seconds(&mut self, s: f32) {
        self.day_length_seconds = s.max(1.0);
    }

    /// Paramètres de ciel de l'heure courante.
    pub fn params(&self) -> SkyParams {
        sky_params(self.time_of_day)
    }
}

/// Valeurs nommées des commandes `/time` (référence §7.1).
pub fn named_time(name: &str) -> Option<f32> {
    Some(match name {
        "sunset" => 0.0,
        "night" => 150.0,
        "midnight" => 300.0,
        "sunrise" => 600.0,
        "day" => 750.0,
        "noon" => 900.0,
        _ => return None,
    })
}

/// Paramètres de ciel dérivés de `timeOfDay` (§7.2–7.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyParams {
    /// Sens de propagation de la lumière (soleil → sol).
    pub light_dir: Vec3,
    /// `light_dir`, inversée si `light_dir.y > 0` : c'est `uLightDir` des shaders.
    pub shadow_light_dir: Vec3,
    pub zenith: Vec3,
    pub horizon: Vec3,
    pub sun_color: Vec3,
    pub base_fog: Vec3,
    pub ambient: Vec3,
    pub light_color: Vec3,
    /// Intensité des aurores boréales : `0` le jour et au crépuscule, `1` en pleine nuit
    /// (montée douce entre `h = -0.1` et `h = -0.4`).
    pub aurora_intensity: f32,
    /// Visibilité des étoiles et de la lune : `0` de jour, `1` en pleine nuit (`smoothstep`
    /// entre `h = 0.12` et `h = -0.22`).
    pub night: f32,
    /// `true` quand la lumière principale (`shadow_light_dir`) vient de la lune : le soleil est
    /// sous l'horizon et le disque affiché au ciel est celui de la lune.
    pub moon_up: bool,
}

#[inline]
fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// `sky_params(time_of_day)` (§7.2–7.3).
pub fn sky_params(time_of_day: f32) -> SkyParams {
    let angle = time_of_day / DAY_TICKS * std::f32::consts::TAU;
    let light_dir = Vec3::new(
        (angle as f64).cos() as f32,
        (angle as f64).sin() as f32,
        -0.3,
    )
    .normalize();
    let shadow_light_dir = if light_dir.y > 0.0 {
        -light_dir
    } else {
        light_dir
    };
    let h = -light_dir.y;
    let v = Vec3::new;

    let (zenith, horizon, sun_color, base_fog, ambient, light_color, aurora);
    if h > 0.1 {
        let t = ((h - 0.1) / 0.5).min(1.0);
        zenith = lerp(v(0.8, 0.4, 0.4), v(0.12, 0.32, 0.75), t);
        horizon = lerp(v(0.9, 0.6, 0.3), v(0.65, 0.8, 0.95), t);
        sun_color = lerp(v(1.8, 0.8, 0.4), v(3.0, 2.4, 1.8), t);
        base_fog = lerp(v(0.8, 0.5, 0.3), v(0.45, 0.65, 0.9), t);
        ambient = lerp(v(0.3, 0.25, 0.25), v(0.4, 0.4, 0.4), t);
        light_color = lerp(v(0.5, 0.3, 0.2), v(0.6, 0.6, 0.6), t);
        aurora = 0.0;
    } else if h > -0.1 {
        let t = (h + 0.1) / 0.2;
        zenith = lerp(v(0.05, 0.1, 0.2), v(0.8, 0.4, 0.4), t);
        horizon = lerp(v(0.1, 0.2, 0.3), v(0.9, 0.6, 0.3), t);
        sun_color = lerp(v(0.5, 0.5, 0.8), v(1.8, 0.8, 0.4), t);
        base_fog = lerp(v(0.05, 0.1, 0.15), v(0.8, 0.5, 0.3), t);
        ambient = lerp(v(0.1, 0.1, 0.15), v(0.3, 0.25, 0.25), t);
        light_color = lerp(v(0.1, 0.15, 0.2), v(0.5, 0.3, 0.2), t);
        aurora = 0.0;
    } else {
        let t = ((-h - 0.1) / 0.5).min(1.0);
        zenith = lerp(v(0.05, 0.1, 0.2), v(0.02, 0.04, 0.08), t);
        horizon = lerp(v(0.1, 0.2, 0.3), v(0.03, 0.06, 0.12), t);
        sun_color = v(0.5, 0.5, 0.8);
        base_fog = lerp(v(0.05, 0.1, 0.15), v(0.02, 0.04, 0.08), t);
        ambient = lerp(v(0.1, 0.1, 0.15), v(0.05, 0.05, 0.08), t);
        light_color = lerp(v(0.1, 0.15, 0.2), v(0.05, 0.08, 0.1), t);
        let a = ((-h - 0.1) / 0.3).min(1.0);
        aurora = a * a * (3.0 - 2.0 * a);
    }
    let night = {
        let n = ((0.12 - h) / 0.34).clamp(0.0, 1.0);
        n * n * (3.0 - 2.0 * n)
    };
    SkyParams {
        light_dir,
        shadow_light_dir,
        zenith,
        horizon,
        sun_color,
        base_fog,
        ambient,
        light_color,
        aurora_intensity: aurora,
        night,
        moon_up: light_dir.y > 0.0,
    }
}

/// Distance² de référence quand aucune lave n'est proche (§8.2).
pub const NO_LAVA_DIST_SQ: f32 = 256.0;

/// Intensité de chaleur depuis la distance² à la lave la plus proche (§8.2).
pub fn heat_from_nearest_lava_dist_sq(min_dist_sq: f32) -> f32 {
    if min_dist_sq < 64.0 {
        let t = (1.0 - min_dist_sq.sqrt() / 8.0).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: [f32; 3]) {
        let b = Vec3::from(b);
        assert!((a - b).abs().max_element() < 1e-5, "{a:?} != {b:?}");
    }

    #[test]
    fn noon_vector() {
        let p = sky_params(900.0);
        close(p.light_dir, [0.0, -0.957_826_3, -0.287_347_9]);
        close(p.shadow_light_dir, [0.0, -0.957_826_3, -0.287_347_9]);
        close(p.zenith, [0.12, 0.32, 0.75]);
        close(p.horizon, [0.65, 0.8, 0.95]);
        close(p.sun_color, [3.0, 2.4, 1.8]);
        close(p.ambient, [0.4, 0.4, 0.4]);
        close(p.light_color, [0.6, 0.6, 0.6]);
        close(p.base_fog, [0.45, 0.65, 0.9]);
        assert_eq!(p.aurora_intensity, 0.0);
    }

    #[test]
    fn sunset_vector() {
        let p = sky_params(0.0);
        close(p.light_dir, [0.957_826_3, 0.0, -0.287_347_9]);
        close(p.shadow_light_dir, [0.957_826_3, 0.0, -0.287_347_9]);
        close(p.zenith, [0.425, 0.25, 0.3]);
        close(p.horizon, [0.5, 0.4, 0.3]);
        close(p.sun_color, [1.15, 0.65, 0.6]);
        close(p.ambient, [0.2, 0.175, 0.2]);
        close(p.light_color, [0.3, 0.225, 0.2]);
        close(p.base_fog, [0.425, 0.3, 0.225]);
        assert_eq!(p.aurora_intensity, 0.0);
    }

    #[test]
    fn midnight_vector() {
        let p = sky_params(300.0);
        close(p.light_dir, [0.0, 0.957_826_3, -0.287_347_9]);
        close(p.shadow_light_dir, [0.0, -0.957_826_3, 0.287_347_9]);
        close(p.zenith, [0.02, 0.04, 0.08]);
        close(p.horizon, [0.03, 0.06, 0.12]);
        close(p.sun_color, [0.5, 0.5, 0.8]);
        close(p.ambient, [0.05, 0.05, 0.08]);
        close(p.light_color, [0.05, 0.08, 0.1]);
        close(p.base_fog, [0.02, 0.04, 0.08]);
        assert!((p.aurora_intensity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn continuous_at_branch_boundaries() {
        // Recherche des heures où sunHeight = ±0.1 puis comparaison de part et d'autre.
        for target in [0.1f32, -0.1] {
            let (mut lo, mut hi) = if target > 0.0 {
                (600.0f32, 900.0f32)
            } else {
                (300.0, 600.0)
            };
            let f = |t: f32| -sky_params(t).light_dir.y - target;
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if (f(lo) < 0.0) == (f(mid) < 0.0) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let (a, b) = (sky_params(lo), sky_params(hi));
            for (x, y) in [
                (a.zenith, b.zenith),
                (a.horizon, b.horizon),
                (a.sun_color, b.sun_color),
                (a.base_fog, b.base_fog),
                (a.ambient, b.ambient),
                (a.light_color, b.light_color),
            ] {
                assert!((x - y).abs().max_element() < 1e-3, "{x:?} / {y:?}");
            }
        }
    }

    #[test]
    fn clock_full_day_and_set_time() {
        let mut c = SkyClock::new();
        for _ in 0..1200 * 60 {
            c.update(1.0 / 60.0);
        }
        assert!(
            (c.time_of_day - 900.0).abs() < 3.0,
            "time = {}",
            c.time_of_day
        );
        assert!((c.aurora_phase - 18.0).abs() < 0.1);
        c.set_time(-10.0);
        assert_eq!(c.time_of_day, 1190.0);
        c.set_time(2500.0);
        assert_eq!(c.time_of_day, 100.0);
        c.set_day_length_seconds(0.2);
        assert_eq!(c.day_length_seconds, 1.0);
        assert_eq!(named_time("midnight"), Some(300.0));
        assert_eq!(named_time("teatime"), None);
    }

    #[test]
    fn heat_curve() {
        assert_eq!(heat_from_nearest_lava_dist_sq(0.0), 1.0);
        assert!((heat_from_nearest_lava_dist_sq(16.0) - 0.5).abs() < 1e-6);
        assert_eq!(heat_from_nearest_lava_dist_sq(64.0), 0.0);
        assert_eq!(heat_from_nearest_lava_dist_sq(NO_LAVA_DIST_SQ), 0.0);
    }
}
