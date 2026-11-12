//! Calendrier, saisons et lumière du jour (plan Norrcraft, session 3.1 ; GDD doc 04 §2.1, §2.2).
//!
//! Tout est calculé à l'exécution à partir d'un compteur de jours (`SkyClock::day` côté ciel,
//! `WorldConfig::calendar_day` à la sauvegarde) : le cosinus est permis ici, contrairement à la
//! génération du terrain. En LAN, l'hôte fait foi pour le compteur ; chaque joueur calcule la
//! durée du jour pour sa propre latitude.
//!
//! # Conventions
//!
//! * L'année de jeu compte `12 × d` jours (`d` = jours par mois, défaut 2 : 24 jours, 8 heures).
//!   La fraction d'année `a ∈ [0, 1)` vaut 0 au **solstice d'hiver** (21 décembre réel) et 0,5 au
//!   solstice d'été.
//! * Les dates affichées suivent le calendrier réel (365 jours, mois suédois) : un jour de jeu
//!   couvre `365 / (12 d)` jours réels et porte la date de son premier jour réel.
//! * Un jour de jeu commence au coucher du soleil de l'horloge du ciel (`time_of_day = 0`).
//! * Un nouveau monde démarre à `a = 0,40` (mi-mai) : la saison des semailles, loin de l'hiver.
//! * Écart au doc : le solstice d'été tombe le 21 juin réel, au milieu de `juni` et non au début ;
//!   les fêtes à date fixe sont placées sur le jour de jeu qui contient leur date réelle.

use std::f64::consts::TAU;

/// Durée d'un jour de jeu en secondes (inchangée par rapport au moteur).
pub const DAY_SECONDS: f32 = 1200.0;

/// Jours par mois par défaut (réglage de création : 2, 4 ou 8).
pub const DEFAULT_DAYS_PER_MONTH: u32 = 2;

/// Fraction d'année à la création d'un monde (mi-mai).
pub const START_YEAR_FRACTION: f64 = 0.40;

/// Jour réel (base 0, 1er janvier = 0) du solstice d'hiver, le 21 décembre.
const WINTER_SOLSTICE_DOY: u32 = 354;
const REAL_YEAR_DAYS: u32 = 365;

/// Longueur des mois réels, de janvier à décembre.
const MONTH_LENGTHS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Noms suédois des mois, de janvier à décembre.
pub const MONTH_NAMES: [&str; 12] = [
    "januari",
    "februari",
    "mars",
    "april",
    "maj",
    "juni",
    "juli",
    "augusti",
    "september",
    "oktober",
    "november",
    "december",
];

/// Les quatre saisons du GDD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Season {
    /// Printemps : mars, april, maj.
    Var,
    /// Été : juni, juli, augusti.
    Sommar,
    /// Automne : september, oktober, november.
    Host,
    /// Hiver : december, januari, februari.
    Vinter,
}

impl Season {
    /// Nom suédois (clé stable).
    pub fn name(self) -> &'static str {
        match self {
            Season::Var => "Vår",
            Season::Sommar => "Sommar",
            Season::Host => "Höst",
            Season::Vinter => "Vinter",
        }
    }

    /// Nom français, pour l'Almanach.
    pub fn french(self) -> &'static str {
        match self {
            Season::Var => "printemps",
            Season::Sommar => "été",
            Season::Host => "automne",
            Season::Vinter => "hiver",
        }
    }

    /// Saison d'un mois réel (0 = janvier).
    pub fn of_month(month: usize) -> Season {
        match month {
            2..=4 => Season::Var,
            5..=7 => Season::Sommar,
            8..=10 => Season::Host,
            _ => Season::Vinter,
        }
    }
}

/// Date d'un jour de jeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    /// Numéro d'année de jeu, à partir de 1.
    pub year: u64,
    /// Rang du jour dans l'année de jeu, à partir de 0 (0 = solstice d'hiver).
    pub day_of_year: u32,
    /// Mois réel, 0 = januari.
    pub month: usize,
    /// Jour du mois réel, à partir de 1.
    pub day_of_month: u32,
    pub season: Season,
}

impl Date {
    /// « 14 maj ».
    pub fn label(&self) -> String {
        format!("{} {}", self.day_of_month, MONTH_NAMES[self.month])
    }
}

/// Une fête du calendrier (GDD doc 04 §8.5). Les rituels et effets viennent avec la phase 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Festival {
    pub name: &'static str,
    /// Mois réel, 0 = januari.
    pub month: usize,
    /// Jour du mois réel, à partir de 1.
    pub day: u32,
    /// Explication courte pour l'Almanach.
    pub note: &'static str,
}

/// Fêtes de l'année, dans l'ordre du calendrier réel. Les fêtes mobiles du doc (mardi gras,
/// Pâques) ont une date fixe, au milieu de leur plage.
pub const FESTIVALS: [Festival; 12] = [
    Festival {
        name: "Knut",
        month: 0,
        day: 13,
        note: "on sort le sapin",
    },
    Festival {
        name: "Fettisdagen",
        month: 1,
        day: 16,
        note: "on mange une semla",
    },
    Festival {
        name: "Våffeldagen",
        month: 2,
        day: 25,
        note: "gaufres, retour des grues",
    },
    Festival {
        name: "Påsk",
        month: 3,
        day: 12,
        note: "feux de Pâques",
    },
    Festival {
        name: "Valborg",
        month: 3,
        day: 30,
        note: "grand bûcher, fin des ténèbres d'hiver",
    },
    Festival {
        name: "Midsommar",
        month: 5,
        day: 22,
        note: "le mât fleuri, la nuit des sept fleurs",
    },
    Festival {
        name: "Kräftskiva",
        month: 7,
        day: 10,
        note: "banquet d'écrevisses",
    },
    Festival {
        name: "Surströmming",
        month: 7,
        day: 22,
        note: "on ouvre la boîte dehors",
    },
    Festival {
        name: "Kanelbullens dag",
        month: 9,
        day: 4,
        note: "fika doublée",
    },
    Festival {
        name: "Mårten gås",
        month: 10,
        day: 11,
        note: "oie rôtie",
    },
    Festival {
        name: "Lucia",
        month: 11,
        day: 13,
        note: "bougies, veillée de la plus longue nuit",
    },
    Festival {
        name: "Jul",
        month: 11,
        day: 21,
        note: "bouillie pour le tomte, solstice d'hiver",
    },
];

/// Signes du temps de chaque mois réel (janvier à décembre), de ce que dit le GDD de la saison.
const SIGNS: [&str; 12] = [
    "le gel craque, nuits d'aurores, on veille au coin du feu",
    "neige épaisse, glace portante, bûcheronnage",
    "la débâcle approche, les jours rallongent vite",
    "la glace se fend, la sève de bouleau monte, les grues reviennent",
    "premières anémones, semailles, veaux dans les prés",
    "la neige a fui, les nuits blanches commencent",
    "soleil de minuit au nord, moustiques, fenaison, premières baies",
    "baies mûres, estive, écrevisses dans les ruisseaux",
    "premiers champignons, moissons, l'élan entre en rut",
    "feuillages d'or et de rouge, chasse, premières gelées",
    "abattage du bétail, lacs qui se couvrent, jours très courts",
    "nuit longue, neige durable, veillées et bougies",
];

/// Phases de la lune, de la nouvelle à la dernière.
const MOON_PHASES: [&str; 8] = [
    "nouvelle lune",
    "premier croissant",
    "premier quartier",
    "lune gibbeuse croissante",
    "pleine lune",
    "lune gibbeuse décroissante",
    "dernier quartier",
    "dernier croissant",
];

/// Durée d'une lunaison en jours réels.
const LUNATION_DAYS: f64 = 29.530_588;

/// Régime de lumière aux hautes latitudes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polar {
    /// Le soleil se lève et se couche chaque jour.
    None,
    /// Le soleil ne se couche pas (autour du solstice d'été, au-delà du cercle polaire).
    MidnightSun,
    /// Le soleil ne se lève pas (autour du solstice d'hiver, au-delà du cercle polaire).
    PolarNight,
}

/// Latitude λ ∈ [0, 1] d'une température de base T₀ : λ = 1 − T₀ (doc 04 §2.2).
pub fn latitude_from_t0(t0: f64) -> f32 {
    (1.0 - t0).clamp(0.0, 1.0) as f32
}

/// Amplitude saisonnière A(λ) = 0,16 + 0,44 λ.
pub fn amplitude(latitude: f32) -> f32 {
    0.16 + 0.44 * latitude.clamp(0.0, 1.0)
}

/// Fraction non bornée du jour éclairé : `0,5 − A(λ) cos(2π a)`. Hors de [0, 1] : soleil de
/// minuit (> 1) ou nuit polaire (< 0).
pub fn raw_daylight(year_fraction: f64, latitude: f32) -> f32 {
    (0.5 - f64::from(amplitude(latitude)) * (TAU * year_fraction).cos()) as f32
}

/// Décalage `c` de la hauteur du soleil qui produit la fraction éclairée `raw`.
///
/// La hauteur normalisée du soleil est `(c − sin θ) / (1 + |c|)` (θ = angle de l'horloge du
/// ciel) : positive pendant la fraction `raw` du jour quand `c = −cos(π raw)`. Au-delà de
/// [0, 1], `c` sort de [−1, 1] : le soleil reste au-dessus (ou au-dessous) de l'horizon, de plus en
/// plus loin avec `raw`. Continu en `raw = 0` et `raw = 1`.
pub fn solar_bias(raw: f32) -> f32 {
    if raw >= 1.0 {
        1.0 + (raw - 1.0) * 4.0
    } else if raw <= 0.0 {
        -1.0 + raw * 4.0
    } else {
        -(std::f32::consts::PI * raw).cos()
    }
}

/// Hauteur du soleil de −1 à 1 à l'heure `time_of_day` (0..1200) avec le décalage solaire
/// `bias` : `(bias − sin θ) / (1 + |bias|)`, même formule que le ciel de nb-gfx (sert à la vue
/// des animaux quand la météo n'en fournit pas).
pub fn sun_height(time_of_day: f32, bias: f32) -> f32 {
    let angle = f64::from(time_of_day / DAY_SECONDS) * TAU;
    ((f64::from(bias) - angle.sin()) / (1.0 + f64::from(bias.abs()))).clamp(-1.0, 1.0) as f32
}

/// Lumière du jour à un instant et pour une latitude.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayLight {
    /// Fraction éclairée du jour, bornée à [0, 1].
    pub fraction: f32,
    pub polar: Polar,
    /// Décalage solaire pour l'horloge du ciel (voir [`solar_bias`]).
    pub bias: f32,
}

impl DayLight {
    pub fn at(year_fraction: f64, latitude: f32) -> Self {
        let raw = raw_daylight(year_fraction, latitude);
        Self {
            fraction: raw.clamp(0.0, 1.0),
            polar: if raw > 1.0 {
                Polar::MidnightSun
            } else if raw < 0.0 {
                Polar::PolarNight
            } else {
                Polar::None
            },
            bias: solar_bias(raw),
        }
    }

    /// Heures de jour sur un jour de 24 h.
    pub fn hours(&self) -> f32 {
        self.fraction * 24.0
    }
}

/// Réglages du calendrier d'un monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calendar {
    days_per_month: u32,
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new(DEFAULT_DAYS_PER_MONTH)
    }
}

impl Calendar {
    /// `days_per_month` est borné à [1, 64].
    pub fn new(days_per_month: u32) -> Self {
        Self {
            days_per_month: days_per_month.clamp(1, 64),
        }
    }

    pub fn days_per_month(&self) -> u32 {
        self.days_per_month
    }

    /// Jours de jeu dans une année.
    pub fn days_per_year(&self) -> u32 {
        12 * self.days_per_month
    }

    /// Rang dans l'année du jour 0 d'un monde neuf.
    fn start_index(&self) -> u64 {
        (START_YEAR_FRACTION * f64::from(self.days_per_year())).round() as u64
    }

    /// Jour d'année absolu (depuis la création du monde) : `day` plus le décalage de départ.
    fn absolute(&self, day: u64) -> u64 {
        day + self.start_index()
    }

    /// Fraction d'année `a ∈ [0, 1)` à l'instant `time_of_day` (0..1200) du jour `day`.
    pub fn year_fraction(&self, day: u64, time_of_day: f32) -> f64 {
        let n = u64::from(self.days_per_year());
        let in_day = f64::from(time_of_day / DAY_SECONDS).clamp(0.0, 1.0);
        let idx = (self.absolute(day) % n) as f64;
        ((idx + in_day) / n as f64).fract()
    }

    /// Années de jeu écoulées (fractionnaires) depuis la création, pour la lune.
    fn elapsed_years(&self, day: u64, time_of_day: f32) -> f64 {
        let n = f64::from(self.days_per_year());
        let in_day = f64::from(time_of_day / DAY_SECONDS).clamp(0.0, 1.0);
        (self.absolute(day) as f64 + in_day) / n
    }

    /// Date du jour de jeu `day`.
    pub fn date(&self, day: u64) -> Date {
        let n = u64::from(self.days_per_year());
        let abs = self.absolute(day);
        let idx = (abs % n) as u32;
        let real = (WINTER_SOLSTICE_DOY + (u64::from(idx) * u64::from(REAL_YEAR_DAYS) / n) as u32)
            % REAL_YEAR_DAYS;
        let (month, day_of_month) = month_and_day(real);
        Date {
            year: abs / n + 1,
            day_of_year: idx,
            month,
            day_of_month,
            season: Season::of_month(month),
        }
    }

    /// Compteur de jours du jour de rang `day_of_year` (0..) de l'année de jeu `year` (à partir
    /// de 1). Une date antérieure à la création du monde passe à l'année suivante.
    pub fn day_for(&self, year: u64, day_of_year: u32) -> u64 {
        let n = u64::from(self.days_per_year());
        let idx = u64::from(day_of_year).min(n - 1);
        let mut abs = year.saturating_sub(1) * n + idx;
        if abs < self.start_index() {
            abs += n;
        }
        abs - self.start_index()
    }

    /// Rang dans l'année d'un jour de jeu `day`, comme [`Date::day_of_year`].
    pub fn day_of_year(&self, day: u64) -> u32 {
        (self.absolute(day) % u64::from(self.days_per_year())) as u32
    }

    /// Rang dans l'année du jour de jeu qui contient la fête.
    pub fn festival_index(&self, f: &Festival) -> u32 {
        let real = real_day_of_year(f.month, f.day);
        let since_solstice = (real + REAL_YEAR_DAYS - WINTER_SOLSTICE_DOY) % REAL_YEAR_DAYS;
        let n = self.days_per_year();
        ((u64::from(since_solstice) * u64::from(n)) / u64::from(REAL_YEAR_DAYS)) as u32
    }

    /// Fête du jour, s'il y en a une (la première si plusieurs tombent le même jour de jeu).
    pub fn festival_today(&self, day: u64) -> Option<&'static Festival> {
        let idx = self.day_of_year(day);
        FESTIVALS.iter().find(|f| self.festival_index(f) == idx)
    }

    /// Prochaine fête strictement après aujourd'hui et nombre de jours de jeu à attendre.
    pub fn next_festival(&self, day: u64) -> (&'static Festival, u32) {
        let idx = self.day_of_year(day);
        let n = self.days_per_year();
        FESTIVALS
            .iter()
            .map(|f| {
                let wait = (self.festival_index(f) + n - idx) % n;
                (f, if wait == 0 { n } else { wait })
            })
            .min_by_key(|&(_, wait)| wait)
            .expect("la table des fêtes n'est pas vide")
    }

    /// Phase de la lune, de 0 (nouvelle) à 1 (exclu), à partir du calendrier réel.
    pub fn moon_phase(&self, day: u64, time_of_day: f32) -> f32 {
        let real_days = self.elapsed_years(day, time_of_day) * f64::from(REAL_YEAR_DAYS);
        (real_days / LUNATION_DAYS + 0.3).fract() as f32
    }

    /// Almanach d'un instant pour une latitude donnée.
    pub fn almanac(&self, day: u64, time_of_day: f32, latitude: f32) -> Almanac {
        let date = self.date(day);
        let light = DayLight::at(self.year_fraction(day, time_of_day), latitude);
        let (next, wait) = self.next_festival(day);
        let phase = self.moon_phase(day, time_of_day);
        Almanac {
            date,
            light,
            today: self.festival_today(day),
            next,
            next_in_days: wait,
            moon_phase: phase,
            moon: moon_name(phase),
            sign: SIGNS[date.month],
        }
    }
}

/// Instantané de l'Almanach : date, saison, durée du jour, prochaine fête, lune, signes du temps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Almanac {
    pub date: Date,
    pub light: DayLight,
    pub today: Option<&'static Festival>,
    pub next: &'static Festival,
    pub next_in_days: u32,
    pub moon_phase: f32,
    pub moon: &'static str,
    pub sign: &'static str,
}

impl Almanac {
    /// Lignes de texte de l'Almanach (messages du chat, futur onglet de la Sagabok).
    pub fn lines(&self) -> Vec<String> {
        let d = &self.date;
        let mut out = vec![format!(
            "Almanach : {} (année {}), {} / {}",
            d.label(),
            d.year,
            d.season.name(),
            d.season.french()
        )];
        let l = &self.light;
        out.push(match l.polar {
            Polar::MidnightSun => "Jour : soleil de minuit, il ne se couche pas".to_owned(),
            Polar::PolarNight => "Jour : nuit polaire, un crépuscule bleu à midi".to_owned(),
            Polar::None => {
                let h = l.hours();
                let whole = h.floor();
                let min = ((h - whole) * 60.0).round() as u32;
                let (whole, min) = if min == 60 {
                    (whole as u32 + 1, 0)
                } else {
                    (whole as u32, min)
                };
                format!("Jour : {whole} h {min:02} de lumière sur 24 h")
            }
        });
        out.push(match self.today {
            Some(f) => format!("Fête : aujourd'hui {} ({})", f.name, f.note),
            None => format!(
                "Fête : {} dans {} jour{} ({})",
                self.next.name,
                self.next_in_days,
                if self.next_in_days > 1 { "s" } else { "" },
                self.next.note
            ),
        });
        out.push(format!("Lune : {}", self.moon));
        out.push(format!("Signes : {}", self.sign));
        out
    }
}

/// Nom de la phase de lune (huit phases).
pub fn moon_name(phase: f32) -> &'static str {
    let i = ((phase.rem_euclid(1.0) * 8.0) + 0.5).floor() as usize % 8;
    MOON_PHASES[i]
}

/// Jour de l'année réelle (base 0) d'une date.
fn real_day_of_year(month: usize, day: u32) -> u32 {
    MONTH_LENGTHS[..month].iter().sum::<u32>() + day - 1
}

/// Mois (0 = januari) et jour du mois (base 1) d'un jour réel (base 0).
fn month_and_day(mut doy: u32) -> (usize, u32) {
    for (m, &len) in MONTH_LENGTHS.iter().enumerate() {
        if doy < len {
            return (m, doy + 1);
        }
        doy -= len;
    }
    (11, 31)
}

/// Lisse la latitude pour éviter tout saut de lumière quand le joueur change de région.
///
/// `target` est la latitude du lieu, `dt` le pas en secondes. Constante de temps : 4 s.
pub fn smooth_latitude(current: f32, target: f32, dt: f64) -> f32 {
    let k = (1.0 - (-dt / 4.0).exp()).clamp(0.0, 1.0) as f32;
    current + (target - current) * k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_has_twelve_d_days_and_wraps() {
        for d in [1, 2, 4, 8] {
            let c = Calendar::new(d);
            assert_eq!(c.days_per_year(), 12 * d);
            let n = u64::from(c.days_per_year());
            assert_eq!(c.date(0).year, 1);
            assert_eq!(c.date(0).day_of_year, c.day_of_year(n));
            assert_eq!(c.date(n).day_of_year, c.date(0).day_of_year);
            assert_eq!(c.date(n).year, c.date(0).year + 1);
        }
    }

    #[test]
    fn new_world_starts_in_spring_and_seasons_follow() {
        let c = Calendar::default();
        assert_eq!(c.date(0).season, Season::Var);
        // Un tour d'année couvre les quatre saisons, dans l'ordre.
        let mut seen = Vec::new();
        for day in 0..u64::from(c.days_per_year()) {
            let s = c.date(day).season;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        assert_eq!(
            seen,
            [
                Season::Var,
                Season::Sommar,
                Season::Host,
                Season::Vinter,
                Season::Var
            ]
        );
        assert_eq!(Season::of_month(11), Season::Vinter);
        assert_eq!(Season::of_month(0), Season::Vinter);
        assert_eq!(Season::of_month(2), Season::Var);
    }

    #[test]
    fn dates_follow_the_real_calendar() {
        assert_eq!(real_day_of_year(0, 1), 0);
        assert_eq!(real_day_of_year(11, 31), 364);
        assert_eq!(month_and_day(0), (0, 1));
        assert_eq!(month_and_day(31), (1, 1));
        assert_eq!(month_and_day(364), (11, 31));
        // Le jour 0 de l'année de jeu est le 21 décembre.
        let c = Calendar::default();
        let n = u64::from(c.days_per_year());
        let start = n - u64::from(c.day_of_year(0));
        let d = c.date(start);
        assert_eq!(d.day_of_year, 0);
        assert_eq!((d.month, d.day_of_month), (11, 21));
        assert_eq!(d.label(), "21 december");
    }

    #[test]
    fn every_festival_falls_on_a_distinct_day_with_the_default_year() {
        let c = Calendar::default();
        let mut seen = std::collections::HashSet::new();
        for f in &FESTIVALS {
            assert!(
                seen.insert(c.festival_index(f)),
                "{} tombe le même jour qu'une autre fête",
                f.name
            );
        }
        // Jul au solstice d'hiver, Midsommar au solstice d'été.
        let jul = FESTIVALS.iter().find(|f| f.name == "Jul").unwrap();
        let mid = FESTIVALS.iter().find(|f| f.name == "Midsommar").unwrap();
        assert_eq!(c.festival_index(jul), 0);
        assert_eq!(c.festival_index(mid), c.days_per_year() / 2);
    }

    #[test]
    fn next_festival_is_strictly_in_the_future() {
        for d in [2, 4, 8] {
            let c = Calendar::new(d);
            for day in 0..u64::from(c.days_per_year()) * 2 {
                let (f, wait) = c.next_festival(day);
                assert!(wait >= 1 && wait <= c.days_per_year());
                assert_eq!(
                    c.festival_index(f),
                    c.day_of_year(day + u64::from(wait)),
                    "{} au jour {day}",
                    f.name
                );
                if let Some(t) = c.festival_today(day) {
                    assert_eq!(c.festival_index(t), c.day_of_year(day));
                }
            }
        }
    }

    #[test]
    fn daylight_matches_the_doc_table() {
        // Au sud (λ = 0,1) : de 30 % à 70 % de jour.
        let winter = DayLight::at(0.0, 0.1);
        let summer = DayLight::at(0.5, 0.1);
        assert!((winter.fraction - 0.296).abs() < 1e-3, "{winter:?}");
        assert!((summer.fraction - 0.704).abs() < 1e-3, "{summer:?}");
        assert_eq!(winter.polar, Polar::None);
        // Équinoxe : moitié-moitié partout.
        for lat in [0.0, 0.5, 1.0] {
            let eq = DayLight::at(0.25, lat);
            assert!((eq.fraction - 0.5).abs() < 1e-5);
            assert!(eq.bias.abs() < 1e-5);
        }
    }

    #[test]
    fn polar_circle_is_at_lambda_077() {
        let below = 0.76;
        let above = 0.80;
        assert_eq!(DayLight::at(0.5, below).polar, Polar::None);
        assert_eq!(DayLight::at(0.0, below).polar, Polar::None);
        assert_eq!(DayLight::at(0.5, above).polar, Polar::MidnightSun);
        assert_eq!(DayLight::at(0.0, above).polar, Polar::PolarNight);
        let top = DayLight::at(0.5, 1.0);
        assert_eq!(top.fraction, 1.0);
        let bottom = DayLight::at(0.0, 1.0);
        assert_eq!(bottom.fraction, 0.0);
        assert!(top.bias > 1.0 && bottom.bias < -1.0);
    }

    #[test]
    fn solar_bias_is_continuous_and_monotonic() {
        let mut prev = solar_bias(-0.3);
        let mut r = -0.3f32;
        while r < 1.3 {
            r += 0.001;
            let b = solar_bias(r);
            assert!(b >= prev - 1e-6, "pas monotone en {r}");
            assert!((b - prev).abs() < 0.02, "saut en {r}");
            prev = b;
        }
        assert!((solar_bias(0.5)).abs() < 1e-6);
        assert!((solar_bias(1.0) - 1.0).abs() < 1e-6);
        assert!((solar_bias(0.0) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn year_fraction_is_zero_at_the_winter_solstice() {
        let c = Calendar::default();
        let n = u64::from(c.days_per_year());
        let jul_day = n - u64::from(c.day_of_year(0));
        assert_eq!(c.year_fraction(jul_day, 0.0), 0.0);
        let half = jul_day + n / 2;
        assert!((c.year_fraction(half, 0.0) - 0.5).abs() < 1e-12);
        let f = c.year_fraction(jul_day, 600.0);
        assert!((f - 0.5 / n as f64).abs() < 1e-12);
    }

    #[test]
    fn moon_cycles_about_twelve_times_a_year() {
        let c = Calendar::default();
        let n = u64::from(c.days_per_year());
        let mut new_moons = 0;
        let mut prev = c.moon_phase(0, 0.0);
        for step in 1..=(n * 4) {
            let p = c.moon_phase(step / 4, ((step % 4) as f32) * 300.0);
            if p < prev {
                new_moons += 1;
            }
            prev = p;
        }
        assert!((12..=13).contains(&new_moons), "{new_moons} lunaisons");
        assert_eq!(moon_name(0.0), "nouvelle lune");
        assert_eq!(moon_name(0.5), "pleine lune");
        assert_eq!(moon_name(0.99), "nouvelle lune");
    }

    #[test]
    fn almanac_text_mentions_every_part() {
        let c = Calendar::default();
        let a = c.almanac(3, 900.0, 0.3);
        let text = a.lines().join("\n");
        for needle in ["Almanach", "Jour", "Fête", "Lune", "Signes"] {
            assert!(text.contains(needle), "{needle} manque dans {text}");
        }
        let n = u64::from(c.days_per_year());
        let midsummer = (n / 2 + n - u64::from(c.day_of_year(0))) % n;
        let polar = c.almanac(midsummer, 0.0, 1.0);
        assert!(polar.lines().join("\n").contains("soleil de minuit"));
    }

    #[test]
    fn smoothing_converges_without_overshoot() {
        let mut l = 0.2;
        for _ in 0..1800 {
            l = smooth_latitude(l, 0.9, 1.0 / 60.0);
            assert!(l <= 0.9);
        }
        assert!((l - 0.9).abs() < 0.05, "{l}");
        assert_eq!(smooth_latitude(0.5, 0.5, 1.0), 0.5);
    }
}
