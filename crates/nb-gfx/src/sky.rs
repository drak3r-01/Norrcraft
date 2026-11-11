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
    /// Jours de jeu écoulés depuis la création du monde (calendrier, session 3.1) : incrémenté
    /// à chaque tour d'horloge.
    pub day: u64,
    /// Latitude λ ∈ [0, 1] du lieu d'observation (lissée par le jeu), 0 sans calendrier.
    pub latitude: f32,
    /// Décalage solaire de la saison et de la latitude (voir [`sky_params_biased`]) : 0 = jour et
    /// nuit égaux, comportement d'origine.
    pub solar_bias: f32,
    /// Météo vue du ciel (session 3.3) ; neutre par défaut : ciel d'origine.
    pub weather: SkyWeather,
}

/// Ce que la météo change au ciel (assombrissement, brouillard, aurore, éclair, ciel rouge).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyWeather {
    /// Couverture nuageuse, 0 (dégagé) à 1 (plafond bas) : grise et assombrit le ciel.
    pub overcast: f32,
    /// Densité de brouillard ajoutée à celle de l'air du shader (0,002).
    pub fog: f32,
    /// Part de l'aurore de la nuit qui est visible (1 : comportement d'origine).
    pub aurora_scale: f32,
    /// Éclair en cours, 0 à 1.
    pub flash: f32,
    /// Signe avant-coureur : ciel rouge à l'aube et au crépuscule, 0 à 1.
    pub red_sky: f32,
    /// Vent local, 0 à 1 (ambiance sonore).
    pub wind: f32,
    /// Silence des oiseaux annonciateur d'un temps qui se gâte, 0 à 1.
    pub birds_quiet: f32,
}

impl Default for SkyWeather {
    fn default() -> Self {
        Self {
            overcast: 0.0,
            fog: 0.0,
            aurora_scale: 1.0,
            flash: 0.0,
            red_sky: 0.0,
            wind: 0.0,
            birds_quiet: 0.0,
        }
    }
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
            day: 0,
            latitude: 0.0,
            solar_bias: 0.0,
            weather: SkyWeather::default(),
        }
    }

    /// `updateTime(dt)`, appelé à chaque tick fixe (§7.1).
    pub fn update(&mut self, dt: f64) {
        let dt = dt as f32;
        let t = self.time_of_day + dt * DAY_TICKS / self.day_length_seconds;
        self.day += (t / DAY_TICKS).floor().max(0.0) as u64;
        self.time_of_day = t % DAY_TICKS;
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
        apply_weather(
            sky_params_biased(self.time_of_day, self.solar_bias),
            &self.weather,
        )
    }

    /// Hauteur du soleil de −1 à 1 pour l'heure, la saison et la latitude.
    pub fn sun_height(&self) -> f32 {
        let angle = self.time_of_day / DAY_TICKS * std::f32::consts::TAU;
        biased_sun_height(angle, self.solar_bias).clamp(-1.0, 1.0)
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
    /// Densité de brouillard à ajouter à celle de l'air (météo ; 0 par défaut).
    pub fog_density: f32,
}

#[inline]
fn luma(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.299, 0.587, 0.114))
}

/// Applique la météo à un ciel de base : plafond gris et plus sombre, lumière directe qui
/// s'éteint, étoiles et aurore masquées, éclair, ciel rouge, brouillard. Avec
/// `SkyWeather::default()` le ciel est inchangé.
pub fn apply_weather(mut p: SkyParams, w: &SkyWeather) -> SkyParams {
    let o = w.overcast.clamp(0.0, 1.0);
    if o > 0.0 {
        let grey = Vec3::splat(luma(p.horizon));
        let dim = 1.0 - 0.35 * o;
        p.zenith = lerp(p.zenith, grey * 0.8, 0.8 * o) * dim;
        p.horizon = lerp(p.horizon, grey, 0.75 * o) * dim;
        p.base_fog = lerp(p.base_fog, Vec3::splat(luma(p.base_fog)), 0.75 * o) * dim;
        p.sun_color *= 1.0 - 0.75 * o;
        p.light_color *= 1.0 - 0.55 * o;
        p.ambient = lerp(p.ambient, Vec3::splat(luma(p.ambient)), 0.5 * o) * (1.0 - 0.1 * o);
        p.night *= 1.0 - 0.9 * o;
    }
    p.aurora_intensity *= w.aurora_scale.clamp(0.0, 1.0);
    // Ciel rouge : seulement quand le soleil est bas (h entre −0,05 et 0,3).
    if w.red_sky > 0.0 {
        let h = -p.light_dir.y;
        let band = ((h + 0.05) / 0.15).clamp(0.0, 1.0) * ((0.3 - h) / 0.2).clamp(0.0, 1.0);
        let k = w.red_sky.clamp(0.0, 1.0) * band;
        p.horizon = lerp(p.horizon, Vec3::new(1.0, 0.25, 0.14), 0.5 * k);
        p.zenith = lerp(p.zenith, Vec3::new(0.55, 0.2, 0.22), 0.25 * k);
        p.base_fog = lerp(p.base_fog, Vec3::new(0.8, 0.3, 0.2), 0.4 * k);
    }
    if w.flash > 0.0 {
        let f = w.flash.clamp(0.0, 1.0);
        p.ambient += Vec3::splat(0.6 * f);
        p.light_color += Vec3::new(0.35, 0.37, 0.5) * f;
        p.zenith = lerp(p.zenith, Vec3::new(0.7, 0.75, 0.95), 0.6 * f);
        p.horizon = lerp(p.horizon, Vec3::new(0.8, 0.82, 1.0), 0.6 * f);
    }
    p.fog_density = w.fog.max(0.0);
    p
}

#[inline]
fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// `sky_params(time_of_day)` (§7.2–7.3).
pub fn sky_params(time_of_day: f32) -> SkyParams {
    sky_params_biased(time_of_day, 0.0)
}

/// Hauteur du soleil de −1 (nadir) à 1 (zénith) pour l'angle `angle` de l'horloge et un décalage
/// solaire `bias` : `(bias − sin θ) / (1 + |bias|)`. Avec `bias = 0` c'est `−sin θ`. Le soleil est
/// levé pendant la fraction `(acos(−bias)) / π` du jour quand `|bias| ≤ 1` ; au-delà, il ne se
/// couche pas (`bias > 1`, soleil de minuit) ou ne se lève pas (`bias < −1`, nuit polaire).
fn biased_sun_height(angle: f32, bias: f32) -> f32 {
    (bias - (angle as f64).sin() as f32) / (1.0 + bias.abs())
}

/// Comme [`sky_params`] avec le décalage solaire de la saison et de la latitude (session 3.1).
pub fn sky_params_biased(time_of_day: f32, bias: f32) -> SkyParams {
    let angle = time_of_day / DAY_TICKS * std::f32::consts::TAU;
    let light_dir = Vec3::new(
        (angle as f64).cos() as f32,
        -biased_sun_height(angle, bias),
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
        // Courbe en racine pour le ciel : la dominante chaude de l'aube disparaît plus vite (sinon un
        // ciel violet persiste tard dans la matinée).
        let ts = t.sqrt();
        // Jour : zénith bleu franc, horizon bleu clair mais plus saturé (le Java donnait un horizon
        // presque blanc après tonemapping, d'où un voile « plastique » à moyenne distance).
        zenith = lerp(v(0.8, 0.4, 0.4), v(0.09, 0.27, 0.70), ts);
        horizon = lerp(v(0.9, 0.6, 0.3), v(0.46, 0.65, 0.90), ts);
        sun_color = lerp(v(1.8, 0.8, 0.4), v(3.0, 2.4, 1.8), t);
        base_fog = lerp(v(0.8, 0.5, 0.3), v(0.45, 0.65, 0.9), t);
        // Ombre bleutée (ciel) et soleil chaud : même éclairement total qu'avant (≈ 1,0) mais plus
        // de contraste et de relief entre zones ombragées et ensoleillées.
        ambient = lerp(v(0.3, 0.25, 0.25), v(0.29, 0.35, 0.46), t);
        light_color = lerp(v(0.5, 0.3, 0.2), v(0.70, 0.64, 0.50), t);
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
        fog_density: 0.0,
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
        close(p.zenith, [0.09, 0.27, 0.70]);
        close(p.horizon, [0.46, 0.65, 0.90]);
        close(p.sun_color, [3.0, 2.4, 1.8]);
        close(p.ambient, [0.29, 0.35, 0.46]);
        close(p.light_color, [0.70, 0.64, 0.50]);
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
    fn zero_bias_is_the_original_sky() {
        for t in (0..1200).step_by(25) {
            assert_eq!(sky_params_biased(t as f32, 0.0), sky_params(t as f32));
        }
        let c = SkyClock::new();
        assert_eq!(c.params(), sky_params(900.0));
    }

    /// Fraction du jour où le soleil est au-dessus de l'horizon pour un décalage donné.
    fn lit_fraction(bias: f32) -> f32 {
        let n = 2400;
        let lit = (0..n)
            .filter(|&i| sky_params_biased(i as f32 * 0.5, bias).light_dir.y < 0.0)
            .count();
        lit as f32 / n as f32
    }

    #[test]
    fn bias_sets_the_lit_fraction_of_the_day() {
        for raw in [0.2f32, 0.35, 0.5, 0.7, 0.9] {
            let bias = -(std::f32::consts::PI * raw).cos();
            let f = lit_fraction(bias);
            assert!((f - raw).abs() < 0.01, "raw {raw} : {f}");
        }
    }

    #[test]
    fn midnight_sun_never_sets_and_polar_night_never_rises() {
        for i in 0..2400 {
            let t = i as f32 * 0.5;
            assert!(sky_params_biased(t, 1.3).light_dir.y < 0.0, "t {t}");
            let night = sky_params_biased(t, -1.3);
            assert!(night.light_dir.y > 0.0, "t {t}");
            assert!(night.moon_up);
        }
        // Soleil de minuit : le soleil descend très bas à minuit, sans toucher l'horizon.
        let low = sky_params_biased(300.0, 1.3).light_dir;
        assert!(low.y > -0.5 && low.y < 0.0, "{low:?}");
        // Nuit polaire : à midi seulement un crépuscule bleu (ciel sombre, sans aurore pleine).
        let noon = sky_params_biased(900.0, -1.2);
        assert!(
            noon.zenith.z < 0.4 && noon.zenith.x < 0.2,
            "{:?}",
            noon.zenith
        );
    }

    #[test]
    fn sky_changes_continuously_with_the_bias() {
        for t in [0.0, 150.0, 600.0, 750.0, 900.0, 1100.0] {
            let mut prev = sky_params_biased(t, -1.5);
            let mut b = -1.5f32;
            while b < 1.6 {
                b += 0.01;
                let p = sky_params_biased(t, b);
                for (x, y) in [
                    (prev.zenith, p.zenith),
                    (prev.horizon, p.horizon),
                    (prev.ambient, p.ambient),
                    (prev.light_color, p.light_color),
                ] {
                    // La courbe en racine du jour est raide au-dessus de h = 0,1 : borne large.
                    assert!((x - y).abs().max_element() < 0.12, "t {t} bias {b}");
                }
                prev = p;
            }
        }
    }

    #[test]
    fn day_counter_follows_the_clock() {
        let mut c = SkyClock::new();
        c.set_day_length_seconds(10.0);
        for _ in 0..(10 * 60 * 3) {
            c.update(1.0 / 60.0);
        }
        assert_eq!(c.day, 3, "t = {}", c.time_of_day);
        c.set_time(5.0);
        assert_eq!(c.day, 3, "set_time ne change pas le jour");
        // Un très grand pas ne perd aucun jour.
        c.update(35.0);
        assert!(c.day >= 6);
    }

    #[test]
    fn sun_height_matches_the_light_direction() {
        let mut c = SkyClock::new();
        assert!((c.sun_height() - 1.0).abs() < 1e-5);
        c.set_time(300.0);
        assert!((c.sun_height() + 1.0).abs() < 1e-5);
        c.solar_bias = 1.0;
        assert!(c.sun_height() >= 0.0);
        c.solar_bias = -1.0;
        c.set_time(900.0);
        assert!(c.sun_height() <= 0.0);
    }

    #[test]
    fn neutral_weather_leaves_the_sky_alone() {
        for t in [0.0, 300.0, 600.0, 750.0, 900.0, 1100.0] {
            let mut c = SkyClock::new();
            c.set_time(t);
            c.solar_bias = 0.3;
            assert_eq!(c.params(), sky_params_biased(t, 0.3), "t = {t}");
        }
    }

    #[test]
    fn overcast_dims_greys_and_hides_the_night_sky() {
        let mut c = SkyClock::new();
        c.set_time(900.0);
        let clear = c.params();
        c.weather.overcast = 1.0;
        let grey = c.params();
        assert!(luma(grey.light_color) < luma(clear.light_color) * 0.6);
        assert!(luma(grey.sun_color) < luma(clear.sun_color) * 0.4);
        // Plafond uniforme : le zénith et l'horizon sont presque de la même teinte grise.
        let spread = (grey.zenith - grey.horizon).abs().max_element();
        assert!(spread < (clear.zenith - clear.horizon).abs().max_element() * 0.5);
        // Les étoiles et l'aurore s'effacent de nuit.
        c.set_time(300.0);
        c.weather.overcast = 0.0;
        let night = c.params();
        c.weather.overcast = 1.0;
        let cloudy = c.params();
        assert!(cloudy.night < night.night * 0.2);
        c.weather.overcast = 0.0;
        c.weather.aurora_scale = 0.0;
        assert_eq!(c.params().aurora_intensity, 0.0);
        assert!(night.aurora_intensity > 0.5);
    }

    #[test]
    fn red_sky_only_at_low_sun_and_flash_lights_the_scene() {
        let mut c = SkyClock::new();
        c.weather.red_sky = 1.0;
        c.set_time(900.0);
        assert_eq!(
            c.params().horizon,
            sky_params_biased(900.0, 0.0).horizon,
            "midi"
        );
        c.set_time(1180.0);
        let base = sky_params_biased(1180.0, 0.0);
        let red = c.params();
        assert!(red.horizon.x > red.horizon.z && red.horizon != base.horizon);
        c.weather.red_sky = 0.0;
        c.weather.flash = 1.0;
        c.set_time(300.0);
        assert!(c.params().ambient.x > sky_params_biased(300.0, 0.0).ambient.x + 0.5);
        c.weather.flash = 0.0;
        c.weather.fog = 0.12;
        assert_eq!(c.params().fog_density, 0.12);
    }

    #[test]
    fn heat_curve() {
        assert_eq!(heat_from_nearest_lava_dist_sq(0.0), 1.0);
        assert!((heat_from_nearest_lava_dist_sq(16.0) - 0.5).abs() < 1e-6);
        assert_eq!(heat_from_nearest_lava_dist_sq(64.0), 0.0);
        assert_eq!(heat_from_nearest_lava_dist_sq(NO_LAVA_DIST_SQ), 0.0);
    }
}
