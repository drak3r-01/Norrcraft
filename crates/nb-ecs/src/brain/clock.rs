//! Horloges des animaux (doc 03 §2.3) : journalière, saisonnière et pluriannuelle. Fonctions pures.

use nb_data::behavior::{in_window, Activity, BehaviorBook, Profile, Window};
use nb_world::calendar::{self, Calendar, DayLight};
use nb_world::WorldConfig;

/// Latitude par défaut quand la météo ne donne pas la hauteur du soleil (Norrland moyen).
pub const DEFAULT_LATITUDE: f32 = 0.5;

#[inline]
pub(crate) fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Instant vu par les animaux pendant un tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
    /// Mois réel (0 = januari).
    pub month: usize,
    /// Année de jeu (à partir de 1).
    pub year: u64,
    /// Hauteur du soleil, de −1 à 1.
    pub sun: f32,
    /// Fraction de l'année écoulée (0 à 1), pour les courbes saisonnières.
    pub year_fraction: f64,
}

impl Clock {
    /// Instant du monde : date du calendrier, soleil de la météo s'il y en a une, sinon soleil
    /// calculé à la latitude par défaut.
    pub fn of(cfg: &WorldConfig, sun: Option<f32>) -> Self {
        let cal = Calendar::new(cfg.days_per_month);
        let date = cal.date(cfg.calendar_day);
        let a = cal.year_fraction(cfg.calendar_day, cfg.time_of_day);
        let sun = sun.unwrap_or_else(|| {
            calendar::sun_height(cfg.time_of_day, DayLight::at(a, DEFAULT_LATITUDE).bias)
        });
        Self {
            month: date.month,
            year: date.year,
            sun,
            year_fraction: a,
        }
    }

    pub fn within(&self, w: Option<Window>) -> bool {
        w.is_some_and(|w| in_window(self.month, w))
    }
}

/// Activité de l'heure (0 : repos, 1 : pleine activité).
pub fn activity(kind: Activity, sun: f32) -> f32 {
    match kind {
        Activity::Diurnal => smooth(-0.10, 0.15, sun),
        Activity::Nocturnal => 1.0 - smooth(-0.15, 0.10, sun),
        // Aube et crépuscule, avec un fond d'activité la nuit.
        Activity::Crepuscular => {
            (1.0 - smooth(0.15, 0.45, sun.abs())).max(0.3 * (1.0 - smooth(-0.2, 0.2, sun)))
        }
        Activity::Always => 1.0,
    }
}

/// Fraction de la vue gardée à cette lumière (`night_vision` en pleine nuit).
pub fn light_factor(night_vision: f32, sun: f32) -> f32 {
    night_vision + (1.0 - night_vision) * smooth(-0.15, 0.10, sun)
}

/// Pelage d'hiver (blanc) à cette date.
pub fn winter_coat(p: &Profile, clock: &Clock) -> bool {
    clock.within(p.seasons.coat)
}

/// Rang de la fenêtre de migration en cours (0 : printemps, suit `heading` ; 1 : retour).
pub fn migration(p: &Profile, clock: &Clock) -> Option<usize> {
    p.seasons
        .migrate
        .iter()
        .position(|&w| in_window(clock.month, w))
}

/// Courage du profil à cette date (rut, mise bas).
pub fn courage(p: &Profile, clock: &Clock) -> f32 {
    let mut c = p.courage;
    if clock.within(p.seasons.rut) {
        c = c.max(p.rut_courage.unwrap_or(c));
    }
    if clock.within(p.seasons.birth) {
        c = (c + 0.2).min(1.0);
    }
    c
}

/// Facteur de densité pluriannuel (cycle des lemmings et de leurs prédateurs).
pub fn cycle_factor(book: &BehaviorBook, p: &Profile, clock: &Clock) -> f32 {
    book.cycle_factor(p, clock.year)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_clocks_follow_the_sun() {
        let noon = 1.0;
        let night = -1.0;
        let dusk = 0.0;
        assert!(activity(Activity::Diurnal, noon) > 0.99);
        assert!(activity(Activity::Diurnal, night) < 0.01);
        assert!(activity(Activity::Nocturnal, night) > 0.99);
        assert!(activity(Activity::Nocturnal, noon) < 0.01);
        assert!(activity(Activity::Crepuscular, dusk) > 0.99);
        assert!(activity(Activity::Crepuscular, noon) < 0.01);
        assert!(activity(Activity::Crepuscular, night) > 0.25);
        assert!(light_factor(0.4, night) < 0.41);
        assert!(light_factor(0.4, noon) > 0.99);
    }

    #[test]
    fn clock_reads_the_calendar() {
        let mut cfg = WorldConfig {
            days_per_month: 2,
            ..Default::default()
        };
        let cal = Calendar::new(2);
        // Solstice d'hiver de l'année 3 : décembre.
        cfg.calendar_day = cal.day_for(3, 0);
        cfg.time_of_day = 900.0; // midi
        let c = Clock::of(&cfg, None);
        assert_eq!((c.month, c.year), (11, 3));
        // Midi au solstice d'hiver par 0,5 de latitude : soleil bas mais levé.
        assert!(c.sun > 0.0 && c.sun < 0.6, "{}", c.sun);
        cfg.time_of_day = 300.0; // minuit
        assert!(Clock::of(&cfg, None).sun < -0.5);
        // La météo, quand elle existe, fait foi.
        assert_eq!(Clock::of(&cfg, Some(0.25)).sun, 0.25);
    }
}
