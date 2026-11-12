//! Interactions de la faune avec le monde, chasse et piégeage (doc 03 §4.5 et §5, session 5.8).
//!
//! [`EcologySystem`] lit le bloc `ecology` des fiches d'espèces (`nb_data::ecology`) et fait :
//!
//! * le **broutage** : un élan ou un chevreuil à l'état « brouter » mange les pousses voisines,
//!   sauf à moins de `fence_radius` d'une clôture ;
//! * le **castor** : à l'état « construire » près de sa hutte, il abat un arbre voisin toutes les
//!   `every_secs`, jusqu'au plafond de la colonie ; [`dam_broken`] vide l'étang en amont quand on
//!   casse la digue ;
//! * les **traces** : les animaux marchant sur la neige laissent des empreintes (et des crottes,
//!   des arbres écorcés), qui s'effacent ; [`EcologyState::read_sign`] donne l'espèce et la
//!   direction ;
//! * le **piégeage** : collets, nasses, filets et fosses ([`EcologyState::place_trap`]) prennent
//!   les animaux à portée selon la fiche de l'espèce ;
//! * la **carcasse** : un animal mort laisse une [`Carcass`] (charogne pour les charognards) que
//!   [`skin`] dépèce, selon l'outil ;
//! * le **venin** ([`envenom`], [`Poisoned`]) ;
//! * la **naturfrid** ([`Standing`]) : tuer plus qu'on ne consomme, une femelle suitée ou un
//!   animal sacré abaisse la réputation, donc les densités ([`nature_factor`]).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use nb_core::glam::{DVec3, IVec3};
use nb_core::JavaRandom;
use nb_data::behavior::{in_window, StateKind, Stimulus};
use nb_data::ecology::{Fell, Tool, TrapKind, Venom};
use nb_data::{BlockId, BlockRegistry, MobConfig, MobRegistry};

use crate::blocks::BlockAccess;
use crate::brain::{Brain, Emitter};
use crate::components::{EntityId, Health, MobType, Player, Transform, Velocity};
use crate::jmath::floor_i;
use crate::schedule::{System, TickCtx};
use crate::world::EntityWorld;

/// Secondes entre deux passes lentes.
pub const INTERVAL: f32 = 1.0;
/// Empreintes conservées au plus.
pub const SIGN_CAP: usize = 4096;
/// Chance par seconde qu'un brouteur mange une pousse à portée.
pub const BROWSE_CHANCE: f32 = 0.25;
/// Rayon de la bouche d'un brouteur.
pub const BROWSE_REACH: i32 = 1;
/// Distance de la hutte à laquelle le castor travaille.
pub const LODGE_REACH: f64 = 4.0;
/// Troncs d'un arbre abattu, au plus.
pub const FELL_MAX_LOGS: usize = 64;
/// Cellules d'eau parcourues au plus quand une digue cède.
pub const DRAIN_MAX: usize = 8192;
/// Distance horizontale maximale d'une prise.
pub const SNARE_REACH: f64 = 1.6;
pub const PIT_REACH: f64 = 1.0;
pub const FISH_TRAP_REACH: f64 = 2.5;
pub const NET_REACH: f64 = 3.5;
/// Portée de l'odeur d'une carcasse.
pub const CARRION_RADIUS: f32 = 24.0;
/// Gibier d'une même espèce toléré par jour de jeu avant le « tuer plus qu'on ne consomme ».
pub const DAILY_ALLOWANCE: usize = 3;
/// Pénalités de réputation (par point de `worth`).
pub const WASTE: f32 = 0.04;
pub const OVERKILL: f32 = 0.03;
pub const WITH_YOUNG: f32 = 0.12;
pub const SACRED: f32 = 0.4;
/// Réputation regagnée par jour de jeu.
pub const RECOVERY_PER_DAY: f32 = 0.03;

// ---------------------------------------------------------------------------------------------
// Composants
// ---------------------------------------------------------------------------------------------

/// Le venin d'une morsure : dégâts par seconde pendant `remaining` secondes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Poisoned {
    pub remaining: f32,
    pub per_sec: f32,
}

/// Dépouille d'un animal mort : source de charogne et de butin.
#[derive(Debug, Clone, PartialEq)]
pub struct Carcass {
    pub mob: String,
    /// Reste avant que la carcasse se perde.
    pub left: f32,
    /// Joueur qui l'a tuée (naturfrid).
    pub hunter: Option<EntityId>,
}

/// Réputation de chasseur d'un joueur : 0 (neutre) à −1 (fléau de la forêt).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Standing {
    pub score: f32,
    /// Mises à mort récentes (espèce, instant).
    kills: Vec<(String, f64)>,
}

impl Standing {
    /// Réputation de départ `score` (de −1 à 0), sans mise à mort récente.
    pub fn with_score(score: f32) -> Self {
        Self {
            score: score.clamp(-1.0, 0.0),
            kills: Vec::new(),
        }
    }

    /// Humeur du folklore (0 à 1) : 1 tant que le joueur respecte la nature.
    pub fn mood(&self) -> f32 {
        (1.0 + self.score).clamp(0.0, 1.0)
    }

    /// Facteur de densité du gibier (0,3 à 1).
    pub fn density(&self) -> f32 {
        (1.0 + 0.7 * self.score).clamp(0.3, 1.0)
    }

    fn punish(&mut self, amount: f32) {
        self.score = (self.score - amount).max(-1.0);
    }
}

/// Facteur de densité de la faune d'après la réputation du joueur le moins respectueux.
pub fn nature_factor(world: &EntityWorld) -> f32 {
    world
        .query_ids::<(&Player, &Standing)>()
        .into_iter()
        .filter_map(|id| world.get::<Standing>(id).map(|s| s.density()))
        .fold(1.0, f32::min)
}

// ---------------------------------------------------------------------------------------------
// État partagé : traces et pièges
// ---------------------------------------------------------------------------------------------

/// Nature d'un indice laissé par un animal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignKind {
    Print,
    Dropping,
    Debarked,
}

/// Un indice : empreinte dans la neige, crottes, arbre écorcé.
#[derive(Debug, Clone, PartialEq)]
pub struct Sign {
    pub kind: SignKind,
    pub species: String,
    /// Forme de l'empreinte (`tass`, `klov`…).
    pub print: String,
    pub pos: DVec3,
    /// Cap de la marche, en radians (`atan2(z, x)`).
    pub heading: f32,
    pub born: f64,
    pub ttl: f32,
}

/// Ce qu'on lit en observant un indice.
#[derive(Debug, Clone, PartialEq)]
pub struct SignReading {
    pub species: String,
    pub kind: SignKind,
    pub print: String,
    pub heading: f32,
    /// Âge de l'indice en secondes de jeu.
    pub age: f32,
    pub pos: DVec3,
}

/// Un piège posé.
#[derive(Debug, Clone, PartialEq)]
pub struct Trap {
    pub id: u32,
    pub kind: TrapKind,
    pub pos: IVec3,
    /// Appâté : deux fois plus efficace.
    pub baited: bool,
    pub armed: bool,
    /// Espèce prise, à relever.
    pub catch: Option<String>,
}

/// Colonie de castors (une par hutte).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Colony {
    pub timer: f32,
    pub felled: u32,
}

/// État que le jeu consulte et modifie : traces, pièges, colonies.
#[derive(Debug, Default)]
pub struct EcologyState {
    /// Secondes de jeu écoulées depuis le début de la session.
    pub clock: f64,
    signs: VecDeque<Sign>,
    traps: Vec<Trap>,
    next_trap: u32,
    pub colonies: HashMap<IVec3, Colony>,
}

/// Registre partagé entre le système et le jeu.
pub type EcologyHandle = Arc<Mutex<EcologyState>>;

impl EcologyState {
    pub fn signs(&self) -> impl Iterator<Item = &Sign> {
        self.signs.iter()
    }

    pub fn sign_count(&self) -> usize {
        self.signs.len()
    }

    fn push_sign(&mut self, s: Sign) {
        if self.signs.len() >= SIGN_CAP {
            self.signs.pop_front();
        }
        self.signs.push_back(s);
    }

    /// Indices encore lisibles à moins de `reach` de `pos`, les plus proches d'abord.
    pub fn signs_near(&self, pos: DVec3, reach: f64) -> Vec<SignReading> {
        let mut v: Vec<(f64, SignReading)> = self
            .signs
            .iter()
            .filter(|s| self.clock - s.born <= f64::from(s.ttl))
            .filter_map(|s| {
                let d = s.pos.distance(pos);
                (d <= reach).then(|| {
                    (
                        d,
                        SignReading {
                            species: s.species.clone(),
                            kind: s.kind,
                            print: s.print.clone(),
                            heading: s.heading,
                            age: (self.clock - s.born) as f32,
                            pos: s.pos,
                        },
                    )
                })
            })
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter().map(|(_, r)| r).collect()
    }

    /// Lire une piste : l'indice lisible le plus proche (espèce, direction, âge).
    pub fn read_sign(&self, pos: DVec3, reach: f64) -> Option<SignReading> {
        self.signs_near(pos, reach).into_iter().next()
    }

    /// Pose un piège ; renvoie son identifiant.
    pub fn place_trap(&mut self, kind: TrapKind, pos: IVec3, baited: bool) -> u32 {
        self.next_trap += 1;
        let id = self.next_trap;
        self.traps.push(Trap {
            id,
            kind,
            pos,
            baited,
            armed: true,
            catch: None,
        });
        id
    }

    pub fn trap(&self, id: u32) -> Option<&Trap> {
        self.traps.iter().find(|t| t.id == id)
    }

    pub fn traps(&self) -> &[Trap] {
        &self.traps
    }

    /// Relève la prise : l'espèce, et le piège est réarmé.
    pub fn take_catch(&mut self, id: u32) -> Option<String> {
        let t = self.traps.iter_mut().find(|t| t.id == id)?;
        let c = t.catch.take()?;
        t.armed = true;
        Some(c)
    }

    pub fn remove_trap(&mut self, id: u32) -> bool {
        let n = self.traps.len();
        self.traps.retain(|t| t.id != id);
        self.traps.len() != n
    }
}

// ---------------------------------------------------------------------------------------------
// Venin
// ---------------------------------------------------------------------------------------------

/// Inocule le venin de `by` à `target` selon la chance de la morsure (déterministe).
pub fn envenom(world: &mut EntityWorld, by: EntityId, target: EntityId, v: &Venom) {
    let h = hash01(u64::from(by.0 as u32) << 32 | u64::from(target.0 as u32), {
        world
            .get::<Health>(target)
            .map_or(0, |h| u64::from(h.health.to_bits()))
    });
    if h >= v.chance {
        return;
    }
    match world.get_mut::<Poisoned>(target) {
        Some(p) => {
            p.remaining = p.remaining.max(v.seconds);
            p.per_sec = p.per_sec.max(v.damage_per_sec);
        }
        None => world.insert(
            target,
            Poisoned {
                remaining: v.seconds,
                per_sec: v.damage_per_sec,
            },
        ),
    }
}

/// Soigne un empoisonné avec `item` si l'objet figure parmi les remèdes de `venom` ; vrai si guéri.
pub fn cure(world: &mut EntityWorld, patient: EntityId, venom: &Venom, item: &str) -> bool {
    if venom.cures.iter().any(|c| c == item) && world.has::<Poisoned>(patient) {
        world.remove::<Poisoned>(patient);
        return true;
    }
    false
}

fn hash01(a: u64, b: u64) -> f32 {
    let mut x = a ^ b.rotate_left(29) ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

// ---------------------------------------------------------------------------------------------
// Dépeçage
// ---------------------------------------------------------------------------------------------

/// Dépèce une carcasse : les morceaux de la fiche, selon l'outil et le mois (0 = janvier). La
/// carcasse disparaît et la faute de gaspillage est évitée. `None` si l'entité n'est pas une
/// carcasse.
pub fn skin(
    world: &mut EntityWorld,
    mobs: &MobRegistry,
    carcass: EntityId,
    tool: Tool,
    month: usize,
    rng: &mut JavaRandom,
) -> Option<Vec<(String, u32)>> {
    let c: Carcass = {
        let r = world.get::<Carcass>(carcass)?;
        (*r).clone()
    };
    let cuts = mobs.get(&c.mob)?.ecology.as_ref()?.loot.clone();
    let mut out: Vec<(String, u32)> = Vec::new();
    for cut in &cuts {
        if cut.months.is_some_and(|w| !in_window(month, w)) {
            continue;
        }
        if cut.needs_knife && tool != Tool::Knife {
            continue;
        }
        if rng.next_float() >= cut.chance {
            continue;
        }
        let span = cut.max.saturating_sub(cut.min) as i32 + 1;
        let base = cut.min + rng.next_int_bound(span) as u32;
        let n = if tool == Tool::Knife {
            (base as f32 * cut.knife_bonus).round() as u32
        } else {
            // À mains nues, on déchire : la moitié se perd.
            (base as f32 * 0.5).ceil() as u32
        };
        if n > 0 {
            out.push((cut.item.clone(), n));
        }
    }
    world.destroy(carcass);
    Some(out)
}

// ---------------------------------------------------------------------------------------------
// Barrage
// ---------------------------------------------------------------------------------------------

const DIRS6: [IVec3; 6] = [
    IVec3::new(1, 0, 0),
    IVec3::new(-1, 0, 0),
    IVec3::new(0, 0, 1),
    IVec3::new(0, 0, -1),
    IVec3::new(0, 1, 0),
    IVec3::new(0, -1, 0),
];

/// Un bloc vient d'être cassé : si c'est un bloc de barrage de castor, l'eau au-dessus du niveau
/// aval est retirée côté amont (doc 03 §4.5). Renvoie le nombre de cellules d'eau retirées.
pub fn dam_broken(
    blocks: &mut dyn BlockAccess,
    reg: &BlockRegistry,
    mobs: &MobRegistry,
    pos: IVec3,
    block: BlockId,
) -> usize {
    let name = reg.from_id(block).name.as_str();
    let is_dam = mobs
        .all()
        .iter()
        .filter_map(|c| c.ecology.as_ref()?.fell.as_ref())
        .any(|f| f.dam.iter().any(|d| d == name));
    if !is_dam {
        return 0;
    }
    let water = reg.ids().water;
    let air = reg.ids().air;
    // Surface de chaque côté du barrage.
    let mut sides: Vec<(IVec3, i32)> = Vec::new();
    for d in &DIRS6[..4] {
        let n = pos + *d;
        if blocks.block_at(n) == water {
            let mut top = n.y;
            while blocks.block(n.x, top + 1, n.z) == water {
                top += 1;
            }
            sides.push((n, top));
        }
    }
    if sides.is_empty() {
        return 0;
    }
    sides.sort_by_key(|s| s.1);
    // Niveau aval : le côté le plus bas, ou le fond de la brèche s'il n'y en a qu'un.
    let downstream = if sides.len() >= 2 { sides[0].1 } else { pos.y - 1 };
    let mut removed = 0;
    let mut seen: HashSet<IVec3> = HashSet::new();
    let mut queue: VecDeque<IVec3> = VecDeque::new();
    for (n, top) in sides.iter().rev() {
        if *top > downstream {
            queue.push_back(*n);
            seen.insert(*n);
        }
    }
    while let Some(c) = queue.pop_front() {
        if seen.len() > DRAIN_MAX {
            break;
        }
        if c.y > downstream {
            blocks.set_block(c.x, c.y, c.z, air);
            removed += 1;
        }
        for d in DIRS6 {
            let n = c + d;
            if (n.x - pos.x).abs() > 64 || (n.z - pos.z).abs() > 64 {
                continue;
            }
            if blocks.block_at(n) == water && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    removed
}

// ---------------------------------------------------------------------------------------------
// Système
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct EcologySystem {
    handle: EcologyHandle,
    timer: f32,
    last_print: HashMap<EntityId, DVec3>,
    snow: Option<Vec<BlockId>>,
}

impl Default for EcologySystem {
    fn default() -> Self {
        Self::new(EcologyHandle::default())
    }
}

impl EcologySystem {
    pub fn new(handle: EcologyHandle) -> Self {
        Self {
            handle,
            timer: 0.0,
            last_print: HashMap::new(),
            snow: None,
        }
    }

    pub fn handle(&self) -> EcologyHandle {
        self.handle.clone()
    }
}

fn cell(p: DVec3) -> IVec3 {
    IVec3::new(floor_i(p.x), floor_i(p.y), floor_i(p.z))
}

fn ids_of(reg: &BlockRegistry, names: &[String]) -> Vec<BlockId> {
    let air = reg.ids().air;
    names
        .iter()
        .map(|n| reg.get(n).id)
        .filter(|&i| i != air)
        .collect()
}

impl System for EcologySystem {
    fn name(&self) -> &'static str {
        "Ecology"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let clock = {
            let mut st = self.handle.lock().expect("écologie");
            st.clock += f64::from(dt);
            st.clock
        };
        // Chaque tick : les morts (la santé les détruit juste après), le venin, les empreintes.
        self.bury(world, ctx);
        poison(world, dt);
        self.prints(world, ctx, clock);

        self.timer += dt;
        if self.timer < INTERVAL {
            return;
        }
        let step = self.timer;
        self.timer = 0.0;
        self.browse(world, ctx, step);
        self.fell(world, ctx, step);
        self.traps(world, ctx, step);
        self.rot(world, ctx, step);
    }
}

fn poison(world: &mut EntityWorld, dt: f32) {
    for id in world.query_ids::<&Poisoned>() {
        let Some(p) = world.copy::<Poisoned>(id) else {
            continue;
        };
        if let Some(h) = world.get_mut::<Health>(id) {
            if h.alive {
                h.health -= p.per_sec * dt;
                if h.health <= 0.0 {
                    h.health = 0.0;
                    h.alive = false;
                }
            }
        }
        let left = p.remaining - dt;
        if left <= 0.0 || !world.get::<Health>(id).is_some_and(|h| h.alive) {
            world.remove::<Poisoned>(id);
        } else if let Some(pp) = world.get_mut::<Poisoned>(id) {
            pp.remaining = left;
        }
    }
}

impl EcologySystem {
    /// Un animal mort laisse une carcasse ; le chasseur qui l'a tué en répond devant la nature.
    fn bury(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>) {
        let dead: Vec<EntityId> = world
            .query_ids::<(&MobType, &Health)>()
            .into_iter()
            .filter(|&id| world.get::<Health>(id).is_some_and(|h| !h.alive))
            .collect();
        for id in dead {
            let Some(mob) = world.get::<MobType>(id).map(|m| m.mob_id.clone()) else {
                continue;
            };
            let Some(cfg) = ctx.mobs.get(&mob) else {
                continue;
            };
            let Some(eco) = cfg.ecology.as_ref() else {
                continue;
            };
            let hunter = world
                .get::<Brain>(id)
                .and_then(|b| b.provoker)
                .filter(|p| world.has::<Player>(*p));
            let pos = world.copy::<Transform>(id).map(|t| t.pos);
            if let (Some(h), Some(n)) = (hunter, eco.naturfrid.as_ref()) {
                let month = crate::brain::clock::Clock::of(ctx.config, None).month;
                let clock = self.handle.lock().expect("écologie").clock;
                let day = f64::from(ctx.config.day_length_seconds.max(1.0));
                if !world.has::<Standing>(h) {
                    world.insert(h, Standing::default());
                }
                if let Some(s) = world.get_mut::<Standing>(h) {
                    s.kills.retain(|(_, t)| clock - *t < day);
                    s.kills.push((mob.clone(), clock));
                    let recent = s.kills.iter().filter(|(m, _)| *m == mob).count();
                    if recent > DAILY_ALLOWANCE {
                        s.punish(OVERKILL * n.worth);
                    }
                    if n.sacred {
                        s.punish(SACRED * n.worth.max(1.0));
                    } else if n.breeding.is_some_and(|w| in_window(month, w)) {
                        s.punish(WITH_YOUNG * n.worth);
                    }
                }
            }
            if !eco.loot.is_empty() {
                if let Some(p) = pos {
                    let c = world.create();
                    world.insert(c, Transform::new(p.x, p.y, p.z));
                    world.insert(
                        c,
                        Carcass {
                            mob: mob.clone(),
                            left: eco.carcass,
                            hunter,
                        },
                    );
                    world.insert(
                        c,
                        Emitter {
                            kind: Stimulus::Carrion,
                            radius: CARRION_RADIUS,
                        },
                    );
                }
            }
        }
    }

    /// Empreintes sur la neige fraîche (chaque tick : un animal rapide fait plusieurs pas).
    fn prints(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, clock: f64) {
        if self.snow.is_none() {
            let mut names: Vec<String> = (1..=7).map(|i| format!("snotacke_{i}")).collect();
            names.push("snow".into());
            self.snow = Some(ids_of(ctx.block_reg, &names));
        }
        let snow = self.snow.as_deref().unwrap_or(&[]);
        if snow.is_empty() {
            return;
        }
        let mut live: HashSet<EntityId> = HashSet::new();
        for id in world.query_ids::<(&Brain, &MobType, &Transform)>() {
            let (Some(m), Some(t)) = (world.get::<MobType>(id), world.copy::<Transform>(id)) else {
                continue;
            };
            let Some(tr) = ctx
                .mobs
                .get(&m.mob_id)
                .and_then(|c| c.ecology.as_ref())
                .and_then(|e| e.tracks.as_ref())
            else {
                continue;
            };
            if !world.copy::<Velocity>(id).is_some_and(|v| v.grounded) {
                continue;
            }
            live.insert(id);
            let here = cell(t.pos);
            let on_snow = snow.contains(&ctx.blocks.block_at(here))
                || snow.contains(&ctx.blocks.block_at(here - IVec3::Y));
            let last = *self.last_print.entry(id).or_insert(t.pos);
            let moved = t.pos - last;
            if moved.length() < f64::from(tr.stride) {
                continue;
            }
            self.last_print.insert(id, t.pos);
            if !on_snow {
                continue;
            }
            let species = m.mob_id.clone();
            let sign = Sign {
                kind: SignKind::Print,
                species,
                print: tr.print.clone(),
                pos: t.pos,
                heading: moved.z.atan2(moved.x) as f32,
                born: clock,
                ttl: tr.ttl,
            };
            self.handle.lock().expect("écologie").push_sign(sign);
        }
        self.last_print.retain(|id, _| live.contains(id));
    }

    /// Broutage des pousses et crottes.
    fn browse(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, _step: f32) {
        let month = crate::brain::clock::Clock::of(ctx.config, None).month;
        let clock = self.handle.lock().expect("écologie").clock;
        for id in world.query_ids::<(&Brain, &MobType, &Transform)>() {
            let Some(b) = world.copy::<Brain>(id) else {
                continue;
            };
            if b.state != StateKind::Graze || b.in_water {
                continue;
            }
            let (Some(m), Some(t)) = (world.get::<MobType>(id), world.copy::<Transform>(id)) else {
                continue;
            };
            let Some(cfg) = ctx.mobs.get(&m.mob_id) else {
                continue;
            };
            let Some(eco) = cfg.ecology.as_ref() else {
                continue;
            };
            let species = m.mob_id.clone();
            drop(m);
            if let Some(br) = &eco.browse {
                if br.months.is_none_or(|w| in_window(month, w))
                    && ctx.rngs.ambient.next_float() < BROWSE_CHANCE
                {
                    let eats = ids_of(ctx.block_reg, &br.eats);
                    let fences = ids_of(ctx.block_reg, &br.fences);
                    if let Some(p) = find_plant(&*ctx.blocks, cell(t.pos), &eats) {
                        if !fenced(&*ctx.blocks, p, &fences, br.fence_radius) {
                            let air = ctx.block_reg.ids().air;
                            ctx.blocks.set_block(p.x, p.y, p.z, air);
                            if let Some(bm) = world.get_mut::<Brain>(id) {
                                bm.needs.hunger = (bm.needs.hunger - 0.15).max(0.0);
                            }
                            if let Some(tr) = &eco.tracks {
                                if tr.droppings > 0.0
                                    && ctx.rngs.ambient.next_float() < tr.droppings
                                {
                                    self.handle.lock().expect("écologie").push_sign(Sign {
                                        kind: SignKind::Dropping,
                                        species: species.clone(),
                                        print: tr.print.clone(),
                                        pos: t.pos,
                                        heading: 0.0,
                                        born: clock,
                                        ttl: tr.ttl * 2.0,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            if let Some(tr) = eco.tracks.as_ref().filter(|t| t.debark) {
                {
                    let f = eco.fell.clone().unwrap_or_else(default_fell);
                    if let Some(p) = find_log(&*ctx.blocks, ctx.block_reg, cell(t.pos), 2, &f) {
                        if ctx.rngs.ambient.next_float() < BROWSE_CHANCE * 0.5 {
                            self.handle.lock().expect("écologie").push_sign(Sign {
                                kind: SignKind::Debarked,
                                species,
                                print: tr.print.clone(),
                                pos: DVec3::new(
                                    f64::from(p.x) + 0.5,
                                    f64::from(p.y),
                                    f64::from(p.z) + 0.5,
                                ),
                                heading: 0.0,
                                born: clock,
                                ttl: tr.ttl * 4.0,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Les castors abattent des arbres.
    fn fell(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, step: f32) {
        for id in world.query_ids::<(&Brain, &MobType, &Transform)>() {
            let Some(b) = world.copy::<Brain>(id) else {
                continue;
            };
            let Some(t) = world.copy::<Transform>(id) else {
                continue;
            };
            let Some(f) = world
                .get::<MobType>(id)
                .and_then(|m| ctx.mobs.get(&m.mob_id))
                .and_then(|c| c.ecology.as_ref())
                .and_then(|e| e.fell.clone())
            else {
                continue;
            };
            if b.state != StateKind::Build || b.home.distance(t.pos) > LODGE_REACH {
                continue;
            }
            let lodge = cell(b.home);
            let go = {
                let mut st = self.handle.lock().expect("écologie");
                let c = st.colonies.entry(lodge).or_default();
                c.timer += step;
                c.timer >= f.every_secs && c.felled < f.cap
            };
            if !go {
                continue;
            }
            let Some(tree) = find_log(&*ctx.blocks, ctx.block_reg, lodge, f.reach, &f) else {
                continue;
            };
            let air = ctx.block_reg.ids().air;
            let n = fell_tree(ctx.blocks, ctx.block_reg, tree, &f, air);
            if n > 0 {
                let mut st = self.handle.lock().expect("écologie");
                let c = st.colonies.entry(lodge).or_default();
                c.timer = 0.0;
                c.felled += 1;
            }
        }
    }

    /// Les pièges prennent les animaux à portée.
    fn traps(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, step: f32) {
        let mut st = self.handle.lock().expect("écologie");
        if st.traps.iter().all(|t| !t.armed) {
            return;
        }
        let animals: Vec<(EntityId, String, DVec3)> = world
            .query_ids::<(&MobType, &Transform)>()
            .into_iter()
            .filter(|&id| world.get::<Health>(id).is_some_and(|h| h.alive))
            .filter_map(|id| {
                Some((
                    id,
                    world.get::<MobType>(id)?.mob_id.clone(),
                    world.copy::<Transform>(id)?.pos,
                ))
            })
            .collect();
        let mut taken: HashSet<EntityId> = HashSet::new();
        for t in st.traps.iter_mut().filter(|t| t.armed) {
            let c = DVec3::new(
                f64::from(t.pos.x) + 0.5,
                f64::from(t.pos.y),
                f64::from(t.pos.z) + 0.5,
            );
            let reach = match t.kind {
                TrapKind::Snare => SNARE_REACH,
                TrapKind::Pit => PIT_REACH,
                TrapKind::FishTrap => FISH_TRAP_REACH,
                TrapKind::Net => NET_REACH,
            };
            for (id, mob, p) in &animals {
                if taken.contains(id) || (p.y - c.y).abs() > 3.0 {
                    continue;
                }
                let (dx, dz) = (p.x - c.x, p.z - c.z);
                if (dx * dx + dz * dz).sqrt() > reach {
                    continue;
                }
                let rate = ctx
                    .mobs
                    .get(mob)
                    .and_then(|m| m.ecology.as_ref())
                    .map_or(0.0, |e| e.catch_rate(t.kind));
                if rate <= 0.0 {
                    continue;
                }
                let bait = if t.baited { 2.0 } else { 1.0 };
                if ctx.rngs.ambient.next_float() < rate * bait * step / 60.0 {
                    t.catch = Some(mob.clone());
                    t.armed = false;
                    taken.insert(*id);
                    break;
                }
            }
        }
        drop(st);
        for id in taken {
            world.destroy(id);
        }
    }

    /// Carcasses : elles se perdent, et le gaspillage pèse sur le chasseur ; la réputation revient
    /// avec les jours.
    fn rot(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, step: f32) {
        for id in world.query_ids::<&Carcass>() {
            let Some(c) = world.get_mut::<Carcass>(id) else {
                continue;
            };
            c.left -= step;
            if c.left > 0.0 {
                continue;
            }
            let (mob, hunter) = (c.mob.clone(), c.hunter);
            world.destroy(id);
            let worth = ctx
                .mobs
                .get(&mob)
                .and_then(|m| m.ecology.as_ref())
                .and_then(|e| e.naturfrid.as_ref())
                .map(|n| n.worth);
            if let (Some(h), Some(w)) = (hunter, worth) {
                if let Some(s) = world.get_mut::<Standing>(h) {
                    s.punish(WASTE * w);
                }
            }
        }
        let day = ctx.config.day_length_seconds.max(1.0);
        for id in world.query_ids::<&Standing>() {
            if let Some(s) = world.get_mut::<Standing>(id) {
                s.score = (s.score + RECOVERY_PER_DAY * step / day).min(0.0);
            }
        }
    }
}

/// Plante mangeable dans la bouche (1 bloc autour, du sol à la tête).
fn find_plant(blocks: &dyn BlockAccess, at: IVec3, eats: &[BlockId]) -> Option<IVec3> {
    if eats.is_empty() {
        return None;
    }
    let mut best: Option<(i32, IVec3)> = None;
    for dx in -BROWSE_REACH..=BROWSE_REACH {
        for dz in -BROWSE_REACH..=BROWSE_REACH {
            for dy in -1..=1 {
                let p = at + IVec3::new(dx, dy, dz);
                if eats.contains(&blocks.block_at(p)) {
                    let d = dx * dx + dz * dz + dy * dy;
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, p));
                    }
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

fn fenced(blocks: &dyn BlockAccess, p: IVec3, fences: &[BlockId], radius: i32) -> bool {
    if fences.is_empty() {
        return false;
    }
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            for dy in -1..=2 {
                if fences.contains(&blocks.block_at(p + IVec3::new(dx, dy, dz))) {
                    return true;
                }
            }
        }
    }
    false
}

/// Troncs vivants des essences de Norrcraft quand la fiche n'a pas de bloc `fell`.
fn default_fell() -> Fell {
    Fell {
        log_suffix: "_stam".into(),
        spare: vec!["dod_stam".into(), "mossig_stam".into()],
        every_secs: 0.0,
        cap: 0,
        reach: 0,
        dam: Vec::new(),
    }
}

fn is_living_log(reg: &BlockRegistry, id: BlockId, f: &Fell) -> bool {
    let name = reg.from_id(id).name.as_str();
    name.ends_with(f.log_suffix.as_str()) && !f.spare.iter().any(|s| s == name)
}

/// Tronc vivant le plus proche de `at` dans un rayon `reach`.
fn find_log(
    blocks: &dyn BlockAccess,
    reg: &BlockRegistry,
    at: IVec3,
    reach: i32,
    f: &Fell,
) -> Option<IVec3> {
    let mut best: Option<(i32, IVec3)> = None;
    for dx in -reach..=reach {
        for dz in -reach..=reach {
            let d2 = dx * dx + dz * dz;
            if d2 > reach * reach || best.is_some_and(|(bd, _)| d2 >= bd) {
                continue;
            }
            for y in (at.y - 4)..=(at.y + 12) {
                let p = IVec3::new(at.x + dx, y, at.z + dz);
                if is_living_log(reg, blocks.block_at(p), f) {
                    best = Some((d2, p));
                    break;
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Abat l'arbre dont `from` fait partie : tous les troncs vivants connexes, au plus
/// [`FELL_MAX_LOGS`]. Renvoie le nombre de troncs retirés.
fn fell_tree(
    blocks: &mut dyn BlockAccess,
    reg: &BlockRegistry,
    from: IVec3,
    f: &Fell,
    air: BlockId,
) -> usize {
    let mut seen: HashSet<IVec3> = HashSet::from([from]);
    let mut queue = VecDeque::from([from]);
    let mut n = 0;
    while let Some(c) = queue.pop_front() {
        if n >= FELL_MAX_LOGS {
            break;
        }
        blocks.set_block(c.x, c.y, c.z, air);
        n += 1;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let p = c + IVec3::new(dx, dy, dz);
                    if is_living_log(reg, blocks.block_at(p), f) && seen.insert(p) {
                        queue.push_back(p);
                    }
                }
            }
        }
    }
    n
}

/// Fiche d'écologie d'une espèce (raccourci).
pub fn sheet<'a>(mobs: &'a MobRegistry, id: &str) -> Option<&'a nb_data::Ecology> {
    mobs.get(id).and_then(|c: &MobConfig| c.ecology.as_ref())
}
