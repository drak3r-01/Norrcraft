//! Météo, vent, aurore et température ressentie (plan Norrcraft, session 3.3 ; GDD doc 04 §3).
//!
//! Tout est **pur** et déterministe : la météo en un lieu et à un instant est une fonction de la
//! graine du monde, de l'horloge de jeu et du [`Locale`] (climat du lieu). Aucun état à
//! sauvegarder ni à synchroniser : en LAN, chaque machine qui connaît la graine et l'horloge
//! (l'hôte fait foi pour l'horloge) retrouve la même météo.
//!
//! # Chaîne de Markov
//!
//! Le temps est découpé en *créneaux* de [`SLOT_TICKS`] (un quart de jour). Une chaîne de Markov
//! **synoptique** à trois états ([`Front`] : beau, variable, dépression) donne le régime du
//! créneau ; ses probabilités dépendent de la saison (dépressions plus fréquentes à l'automne).
//! La chaîne est évaluée sans état : on la fait tourner depuis [`BURN_IN`] créneaux plus tôt, ce
//! qui suffit à l'oubli de l'état initial. Le régime est ensuite **résolu localement** en l'un des
//! dix états de [`Weather`] selon la température de l'air (neige sous le gel), l'océanité (pluie,
//! brume, tempêtes de mer), l'altitude et la continentalité.
//!
//! La transition d'un créneau au suivant est fondue sur son dernier quart (aucun saut de pluie,
//! de brouillard ou de ciel), et l'écart de gravité avec le créneau suivant alimente les
//! **signes avant-coureurs** ([`Omens`]).
//!
//! # Vent
//!
//! Vecteur global lentement variable (bruit de valeur dans le temps, vents d'ouest dominants) dont
//! le module croît avec le régime, plus les effets locaux : brise de mer le jour et de terre la
//! nuit (le long du gradient de continentalité), canalisation le long de l'axe des fjords
//! (jusqu'à × 1,5).

use std::f64::consts::TAU;

use crate::calendar::Calendar;
use crate::profile::Climate;
use crate::seasons::{SeasonRules, Site};

/// Durée d'un créneau météo, en ticks de `time_of_day` (un quart de jour).
pub const SLOT_TICKS: f64 = 300.0;
/// Créneaux de chauffe de la chaîne de Markov (oubli de l'état initial).
const BURN_IN: i64 = 14;
/// Période du bruit de direction du vent, en ticks.
const WIND_PERIOD: f64 = 450.0;
/// Température de l'air (échelle de T₀) jugée confortable.
pub const COMFORT: f64 = 0.50;
/// Passage de l'échelle de température (0 à 1) à celle du ressenti de la survie.
pub const FELT_SCALE: f64 = 1.6;
/// Ticks par jour de jeu.
const DAY_TICKS: f64 = 1200.0;

// ---------------------------------------------------------------------------------------------
// Hachage déterministe
// ---------------------------------------------------------------------------------------------

fn finalize(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn mix(seed: u64, a: i64, tag: u64) -> u64 {
    finalize(seed ^ finalize((a as u64).wrapping_add(tag.wrapping_mul(0x9E37_79B9_7F4A_7C15))))
}

/// Flottant dans [0, 1) d'un hachage.
fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

fn smooth(lo: f64, hi: f64, v: f64) -> f64 {
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ---------------------------------------------------------------------------------------------
// États
// ---------------------------------------------------------------------------------------------

/// Régime synoptique d'un créneau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Front {
    /// Haute pression.
    Fair,
    /// Temps variable, front qui passe.
    Unsettled,
    /// Dépression creuse.
    Low,
}

impl Front {
    /// Vent ajouté par le régime (fraction du module maximal).
    fn wind(self) -> f64 {
        match self {
            Front::Fair => 0.0,
            Front::Unsettled => 0.10,
            Front::Low => 0.28,
        }
    }
}

/// Instabilité de la saison : les dépressions sont plus fréquentes à l'automne (a ≈ 0,8).
fn storminess(a: f64) -> f64 {
    1.0 + 0.5 * (TAU * (a - 0.80)).cos()
}

fn transition(from: Front, storm: f64, u: f64) -> Front {
    let (fair, unsettled, low) = match from {
        Front::Fair => (0.62, 0.30, 0.08),
        Front::Unsettled => (0.30, 0.40, 0.30),
        Front::Low => (0.15, 0.45, 0.40),
    };
    let low = low * storm;
    let r = u * (fair + unsettled + low);
    if r < fair {
        Front::Fair
    } else if r < fair + unsettled {
        Front::Unsettled
    } else {
        Front::Low
    }
}

/// États météorologiques (doc 04 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weather {
    Clear,
    /// Ciel voilé (annonce un front).
    Veiled,
    /// Bruine.
    Drizzle,
    Rain,
    Mist,
    /// Orage (pluie, éclairs).
    Storm,
    Snow,
    /// Tempête de neige (snöstorm).
    Blizzard,
    /// Tempête de mer.
    SeaGale,
    /// Grand froid clair.
    ColdClear,
}

/// Paramètres d'un état météo (cibles du fondu).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Params {
    rain: f32,
    snow: f32,
    /// Couverture nuageuse : assombrit et grise le ciel (0 à 1).
    overcast: f32,
    /// Densité de brouillard ajoutée à celle de l'air (0,002).
    fog: f32,
    thunder: f32,
    /// Vent ajouté (fraction du module maximal).
    gust: f32,
    /// Multiplicateur de refroidissement par le vent.
    chill: f32,
    /// Houle (vagues, poussée des bateaux).
    swell: f32,
}

#[allow(clippy::too_many_arguments)]
const fn params(
    rain: f32,
    snow: f32,
    overcast: f32,
    fog: f32,
    thunder: f32,
    gust: f32,
    chill: f32,
    swell: f32,
) -> Params {
    Params {
        rain,
        snow,
        overcast,
        fog,
        thunder,
        gust,
        chill,
        swell,
    }
}

impl Weather {
    pub const ALL: [Weather; 10] = [
        Weather::Clear,
        Weather::Veiled,
        Weather::Drizzle,
        Weather::Rain,
        Weather::Mist,
        Weather::Storm,
        Weather::Snow,
        Weather::Blizzard,
        Weather::SeaGale,
        Weather::ColdClear,
    ];

    /// Nom court de la commande `/weather`.
    pub fn key(self) -> &'static str {
        match self {
            Weather::Clear => "clear",
            Weather::Veiled => "veiled",
            Weather::Drizzle => "drizzle",
            Weather::Rain => "rain",
            Weather::Mist => "mist",
            Weather::Storm => "storm",
            Weather::Snow => "snow",
            Weather::Blizzard => "blizzard",
            Weather::SeaGale => "gale",
            Weather::ColdClear => "frost",
        }
    }

    pub fn parse(s: &str) -> Option<Weather> {
        let s = s.trim().to_ascii_lowercase();
        Weather::ALL.into_iter().find(|w| w.key() == s)
    }

    /// Nom français.
    pub fn name_fr(self) -> &'static str {
        match self {
            Weather::Clear => "clair",
            Weather::Veiled => "voilé",
            Weather::Drizzle => "bruine",
            Weather::Rain => "pluie",
            Weather::Mist => "brume",
            Weather::Storm => "orage",
            Weather::Snow => "neige",
            Weather::Blizzard => "blizzard",
            Weather::SeaGale => "tempête de mer",
            Weather::ColdClear => "grand froid clair",
        }
    }

    /// Nom suédois.
    pub fn name_sv(self) -> &'static str {
        match self {
            Weather::Clear => "klart",
            Weather::Veiled => "slöjmoln",
            Weather::Drizzle => "duggregn",
            Weather::Rain => "regn",
            Weather::Mist => "dimma",
            Weather::Storm => "åskväder",
            Weather::Snow => "snöfall",
            Weather::Blizzard => "snöstorm",
            Weather::SeaGale => "havsstorm",
            Weather::ColdClear => "klar kyla",
        }
    }

    /// Gravité de 0 (beau) à 1 (tempête), pour les signes avant-coureurs.
    pub fn severity(self) -> f32 {
        match self {
            Weather::Clear | Weather::ColdClear => 0.0,
            Weather::Mist => 0.1,
            Weather::Veiled => 0.2,
            Weather::Drizzle => 0.4,
            Weather::Snow => 0.5,
            Weather::Rain => 0.6,
            Weather::Storm | Weather::SeaGale | Weather::Blizzard => 1.0,
        }
    }

    fn params(self) -> Params {
        match self {
            //                    pluie neige couv  brume  orage vent  froid houle
            Weather::Clear => params(0.0, 0.0, 0.05, 0.0, 0.0, 0.0, 1.0, 0.0),
            Weather::Veiled => params(0.0, 0.0, 0.45, 0.0, 0.0, 0.03, 1.0, 0.1),
            Weather::Drizzle => params(0.28, 0.0, 0.65, 0.006, 0.0, 0.03, 1.0, 0.15),
            Weather::Rain => params(0.7, 0.0, 0.8, 0.004, 0.0, 0.08, 1.0, 0.3),
            Weather::Mist => params(0.0, 0.0, 0.3, 0.03, 0.0, -0.05, 1.0, 0.0),
            Weather::Storm => params(1.0, 0.0, 0.95, 0.005, 1.0, 0.30, 1.0, 0.6),
            Weather::Snow => params(0.0, 0.6, 0.75, 0.008, 0.0, 0.05, 1.0, 0.2),
            Weather::Blizzard => params(0.0, 1.0, 0.95, 0.12, 0.0, 0.50, 3.0, 0.4),
            Weather::SeaGale => params(0.6, 0.0, 0.9, 0.004, 0.0, 0.50, 1.0, 1.0),
            Weather::ColdClear => params(0.0, 0.0, 0.0, 0.0, 0.0, -0.02, 1.15, 0.0),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Horloge et lieu
// ---------------------------------------------------------------------------------------------

/// Horloge de la météo : temps de jeu en ticks et calage de l'année.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherClock {
    /// Ticks écoulés depuis la création du monde (`jour × 1200 + time_of_day`).
    pub ticks: f64,
    /// Longueur de l'année de jeu, en ticks.
    pub year_ticks: f64,
    /// Fraction d'année à l'instant 0.
    pub a0: f64,
}

impl WeatherClock {
    pub fn new(cal: &Calendar, day: u64, time_of_day: f32) -> Self {
        let year_ticks = f64::from(cal.days_per_year()) * DAY_TICKS;
        let ticks = day as f64 * DAY_TICKS + f64::from(time_of_day);
        let a = cal.year_fraction(day, time_of_day);
        Self {
            ticks,
            year_ticks,
            a0: (a - ticks / year_ticks).rem_euclid(1.0),
        }
    }

    /// Fraction d'année à l'instant `ticks`.
    pub fn year_fraction_at(&self, ticks: f64) -> f64 {
        (self.a0 + ticks / self.year_ticks).rem_euclid(1.0)
    }

    /// Fraction d'année courante.
    pub fn year_fraction(&self) -> f64 {
        self.year_fraction_at(self.ticks)
    }
}

/// Climat du lieu vu par la météo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Locale {
    /// Température au niveau de la mer (T₀).
    pub t0: f64,
    /// Océanité O : 1 = façade atlantique (humide, douce).
    pub ocean: f64,
    /// Continentalité C : 0 = haute mer, 0,465 = trait de côte, 1 = cœur des terres.
    pub continental: f64,
    /// Humidité Hm.
    pub humidity: f64,
    /// Relief accidenté (1 − E) : fjords et vallées qui canalisent le vent.
    pub rugged: f64,
    /// Axe des fjords (vecteur unitaire).
    pub grain: [f64; 2],
    /// Direction de la mer vers l'intérieur des terres (vecteur unitaire, nul si inconnu).
    pub inland: [f64; 2],
    /// Hauteur au-dessus du niveau de la mer, en blocs.
    pub altitude: f64,
}

impl Locale {
    /// Lieu tempéré, plat et à la mer (tests).
    pub fn plain(t0: f64) -> Self {
        Self {
            t0,
            ocean: 0.5,
            continental: 0.5,
            humidity: 0.5,
            rugged: 0.0,
            grain: [1.0, 0.0],
            inland: [0.0, 0.0],
            altitude: 0.0,
        }
    }

    /// Lieu d'après le climat d'une colonne, la direction de l'intérieur des terres et la hauteur
    /// `y` (bloc) du joueur.
    pub fn from_climate(c: &Climate, inland: [f64; 2], y: f64) -> Self {
        Self {
            t0: c.temperature,
            ocean: c.ocean,
            continental: c.continental,
            humidity: c.humidity,
            rugged: c.rugged,
            grain: c.grain,
            inland,
            altitude: y - f64::from(c.sea_level),
        }
    }

    /// Direction de la mer vers l'intérieur : gradient de la continentalité mesuré sur deux
    /// pas (`c0` au point, `cx` à `+d` en x, `cz` à `+d` en z).
    pub fn inland_direction(c0: f64, cx: f64, cz: f64) -> [f64; 2] {
        let (gx, gz) = (cx - c0, cz - c0);
        let n = (gx * gx + gz * gz).sqrt();
        if n < 1e-9 {
            [0.0, 0.0]
        } else {
            [gx / n, gz / n]
        }
    }

    /// Latitude λ = 1 − T₀.
    pub fn lambda(&self) -> f64 {
        (1.0 - self.t0).clamp(0.0, 1.0)
    }

    /// Le lieu pour les règles saisonnières (température moyenne annuelle à l'altitude).
    pub fn site(&self) -> Site {
        Site {
            mean: self.t0 - 0.006 * self.altitude.max(0.0) + 0.05 * (self.ocean - 0.5),
            lambda: self.lambda(),
            wet: self.humidity,
        }
    }

    /// Température de l'air à la fraction d'année `a` (doc 04 §3.3) : règles de saison de la 3.2,
    /// hivers plus durs et étés plus chauds à l'intérieur des terres, et `0,04 ×` jour/nuit
    /// (`day_night` de −1 à 1).
    pub fn air_temperature(&self, a: f64, day_night: f64) -> f64 {
        let rules = SeasonRules::builtin();
        let cos = (TAU * (a - rules.temperature.coldest)).cos();
        rules.temperature(a, &self.site()) - 0.12 * (self.continental - 0.5) * cos
            + 0.04 * day_night.clamp(-1.0, 1.0)
    }
}

/// Température de l'air (échelle de T₀) sous laquelle il neige.
pub fn freezing_point() -> f64 {
    SeasonRules::builtin().snow.threshold
}

// ---------------------------------------------------------------------------------------------
// Atmosphère
// ---------------------------------------------------------------------------------------------

/// Vent local.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wind {
    /// Direction vers laquelle le vent souffle (vecteur unitaire en x, z).
    pub dir: [f32; 2],
    /// Module de 0 à 1.
    pub speed: f32,
}

impl Wind {
    pub const CALM: Wind = Wind {
        dir: [1.0, 0.0],
        speed: 0.0,
    };

    /// Vecteur vent (direction × module).
    pub fn vector(&self) -> [f32; 2] {
        [self.dir[0] * self.speed, self.dir[1] * self.speed]
    }
}

/// Signes avant-coureurs d'un temps qui se gâte (doc 04 §3.1), de 0 à 1.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Omens {
    /// Halo autour du soleil et de la lune (voile qui s'épaissit).
    pub halo: f32,
    /// Hirondelles qui volent bas (temps doux seulement).
    pub swallows_low: f32,
    /// Ciel rouge à l'aube et au crépuscule.
    pub red_sky: f32,
    /// Silence des oiseaux.
    pub birds_quiet: f32,
}

/// Météo en un lieu et à un instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Atmosphere {
    pub state: Weather,
    /// État du créneau suivant (vers lequel on se fond en fin de créneau).
    pub next: Weather,
    /// Avancement du fondu vers `next`, de 0 à 1.
    pub blend: f32,
    pub rain: f32,
    pub snow: f32,
    pub overcast: f32,
    /// Densité de brouillard à ajouter à celle de l'air.
    pub fog: f32,
    pub thunder: f32,
    pub chill: f32,
    pub swell: f32,
    pub wind: Wind,
    pub omens: Omens,
    /// Température de l'air ici et maintenant (échelle de T₀, altitude, saison et heure comprises).
    pub air: f32,
    /// Hauteur du soleil (−1 à 1) utilisée pour l'échantillon.
    pub sun: f32,
}

/// Ce que la survie lit de l'atmosphère du joueur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Température de l'air (échelle de T₀).
    pub air: f32,
    pub wind: Wind,
    /// Multiplicateur de refroidissement par le vent (3 dans un blizzard).
    pub chill: f32,
    pub rain: f32,
    pub snow: f32,
    /// Densité de brouillard ajoutée à celle de l'air (portée de vue des animaux).
    pub fog: f32,
    /// Hauteur du soleil, de −1 à 1 (lumière pour la vue des animaux).
    pub sun: f32,
}

impl Atmosphere {
    pub fn environment(&self) -> Environment {
        Environment {
            air: self.air,
            wind: self.wind,
            chill: self.chill,
            rain: self.rain,
            snow: self.snow,
            fog: self.fog,
            sun: self.sun,
        }
    }

    /// Atmosphère figée sur un état (commande `/weather`), sans vent ni signes avant-coureurs
    /// propres : le vent est conservé de `base`.
    pub fn forced(base: &Atmosphere, state: Weather) -> Atmosphere {
        let p = state.params();
        let mut wind = base.wind;
        wind.speed = (wind.speed + p.gust.max(0.0)).clamp(0.0, 1.0);
        Atmosphere {
            state,
            next: state,
            blend: 0.0,
            rain: p.rain,
            snow: p.snow,
            overcast: p.overcast,
            fog: p.fog,
            thunder: p.thunder,
            chill: p.chill,
            swell: p.swell,
            wind,
            omens: Omens::default(),
            air: base.air,
            sun: base.sun,
        }
    }
}

/// Modèle météo d'un monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeatherModel {
    seed: u64,
}

impl WeatherModel {
    pub fn new(world_seed: i64) -> Self {
        Self {
            seed: finalize((world_seed as u64) ^ 0x57EA_7E43_C0DE_0033),
        }
    }

    /// Régimes des créneaux `slot`, `slot + 1` et `slot + 2`.
    pub fn fronts(&self, slot: i64, clock: &WeatherClock) -> [Front; 3] {
        let start = slot - BURN_IN;
        let u = unit(mix(self.seed, start, 10));
        let mut f = if u < 0.6 {
            Front::Fair
        } else if u < 0.9 {
            Front::Unsettled
        } else {
            Front::Low
        };
        let mut out = [f; 3];
        for s in start + 1..=slot + 2 {
            let a = clock.year_fraction_at(s as f64 * SLOT_TICKS);
            f = transition(f, storminess(a), unit(mix(self.seed, s, 11)));
            if s >= slot {
                out[(s - slot) as usize] = f;
            }
        }
        out
    }

    /// Résolution locale du régime d'un créneau en un état de [`Weather`].
    fn resolve(
        &self,
        slot: i64,
        front: Front,
        next: Front,
        clock: &WeatherClock,
        loc: &Locale,
    ) -> Weather {
        let a = clock.year_fraction_at((slot as f64 + 0.5) * SLOT_TICKS);
        let t = loc.air_temperature(a, 0.0);
        let freeze = freezing_point();
        let cold = t < freeze;
        let frigid = t < freeze - 0.10;
        let u1 = unit(mix(self.seed, slot, 21));
        let u2 = unit(mix(self.seed, slot, 22));
        // Altitude : plus de nuages et de précipitations en montagne.
        let alt = smooth(30.0, 110.0, loc.altitude);
        // Façade océanique et proximité de la mer.
        let wet = (0.35 * loc.ocean + 0.25 * loc.humidity + 0.4 * alt).clamp(0.0, 1.0);
        let coast = 1.0 - smooth(0.50, 0.70, loc.continental);
        match front {
            Front::Fair => {
                if frigid && u1 < 0.65 {
                    Weather::ColdClear
                } else if !cold && u2 < 0.10 + 0.22 * loc.ocean * coast + 0.06 * loc.humidity {
                    Weather::Mist
                } else if (next != Front::Fair && u1 < 0.55) || u1 < 0.35 * alt {
                    Weather::Veiled
                } else {
                    Weather::Clear
                }
            }
            Front::Unsettled => {
                if cold {
                    if u1 < 0.45 + 0.3 * wet {
                        Weather::Snow
                    } else {
                        Weather::Veiled
                    }
                } else if u1 < 0.40 - 0.2 * wet {
                    Weather::Veiled
                } else if u1 < 0.72 - 0.1 * wet {
                    Weather::Drizzle
                } else {
                    Weather::Rain
                }
            }
            Front::Low => {
                if cold {
                    if ((frigid || alt > 0.5) && u1 < 0.75) || u1 < 0.25 {
                        Weather::Blizzard
                    } else {
                        Weather::Snow
                    }
                } else if loc.ocean > 0.55 && coast > 0.4 && u2 < 0.55 {
                    Weather::SeaGale
                } else if t > 0.55 && u1 < 0.55 {
                    Weather::Storm
                } else {
                    Weather::Rain
                }
            }
        }
    }

    /// Vecteur vent global (direction, module) à l'instant `ticks`, sans effet local.
    fn global_wind(&self, ticks: f64) -> ([f64; 2], f64) {
        let k = (ticks / WIND_PERIOD).floor();
        let f = smooth(0.0, 1.0, ticks / WIND_PERIOD - k);
        let angle = |i: i64| (unit(mix(self.seed, i, 31)) - 0.5) * 2.4;
        let speed = |i: i64| 0.14 + 0.22 * unit(mix(self.seed, i, 32));
        let k = k as i64;
        let ang = angle(k) * (1.0 - f) + angle(k + 1) * f;
        let spd = speed(k) * (1.0 - f) + speed(k + 1) * f;
        ([ang.cos(), ang.sin()], spd)
    }

    /// Météo au lieu `loc` et à l'instant `clock` ; `sun_height` de −1 à 1 (jour/nuit et brises).
    pub fn sample(&self, clock: &WeatherClock, loc: &Locale, sun_height: f64) -> Atmosphere {
        let slot = (clock.ticks / SLOT_TICKS).floor() as i64;
        let frac = clock.ticks / SLOT_TICKS - slot as f64;
        let fronts = self.fronts(slot, clock);
        let w0 = self.resolve(slot, fronts[0], fronts[1], clock, loc);
        let w1 = self.resolve(slot + 1, fronts[1], fronts[2], clock, loc);
        let s = smooth(0.7, 1.0, frac) as f32;
        let (p0, p1) = (w0.params(), w1.params());
        let mixp = |f: fn(&Params) -> f32| lerp(f(&p0), f(&p1), s);
        let gust = mixp(|p| p.gust);

        // Vent : global + régime + rafales de l'état, brise, canalisation par les fjords.
        let (gdir, gspeed) = self.global_wind(clock.ticks);
        let front_wind = fronts[0].wind() * f64::from(1.0 - s) + fronts[1].wind() * f64::from(s);
        let total = gspeed + front_wind + f64::from(gust);
        let mut v = [gdir[0] * total, gdir[1] * total];
        let coast = 1.0 - smooth(0.50, 0.70, loc.continental);
        let breeze = coast
            * if sun_height > 0.0 {
                0.12 * sun_height
            } else {
                0.07 * sun_height
            };
        v[0] += loc.inland[0] * breeze;
        v[1] += loc.inland[1] * breeze;
        let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
        let (dir, mut speed) = if len > 1e-6 {
            ([v[0] / len, v[1] / len], len)
        } else {
            (gdir, 0.0)
        };
        let align = dir[0] * loc.grain[0] + dir[1] * loc.grain[1];
        speed *= 1.0 + 0.5 * loc.rugged * align * align;
        let wind = Wind {
            dir: [dir[0] as f32, dir[1] as f32],
            speed: speed.clamp(0.0, 1.0) as f32,
        };

        // Signes avant-coureurs : montent dans la moitié du créneau qui précède un temps pire.
        let worse = (w1.severity() - w0.severity()).max(0.0);
        let lead = smooth(0.1, 0.6, frac) as f32;
        let omens = Omens {
            halo: (worse * lead * 1.4).min(1.0),
            swallows_low: if loc.air_temperature(clock.year_fraction(), 0.0)
                > freezing_point() + 0.1
            {
                worse * lead
            } else {
                0.0
            },
            red_sky: worse * lead,
            birds_quiet: (worse * lead * 1.2).min(1.0),
        };

        let state = if s < 0.5 { w0 } else { w1 };
        Atmosphere {
            state,
            next: w1,
            blend: s,
            rain: mixp(|p| p.rain),
            snow: mixp(|p| p.snow),
            overcast: mixp(|p| p.overcast),
            fog: mixp(|p| p.fog),
            thunder: mixp(|p| p.thunder),
            chill: mixp(|p| p.chill),
            swell: mixp(|p| p.swell),
            wind,
            omens,
            air: loc.air_temperature(clock.year_fraction(), sun_height) as f32,
            sun: sun_height as f32,
        }
    }

    /// Intensité de l'aurore boréale (0 à 1) pour la nuit du jour `day` : nuits claires d'hiver,
    /// d'autant plus fréquentes que la latitude est grande (doc 04 §3.1).
    pub fn aurora(&self, day: u64, a: f64, lambda: f64, overcast: f32) -> f32 {
        let rules = SeasonRules::builtin();
        let cold = (TAU * (a - rules.temperature.coldest)).cos();
        let winter = smooth(-0.1, 0.6, cold);
        let north = smooth(0.25, 0.8, lambda);
        let chance = (0.1 + 0.8 * north) * winter;
        let u = unit(mix(self.seed, day as i64, 41));
        if u >= chance {
            return 0.0;
        }
        let strength = 0.45 + 0.55 * unit(mix(self.seed, day as i64, 42));
        let clear = (1.0 - f64::from(overcast)).clamp(0.0, 1.0);
        (strength * clear * clear) as f32
    }
}

// ---------------------------------------------------------------------------------------------
// Température ressentie
// ---------------------------------------------------------------------------------------------

/// Ressenti sur l'échelle de la survie (≥ −0,1 : le corps se réchauffe) :
/// `(air − 0,10 · vent · (1 − abri) · refroidissement · (1 − coupe-vent) − 0,15 · mouillé − 0,50) × 1,6`.
/// Les sources de chaleur et l'isolation s'ajoutent ensuite, comme avant.
pub fn felt_ambient(
    air: f32,
    wind: f32,
    shelter: f32,
    chill: f32,
    windproof: f32,
    wet: f32,
) -> f32 {
    let t = f64::from(air)
        - 0.10
            * f64::from(wind)
            * f64::from(1.0 - shelter.clamp(0.0, 1.0))
            * f64::from(chill)
            * f64::from(1.0 - windproof.clamp(0.0, 1.0))
        - 0.15 * f64::from(wet.clamp(0.0, 1.0));
    ((t - COMFORT) * FELT_SCALE) as f32
}

/// Mouillure gagnée par seconde sous la pluie (négligeable à couvert ou en tenue imperméable).
pub fn wetting_rate(rain: f32, snow: f32, covered: bool, waterproof: f32) -> f32 {
    if covered {
        return 0.0;
    }
    (0.040 * rain + 0.012 * snow) * (1.0 - waterproof.clamp(0.0, 1.0))
}

/// Séchage par seconde : une minute près d'un feu, bien plus longtemps au froid sans feu.
pub fn drying_rate(near_fire: bool, covered: bool, air: f32) -> f32 {
    if near_fire {
        1.0 / 60.0
    } else if covered {
        1.0 / 200.0
    } else if f64::from(air) > freezing_point() {
        1.0 / 300.0
    } else {
        1.0 / 600.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(cal: &Calendar, ticks: f64) -> WeatherClock {
        let day = (ticks / DAY_TICKS).floor() as u64;
        let tod = (ticks - day as f64 * DAY_TICKS) as f32;
        WeatherClock::new(cal, day, tod)
    }

    fn site(t0: f64, ocean: f64, continental: f64, altitude: f64) -> Locale {
        Locale {
            ocean,
            continental,
            altitude,
            ..Locale::plain(t0)
        }
    }

    #[test]
    fn same_inputs_same_weather() {
        let cal = Calendar::default();
        let m = WeatherModel::new(42);
        let loc = site(0.4, 0.5, 0.5, 0.0);
        for k in 0..200 {
            let c = clock(&cal, f64::from(k) * 137.0);
            assert_eq!(m.sample(&c, &loc, 0.3), m.sample(&c, &loc, 0.3));
        }
        let other = WeatherModel::new(43);
        let c = clock(&cal, 5000.0);
        let differs = (0..200).any(|k| {
            let c = clock(&cal, f64::from(k) * 300.0 + 5000.0);
            m.sample(&c, &loc, 0.0).state != other.sample(&c, &loc, 0.0).state
        });
        assert!(differs, "deux graines ont la même météo {c:?}");
    }

    #[test]
    fn atmosphere_changes_continuously() {
        let cal = Calendar::default();
        let m = WeatherModel::new(7);
        let loc = site(0.4, 0.7, 0.4, 20.0);
        let mut prev = m.sample(&clock(&cal, 0.0), &loc, 0.0);
        for k in 1..6000 {
            let cur = m.sample(&clock(&cal, f64::from(k)), &loc, 0.0);
            for (a, b, what) in [
                (prev.rain, cur.rain, "pluie"),
                (prev.snow, cur.snow, "neige"),
                (prev.overcast, cur.overcast, "ciel"),
                (prev.fog * 10.0, cur.fog * 10.0, "brouillard"),
                (prev.wind.speed, cur.wind.speed, "vent"),
            ] {
                assert!((a - b).abs() < 0.03, "{what} saute à t = {k} : {a} → {b}");
            }
            prev = cur;
        }
    }

    #[test]
    fn it_snows_only_below_freezing() {
        let cal = Calendar::default();
        let m = WeatherModel::new(11);
        let freeze = freezing_point();
        for t0 in [0.2, 0.4, 0.6, 0.8] {
            let loc = site(t0, 0.5, 0.5, 0.0);
            for k in 0..800 {
                let ticks = f64::from(k) * SLOT_TICKS;
                let c = clock(&cal, ticks);
                let a = c.year_fraction_at(ticks + SLOT_TICKS * 0.5);
                let t = loc.air_temperature(a, 0.0);
                let w = m.sample(&c, &loc, 0.0);
                if t >= freeze + 0.01 && w.blend == 0.0 {
                    assert_eq!(w.snow, 0.0, "neige à T = {t} (t0 {t0})");
                }
                if t < freeze - 0.01 && w.blend == 0.0 {
                    assert_eq!(w.rain, 0.0, "pluie à T = {t} (t0 {t0})");
                }
            }
        }
    }

    #[test]
    fn every_state_occurs_somewhere_in_a_few_years() {
        let cal = Calendar::default();
        let m = WeatherModel::new(5);
        let mut seen = std::collections::HashSet::new();
        for (t0, ocean, cont, alt) in [
            (0.15, 0.3, 0.9, 80.0),
            (0.3, 0.8, 0.45, 0.0),
            (0.45, 0.9, 0.4, 10.0),
            (0.75, 0.6, 0.6, 0.0),
            (0.6, 0.2, 0.8, 0.0),
        ] {
            let loc = site(t0, ocean, cont, alt);
            for k in 0..(24 * 4 * 8) {
                let c = clock(&cal, f64::from(k) * SLOT_TICKS);
                seen.insert(m.sample(&c, &loc, 0.0).state);
            }
        }
        for w in Weather::ALL {
            assert!(seen.contains(&w), "{w:?} n'apparaît jamais");
        }
    }

    #[test]
    fn blizzards_need_the_cold_and_sea_gales_the_coast() {
        let cal = Calendar::default();
        let m = WeatherModel::new(9);
        let warm = site(0.8, 0.9, 0.4, 0.0);
        let inland = site(0.6, 0.9, 0.95, 0.0);
        for k in 0..2000 {
            let c = clock(&cal, f64::from(k) * SLOT_TICKS);
            assert_ne!(m.sample(&c, &warm, 0.0).state, Weather::Blizzard);
            assert_ne!(m.sample(&c, &inland, 0.0).state, Weather::SeaGale);
        }
    }

    #[test]
    fn the_atlantic_coast_is_wetter_than_the_baltic() {
        let cal = Calendar::default();
        let m = WeatherModel::new(3);
        let count = |ocean: f64| {
            let loc = site(0.5, ocean, 0.5, 0.0);
            (0..1600)
                .filter(|k| {
                    let c = clock(&cal, f64::from(*k) * SLOT_TICKS);
                    let w = m.sample(&c, &loc, 0.0);
                    w.rain > 0.1 || w.snow > 0.1
                })
                .count()
        };
        assert!(
            count(0.95) > count(0.05),
            "{} vs {}",
            count(0.95),
            count(0.05)
        );
    }

    #[test]
    fn omens_precede_a_worse_slot() {
        let cal = Calendar::default();
        let m = WeatherModel::new(21);
        let loc = site(0.45, 0.6, 0.5, 0.0);
        let mut checked = 0;
        for slot in 0..1500 {
            let ticks = f64::from(slot) * SLOT_TICKS + 0.6 * SLOT_TICKS;
            let c = clock(&cal, ticks);
            let w = m.sample(&c, &loc, 0.0);
            let worse = Weather::severity(w.next) - Weather::severity(w.state);
            if worse > 0.5 {
                checked += 1;
                assert!(
                    w.omens.birds_quiet > 0.3,
                    "pas de signe avant {:?}→{:?}",
                    w.state,
                    w.next
                );
            }
            if worse <= 0.0 {
                assert_eq!(w.omens.birds_quiet, 0.0);
            }
        }
        assert!(checked > 10, "peu d'aggravations observées : {checked}");
    }

    #[test]
    fn wind_stays_in_range_and_follows_the_breeze() {
        let cal = Calendar::default();
        let m = WeatherModel::new(1);
        let mut loc = site(0.4, 0.5, 0.45, 0.0);
        loc.inland = [0.0, 1.0];
        loc.rugged = 1.0;
        loc.grain = [0.0, 1.0];
        let mut day_sum = 0.0;
        let mut night_sum = 0.0;
        for k in 0..2000 {
            let c = clock(&cal, f64::from(k) * 11.0);
            let day = m.sample(&c, &loc, 0.8).wind;
            let night = m.sample(&c, &loc, -0.8).wind;
            for w in [day, night] {
                assert!((0.0..=1.0).contains(&w.speed));
                let n = w.dir[0] * w.dir[0] + w.dir[1] * w.dir[1];
                assert!((n - 1.0).abs() < 1e-3);
            }
            day_sum += day.vector()[1];
            night_sum += night.vector()[1];
        }
        // La brise de mer pousse vers l'intérieur le jour, la brise de terre en sens inverse.
        assert!(day_sum > night_sum, "{day_sum} vs {night_sum}");
    }

    #[test]
    fn fjord_axis_channels_the_wind() {
        let cal = Calendar::default();
        let m = WeatherModel::new(2);
        let mut open = site(0.4, 0.5, 0.6, 0.0);
        open.rugged = 0.0;
        let mut fjord = open;
        fjord.rugged = 1.0;
        let mut gained = 0;
        for k in 0..1000 {
            let c = clock(&cal, f64::from(k) * 13.0);
            let (a, b) = (
                m.sample(&c, &open, 0.0).wind,
                m.sample(&c, &fjord, 0.0).wind,
            );
            assert!(b.speed + 1e-6 >= a.speed.min(1.0) || b.speed >= 0.999);
            if b.speed > a.speed + 0.01 {
                gained += 1;
            }
        }
        assert!(gained > 100);
    }

    #[test]
    fn aurora_loves_clear_winter_nights_in_the_north() {
        let m = WeatherModel::new(17);
        let count = |a: f64, lambda: f64, overcast: f32| {
            (0..400)
                .filter(|d| m.aurora(*d, a, lambda, overcast) > 0.0)
                .count()
        };
        let winter_north = count(0.0, 0.9, 0.0);
        assert!(winter_north > 100, "{winter_north}");
        assert!(count(0.5, 0.9, 0.0) < winter_north / 6, "été");
        assert!(count(0.0, 0.2, 0.0) < winter_north / 3, "sud");
        for d in 0..400 {
            let v = m.aurora(d, 0.0, 0.9, 0.0);
            assert!((0.0..=1.0).contains(&v));
            assert_eq!(m.aurora(d, 0.0, 0.9, 1.0), 0.0, "ciel couvert");
        }
    }

    #[test]
    fn inland_winters_are_harder_and_summers_hotter() {
        let coast = site(0.4, 0.5, 0.2, 0.0);
        let inland = site(0.4, 0.5, 0.9, 0.0);
        let winter = SeasonRules::builtin().temperature.coldest;
        assert!(inland.air_temperature(winter, 0.0) < coast.air_temperature(winter, 0.0));
        assert!(
            inland.air_temperature(winter + 0.5, 0.0) > coast.air_temperature(winter + 0.5, 0.0)
        );
        // Altitude : plus froid.
        let high = site(0.4, 0.5, 0.5, 100.0);
        assert!(
            high.air_temperature(0.5, 0.0) < site(0.4, 0.5, 0.5, 0.0).air_temperature(0.5, 0.0)
        );
    }

    #[test]
    fn felt_temperature_follows_the_doc() {
        // Confort : pas de vent, sec.
        assert!(felt_ambient(0.5, 0.0, 0.0, 1.0, 0.0, 0.0).abs() < 1e-6);
        // Le vent refroidit, l'abri et le coupe-vent le compensent.
        let bare = felt_ambient(0.4, 0.8, 0.0, 1.0, 0.0, 0.0);
        let sheltered = felt_ambient(0.4, 0.8, 1.0, 1.0, 0.0, 0.0);
        let coat = felt_ambient(0.4, 0.8, 0.0, 1.0, 1.0, 0.0);
        assert!(bare < sheltered && (sheltered - coat).abs() < 1e-6);
        // Blizzard : ×3 sur le refroidissement par le vent.
        let gale = felt_ambient(0.4, 0.8, 0.0, 3.0, 0.0, 0.0);
        assert!(((sheltered - gale) - 3.0 * (sheltered - bare)).abs() < 1e-5);
        // Mouillé : 0,15 de température.
        let wet = felt_ambient(0.4, 0.0, 0.0, 1.0, 0.0, 1.0);
        assert!((felt_ambient(0.4, 0.0, 0.0, 1.0, 0.0, 0.0) - wet - 0.15 * 1.6).abs() < 1e-5);
    }

    #[test]
    fn wetting_and_drying() {
        assert_eq!(wetting_rate(1.0, 0.0, true, 0.0), 0.0);
        assert!(wetting_rate(1.0, 0.0, false, 0.0) > wetting_rate(1.0, 0.0, false, 0.8));
        assert_eq!(wetting_rate(1.0, 0.0, false, 1.0), 0.0);
        // Sous la pluie sans vêtement : trempé en moins d'une minute.
        assert!(1.0 / wetting_rate(1.0, 0.0, false, 0.0) < 60.0);
        // Un feu sèche en une minute.
        assert!((1.0 / drying_rate(true, false, 0.4) - 60.0).abs() < 1e-3);
        assert!(drying_rate(true, false, 0.4) > drying_rate(false, true, 0.4));
    }

    #[test]
    fn forced_weather_and_names() {
        let cal = Calendar::default();
        let m = WeatherModel::new(1);
        let base = m.sample(&clock(&cal, 100.0), &Locale::plain(0.4), 0.0);
        let f = Atmosphere::forced(&base, Weather::Blizzard);
        assert_eq!(f.state, Weather::Blizzard);
        assert_eq!(f.chill, 3.0);
        assert_eq!(f.snow, 1.0);
        for w in Weather::ALL {
            assert_eq!(Weather::parse(w.key()), Some(w));
            assert!(!w.name_fr().is_empty() && !w.name_sv().is_empty());
        }
        assert_eq!(Weather::parse("nimportequoi"), None);
    }

    #[test]
    fn clock_year_fraction_matches_the_calendar() {
        let cal = Calendar::new(2);
        for (day, tod) in [(0, 0.0), (3, 450.0), (30, 1100.0), (100, 5.0)] {
            let c = WeatherClock::new(&cal, day, tod);
            assert!((c.year_fraction() - cal.year_fraction(day, tod)).abs() < 1e-9);
        }
    }
}
