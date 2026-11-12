//! Élevage et compagnons côté ECS (doc 03 §3.6, session 5.7).
//!
//! Un animal dont le `MobConfig` porte un bloc `husbandry` devient **domestique** quand un joueur
//! l'apprivoise ([`tame`]) : il reçoit le composant [`Domestic`]. Le système [`HusbandrySystem`]
//! (1 Hz) fait alors vivre :
//!
//! * le **fourrage** : l'herbe de la saison de pâture remplit la réserve, le reste de l'année elle
//!   se vide ; à sec l'animal **dépérit** (perd des points de vie) ; [`feed`] rend une ration ;
//! * la **production** (lait, laine, œufs) : elle s'accumule bien nourri et en saison, plus
//!   généreuse à l'estive ; [`collect`] la récolte ;
//! * le **suivi du maître** (chien, chat, cheval de bât) par `Brain::goal` ;
//! * le **chien de chasse** : il marque l'arrêt devant le gibier et aboie pour le tenir, aboie aussi
//!   sur les prédateurs de sa liste d'alerte ([`AnimalAct::Bark`]).

use nb_core::glam::DVec3;
use nb_data::husbandry::{Husbandry, Role};
use nb_data::MobRegistry;

use crate::brain::{face, stop, AnimalAct, Brain};
use crate::components::{EntityId, Health, MobType, Transform, Velocity};
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;

/// Secondes entre deux passes.
pub const INTERVAL: f32 = 1.0;
/// Réserve de fourrage d'un animal fraîchement apprivoisé.
pub const START_FODDER: f32 = 0.6;
/// Points de vie perdus par seconde à sec.
pub const STARVE_RATE: f32 = 0.1;
/// Part de la réserve rendue par jour de pâture.
pub const PASTURE_GAIN: f32 = 5.0;
/// Réserve en dessous de laquelle la production s'arrête.
pub const PRODUCE_MIN_FODDER: f32 = 0.25;
/// Secondes entre deux aboiements.
pub const BARK_PERIOD: f32 = 4.0;

/// Animal domestique (apprivoisé).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Domestic {
    pub owner: Option<EntityId>,
    /// Réserve de fourrage (0 : à sec, 1 : pleine).
    pub fodder: f32,
    /// Production prête à être récoltée, en unités.
    pub stock: f32,
    /// Gibier devant lequel le chien est à l'arrêt.
    pub pointing: Option<EntityId>,
    bark_timer: f32,
    /// Une destination du suivi est posée sur `Brain::goal`.
    following: bool,
    /// Le bétail est à une fäbod (une baratte `karna` à proximité) : l'estive double le lait.
    pub at_fabod: bool,
    fabod_age: f32,
}

impl Domestic {
    fn new(owner: Option<EntityId>) -> Self {
        Self {
            owner,
            fodder: START_FODDER,
            stock: 0.0,
            pointing: None,
            bark_timer: 0.0,
            following: false,
            at_fabod: false,
            fabod_age: 99.0,
        }
    }
}

fn sheet<'a>(world: &EntityWorld, mobs: &'a MobRegistry, id: EntityId) -> Option<&'a Husbandry> {
    let mob = world.get::<MobType>(id)?;
    mobs.get(&mob.mob_id)?.husbandry.as_ref()
}

/// Apprivoise `animal` avec `item` (friandise) : vrai si l'objet convient et que l'animal n'est
/// pas déjà domestique.
pub fn tame(
    world: &mut EntityWorld,
    mobs: &MobRegistry,
    animal: EntityId,
    owner: EntityId,
    item: &str,
) -> bool {
    if world.has::<Domestic>(animal) {
        return false;
    }
    let ok = sheet(world, mobs, animal).is_some_and(|h| h.tames_with(item));
    if ok {
        world.insert(animal, Domestic::new(Some(owner)));
    }
    ok
}

/// Donne `item` à un animal domestique : vrai si c'est son fourrage (la réserve monte d'une ration).
pub fn feed(world: &mut EntityWorld, mobs: &MobRegistry, animal: EntityId, item: &str) -> bool {
    let ration = sheet(world, mobs, animal)
        .and_then(|h| h.fodder.as_ref())
        .filter(|f| f.item == item)
        .map(|f| f.ration);
    match (ration, world.get_mut::<Domestic>(animal)) {
        (Some(r), Some(d)) => {
            d.fodder = (d.fodder + r).min(1.0);
            true
        }
        _ => false,
    }
}

/// Récolte la production prête au mois `month` (0 = janvier) : `(objet, quantité)`. Une production saisonnière (tonte) n'est
/// prête que lorsque le stock est plein.
pub fn collect(
    world: &mut EntityWorld,
    mobs: &MobRegistry,
    animal: EntityId,
    month: usize,
) -> Option<(String, u32)> {
    let p = sheet(world, mobs, animal)?.produce.clone()?;
    if p.harvest_months.is_some_and(|w| !nb_data::behavior::in_window(month, w)) {
        return None;
    }
    let d = world.get_mut::<Domestic>(animal)?;
    let ready = if p.seasonal {
        d.stock + 1e-3 >= p.capacity
    } else {
        d.stock >= 1.0
    };
    if !ready {
        return None;
    }
    let n = d.stock.floor() as u32;
    d.stock -= n as f32;
    Some((p.item, n))
}

/// Le rendement du jour : bien nourri, en saison, plus généreux à l'estive.
fn yield_per_day(
    p: &nb_data::husbandry::Produce,
    month: usize,
    fodder: f32,
    summer_farm: bool,
) -> f32 {
    use nb_data::behavior::in_window;
    if fodder < PRODUCE_MIN_FODDER || p.months.is_some_and(|w| !in_window(month, w)) {
        return 0.0;
    }
    let bonus = if summer_farm && p.bonus_months.is_some_and(|w| in_window(month, w)) {
        p.summer_bonus
    } else {
        1.0
    };
    p.per_day * bonus * fodder.min(1.0)
}

#[derive(Debug, Default)]
pub struct HusbandrySystem {
    timer: f32,
}

impl System for HusbandrySystem {
    fn name(&self) -> &'static str {
        "Husbandry"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        for id in world.query_ids::<(&Domestic, &MobType)>() {
            hound(world, ctx.mobs, ctx.events, id, dt);
        }
        self.timer += dt;
        if self.timer < INTERVAL {
            return;
        }
        let step = self.timer;
        self.timer = 0.0;
        let days = step / ctx.config.day_length_seconds.max(1.0);
        let month = crate::brain::clock::Clock::of(ctx.config, None).month;
        let fabod = crate::farming::block_id(ctx.block_reg, "karna");
        let fabod_known = fabod.is_some();

        for id in world.query_ids::<(&Domestic, &MobType)>() {
            let Some(cfg) = world
                .get::<MobType>(id)
                .and_then(|m| ctx.mobs.get(&m.mob_id))
            else {
                continue;
            };
            let Some(h) = cfg.husbandry.as_ref() else {
                continue;
            };
            let mut d = *world.get::<Domestic>(id).expect("requête");

            // Fourrage et production.
            if let Some(f) = &h.fodder {
                if nb_data::behavior::in_window(month, f.pasture) {
                    d.fodder = (d.fodder + PASTURE_GAIN * f.per_day * days).min(1.0);
                } else {
                    d.fodder = (d.fodder - f.per_day * days).max(0.0);
                }
                if d.fodder <= 0.0 {
                    if let Some(hp) = world.get_mut::<Health>(id) {
                        if hp.alive {
                            hp.health -= STARVE_RATE * step;
                            hp.alive = hp.health > 0.0;
                        }
                    }
                }
            }
            if let Some(p) = &h.produce {
                d.stock = (d.stock + yield_per_day(p, month, d.fodder, d.at_fabod || !fabod_known) * days).min(p.capacity);
            }

            if let (Some(b), true) = (fabod, h.produce.as_ref().is_some_and(|p| p.bonus_months.is_some())) {
                d.fabod_age += step;
                if d.fabod_age >= FABOD_RECHECK {
                    d.fabod_age = 0.0;
                    if let Some(t) = world.copy::<Transform>(id) {
                        d.at_fabod = near_block(ctx.blocks, b, t.pos);
                    }
                }
            }
            follow(world, id, h, &mut d);
            // Le minuteur d'aboiement appartient à la passe de chaque tick : ne pas l'écraser.
            if let Some(cur) = world.get_mut::<Domestic>(id) {
                cur.fodder = d.fodder;
                cur.stock = d.stock;
                cur.following = d.following;
                cur.at_fabod = d.at_fabod;
                cur.fabod_age = d.fabod_age;
            }
        }
    }
}

/// Suivi du maître (1 Hz).
fn follow(world: &mut EntityWorld, id: EntityId, h: &Husbandry, d: &mut Domestic) {
    let Some(here) = world.copy::<Transform>(id).map(|t| t.pos) else {
        return;
    };
    // Suivi : un compagnon, un cheval de bât ou un chat revient vers son maître.
    if matches!(h.role, Role::Hound | Role::Ratter | Role::Mount) {
        let owner = d
            .owner
            .and_then(|o| world.copy::<Transform>(o))
            .map(|t| t.pos);
        let far = owner.map(|o| (o - here).length() as f32);
        if let (Some(o), Some(dist)) = (owner, far) {
            if dist > h.follow * 1.5 {
                d.following = true;
                if let Some(b) = world.get_mut::<Brain>(id) {
                    b.goal = Some(o);
                }
            } else if dist <= h.follow && d.following {
                d.following = false;
                if let Some(b) = world.get_mut::<Brain>(id) {
                    b.goal = None;
                }
            } else if d.following {
                if let Some(b) = world.get_mut::<Brain>(id) {
                    b.goal = Some(o);
                }
            }
        }
    }

}

/// Arrêt et alerte du chien, à chaque tick (la consigne de l'IA est reposée à chaque tick).
fn hound(
    world: &mut EntityWorld,
    mobs: &MobRegistry,
    events: &mut Vec<EcsEvent>,
    id: EntityId,
    dt: f32,
) {
    let Some(mut d) = world.copy::<Domestic>(id) else {
        return;
    };
    let Some(h) = world
        .get::<MobType>(id)
        .and_then(|m| mobs.get(&m.mob_id))
        .and_then(|c| c.husbandry.as_ref())
    else {
        return;
    };
    let Some(hound) = h.hound.as_ref() else {
        return;
    };
    let (Some(here), Some(brain)) = (
        world.copy::<Transform>(id).map(|t| t.pos),
        world.copy::<Brain>(id),
    ) else {
        return;
    };
    d.bark_timer = (d.bark_timer - dt).max(0.0);
    let cell =|p: DVec3| {
        nb_core::glam::IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
    };

    // Alerte : un prédateur de la liste est senti.
    let alert = brain.percepts.threat.filter(|t| t.dist <= hound.alert_range).and_then(|t| {
        let kind = t.id.and_then(|e| world.get::<MobType>(e).map(|m| m.mob_id.clone()))?;
        let profile = mobs.get(&kind).and_then(|c| c.behavior.clone());
        let listed = hound
            .alerts
            .iter()
            .any(|a| *a == kind || Some(a) == profile.as_ref());
        listed.then_some(t)
    });
    if let Some(t) = alert {
        if d.bark_timer <= 0.0 {
            d.bark_timer = BARK_PERIOD;
            events.push(EcsEvent::AnimalAct {
                entity: id,
                act: AnimalAct::Bark,
                pos: cell(here),
            });
        }
        if let Some(tr) = world.get_mut::<Transform>(id) {
            face(tr, t.pos);
        }
        world.insert(id, d);
        return;
    }

    // Arrêt : gibier senti à portée, le chien se fige face à lui et aboie pour le tenir.
    d.pointing = None;
    if let Some(prey) = brain.percepts.prey {
        if prey.dist <= hound.point_range {
            d.pointing = prey.id;
            if let Some(v) = world.get_mut::<Velocity>(id) {
                stop(v);
            }
            if let Some(tr) = world.get_mut::<Transform>(id) {
                face(tr, prey.pos);
            }
            if d.bark_timer <= 0.0 {
                d.bark_timer = BARK_PERIOD;
                events.push(EcsEvent::AnimalAct {
                    entity: id,
                    act: AnimalAct::Bark,
                    pos: cell(here),
                });
            }
        }
    }
    world.insert(id, d);
}

impl Domestic {
    /// Pose la production prête (outils et scénarios).
    pub fn set_stock(&mut self, stock: f32) {
        self.stock = stock.max(0.0);
    }
}

/// Rayon (blocs) autour d'une baratte `karna` où le bétail est à l'estive.
pub const FABOD_RANGE: i32 = 16;
/// Secondes entre deux sondages de l'estive.
pub const FABOD_RECHECK: f32 = 10.0;

/// Un bloc `target` se trouve-t-il dans [`FABOD_RANGE`] autour de `pos` ?
fn near_block(blocks: &dyn crate::blocks::BlockAccess, target: nb_data::BlockId, pos: DVec3) -> bool {
    let (cx, cz) = (pos.x.floor() as i32, pos.z.floor() as i32);
    let mut col = [0u8; 128];
    for dx in -FABOD_RANGE..=FABOD_RANGE {
        for dz in -FABOD_RANGE..=FABOD_RANGE {
            if blocks.is_chunk_loaded(cx + dx, cz + dz) {
                blocks.read_column(cx + dx, cz + dz, &mut col);
                if col.contains(&target) {
                    return true;
                }
            }
        }
    }
    false
}
