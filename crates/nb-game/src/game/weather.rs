//! Météo dans la boucle de jeu (plan Norrcraft, session 3.3 ; doc 04 §3).
//!
//! Chaque tick, le jeu évalue le modèle pur [`nb_world::weather`] au lieu du joueur local
//! (climat relevé deux fois par seconde, hauteur lue à chaque tick), puis diffuse le résultat :
//! ciel (couverture, brouillard, aurore, éclairs, ciel rouge, vent sonore), particules (pluie et
//! neige inclinées par le vent) et survie (`Environment`). Rien n'est sauvegardé ni échangé en
//! LAN : la météo est une fonction de la graine et de l'horloge. Les mondes sans saisons
//! (vanilla) gardent la commande `/weather` d'origine et un ciel neutre.

use nb_ecs::Transform;
use nb_gfx::SkyWeather;
use nb_world::profile::Climate;
use nb_world::weather::{Atmosphere, Locale, Weather, WeatherClock, WeatherModel};

use super::*;

/// Secondes entre deux relevés du climat.
const PROBE_INTERVAL: f32 = 0.5;
/// Pas (blocs) des relevés qui donnent la direction de l'intérieur des terres.
const INLAND_STEP: i32 = 48;

/// État de la météo du jeu.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct WeatherState {
    /// Dernier climat relevé et direction de l'intérieur des terres.
    climate: Option<(Climate, [f64; 2])>,
    timer: f32,
    /// État imposé par `/weather` (`None` : automatique).
    forced: Option<Weather>,
    /// Atmosphère du dernier tick ; `None` dans un monde sans météo.
    atmosphere: Option<Atmosphere>,
    flash: f32,
    flash_timer: f32,
    rng: u32,
}

impl WeatherState {
    /// Nombre pseudo-aléatoire dans [0, 1[ (éclairs : purement visuel, propre à chaque machine).
    fn random(&mut self) -> f32 {
        if self.rng == 0 {
            self.rng = 0x9E37_79B9;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// `/weather [auto|clear|veiled|drizzle|rain|mist|storm|snow|blizzard|gale|frost]` dans un
    /// monde à météo ; `None` ailleurs (la commande d'origine répond).
    pub(super) fn command(&mut self, text: &str) -> Option<String> {
        let mut it = text.split_whitespace();
        if it.next()? != "/weather" {
            return None;
        }
        let atm = self.atmosphere?;
        Some(match it.next() {
            None => format!(
                "Météo : {} ({}){}, vent {:.0} %, air {:.2}",
                atm.state.name_fr(),
                atm.state.name_sv(),
                if self.forced.is_some() {
                    ", forcée"
                } else {
                    ""
                },
                atm.wind.speed * 100.0,
                atm.air
            ),
            Some("auto") => {
                self.forced = None;
                "Météo : automatique".to_owned()
            }
            Some(key) => match Weather::parse(key) {
                Some(w) => {
                    self.forced = Some(w);
                    format!("Météo forcée : {}", w.name_fr())
                }
                None => {
                    let keys: Vec<_> = Weather::ALL.iter().map(|w| w.key()).collect();
                    format!("Usage : /weather [auto|{}]", keys.join("|"))
                }
            },
        })
    }
}

impl Game {
    /// À appeler après [`Self::update_calendar`] : règle ciel, particules et survie.
    pub(super) fn update_weather(&mut self, dt: f64) {
        let Some(cal) = self.session.calendar() else {
            // Monde sans météo : ciel neutre, survie d'origine, `/weather` d'origine.
            self.sky.weather = SkyWeather::default();
            self.session.environment = None;
            self.weather.atmosphere = None;
            self.weather.climate = None;
            self.fx.set_wind([0.0, 0.0]);
            return;
        };
        let pos = self.local_position();
        self.weather.timer -= dt as f32;
        if self.weather.timer <= 0.0 {
            self.weather.timer = PROBE_INTERVAL;
            if let Some(p) = pos {
                self.weather.climate = self.probe_climate(p.x.floor() as i32, p.z.floor() as i32);
            }
        }
        let (Some((climate, inland)), Some(pos)) = (self.weather.climate, pos) else {
            return;
        };

        let model = WeatherModel::new(self.session.world.seed());
        let clock = WeatherClock::new(&cal, self.sky.day, self.sky.time_of_day);
        let loc = Locale::from_climate(&climate, inland, pos.y);
        let mut atm = model.sample(&clock, &loc, f64::from(self.sky.sun_height()));
        if let Some(w) = self.weather.forced {
            atm = Atmosphere::forced(&atm, w);
        }
        let aurora = model.aurora(
            self.sky.day,
            clock.year_fraction(),
            loc.lambda(),
            atm.overcast,
        );

        // Éclairs : des coups espacés d'autant plus que l'orage est faible, qui s'éteignent vite.
        let w = &mut self.weather;
        if atm.thunder > 0.3 {
            w.flash_timer -= dt as f32;
            if w.flash_timer <= 0.0 {
                w.flash = 1.0;
                w.flash_timer = (3.0 + 9.0 * w.random()) / atm.thunder;
            }
        } else {
            w.flash_timer = 0.0;
        }
        w.flash *= (-(dt as f32) * 7.0).exp();
        if w.flash < 0.01 {
            w.flash = 0.0;
        }

        self.sky.weather = SkyWeather {
            overcast: atm.overcast,
            fog: atm.fog,
            aurora_scale: aurora,
            flash: w.flash,
            red_sky: atm.omens.red_sky,
            wind: atm.wind.speed,
            birds_quiet: atm.omens.birds_quiet,
        };
        self.session.environment = Some(atm.environment());
        self.fx.set_weather(atm.rain, atm.snow);
        self.fx.set_wind(atm.wind.vector());
        w.atmosphere = Some(atm);
    }

    /// Position (pieds) du joueur local.
    fn local_position(&self) -> Option<nb_core::glam::DVec3> {
        let player = self.session.player()?;
        Some(self.session.sim.entities.copy::<Transform>(player)?.pos)
    }

    /// Climat de la colonne `(x, z)` et direction de la mer vers l'intérieur des terres (gradient
    /// de la continentalité sur deux relevés voisins).
    fn probe_climate(&self, x: i32, z: i32) -> Option<(Climate, [f64; 2])> {
        let generator = self.session.world.profile().generator();
        let c0 = generator.climate_at(x, z)?;
        let inland = match (
            generator.climate_at(x + INLAND_STEP, z),
            generator.climate_at(x, z + INLAND_STEP),
        ) {
            (Some(cx), Some(cz)) => {
                Locale::inland_direction(c0.continental, cx.continental, cz.continental)
            }
            _ => [0.0, 0.0],
        };
        Some((c0, inland))
    }
}
