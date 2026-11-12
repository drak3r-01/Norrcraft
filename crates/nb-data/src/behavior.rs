//! Profils de comportement du bestiaire Norrcraft (GDD doc 03 §2, session 5.1).
//!
//! Un profil décrit en données ce qu'un animal **peut** faire : ses états (avec un poids d'utilité
//! et le besoin qui les nourrit), ses sens, la vitesse à laquelle ses besoins montent, son horloge
//! (activité journalière, fenêtres saisonnières, cycle pluriannuel) et ses réactions aux facteurs
//! du joueur (feu, fumée, surströmming, cloche, kulning). Le moteur (`nb-ecs::brain`) choisit
//! l'état par utilité : `score = poids · besoin · opportunité`.
//!
//! Les espèces (sessions 5.4 à 5.7) désignent un profil par le champ `behavior` de leur
//! `MobConfig`. Fichier : `assets/norrcraft/behaviors.json`.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use nb_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Profils intégrés.
pub const BEHAVIORS_JSON: &str = include_str!("../../../assets/norrcraft/behaviors.json");

/// Cible spéciale de `hunts` / `fears` : les joueurs.
pub const PLAYER: &str = "player";

/// États de base fournis par le moteur (doc 03 §2.1), dans l'ordre du document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StateKind {
    /// Errer.
    Wander,
    /// Brouter.
    Graze,
    /// Boire.
    Drink,
    /// Dormir.
    Sleep,
    /// Fuir.
    Flee,
    /// Menacer.
    Threaten,
    /// Charger.
    Charge,
    /// Traquer (approche furtive).
    Stalk,
    /// Poursuivre.
    Pursue,
    /// Attaquer.
    Attack,
    /// Encercler (meute).
    Encircle,
    /// Garder (territoire, petits).
    Guard,
    /// Suivre (chef de harde, appel).
    Follow,
    /// Migrer.
    Migrate,
    /// Hiberner.
    Hibernate,
    /// Parader (saison des amours).
    Display,
    /// Voler.
    Fly,
    /// Se percher.
    Perch,
    /// Plonger.
    Dive,
    /// Nager.
    Swim,
    /// Fouir.
    Burrow,
    /// Charogner.
    Scavenge,
    /// Voler de la nourriture.
    Steal,
    /// Construire.
    Build,
    /// Former le cercle défensif (bœuf musqué) : les adultes se rangent en couronne face à la
    /// menace, les jeunes au centre.
    Circle,
}

impl StateKind {
    pub const COUNT: usize = 25;
    pub const ALL: [StateKind; Self::COUNT] = [
        StateKind::Wander,
        StateKind::Graze,
        StateKind::Drink,
        StateKind::Sleep,
        StateKind::Flee,
        StateKind::Threaten,
        StateKind::Charge,
        StateKind::Stalk,
        StateKind::Pursue,
        StateKind::Attack,
        StateKind::Encircle,
        StateKind::Guard,
        StateKind::Follow,
        StateKind::Migrate,
        StateKind::Hibernate,
        StateKind::Display,
        StateKind::Fly,
        StateKind::Perch,
        StateKind::Dive,
        StateKind::Swim,
        StateKind::Burrow,
        StateKind::Scavenge,
        StateKind::Steal,
        StateKind::Build,
        StateKind::Circle,
    ];

    /// Besoin qui nourrit l'état quand le profil n'en précise pas.
    pub fn default_need(self) -> Need {
        use StateKind::*;
        match self {
            Graze | Stalk | Pursue | Attack | Encircle | Dive | Scavenge | Steal => Need::Hunger,
            Drink => Need::Thirst,
            Sleep => Need::Rest,
            Perch => Need::Fatigue,
            Flee | Threaten | Charge | Burrow | Circle => Need::Fear,
            Wander | Guard | Follow | Migrate | Hibernate | Display | Fly | Swim | Build => {
                Need::Constant
            }
        }
    }

    /// Nom français (doc 03 §2.1), pour l'Almanach et le débogage.
    pub fn french(self) -> &'static str {
        use StateKind::*;
        match self {
            Wander => "errer",
            Graze => "brouter",
            Drink => "boire",
            Sleep => "dormir",
            Flee => "fuir",
            Threaten => "menacer",
            Charge => "charger",
            Stalk => "traquer",
            Pursue => "poursuivre",
            Attack => "attaquer",
            Encircle => "encercler",
            Guard => "garder",
            Follow => "suivre",
            Migrate => "migrer",
            Hibernate => "hiberner",
            Display => "parader",
            Fly => "voler",
            Perch => "se percher",
            Dive => "plonger",
            Swim => "nager",
            Burrow => "fouir",
            Scavenge => "charogner",
            Steal => "voler de la nourriture",
            Build => "construire",
            Circle => "former le cercle",
        }
    }
}

/// Besoins (0 à 1). `Rest` = max(fatigue, 1 − activité de l'heure) ; `Constant` = 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Need {
    Hunger,
    Thirst,
    Fear,
    Fatigue,
    Cold,
    Rest,
    Constant,
}

/// Horloge journalière (doc 03 §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Activity {
    Diurnal,
    Nocturnal,
    Crepuscular,
    #[default]
    Always,
}

/// Milieu de déplacement que le profil suppose (le `medium` du mob fait foi pour la physique).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Locomotion {
    #[default]
    Walk,
    Swim,
    Fly,
}

/// Facteurs posés dans le monde ou portés par le joueur (doc 03 §2.2), et nourritures au sol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stimulus {
    Fire,
    Smoke,
    Surstromming,
    Bell,
    Kulning,
    /// Carcasse (charognards).
    Carrion,
    /// Nourriture accessible (séchoir, garde-manger ouvert).
    Food,
}

/// Portées des sens, en blocs ; `fov` = ouverture du cône de vue en degrés.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Senses {
    pub sight: f32,
    pub fov: f32,
    pub hearing: f32,
    pub smell: f32,
}

impl Default for Senses {
    fn default() -> Self {
        Self {
            sight: 24.0,
            fov: 270.0,
            hearing: 16.0,
            smell: 24.0,
        }
    }
}

/// Montée des besoins par seconde et tolérance au froid.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NeedRates {
    pub hunger: f32,
    pub thirst: f32,
    pub fatigue: f32,
    /// Température ressentie (échelle de T₀) sous laquelle le froid monte.
    pub cold_below: f32,
    /// Chaleur ajoutée par le pelage d'hiver.
    pub winter_coat: f32,
}

impl Default for NeedRates {
    fn default() -> Self {
        Self {
            hunger: 0.004,
            thirst: 0.005,
            fatigue: 0.003,
            cold_below: 0.1,
            winter_coat: 0.0,
        }
    }
}

/// Réaction à un facteur : peur (0 à 1), distance tenue, attirance (kulning, nourriture).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Reaction {
    pub fear: f32,
    pub keep_away: f32,
    pub attract: f32,
}

/// Fenêtre de mois réels `[début, fin]` inclus (0 = januari), qui peut passer le nouvel an.
pub type Window = [u8; 2];

/// Vrai si le mois `month` (0..12) est dans la fenêtre.
pub fn in_window(month: usize, w: Window) -> bool {
    let (a, b, m) = (usize::from(w[0]), usize::from(w[1]), month);
    if a <= b {
        (a..=b).contains(&m)
    } else {
        m >= a || m <= b
    }
}

/// Horloge saisonnière (doc 03 §2.3).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Seasons {
    /// Pelage blanc.
    pub coat: Option<Window>,
    pub hibernate: Option<Window>,
    /// Rut : courage porté à `rut_courage`.
    pub rut: Option<Window>,
    /// Mise bas : les mères gardent leur territoire, plus courageuses.
    pub birth: Option<Window>,
    /// Fenêtres de migration ; la première suit `heading`, la deuxième l'inverse.
    pub migrate: Vec<Window>,
    pub build: Option<Window>,
}

/// Renvoi vers un cycle pluriannuel, avec un décalage en années (prédateurs : 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleRef {
    pub name: String,
    #[serde(default)]
    pub lag: u32,
}

/// Cycle pluriannuel : `phase = (année − 1 − décalage) mod période`, facteur de densité par phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cycle {
    pub period: u32,
    pub factors: Vec<f32>,
}

impl Cycle {
    /// Phase de l'année de jeu `year` (à partir de 1) avec le décalage `lag`.
    pub fn phase(&self, year: u64, lag: u32) -> usize {
        let p = u64::from(self.period.max(1));
        let shifted = (year.max(1) - 1) + p * u64::from(lag).div_ceil(p) - u64::from(lag);
        (shifted % p) as usize
    }

    pub fn factor(&self, year: u64, lag: u32) -> f32 {
        self.factors
            .get(self.phase(year, lag))
            .copied()
            .unwrap_or(1.0)
    }
}

/// Un état offert au profil.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateDef {
    pub state: StateKind,
    pub weight: f32,
    /// Besoin qui nourrit l'état (défaut : [`StateKind::default_need`]).
    #[serde(default)]
    pub need: Option<Need>,
}

impl StateDef {
    pub fn need(&self) -> Need {
        self.need.unwrap_or_else(|| self.state.default_need())
    }
}

/// Profil de comportement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(default)]
    pub activity: Activity,
    #[serde(default)]
    pub locomotion: Locomotion,
    /// Fraction de la portée de vue gardée en pleine nuit (1 : nocturne).
    #[serde(default = "half")]
    pub night_vision: f32,
    #[serde(default)]
    pub senses: Senses,
    #[serde(default)]
    pub needs: NeedRates,
    /// 0 : fuit toujours ; 1 : tient tête (menace, charge).
    #[serde(default)]
    pub courage: f32,
    /// Courage pendant le rut.
    #[serde(default)]
    pub rut_courage: Option<f32>,
    /// Profils, identifiants de mobs ou `player` chassés.
    #[serde(default)]
    pub hunts: Vec<String>,
    /// Profils, identifiants de mobs ou `player` craints.
    #[serde(default)]
    pub fears: Vec<String>,
    /// Suit un chef de harde (le congénère d'identifiant le plus petit à portée).
    #[serde(default)]
    pub herd: bool,
    /// Chasse en meute (encercle avec ses congénères).
    #[serde(default)]
    pub pack: bool,
    #[serde(default)]
    pub reactions: BTreeMap<Stimulus, Reaction>,
    #[serde(default)]
    pub seasons: Seasons,
    /// Cap de la migration de printemps (x, z) ; celle d'automne va dans l'autre sens.
    #[serde(default)]
    pub heading: [f32; 2],
    #[serde(default)]
    pub cycle: Option<CycleRef>,
    /// Altitude de croisière au-dessus du sol (vol).
    #[serde(default)]
    pub cruise: f32,
    pub states: Vec<StateDef>,
}

fn half() -> f32 {
    0.5
}

impl Profile {
    /// Le profil chasse-t-il `who` (nom de profil ou identifiant de mob) ?
    pub fn hunts(&self, profile: &str, mob: &str) -> bool {
        self.hunts.iter().any(|h| h == profile || h == mob)
    }

    pub fn fears(&self, profile: &str, mob: &str) -> bool {
        self.fears.iter().any(|h| h == profile || h == mob)
    }

    pub fn offers(&self, s: StateKind) -> bool {
        self.states.iter().any(|d| d.state == s)
    }

    pub fn reaction(&self, s: Stimulus) -> Reaction {
        self.reactions.get(&s).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
struct File {
    #[serde(default)]
    cycles: BTreeMap<String, Cycle>,
    profiles: BTreeMap<String, Profile>,
}

/// Livre des profils (ordre alphabétique des noms : rang stable).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BehaviorBook {
    names: Vec<String>,
    profiles: Vec<Profile>,
    cycles: BTreeMap<String, Cycle>,
}

impl BehaviorBook {
    /// Profils intégrés (analysés une fois).
    pub fn builtin() -> Arc<BehaviorBook> {
        static BOOK: OnceLock<Arc<BehaviorBook>> = OnceLock::new();
        BOOK.get_or_init(|| {
            Arc::new(Self::from_json(BEHAVIORS_JSON).expect("behaviors.json invalide"))
        })
        .clone()
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let file: File =
            serde_json::from_str(json).map_err(|e| Error::data("behaviors.json", e))?;
        let (names, profiles) = file.profiles.into_iter().unzip();
        let book = Self {
            names,
            profiles,
            cycles: file.cycles,
        };
        book.validate()?;
        Ok(book)
    }

    fn validate(&self) -> Result<()> {
        let bad = |m: String| Err(Error::data("behaviors.json", m));
        for (name, c) in &self.cycles {
            if c.period == 0 || c.factors.len() != c.period as usize {
                return bad(format!(
                    "cycle {name} : {} facteurs pour une période {}",
                    c.factors.len(),
                    c.period
                ));
            }
        }
        for (name, p) in self.names.iter().zip(&self.profiles) {
            if p.states.is_empty() {
                return bad(format!("{name} : aucun état"));
            }
            if p.states.iter().any(|d| d.weight.is_nan() || d.weight < 0.0) {
                return bad(format!("{name} : poids négatif"));
            }
            if let Some(c) = &p.cycle {
                if !self.cycles.contains_key(&c.name) {
                    return bad(format!("{name} : cycle inconnu {}", c.name));
                }
            }
            let s = &p.seasons;
            let windows = [s.coat, s.hibernate, s.rut, s.birth, s.build]
                .into_iter()
                .flatten()
                .chain(s.migrate.iter().copied());
            for w in windows {
                if w[0] > 11 || w[1] > 11 {
                    return bad(format!("{name} : mois hors de 0..11"));
                }
            }
            if !(0.0..=1.0).contains(&p.courage) || !(0.0..=1.0).contains(&p.night_vision) {
                return bad(format!("{name} : courage ou nightVision hors de [0, 1]"));
            }
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.index_of(name).map(|i| &self.profiles[i])
    }

    pub fn profile(&self, i: usize) -> &Profile {
        &self.profiles[i]
    }

    pub fn name(&self, i: usize) -> &str {
        &self.names[i]
    }

    pub fn cycle(&self, name: &str) -> Option<&Cycle> {
        self.cycles.get(name)
    }

    /// Facteur d'un cycle désigné par un renvoi (1 si le cycle est inconnu).
    pub fn ref_factor(&self, c: &CycleRef, year: u64) -> f32 {
        self.cycle(&c.name)
            .map_or(1.0, |cy| cy.factor(year, c.lag))
    }

    /// Facteur pluriannuel du profil pour l'année `year` (1 sans cycle).
    pub fn cycle_factor(&self, p: &Profile, year: u64) -> f32 {
        p.cycle
            .as_ref()
            .and_then(|c| Some(self.cycle(&c.name)?.factor(year, c.lag)))
            .unwrap_or(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profiles_parse_and_validate() {
        let book = BehaviorBook::builtin();
        assert!(book.len() >= 8, "{:?}", book.names());
        for i in 0..book.len() {
            let p = book.profile(i);
            for h in p.hunts.iter().chain(&p.fears) {
                assert!(
                    h == PLAYER || book.index_of(h).is_some(),
                    "{} : cible inconnue {h}",
                    book.name(i)
                );
            }
        }
        // Tous les états du moteur sont offerts par au moins un profil intégré.
        for s in StateKind::ALL {
            assert!(
                (0..book.len()).any(|i| book.profile(i).offers(s)),
                "état jamais offert : {s:?}"
            );
        }
    }

    #[test]
    fn windows_wrap_around_new_year() {
        assert!(in_window(11, [10, 2]));
        assert!(in_window(0, [10, 2]));
        assert!(in_window(2, [10, 2]));
        assert!(!in_window(5, [10, 2]));
        assert!(in_window(8, [8, 8]));
        assert!(!in_window(9, [8, 8]));
    }

    #[test]
    fn lemming_cycle_and_predator_lag() {
        let book = BehaviorBook::builtin();
        let c = book.cycle("lemming").unwrap();
        // Année 1 = creux, puis montée, pic (« lämmelår »), effondrement.
        let f: Vec<f32> = (1..=8).map(|y| c.factor(y, 0)).collect();
        assert_eq!(f, [0.2, 1.0, 4.0, 0.5, 0.2, 1.0, 4.0, 0.5]);
        // Les prédateurs suivent avec un an de retard : leur pic tombe l'année d'après.
        let g: Vec<f32> = (1..=5).map(|y| c.factor(y, 1)).collect();
        assert_eq!(g, [0.5, 0.2, 1.0, 4.0, 0.5]);
    }

    #[test]
    fn invalid_files_are_rejected() {
        let no_state = r#"{"profiles": {"x": {"states": []}}}"#;
        assert!(BehaviorBook::from_json(no_state).is_err());
        let bad_cycle = r#"{"profiles": {"x": {"cycle": {"name": "y"}, "states": [{"state": "wander", "weight": 1}]}}}"#;
        assert!(BehaviorBook::from_json(bad_cycle).is_err());
        let bad_month = r#"{"profiles": {"x": {"seasons": {"rut": [12, 1]}, "states": [{"state": "wander", "weight": 1}]}}}"#;
        assert!(BehaviorBook::from_json(bad_month).is_err());
    }
}
