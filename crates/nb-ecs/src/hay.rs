//! Fenaison, grange, estive et kulning (session 6.4, doc 04 §6.2).
//!
//! * **Fenaison (slåtter)** : [`mow`] coupe l'herbe haute en juin à août (juillet pleinement),
//!   [`Hassja`] la fait sécher sur perches : trois jours de temps sec, la pluie fait reculer d'un
//!   jour par jour de pluie. Le foin (`ho`) se range dans une [`Barn`].
//! * **Grange** : en hiver (hors de la saison de pâture du bétail), chaque bête à moins de
//!   [`BARN_RANGE`] blocs de la grange mange une ration de foin par jour ; sans foin elle dépérit
//!   ([`crate::husbandry`]). [`herd_days`] dit combien de jours le foin engrangé nourrit.
//! * **Estive (fäbod)** : le lait double de juin à août à une fäbod (voir `HusbandrySystem`, qui
//!   cherche le bloc `karna` de la laiterie).
//! * **Kulning** : appel chanté appris à une fäbod ([`learn_kulning`]). [`call`] pose un
//!   [`Emitter`] qui attire le bétail et fait fuir loups et ours pendant [`WARD_SECS`] ; la corne
//!   de vache et la trompe d'écorce sont ses versions instrumentales.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use nb_core::glam::IVec3;
use nb_core::JavaRandom;
use nb_data::behavior::Stimulus;
use nb_data::husbandry::Role;
use nb_data::BlockId;

use crate::brain::clock::Clock;
use crate::brain::Emitter;
use crate::components::{EntityId, MobType, Transform};
use crate::husbandry::Domestic;
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;
use nb_world::weather::freezing_point;

/// Jours de séchage d'un foin.
pub const DRY_DAYS: f32 = 3.0;
/// Capacité d'une hässja, en brassées d'herbe.
pub const HASSJA_CAPACITY: u8 = 8;
/// Rayon (blocs) où une grange nourrit le bétail.
pub const BARN_RANGE: f64 = 12.0;
/// Durée de la protection d'un kulning (s).
pub const WARD_SECS: f32 = 60.0;
/// Secondes entre deux passes du [`HaySystem`].
pub const INTERVAL: f32 = 1.0;
/// Mois de la fauche (juin à août).
pub const MOW_MONTHS: [u8; 2] = [5, 7];

/// Rendement de la fauche d'un mois : juillet 1, juin et août 0,6 ; ailleurs rien.
pub fn mow_factor(month: usize) -> f32 {
    match month {
        6 => 1.0,
        5 | 7 => 0.6,
        _ => 0.0,
    }
}

/// Coupe l'herbe haute en `pos` (le bloc doit être `tall`) : renvoie les brassées d'herbe, 1 à 2.
pub fn mow(
    blocks: &mut dyn crate::blocks::BlockAccess,
    rng: &mut JavaRandom,
    pos: IVec3,
    tall: BlockId,
    air: BlockId,
    month: usize,
) -> Option<u32> {
    let f = mow_factor(month);
    if f <= 0.0 || blocks.block(pos.x, pos.y, pos.z) != tall {
        return None;
    }
    blocks.set_block(pos.x, pos.y, pos.z, air);
    let n = (1 + rng.next_int_bound(2)) as f32 * f;
    Some(n.round().max(1.0) as u32)
}

// ---------------------------------------------------------------------------------------------
// Hässja
// ---------------------------------------------------------------------------------------------

/// Temps d'un séchage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spell {
    /// Sec : le séchage avance.
    Dry,
    /// Couvert ou gris : il s'arrête.
    Idle,
    /// Pluie, neige, gel : il recule.
    Wet,
}

pub fn spell(env: &nb_world::weather::Environment) -> Spell {
    if env.rain > 0.15 || env.snow > 0.05 || f64::from(env.air) < freezing_point() {
        Spell::Wet
    } else if env.rain > 0.02 || env.fog > 0.4 {
        Spell::Idle
    } else {
        Spell::Dry
    }
}

/// Séchoir à perches chargé d'herbe.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Hassja {
    pub grass: u8,
    /// Jours de séchage accumulés.
    pub dry_days: f32,
}

impl Hassja {
    /// Suspend `n` brassées ; renvoie celles sans place. L'herbe fraîche ramène le séchage à la
    /// moyenne pondérée.
    pub fn load(&mut self, n: u8) -> u8 {
        let room = HASSJA_CAPACITY.saturating_sub(self.grass).min(n);
        let total = f32::from(self.grass + room);
        if total > 0.0 {
            self.dry_days = self.dry_days * f32::from(self.grass) / total;
        }
        self.grass += room;
        n - room
    }

    /// Avance de `days` jours de jeu (le recul de la pluie est d'un jour par jour de pluie).
    pub fn step(&mut self, spell: Spell, days: f32) {
        if self.grass == 0 {
            return;
        }
        match spell {
            Spell::Dry => self.dry_days = (self.dry_days + days).min(DRY_DAYS),
            Spell::Idle => {}
            Spell::Wet => self.dry_days = (self.dry_days - days).max(0.0),
        }
    }

    pub fn ready(&self) -> bool {
        self.grass > 0 && self.dry_days >= DRY_DAYS
    }

    /// Prend le foin sec : une botte par brassée.
    pub fn take(&mut self) -> u32 {
        if !self.ready() {
            return 0;
        }
        let n = u32::from(self.grass);
        self.grass = 0;
        self.dry_days = 0.0;
        n
    }
}

// ---------------------------------------------------------------------------------------------
// Grange
// ---------------------------------------------------------------------------------------------

/// Grange à foin.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Barn {
    pub hay: u32,
    /// Fraction de jour écoulée depuis la dernière ration.
    pub rest: f32,
}

impl Barn {
    pub fn store(&mut self, n: u32) {
        self.hay += n;
    }
}

/// Jours que `hay` bottes nourrissent `animals` bêtes (une botte par bête et par jour).
pub fn herd_days(hay: u32, animals: u32) -> u32 {
    hay.checked_div(animals).unwrap_or(u32::MAX)
}

/// Séchoirs et granges du monde, par case.
#[derive(Debug, Default)]
pub struct HayStore {
    pub racks: HashMap<IVec3, Hassja>,
    pub barns: HashMap<IVec3, Barn>,
}

pub type HayState = Arc<Mutex<HayStore>>;

// ---------------------------------------------------------------------------------------------
// Kulning
// ---------------------------------------------------------------------------------------------

/// Appel du bétail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    /// La voix, apprise à une fäbod.
    Kulning,
    /// Corne de vache.
    CowHorn,
    /// Trompe d'écorce (näverlur).
    BarkHorn,
}

impl Voice {
    /// Portée en blocs.
    pub fn radius(self) -> f32 {
        match self {
            Voice::Kulning => 96.0,
            Voice::CowHorn => 80.0,
            Voice::BarkHorn => 72.0,
        }
    }
}

/// Le joueur a appris le kulning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Kulner;

/// Un appel en cours : il s'éteint après `left` secondes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calling {
    pub left: f32,
}

/// Apprend le kulning : exige une fäbod habitée à proximité.
pub fn learn_kulning(world: &mut EntityWorld, player: EntityId, fabod_inhabited: bool) -> bool {
    if !fabod_inhabited {
        return false;
    }
    world.insert(player, Kulner);
    true
}

/// Lance un appel. La voix demande le kulning appris ; les instruments non. Renvoie vrai si
/// l'appel part.
pub fn call(world: &mut EntityWorld, events: &mut Vec<EcsEvent>, caller: EntityId, voice: Voice) -> bool {
    if voice == Voice::Kulning && !world.has::<Kulner>(caller) {
        return false;
    }
    let Some(pos) = world.copy::<Transform>(caller).map(|t| t.pos) else {
        return false;
    };
    world.insert(
        caller,
        Emitter {
            kind: Stimulus::Kulning,
            radius: voice.radius(),
        },
    );
    world.insert(caller, Calling { left: WARD_SECS });
    events.push(EcsEvent::Kulning {
        entity: caller,
        pos: IVec3::new(pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32),
    });
    true
}

// ---------------------------------------------------------------------------------------------
// Système
// ---------------------------------------------------------------------------------------------

/// Fait sécher les hässjas, nourrit le bétail à la grange et éteint les appels.
pub struct HaySystem {
    state: HayState,
    timer: f32,
}

impl HaySystem {
    pub fn new(state: HayState) -> Self {
        Self { state, timer: 0.0 }
    }
}

impl System for HaySystem {
    fn name(&self) -> &'static str {
        "Hay"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        // Les appels s'éteignent à chaque tick.
        for id in world.query_ids::<(&Calling,)>() {
            let done = world.get_mut::<Calling>(id).is_some_and(|c| {
                c.left -= dt;
                c.left <= 0.0
            });
            if done {
                world.remove::<Calling>(id);
                world.remove::<Emitter>(id);
            }
        }
        self.timer += dt;
        if self.timer < INTERVAL {
            return;
        }
        let step = std::mem::take(&mut self.timer);
        let days = step / ctx.config.day_length_seconds.max(1.0);
        let mut st = self.state.lock().unwrap();

        if let Some(env) = ctx.environment {
            let sp = spell(env);
            for (&pos, r) in st.racks.iter_mut() {
                let was = r.ready();
                r.step(sp, days);
                if r.ready() && !was {
                    ctx.events.push(EcsEvent::HayReady { pos });
                }
            }
        }

        // Rations d'hiver.
        let month = Clock::of(ctx.config, None).month;
        for (&pos, barn) in st.barns.iter_mut() {
            barn.rest += days;
            if barn.rest < 1.0 {
                continue;
            }
            barn.rest -= 1.0;
            let centre = nb_core::glam::DVec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y),
                f64::from(pos.z) + 0.5,
            );
            for id in world.query_ids::<(&Domestic, &MobType, &Transform)>() {
                if barn.hay == 0 {
                    break;
                }
                let Some(cfg) = world
                    .get::<MobType>(id)
                    .and_then(|m| ctx.mobs.get(&m.mob_id))
                else {
                    continue;
                };
                let Some(h) = cfg.husbandry.as_ref() else {
                    continue;
                };
                if !matches!(h.role, Role::Livestock | Role::Mount) {
                    continue;
                }
                let Some(f) = h.fodder.as_ref() else { continue };
                if nb_data::behavior::in_window(month, f.pasture) {
                    continue;
                }
                let near = world
                    .copy::<Transform>(id)
                    .is_some_and(|t| (t.pos - centre).length() <= BARN_RANGE);
                if near {
                    let item = f.item.clone();
                    if crate::husbandry::feed(world, ctx.mobs, id, &item) {
                        barn.hay -= 1;
                    }
                }
            }
        }
    }
}
