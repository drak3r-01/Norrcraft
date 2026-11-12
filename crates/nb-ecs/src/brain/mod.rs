//! Moteur d'IA du bestiaire Norrcraft (GDD doc 03 §2, session 5.1).
//!
//! Chaque animal mené par un profil de `behaviors.json` porte un [`Brain`]. À chaque tick :
//!
//! 1. ses **besoins** montent (faim, soif, fatigue, froid) ou retombent (peur, avec une mémoire de
//!    6 s) ;
//! 2. tous les [`PERCEIVE_EVERY`] ticks (décalés par identifiant), il **perçoit** : vue (cône,
//!    lumière, brouillard, feuillages), ouïe (pas du joueur, pluie, vent), odorat sous le vent ;
//!    facteurs posés ou portés ([`Emitter`] : feu, fumée, surströmming, cloche, kulning,
//!    charogne, nourriture) ;
//! 3. il choisit un **état** par utilité, `score = poids · besoin · opportunité`, avec une
//!    hystérésis de 2 s ;
//! 4. l'état **agit** : consignes de vitesse et de cap (la physique fait le reste), morsures,
//!    vols, événements [`AnimalAct`] pour les sessions qui modifient le monde (5.8).
//!
//! Les horloges (activité journalière, saisons, cycle des lemmings) sont dans [`clock`]. Les IA
//! passive et agressive d'origine ne voient pas ces animaux et restent intactes pour vanilla.

pub mod clock;
pub mod senses;

use std::sync::Arc;

use nb_core::glam::{DVec3, IVec3};
use nb_core::JavaRandom;
use nb_data::behavior::{BehaviorBook, Locomotion, Need, Profile, StateKind, Stimulus, PLAYER};
use nb_data::{BlockRegistry, Gait, MobConfig, MobMedium, MobRegistry};
use nb_world::weather::Environment;

use crate::blocks::BlockAccess;
use crate::components::{
    Collider, EntityId, GameMode, Health, MobType, Player, RemotePlayer, Transform, Velocity,
};
use crate::jmath::floor_i;
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;
use clock::Clock;
use senses::{Ranges, Sense, Source};

/// Durée minimale dans un état avant d'en changer (doc 03 §2.2).
pub const HYSTERESIS: f32 = 2.0;
/// Écart de score qui permet de changer d'état avant la fin de l'hystérésis (fuite soudaine).
pub const OVERRIDE_MARGIN: f32 = 0.35;
/// Période de la perception, en ticks (10 Hz à 60 ticks/s).
pub const PERCEIVE_EVERY: u64 = 6;
/// Constante de temps de l'oubli de la peur, en secondes.
pub const FEAR_MEMORY: f32 = 6.0;
/// Rayon du cercle de la meute autour de la proie.
pub const ENCIRCLE_RADIUS: f64 = 5.0;
/// Durée de l'encerclement avant l'assaut.
pub const ENCIRCLE_TIME: f32 = 6.0;
/// Distance au gîte au-delà de laquelle l'errance ramène vers lui.
pub const HOME_LEASH: f64 = 48.0;
/// Rayon de garde (mise bas, territoire).
pub const GUARD_LEASH: f64 = 6.0;
/// Les congénères se connaissent dans ce rayon (harde, meute).
pub const SOCIAL_RANGE: f64 = 32.0;
/// Distance de menace : en deçà, un animal courageux charge.
pub const CHARGE_RANGE: f32 = 4.0;
/// Distance au-delà de laquelle un animal menacé ne menace plus.
pub const THREATEN_RANGE: f32 = 14.0;

// ---------------------------------------------------------------------------------------------
// Composants
// ---------------------------------------------------------------------------------------------

/// Besoins (0 à 1).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Needs {
    pub hunger: f32,
    pub thirst: f32,
    pub fear: f32,
    pub fatigue: f32,
    pub cold: f32,
}

impl Needs {
    fn clamp(&mut self) {
        for v in [
            &mut self.hunger,
            &mut self.thirst,
            &mut self.fear,
            &mut self.fatigue,
            &mut self.cold,
        ] {
            *v = v.clamp(0.0, 1.0);
        }
    }
}

/// Une chose perçue (entité ou facteur posé).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sensed {
    pub id: Option<EntityId>,
    pub pos: DVec3,
    pub dist: f32,
    /// Éveil (0 à 1) : 1 au contact, 0 à la limite du sens.
    pub awareness: f32,
    pub sense: Sense,
}

/// Résultat de la dernière perception.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Percepts {
    /// Menace la plus pressante (prédateur, joueur, feu…).
    pub threat: Option<Sensed>,
    /// Peur que la menace inspire (0 à 1).
    pub threat_level: f32,
    /// Vrai si un facteur tenu à distance (feu) est trop près.
    pub repelled: bool,
    pub prey: Option<Sensed>,
    /// Chef de harde ou de meute (congénère d'identifiant plus petit).
    pub leader: Option<Sensed>,
    /// Appel qui attire (kulning).
    pub lure: Option<Sensed>,
    pub carrion: Option<Sensed>,
    /// Congénère ou autre animal qui porte de la nourriture.
    pub carrier: Option<Sensed>,
    /// Congénère le plus proche (parade).
    pub mate: Option<Sensed>,
    /// Taille de la meute et rang de l'animal (ordre des identifiants).
    pub pack_size: u8,
    pub pack_rank: u8,
    /// Eau repérée.
    pub water: Option<DVec3>,
}

/// Cerveau d'un animal Norrcraft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Brain {
    /// Rang du profil dans le livre (résolu au premier tick depuis `MobConfig::behavior`).
    pub profile: Option<u16>,
    pub state: StateKind,
    /// Temps passé dans l'état courant.
    pub since: f32,
    /// Derniers scores, par état (débogage, tests).
    pub scores: [f32; StateKind::COUNT],
    pub needs: Needs,
    pub percepts: Percepts,
    /// Gîte, tanière, terrier, hutte.
    pub home: DVec3,
    /// Destination imposée de l'extérieur (hardes, session 5.3).
    pub goal: Option<DVec3>,
    /// Dernière position connue de la menace (pour fuir encore après l'avoir perdue).
    pub last_threat: Option<DVec3>,
    /// Secondes de colère après un coup.
    pub provoked: f32,
    pub provoker: Option<EntityId>,
    pub attack_cooldown: f32,
    pub move_timer: f32,
    pub moving: bool,
    pub encircle_time: f32,
    /// Minuteur des actes répétés (broutage, construction).
    pub act_timer: f32,
    /// Pelage d'hiver (blanc).
    pub winter_coat: bool,
    /// Terré ou en hibernation : invisible comme proie.
    pub hidden: bool,
    pub in_water: bool,
    /// Activité de l'heure (0 à 1) pour l'animation.
    pub activity: f32,
}

impl Brain {
    pub fn new(home: DVec3) -> Self {
        Self {
            profile: None,
            state: StateKind::Wander,
            since: 0.0,
            scores: [0.0; StateKind::COUNT],
            needs: Needs {
                hunger: 0.3,
                thirst: 0.2,
                ..Default::default()
            },
            percepts: Percepts::default(),
            home,
            goal: None,
            last_threat: None,
            provoked: 0.0,
            provoker: None,
            attack_cooldown: 0.0,
            move_timer: 0.0,
            moving: false,
            encircle_time: 0.0,
            act_timer: 0.0,
            winter_coat: false,
            hidden: false,
            in_water: false,
            activity: 1.0,
        }
    }

    pub fn score(&self, s: StateKind) -> f32 {
        self.scores[s as usize]
    }
}

/// Facteur posé dans le monde ou porté par une entité (doc 03 §2.2) : feu, fumée, surströmming,
/// cloche, kulning, charogne, nourriture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Emitter {
    pub kind: Stimulus,
    /// Portée au-delà de laquelle le facteur n'est plus senti (en plus de la distance tenue).
    pub radius: f32,
}

/// L'entité porte de la nourriture (poisson pris par une loutre, gibier) : on peut la lui voler.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Carrying {
    /// Valeur nutritive (0 à 1 de faim apaisée).
    pub food: f32,
}

/// Le joueur avance accroupi : son bruit est divisé par 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Crouching;

/// Acte d'un animal sur le monde, pour les sessions qui le modifient (5.8) et les sons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimalAct {
    /// Broute le bloc sous lui.
    Graze,
    /// Travaille à sa hutte ou à son barrage.
    Build,
    /// A volé la nourriture d'un autre.
    Steal,
    /// A tué une proie.
    Kill,
}

/// Coup reçu : peur maximale, colère contre l'agresseur (un animal courageux charge).
pub fn provoke(world: &mut EntityWorld, mob: EntityId, by: Option<EntityId>) {
    let by_pos = by.and_then(|b| world.copy::<Transform>(b)).map(|t| t.pos);
    if let Some(b) = world.get_mut::<Brain>(mob) {
        b.needs.fear = 1.0;
        b.provoked = 6.0;
        b.provoker = by;
        if by_pos.is_some() {
            b.last_threat = by_pos;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Système
// ---------------------------------------------------------------------------------------------

/// Ce qu'un animal peut percevoir.
#[derive(Debug, Clone)]
struct Actor {
    id: EntityId,
    src: Source,
    alive: bool,
    carrying: bool,
    kind: ActorKind,
}

#[derive(Debug, Clone)]
enum ActorKind {
    Player {
        survival: bool,
    },
    Animal {
        profile: Option<usize>,
        mob: String,
        hidden: bool,
    },
}

/// Conséquence d'un acte, appliquée après le tour de l'animal.
#[derive(Debug, Clone, Copy)]
enum Effect {
    Hit { target: EntityId, hunting: bool },
    Steal { from: EntityId },
}

/// Système du moteur d'états (à placer après les IA d'origine, avant les apparitions).
#[derive(Debug)]
pub struct BrainSystem {
    book: Arc<BehaviorBook>,
    tick: u64,
}

impl Default for BrainSystem {
    fn default() -> Self {
        Self::new(BehaviorBook::builtin())
    }
}

impl BrainSystem {
    pub fn new(book: Arc<BehaviorBook>) -> Self {
        Self { book, tick: 0 }
    }

    pub fn book(&self) -> &BehaviorBook {
        &self.book
    }
}

/// Vitesses tirées du `MobConfig`.
#[derive(Debug, Clone, Copy)]
struct Speeds {
    walk: f32,
    run: f32,
    stalk: f32,
}

impl Speeds {
    fn of(cfg: &MobConfig) -> Self {
        let walk = 0.5 * (cfg.move_speed_min + cfg.move_speed_max);
        Self {
            walk,
            run: cfg
                .player_flee_speed
                .max(cfg.chase_speed)
                .max(cfg.aggressor_flee_speed)
                .max(walk),
            stalk: 0.35 * walk,
        }
    }
}

/// Tout ce que l'état courant doit savoir.
struct Frame<'a> {
    id: EntityId,
    p: &'a Profile,
    cfg: &'a MobConfig,
    clock: &'a Clock,
    grounded: bool,
    courage: f32,
    activity: f32,
    migration: Option<usize>,
    water_adjacent: bool,
    in_water: bool,
}

impl System for BrainSystem {
    fn name(&self) -> &'static str {
        "Brain"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        self.tick += 1;
        let brains = world.query_ids::<(&Brain, &Transform, &Velocity, &MobType)>();
        if brains.is_empty() {
            return;
        }
        let book = self.book.clone();
        let env = ctx.environment.copied();
        let clock = Clock::of(ctx.config, env.map(|e| e.sun));
        let table = senses::sight_table(ctx.block_reg);
        let water = ctx.block_reg.ids().water;
        let actors = snapshot(world, ctx.mobs, &book);
        let emitters: Vec<(EntityId, Emitter, DVec3)> = world
            .query_ids::<(&Emitter, &Transform)>()
            .into_iter()
            .filter_map(|id| {
                Some((
                    id,
                    world.copy::<Emitter>(id)?,
                    world.copy::<Transform>(id)?.pos,
                ))
            })
            .collect();

        for id in brains {
            let (Some(mut b), Some(mut t), Some(mut v)) = (
                world.copy::<Brain>(id),
                world.copy::<Transform>(id),
                world.copy::<Velocity>(id),
            ) else {
                continue;
            };
            if world.copy::<Health>(id).is_some_and(|h| !h.alive) {
                continue;
            }
            let Some(mob_id) = world.get::<MobType>(id).map(|m| m.mob_id.clone()) else {
                continue;
            };
            let Some(cfg) = ctx.mobs.get(&mob_id) else {
                continue;
            };
            if b.profile.is_none() {
                b.profile = cfg
                    .behavior
                    .as_deref()
                    .and_then(|n| book.index_of(n))
                    .and_then(|i| u16::try_from(i).ok());
            }
            let Some(pi) = b.profile.map(usize::from) else {
                continue;
            };
            let p = book.profile(pi);
            let height = world.copy::<Collider>(id).map_or(1.0, |c| c.height);
            let blocks: &dyn BlockAccess = &*ctx.blocks;

            b.in_water =
                blocks.block(floor_i(t.pos.x), floor_i(t.pos.y + 0.3), floor_i(t.pos.z)) == water;
            let activity = clock::activity(p.activity, clock.sun);
            b.activity = activity;
            b.winter_coat = clock::winter_coat(p, &clock);

            // 1. Besoins.
            update_needs(
                &mut b,
                p,
                env.as_ref(),
                activity,
                v.v.length() > Speeds::of(cfg).walk * 1.3,
                dt,
            );

            // 2. Perception, étalée sur les ticks.
            if (self.tick + id.0 as u64).is_multiple_of(PERCEIVE_EVERY)
                || b.since == 0.0 && b.percepts == Percepts::default()
            {
                let ranges = Ranges::new(&p.senses, p.night_vision, clock.sun, env.as_ref());
                perceive_all(
                    &mut b,
                    id,
                    pi,
                    &book,
                    &t,
                    height,
                    &ranges,
                    &actors,
                    &emitters,
                    env.as_ref(),
                    blocks,
                    &table,
                    water,
                    &mut ctx.rngs.brain,
                );
            }
            b.needs.fear = b.needs.fear.max(b.percepts.threat_level);
            if let Some(th) = b.percepts.threat {
                b.last_threat = Some(th.pos);
            } else if b.needs.fear < 0.05 {
                b.last_threat = None;
            }
            b.needs.clamp();

            let water_adjacent = adjacent_water(blocks, t.pos, water);
            let f = Frame {
                id,
                p,
                cfg,
                clock: &clock,
                grounded: v.grounded,
                courage: (clock::courage(p, &clock) + if b.provoked > 0.0 { 0.5 } else { 0.0 })
                    .min(1.0),
                activity,
                migration: clock::migration(p, &clock),
                water_adjacent,
                in_water: b.in_water,
            };

            // 3. Choix de l'état.
            choose(&mut b, &f, dt);

            // 4. Action.
            let mut effects = Vec::new();
            act(
                &mut b,
                &mut t,
                &mut v,
                &f,
                &actors,
                blocks,
                Ground::of(ctx.block_reg, cfg),
                &mut ctx.rngs.brain,
                &mut effects,
                ctx.events,
                dt,
            );
            b.attack_cooldown = (b.attack_cooldown - dt).max(0.0);
            b.provoked = (b.provoked - dt).max(0.0);
            if b.provoked <= 0.0 {
                b.provoker = None;
            }
            world.insert(id, b);
            world.insert(id, t);
            world.insert(id, v);
            for e in effects {
                apply(world, id, cfg, e, ctx.events);
            }
        }
    }
}

/// Instantané des joueurs et des animaux à brain.
fn snapshot(world: &EntityWorld, mobs: &MobRegistry, book: &BehaviorBook) -> Vec<Actor> {
    let mut out = Vec::new();
    let mut players = world.query_ids::<(&Player, &Transform)>();
    players.extend(world.query_ids::<(&RemotePlayer, &Transform)>());
    for id in players {
        let Some(t) = world.copy::<Transform>(id) else {
            continue;
        };
        let speed = world
            .copy::<Velocity>(id)
            .map_or(0.0, |v| (v.v.x * v.v.x + v.v.z * v.v.z).sqrt());
        let survival = world
            .copy::<Player>(id)
            .map(|p| p.game_mode == GameMode::Survival)
            .or_else(|| world.copy::<RemotePlayer>(id).map(|r| !r.creative))
            .unwrap_or(false);
        out.push(Actor {
            id,
            src: Source {
                pos: t.pos,
                noise: senses::noise_factor(speed, world.has::<Crouching>(id)),
                scent: 1.0,
                height: 1.6,
            },
            alive: world.copy::<Health>(id).is_none_or(|h| h.alive),
            carrying: world.has::<Carrying>(id),
            kind: ActorKind::Player { survival },
        });
    }
    for id in world.query_ids::<(&Brain, &Transform, &MobType)>() {
        let (Some(t), Some(b)) = (world.copy::<Transform>(id), world.copy::<Brain>(id)) else {
            continue;
        };
        let Some(mob) = world.get::<MobType>(id).map(|m| m.mob_id.clone()) else {
            continue;
        };
        let profile = b.profile.map(usize::from).or_else(|| {
            mobs.get(&mob)
                .and_then(|c| c.behavior.as_deref())
                .and_then(|n| book.index_of(n))
        });
        let speed = world
            .copy::<Velocity>(id)
            .map_or(0.0, |v| (v.v.x * v.v.x + v.v.z * v.v.z).sqrt());
        let height = world.copy::<Collider>(id).map_or(1.0, |c| c.height);
        out.push(Actor {
            id,
            src: Source {
                pos: t.pos,
                noise: if b.hidden {
                    0.0
                } else {
                    senses::noise_factor(speed, false).min(1.5)
                },
                scent: if b.hidden { 0.3 } else { 1.0 },
                height: height * 0.6,
            },
            alive: world.copy::<Health>(id).is_none_or(|h| h.alive),
            carrying: world.has::<Carrying>(id),
            kind: ActorKind::Animal {
                profile,
                mob,
                hidden: b.hidden,
            },
        });
    }
    out
}

fn update_needs(
    b: &mut Brain,
    p: &Profile,
    env: Option<&Environment>,
    activity: f32,
    running: bool,
    dt: f32,
) {
    let r = &p.needs;
    let n = &mut b.needs;
    let slow = if b.state == StateKind::Hibernate {
        0.05
    } else {
        1.0
    };
    n.hunger += r.hunger * slow * dt;
    n.thirst += r.thirst * slow * dt;
    if matches!(
        b.state,
        StateKind::Sleep | StateKind::Perch | StateKind::Hibernate
    ) {
        n.fatigue -= 0.04 * dt;
    } else {
        let effort = if running { 2.0 } else { 0.5 + activity };
        n.fatigue += r.fatigue * effort * dt;
    }
    let target = env.map_or(0.0, |e| {
        let coat = if b.winter_coat { r.winter_coat } else { 0.0 };
        let felt = e.air - 0.1 * e.wind.speed * e.chill + coat;
        let mut c = ((r.cold_below - felt) / 0.3).clamp(0.0, 1.0);
        if b.hidden {
            c *= 0.2;
        }
        c
    });
    n.cold += (target - n.cold) * (dt / 10.0).min(1.0);
    n.fear *= (-dt / FEAR_MEMORY).exp();
    if b.provoked > 0.0 {
        n.fear = n.fear.max(0.8);
    }
}

#[allow(clippy::too_many_arguments)]
fn perceive_all(
    b: &mut Brain,
    me: EntityId,
    pi: usize,
    book: &BehaviorBook,
    t: &Transform,
    height: f32,
    r: &Ranges,
    actors: &[Actor],
    emitters: &[(EntityId, Emitter, DVec3)],
    env: Option<&Environment>,
    blocks: &dyn BlockAccess,
    table: &[f32; 256],
    water: u8,
    rng: &mut JavaRandom,
) {
    let p = book.profile(pi);
    let mut out = Percepts {
        water: b.percepts.water,
        ..Default::default()
    };
    let reach = r.sight.max(r.hearing * 2.0).max(r.smell * 2.0) as f64;
    let mut pack: Vec<EntityId> = Vec::new();
    let eye = height * 0.85;
    let closer = |slot: &Option<Sensed>, d: f32| slot.is_none_or(|s| d < s.dist);

    for a in actors {
        if a.id == me || !a.alive {
            continue;
        }
        let d2 = a.src.pos.distance_squared(t.pos);
        let (fears, hunts, kin, hidden) = match &a.kind {
            ActorKind::Player { survival } => (
                p.fears.iter().any(|f| f == PLAYER),
                *survival && p.hunts.iter().any(|h| h == PLAYER),
                false,
                false,
            ),
            ActorKind::Animal {
                profile,
                mob,
                hidden,
            } => {
                let other = profile.map_or("", |q| book.name(q));
                (
                    p.fears(other, mob),
                    p.hunts(other, mob),
                    *profile == Some(pi),
                    *hidden,
                )
            }
        };
        // Congénères : connus sans les sens dans le rayon social.
        if kin && d2 <= SOCIAL_RANGE * SOCIAL_RANGE {
            pack.push(a.id);
            let d = d2.sqrt() as f32;
            let s = Sensed {
                id: Some(a.id),
                pos: a.src.pos,
                dist: d,
                awareness: 1.0,
                sense: Sense::Sight,
            };
            if closer(&out.mate, d) {
                out.mate = Some(s);
            }
            if (p.herd || p.pack) && a.id < me && out.leader.is_none_or(|l| l.id > Some(a.id)) {
                out.leader = Some(s);
            }
        }
        if !(fears || hunts || a.carrying) || d2 > reach * reach {
            continue;
        }
        let Some((aw, sense)) = senses::perceive(r, t.pos, eye, t.yaw, &a.src, env, blocks, table)
        else {
            continue;
        };
        let s = Sensed {
            id: Some(a.id),
            pos: a.src.pos,
            dist: d2.sqrt() as f32,
            awareness: aw,
            sense,
        };
        if fears && aw > out.threat_level {
            out.threat_level = aw;
            out.threat = Some(s);
        }
        if hunts && !hidden && closer(&out.prey, s.dist) {
            out.prey = Some(s);
        }
        if a.carrying && !kin && closer(&out.carrier, s.dist) {
            out.carrier = Some(s);
        }
    }

    // Facteurs posés ou portés.
    for &(eid, e, pos) in emitters {
        if eid == me {
            continue;
        }
        let re = p.reaction(e.kind);
        let d = pos.distance(t.pos) as f32;
        let reach = re.keep_away + e.radius;
        if d > reach {
            continue;
        }
        let s = Sensed {
            id: None,
            pos,
            dist: d,
            awareness: 1.0 - d / reach.max(1e-3),
            sense: Sense::Smell,
        };
        if re.fear > 0.0 {
            let level = if d <= re.keep_away {
                re.fear
            } else {
                re.fear * s.awareness
            };
            if d <= re.keep_away {
                out.repelled = true;
            }
            if level > out.threat_level {
                out.threat_level = level;
                out.threat = Some(s);
            }
        }
        if re.attract > 0.0 {
            match e.kind {
                Stimulus::Carrion | Stimulus::Food => {
                    if closer(&out.carrion, d) {
                        out.carrion = Some(s);
                    }
                }
                _ => {
                    if closer(&out.lure, d) {
                        out.lure = Some(s);
                    }
                }
            }
        }
    }

    // Colère : l'agresseur reste la menace tant qu'elle dure.
    if b.provoked > 0.0 {
        if let Some(a) = b
            .provoker
            .and_then(|pid| actors.iter().find(|a| a.id == pid && a.alive))
        {
            out.threat = Some(Sensed {
                id: Some(a.id),
                pos: a.src.pos,
                dist: a.src.pos.distance(t.pos) as f32,
                awareness: 1.0,
                sense: Sense::Sight,
            });
            out.threat_level = 1.0;
        }
    }

    pack.push(me);
    pack.sort();
    out.pack_size = pack.len().min(255) as u8;
    out.pack_rank = pack.iter().position(|&i| i == me).unwrap_or(0) as u8;

    // Eau : quelques sondes en anneau quand la soif monte.
    let known_far = out.water.is_none_or(|w| w.distance(t.pos) > 24.0);
    if b.needs.thirst > 0.3 && known_far {
        for k in 0..8 {
            let ang = f64::from(rng.next_float()) * std::f64::consts::TAU;
            let rad = 4.0 + 12.0 * f64::from(k) / 8.0;
            let (x, z) = (t.pos.x + ang.cos() * rad, t.pos.z + ang.sin() * rad);
            for dy in [-1, -2, 0] {
                let y = floor_i(t.pos.y) + dy;
                if blocks.block(floor_i(x), y, floor_i(z)) == water {
                    out.water = Some(DVec3::new(x, f64::from(y + 1), z));
                    break;
                }
            }
            if out.water.is_some_and(|w| w.distance(t.pos) <= 24.0) {
                break;
            }
        }
    }
    b.percepts = out;
}

fn adjacent_water(blocks: &dyn BlockAccess, pos: DVec3, water: u8) -> bool {
    let (x, y, z) = (floor_i(pos.x), floor_i(pos.y), floor_i(pos.z));
    [(1, 0), (-1, 0), (0, 1), (0, -1), (0, 0)]
        .iter()
        .any(|&(dx, dz)| (y - 1..=y).any(|yy| blocks.block(x + dx, yy, z + dz) == water))
}

/// Besoin d'un état.
fn need_value(n: Need, b: &Brain, activity: f32) -> f32 {
    match n {
        Need::Hunger => b.needs.hunger,
        Need::Thirst => b.needs.thirst,
        Need::Fear => b.needs.fear,
        Need::Fatigue => b.needs.fatigue,
        Need::Cold => b.needs.cold,
        Need::Rest => b.needs.fatigue.max(1.0 - activity),
        Need::Constant => 1.0,
    }
}

/// Distance à la menace (connue ou dernière position).
fn threat_point(b: &Brain) -> Option<(DVec3, Option<EntityId>, f32)> {
    if let Some(t) = b.percepts.threat {
        return Some((t.pos, t.id, t.dist));
    }
    if b.needs.fear > 0.05 {
        return b.last_threat.map(|p| (p, None, f32::MAX));
    }
    None
}

/// Opportunité (0 à 1) d'un état dans la situation présente.
fn opportunity(s: StateKind, b: &Brain, f: &Frame<'_>) -> f32 {
    use StateKind::*;
    let pc = &b.percepts;
    let calm = 1.0 - b.needs.fear;
    let threat = threat_point(b).filter(|_| b.needs.fear > 0.05);
    let threat_entity = threat.and_then(|(_, id, d)| id.map(|_| d));
    let fly = f.p.locomotion == Locomotion::Fly;
    match s {
        Wander => 1.0,
        Graze => {
            if b.in_water || !f.grounded || fly {
                0.0
            } else {
                calm
            }
        }
        Drink => {
            let w = if f.water_adjacent {
                1.0
            } else if pc.water.is_some() {
                0.7
            } else {
                0.0
            };
            w * calm
        }
        Sleep => calm,
        Flee => match threat {
            Some(_) if pc.repelled => 1.0,
            Some(_) => 1.0 - f.courage,
            None => 0.0,
        },
        Threaten => match threat_entity {
            Some(d) if d > CHARGE_RANGE && d < THREATEN_RANGE && b.provoked <= 0.0 => f.courage,
            _ => 0.0,
        },
        Charge => match threat_entity {
            Some(d) if d <= CHARGE_RANGE || (b.provoked > 0.0 && d < THREATEN_RANGE * 2.0) => {
                f.courage
            }
            _ => 0.0,
        },
        Stalk => pc
            .prey
            .map_or(0.0, |p| if p.dist > 10.0 { 1.0 } else { 0.0 }),
        Pursue => pc
            .prey
            .map_or(0.0, |p| if p.dist <= 10.0 { 1.0 } else { 0.5 }),
        // Une meute n'attaque qu'une fois la proie encerclée.
        Attack if f.p.pack && pc.pack_size >= 2 && b.encircle_time < ENCIRCLE_TIME => 0.0,
        Attack => pc.prey.map_or(0.0, |p| {
            if p.dist <= f.cfg.attack_range + 1.5 {
                1.0
            } else {
                0.0
            }
        }),
        Encircle => {
            if f.p.pack && pc.pack_size >= 2 && pc.prey.is_some() && b.encircle_time < ENCIRCLE_TIME
            {
                1.0
            } else {
                0.0
            }
        }
        Guard => {
            if f.clock.within(f.p.seasons.birth) {
                1.0
            } else {
                0.0
            }
        }
        Follow => {
            if pc.lure.is_some() {
                1.0
            } else {
                pc.leader.map_or(0.0, |l| {
                    if l.dist > 5.0 {
                        ((l.dist - 5.0) / 10.0 + 0.3).min(1.0)
                    } else {
                        0.0
                    }
                })
            }
        }
        Migrate => {
            // Les déplacements saisonniers se font aux heures d'activité.
            if f.migration.is_some() && threat.is_none() {
                f.activity
            } else {
                0.0
            }
        }
        Hibernate => {
            if f.clock.within(f.p.seasons.hibernate) {
                1.0
            } else {
                0.0
            }
        }
        Display => {
            if f.clock.within(f.p.seasons.rut) {
                f.activity * if pc.mate.is_some() { 1.0 } else { 0.5 }
            } else {
                0.0
            }
        }
        Fly => {
            if fly {
                1.0 - b.needs.fatigue
            } else {
                0.0
            }
        }
        Perch => {
            if fly {
                1.0
            } else {
                0.0
            }
        }
        Dive => {
            if fly && pc.prey.is_some() {
                1.0
            } else {
                0.0
            }
        }
        Swim => {
            if b.in_water || f.cfg.medium == MobMedium::Water {
                1.0
            } else {
                0.0
            }
        }
        Burrow => match threat {
            Some((_, _, d)) if d > 8.0 => 1.0,
            Some(_) => 0.2,
            None => 0.0,
        },
        Scavenge => {
            if pc.carrion.is_some() {
                1.0
            } else {
                0.0
            }
        }
        Steal => {
            if pc.carrier.is_some() {
                1.0
            } else {
                0.0
            }
        }
        Build => {
            if f.clock.within(f.p.seasons.build) && b.needs.hunger < 0.6 {
                f.activity
            } else {
                0.0
            }
        }
    }
}

/// Utilité de chaque état, puis changement avec hystérésis.
fn choose(b: &mut Brain, f: &Frame<'_>, dt: f32) {
    let mut scores = [0.0f32; StateKind::COUNT];
    for d in &f.p.states {
        let s = d.weight * need_value(d.need(), b, f.activity) * opportunity(d.state, b, f);
        let slot = &mut scores[d.state as usize];
        *slot = slot.max(s);
    }
    b.scores = scores;
    let mut best = b.state;
    let mut best_score = f32::MIN;
    for d in &f.p.states {
        let s = scores[d.state as usize];
        if s > best_score {
            best = d.state;
            best_score = s;
        }
    }
    let current = scores[b.state as usize];
    let may_switch =
        b.since >= HYSTERESIS || current <= 0.0 || best_score > current + OVERRIDE_MARGIN;
    if best != b.state && best_score > current && may_switch {
        b.state = best;
        b.since = 0.0;
        b.moving = false;
        b.move_timer = 0.0;
        b.act_timer = 0.0;
    } else {
        b.since += dt;
    }
    if b.state != StateKind::Encircle && b.percepts.prey.is_none() {
        b.encircle_time = 0.0;
    }
    b.hidden = matches!(b.state, StateKind::Burrow | StateKind::Hibernate);
}

// ---------------------------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------------------------

#[inline]
fn yaw_from_heading(h: f32) -> f32 {
    (std::f64::consts::FRAC_PI_2 - f64::from(h)) as f32
}

fn stop(v: &mut Velocity) {
    v.target = Default::default();
}

fn face(t: &mut Transform, to: DVec3) {
    let d = to - t.pos;
    if d.x * d.x + d.z * d.z > 1e-6 {
        t.yaw = yaw_from_heading(d.z.atan2(d.x) as f32);
    }
}

/// Consigne vers `dir` à `speed` : à plat pour un marcheur, en 3D pour un nageur ou un oiseau.
fn steer(v: &mut Velocity, t: &mut Transform, medium: MobMedium, dir: DVec3, speed: f32) {
    let hd = (dir.x * dir.x + dir.z * dir.z).sqrt();
    if hd < 1e-6 && dir.y.abs() < 1e-6 {
        stop(v);
        return;
    }
    let h = dir.z.atan2(dir.x) as f32;
    t.yaw = yaw_from_heading(h);
    if medium == MobMedium::Land {
        v.target.x = (f64::from(h).cos() as f32) * speed;
        v.target.z = (f64::from(h).sin() as f32) * speed;
        v.target.y = 0.0;
        t.pitch = 0.0;
    } else {
        let len = dir.length();
        v.target.x = (dir.x / len) as f32 * speed;
        v.target.y = (dir.y / len) as f32 * speed;
        v.target.z = (dir.z / len) as f32 * speed;
        t.pitch = -(dir.y.atan2(hd) as f32);
    }
}

/// Le terrain devant est-il praticable pour ce milieu ?
fn passable(
    blocks: &dyn BlockAccess,
    pos: DVec3,
    h: f64,
    medium: MobMedium,
    g: Ground<'_>,
) -> bool {
    let (x, z) = (
        floor_i(pos.x + h.cos() * 1.5),
        floor_i(pos.z + h.sin() * 1.5),
    );
    let y = floor_i(pos.y);
    let ahead = blocks.block(x, y, z);
    match medium {
        // Un amphibie sorti de l'eau peut gagner la rive.
        MobMedium::Water if g.shore => ahead == g.water || !g.reg.from_id(ahead).solid,
        MobMedium::Water => ahead == g.water,
        MobMedium::Land => {
            ahead != g.water
                && blocks.block(x, y - 1, z) != g.water
                // La banquise : le pas suivant doit rester sur la glace.
                && (!g.ice_only || g.reg.is_ice(blocks.block(x, y - 1, z)))
        }
        _ => true,
    }
}

/// Milieu effectif de déplacement, ramené à terre, eau ou air.
fn effective(medium: MobMedium, in_water: bool) -> MobMedium {
    match medium.gait(in_water) {
        Gait::Walk => MobMedium::Land,
        Gait::Swim => MobMedium::Water,
        Gait::Fly => MobMedium::Air,
    }
}

/// Ce que l'IA sait du sol : eau, registre, contraintes de milieu (glace, rive).
#[derive(Clone, Copy)]
struct Ground<'a> {
    water: u8,
    reg: &'a BlockRegistry,
    /// Banquise : ne quitte pas la glace.
    ice_only: bool,
    /// Amphibie : l'eau n'est pas une impasse.
    shore: bool,
}

impl<'a> Ground<'a> {
    fn of(reg: &'a BlockRegistry, cfg: &MobConfig) -> Self {
        Self {
            water: reg.ids().water,
            reg,
            ice_only: cfg.medium == MobMedium::Ice,
            shore: cfg.medium == MobMedium::Amphibian,
        }
    }
}

/// Tourne le cap `h` jusqu'à un terrain praticable (8 essais de 45°).
fn find_heading(
    blocks: &dyn BlockAccess,
    pos: DVec3,
    h: f32,
    medium: MobMedium,
    ground: Ground<'_>,
    in_water: bool,
) -> Option<f32> {
    if in_water && medium == MobMedium::Land && !ground.ice_only {
        return Some(h);
    }
    for k in 0..8 {
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        let hh = h + sign * ((k + 1) / 2) as f32 * std::f32::consts::FRAC_PI_4;
        if passable(blocks, pos, f64::from(hh), medium, ground) {
            return Some(hh);
        }
    }
    None
}

/// Errance : marche et pauses, ramenée vers le gîte au-delà de `leash`.
#[allow(clippy::too_many_arguments)]
fn wander(
    b: &mut Brain,
    t: &mut Transform,
    v: &mut Velocity,
    f: &Frame<'_>,
    leash: f64,
    blocks: &dyn BlockAccess,
    ground: Ground<'_>,
    rng: &mut JavaRandom,
    dt: f32,
) {
    b.move_timer -= dt;
    if b.move_timer > 0.0 {
        return;
    }
    let c = f.cfg;
    if b.moving {
        b.moving = false;
        stop(v);
        b.move_timer = c.pause_duration_min
            + rng.next_float() * (c.pause_duration_max - c.pause_duration_min).max(0.0);
        return;
    }
    let to_home = b.home - t.pos;
    let mut h = rng.next_float() * std::f32::consts::TAU;
    if (to_home.x * to_home.x + to_home.z * to_home.z).sqrt() > leash {
        h = to_home.z.atan2(to_home.x) as f32 + (rng.next_float() - 0.5) * 1.2;
    }
    let medium = effective(c.medium, b.in_water);
    let Some(h) = find_heading(blocks, t.pos, h, medium, ground, b.in_water) else {
        b.move_timer = 1.0;
        return;
    };
    let speed =
        c.move_speed_min + rng.next_float() * (c.move_speed_max - c.move_speed_min).max(0.0);
    let mut dir = DVec3::new(f64::from(h).cos(), 0.0, f64::from(h).sin());
    if medium != MobMedium::Land {
        dir.y = f64::from((rng.next_float() - 0.5) * 0.6);
    }
    if f.p.locomotion == Locomotion::Fly {
        let cruise = b.home.y + f64::from(f.p.cruise);
        dir.y = ((cruise - t.pos.y) / 8.0).clamp(-0.6, 0.6);
    }
    steer(v, t, medium, dir, speed);
    b.moving = true;
    b.move_timer = c.move_duration_min
        + rng.next_float() * (c.move_duration_max - c.move_duration_min).max(0.0);
}

/// Va vers `to` ; renvoie la distance horizontale restante.
fn go_to(
    t: &mut Transform,
    v: &mut Velocity,
    f: &Frame<'_>,
    to: DVec3,
    speed: f32,
    arrive: f64,
) -> f64 {
    let mut d = to - t.pos;
    let hd = (d.x * d.x + d.z * d.z).sqrt();
    let medium = effective(f.cfg.medium, f.in_water);
    if hd <= arrive && (medium == MobMedium::Land || d.y.abs() <= arrive) {
        stop(v);
        face(t, to);
        return hd;
    }
    if medium == MobMedium::Land {
        d.y = 0.0;
    }
    steer(v, t, medium, d, speed);
    hd
}

#[allow(clippy::too_many_arguments)]
fn act(
    b: &mut Brain,
    t: &mut Transform,
    v: &mut Velocity,
    f: &Frame<'_>,
    actors: &[Actor],
    blocks: &dyn BlockAccess,
    ground: Ground<'_>,
    rng: &mut JavaRandom,
    effects: &mut Vec<Effect>,
    events: &mut Vec<EcsEvent>,
    dt: f32,
) {
    use StateKind::*;
    let sp = Speeds::of(f.cfg);
    let medium = effective(f.cfg.medium, b.in_water);
    let here = IVec3::new(floor_i(t.pos.x), floor_i(t.pos.y), floor_i(t.pos.z));
    let mut pulse = |b: &mut Brain, period: f32, act: AnimalAct, pos: IVec3| {
        b.act_timer -= dt;
        if b.act_timer <= 0.0 {
            b.act_timer = period;
            events.push(EcsEvent::AnimalAct {
                entity: f.id,
                act,
                pos,
            });
        }
    };
    let strike = |b: &mut Brain,
                  target: Option<EntityId>,
                  dist: f32,
                  hunting: bool,
                  effects: &mut Vec<Effect>| {
        if let Some(id) = target {
            if dist <= f.cfg.attack_range && b.attack_cooldown <= 0.0 {
                b.attack_cooldown = f.cfg.attack_cooldown.max(0.2);
                effects.push(Effect::Hit {
                    target: id,
                    hunting,
                });
            }
        }
    };
    match b.state {
        Wander => wander(b, t, v, f, HOME_LEASH, blocks, ground, rng, dt),
        Guard => {
            if let Some((p, _, _)) = threat_point(b) {
                stop(v);
                face(t, p);
            } else {
                wander(b, t, v, f, GUARD_LEASH, blocks, ground, rng, dt);
            }
        }
        Fly | Swim => {
            if b.in_water && medium == MobMedium::Land && !ground.ice_only {
                // Un marcheur tombé à l'eau rejoint la rive vers son gîte.
                let to = b.home;
                go_to(t, v, f, to, sp.walk, 0.5);
                v.target.y = 2.0;
            } else {
                wander(b, t, v, f, HOME_LEASH, blocks, ground, rng, dt);
            }
        }
        Graze => {
            stop(v);
            b.needs.hunger = (b.needs.hunger - 0.08 * dt).max(0.0);
            pulse(b, 5.0, AnimalAct::Graze, here - IVec3::Y);
        }
        Drink => {
            if f.water_adjacent {
                stop(v);
                b.needs.thirst = (b.needs.thirst - 0.2 * dt).max(0.0);
            } else if let Some(w) = b.percepts.water {
                if go_to(t, v, f, w, sp.walk, 1.0) <= 1.0 {
                    // Arrivé mais pas au bord : la sonde était trompeuse.
                    b.percepts.water = None;
                }
            }
        }
        Sleep | Perch | Burrow | Display | Threaten => {
            stop(v);
            match b.state {
                Threaten => {
                    if let Some((p, _, _)) = threat_point(b) {
                        face(t, p);
                    }
                }
                Display => {
                    if let Some(m) = b.percepts.mate {
                        face(t, m.pos);
                    }
                }
                Perch if f.p.locomotion == Locomotion::Fly => v.target.y = -1.0,
                _ => {}
            }
        }
        Flee => {
            if let Some((p, _, _)) = threat_point(b) {
                let away = t.pos - p;
                let h = away.z.atan2(away.x) as f32;
                let h = find_heading(blocks, t.pos, h, medium, ground, b.in_water).unwrap_or(h);
                let mut dir = DVec3::new(f64::from(h).cos(), 0.0, f64::from(h).sin());
                if f.p.locomotion == Locomotion::Fly {
                    dir.y = 0.5;
                }
                steer(v, t, medium, dir, sp.run);
            } else {
                stop(v);
            }
        }
        Charge => {
            if let Some((p, id, d)) = threat_point(b) {
                go_to(t, v, f, p, sp.run, 0.0);
                strike(b, id, d, false, effects);
            }
        }
        Stalk => {
            if let Some(p) = b.percepts.prey {
                go_to(t, v, f, p.pos, sp.stalk, 1.0);
            }
        }
        Pursue | Attack | Dive => {
            if let Some(p) = b.percepts.prey {
                let pos = current_pos(actors, p);
                go_to(t, v, f, pos, sp.run, 0.0);
                let d = pos.distance(t.pos) as f32;
                strike(b, p.id, d, true, effects);
            }
        }
        Encircle => {
            b.encircle_time += dt;
            if let Some(p) = b.percepts.prey {
                let n = f64::from(b.percepts.pack_size.max(1));
                let k = f64::from(b.percepts.pack_rank);
                let a = std::f64::consts::TAU * k / n;
                let prey = current_pos(actors, p);
                let slot = prey + DVec3::new(a.cos(), 0.0, a.sin()) * ENCIRCLE_RADIUS;
                if go_to(t, v, f, slot, sp.run, 0.8) <= 0.8 {
                    face(t, prey);
                }
            }
        }
        Follow => {
            let target = b.percepts.lure.or(b.percepts.leader);
            if let Some(l) = target {
                go_to(t, v, f, current_pos(actors, l), sp.walk, 4.0);
            }
        }
        Migrate => {
            let dir = match (b.goal, f.migration) {
                (Some(g), _) => g - t.pos,
                (None, Some(k)) => {
                    let s = if k % 2 == 0 { 1.0 } else { -1.0 };
                    DVec3::new(
                        f64::from(f.p.heading[0]) * s,
                        0.0,
                        f64::from(f.p.heading[1]) * s,
                    )
                }
                (None, None) => DVec3::ZERO,
            };
            let h = dir.z.atan2(dir.x) as f32;
            match find_heading(blocks, t.pos, h, medium, ground, b.in_water) {
                Some(h) => steer(
                    v,
                    t,
                    medium,
                    DVec3::new(f64::from(h).cos(), 0.0, f64::from(h).sin()),
                    sp.walk,
                ),
                None => stop(v),
            }
        }
        Hibernate => {
            go_to(t, v, f, b.home, sp.walk, 1.5);
        }
        Scavenge => {
            if let Some(c) = b.percepts.carrion {
                if go_to(t, v, f, c.pos, sp.walk, 1.5) <= 1.5 {
                    b.needs.hunger = (b.needs.hunger - 0.1 * dt).max(0.0);
                }
            }
        }
        Steal => {
            if let Some(c) = b.percepts.carrier {
                let pos = current_pos(actors, c);
                if go_to(t, v, f, pos, sp.run, 1.5) <= 1.5 {
                    if let Some(id) = c.id {
                        effects.push(Effect::Steal { from: id });
                        b.percepts.carrier = None;
                    }
                }
            }
        }
        Build => {
            if go_to(t, v, f, b.home, sp.walk, 1.5) <= 1.5 {
                let home = IVec3::new(floor_i(b.home.x), floor_i(b.home.y), floor_i(b.home.z));
                pulse(b, 8.0, AnimalAct::Build, home);
            }
        }
    }
}

/// Position actuelle d'une entité perçue (la perception a jusqu'à 0,1 s de retard).
fn current_pos(actors: &[Actor], s: Sensed) -> DVec3 {
    s.id.and_then(|id| actors.iter().find(|a| a.id == id))
        .map_or(s.pos, |a| a.src.pos)
}

/// Applique un coup ou un vol.
fn apply(
    world: &mut EntityWorld,
    me: EntityId,
    cfg: &MobConfig,
    e: Effect,
    events: &mut Vec<EcsEvent>,
) {
    match e {
        Effect::Hit { target, hunting } => {
            let (Some(st), Some(tt)) =
                (world.copy::<Transform>(me), world.copy::<Transform>(target))
            else {
                return;
            };
            let reduction = world
                .get::<crate::survival::Vitals>(target)
                .map_or(0.0, |v| v.reduction);
            let mut killed = false;
            if let Some(h) = world.get_mut::<Health>(target) {
                if !h.alive {
                    return;
                }
                h.health -= cfg.attack_damage * (1.0 - reduction);
                if h.health <= 0.0 {
                    h.alive = false;
                    killed = true;
                }
            }
            if let Some(v) = world.get_mut::<crate::survival::Vitals>(target) {
                v.armor_hits = v.armor_hits.saturating_add(1);
            }
            let d = tt.pos - st.pos;
            let dist = (d.x * d.x + d.z * d.z).sqrt();
            if dist > 0.01 {
                if let Some(tv) = world.get_mut::<Velocity>(target) {
                    let inv = (1.0 / dist) as f32;
                    tv.v.x += d.x as f32 * inv * cfg.knockback_force;
                    tv.v.z += d.z as f32 * inv * cfg.knockback_force;
                    tv.v.y = 5.0;
                }
            }
            provoke(world, target, Some(me));
            if killed && hunting {
                if let Some(b) = world.get_mut::<Brain>(me) {
                    b.needs.hunger = (b.needs.hunger - 0.7).max(0.0);
                }
                let pos = IVec3::new(floor_i(tt.pos.x), floor_i(tt.pos.y), floor_i(tt.pos.z));
                events.push(EcsEvent::AnimalAct {
                    entity: me,
                    act: AnimalAct::Kill,
                    pos,
                });
            }
        }
        Effect::Steal { from } => {
            let Some(food) = world.remove::<Carrying>(from) else {
                return;
            };
            let thief = world.copy::<Transform>(me).map(|t| t.pos);
            if let Some(b) = world.get_mut::<Brain>(from) {
                b.needs.fear = 1.0;
                b.last_threat = thief;
            }
            if let Some(b) = world.get_mut::<Brain>(me) {
                b.needs.hunger = (b.needs.hunger - food.food).max(0.0);
            }
            world.insert(me, food);
            let pos = thief.map_or(IVec3::ZERO, |p| {
                IVec3::new(floor_i(p.x), floor_i(p.y), floor_i(p.z))
            });
            events.push(EcsEvent::AnimalAct {
                entity: me,
                act: AnimalAct::Steal,
                pos,
            });
        }
    }
}
