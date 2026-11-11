//! Calendrier et lumière du jour dans la boucle de jeu (plan Norrcraft, session 3.1).
//!
//! Chaque tick, le jeu lit la latitude du lieu (λ = 1 − T₀, relevée deux fois par seconde),
//! la lisse, puis règle le décalage solaire de l'horloge du ciel d'après la fraction d'année.
//! Les mondes sans saisons (vanilla) gardent un décalage nul : ciel d'origine.

use nb_ecs::Transform;
use nb_world::calendar::{smooth_latitude, DayLight};
use nb_world::seasons::step_of;

use super::*;

/// Secondes entre deux relevés de la latitude.
const PROBE_INTERVAL: f32 = 0.5;

/// État du suivi de latitude.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct CalendarState {
    /// Dernière latitude relevée au pied du joueur.
    target: f32,
    /// Compte à rebours avant le prochain relevé.
    timer: f32,
    /// `true` une fois la première latitude relevée (la lissée démarre alors sur elle).
    ready: bool,
}

impl Game {
    /// À appeler après `SkyClock::update` : latitude, décalage solaire.
    pub(super) fn update_calendar(&mut self, dt: f64) {
        let Some(cal) = self.session.calendar() else {
            self.sky.solar_bias = 0.0;
            self.sky.latitude = 0.0;
            return;
        };
        self.calendar_state.timer -= dt as f32;
        if self.calendar_state.timer <= 0.0 {
            self.calendar_state.timer = PROBE_INTERVAL;
            if let Some(lat) = self.probe_latitude() {
                self.calendar_state.target = lat;
                if !self.calendar_state.ready {
                    self.calendar_state.ready = true;
                    self.sky.latitude = lat;
                }
            }
        }
        if self.calendar_state.ready {
            self.sky.latitude = smooth_latitude(self.sky.latitude, self.calendar_state.target, dt);
        }
        let a = cal.year_fraction(self.sky.day, self.sky.time_of_day);
        // Surcouche saisonnière des chunks : un pas tous les huitièmes d'année.
        self.session
            .world
            .chunk_manager()
            .set_season(Some(step_of(a)));
        self.sky.solar_bias = DayLight::at(a, self.sky.latitude).bias;
    }

    /// Latitude λ au pied du joueur local, si l'univers en a une.
    fn probe_latitude(&self) -> Option<f32> {
        let player = self.session.player()?;
        let pos = self.session.sim.entities.copy::<Transform>(player)?.pos;
        let climate = self
            .session
            .world
            .profile()
            .generator()
            .climate_at(pos.x.floor() as i32, pos.z.floor() as i32)?;
        climate.latitude.map(|l| l as f32)
    }
}
