//! Cultures, croissance par la lumière et brûlis (session 6.4, doc 04 §6.1).
//!
//! * **Espèces** : [`Crop`] (13) avec une fiche [`Spec`] : fenêtre de semis, jours de croissance,
//!   lumière minimale, résistance au gel, rendement. Le seigle se sème en automne et passe l'hiver.
//! * **Croissance par la lumière** : [`growth_rate`] combine les heures de jour (le soleil de
//!   minuit du Nord accélère l'orge), la chaleur de l'air et la fertilité. Le gel tue une culture
//!   sensible ([`frost_kills`]).
//! * **Champs** : [`Plot`] par case labourée, dans [`FieldState`] ; [`till`], [`sow`], [`manure`],
//!   [`harvest`]. Le sol est du [`Soil`] : terre labourée, **cendre** (rendement ×3 pendant deux
//!   ans) puis **épuisée** (reboise).
//! * **Brûlis** (svedjebruk) : [`SlashState`] garde les troncs abattus qui sèchent ; [`burn`]
//!   transforme la parcelle en champ de cendre.
//! * **Clôture** : [`enclosed`] dit si une case est entourée de `gardsgard` ; les herbivores
//!   broutent les cultures non closes.
//!
//! Les gestes (houe, semis, récolte) ne sont pas câblés aux mains du joueur (survie Norrcraft
//! vide jusqu'à la 6.7) : ce module en donne les fonctions et le [`FarmSystem`] fait pousser.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use nb_core::glam::{DVec3, IVec3};
use nb_core::JavaRandom;
use nb_data::{BlockId, BlockRegistry};

use crate::blocks::BlockAccess;
use crate::brain::clock::{smooth, Clock, DEFAULT_LATITUDE};
use crate::components::{MobType, Transform};
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;
use nb_world::calendar::DayLight;
use nb_world::weather::freezing_point;

/// Jours de séchage d'un abattis avant de pouvoir le brûler.
pub const SLASH_DRY_DAYS: f32 = 6.0;
/// Rendement de la cendre.
pub const ASH_YIELD: f32 = 3.0;
/// Durée de la cendre, en années de jeu.
pub const ASH_YEARS: f32 = 2.0;
/// Attente d'un sol épuisé avant qu'il reboise, en années de jeu.
pub const REFOREST_YEARS: f32 = 1.0;
/// Fertilité maximale.
pub const MAX_FERTILITY: f32 = 1.2;
/// Fertilité d'un champ neuf.
pub const START_FERTILITY: f32 = 0.6;
/// Fertilité perdue à chaque récolte.
pub const HARVEST_DRAIN: f32 = 0.25;
/// Rayon (cases) où un champ doit être clos pour échapper aux bêtes.
pub const FENCE_RADIUS: i32 = 6;
/// Secondes entre deux passes du [`FarmSystem`].
pub const INTERVAL: f32 = 1.0;
/// Portée du brûlis (cases) autour du feu.
pub const BURN_MAX_RADIUS: i32 = 5;

// ---------------------------------------------------------------------------------------------
// Espèces
// ---------------------------------------------------------------------------------------------

/// Forme du plant (famille de blocs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Form {
    Grain,
    Root,
    Leaf,
}

impl Form {
    pub const fn prefix(self) -> &'static str {
        match self {
            Form::Grain => "korn",
            Form::Root => "rot",
            Form::Leaf => "blad",
        }
    }
}

/// Les cultures du doc 04 §6.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Crop {
    Rye,
    Barley,
    Oats,
    Turnip,
    Cabbage,
    Pea,
    Flax,
    Hemp,
    Hops,
    Onion,
    Potato,
    Apple,
    Cherry,
}

/// Fiche d'une culture.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub crop: Crop,
    /// Objet récolté, qui sert aussi de semence.
    pub item: &'static str,
    pub form: Form,
    /// Mois de semis (0 = janvier), fenêtre qui peut enjamber l'année.
    pub sow: [u8; 2],
    /// Jours de jeu pour mûrir à plein régime.
    pub days: f32,
    /// Heures de jour en dessous desquelles rien ne pousse.
    pub light: f32,
    /// Supporte le gel (le seigle d'hiver dort sous la neige).
    pub hardy: bool,
    /// Latitude (1 − T₀) au-delà de laquelle la culture ne mûrit pas.
    pub max_latitude: f32,
    /// Récolte `[min, max]` unités.
    pub yield_: [u8; 2],
    /// Vivace : repousse après la récolte (arbres fruitiers).
    pub perennial: bool,
}

#[allow(clippy::too_many_arguments)]
const fn spec(
    crop: Crop,
    item: &'static str,
    form: Form,
    sow: [u8; 2],
    days: f32,
    light: f32,
    hardy: bool,
    max_latitude: f32,
    yield_: [u8; 2],
) -> Spec {
    Spec {
        crop,
        item,
        form,
        sow,
        days,
        light,
        hardy,
        max_latitude,
        yield_,
        perennial: false,
    }
}

/// Table des cultures. `days` est exprimé en jours de jeu d'une année de 12 mois.
pub const SPECS: [Spec; 13] = [
    spec(Crop::Rye, "rag", Form::Grain, [7, 8], 30.0, 5.0, true, 0.80, [2, 4]),
    spec(Crop::Barley, "korn", Form::Grain, [4, 5], 18.0, 8.0, false, 1.00, [2, 3]),
    spec(Crop::Oats, "havre", Form::Grain, [4, 5], 22.0, 8.0, false, 0.85, [2, 4]),
    spec(Crop::Turnip, "rova", Form::Root, [4, 6], 16.0, 8.0, false, 0.90, [2, 3]),
    spec(Crop::Cabbage, "kal", Form::Leaf, [4, 5], 26.0, 9.0, false, 0.75, [1, 2]),
    spec(Crop::Pea, "arter", Form::Leaf, [4, 5], 18.0, 9.0, false, 0.80, [2, 4]),
    spec(Crop::Flax, "lin", Form::Leaf, [4, 5], 24.0, 10.0, false, 0.75, [1, 3]),
    spec(Crop::Hemp, "hampa", Form::Leaf, [4, 5], 26.0, 10.0, false, 0.65, [1, 3]),
    spec(Crop::Hops, "humle", Form::Leaf, [3, 4], 34.0, 10.0, false, 0.60, [1, 3]),
    spec(Crop::Onion, "lok", Form::Root, [3, 4], 22.0, 9.0, false, 0.75, [2, 3]),
    spec(Crop::Potato, "potatis", Form::Root, [4, 5], 24.0, 9.0, false, 0.80, [2, 5]),
    Spec {
        perennial: true,
        ..spec(Crop::Apple, "appel", Form::Leaf, [3, 4], 60.0, 10.0, false, 0.55, [2, 5])
    },
    Spec {
        perennial: true,
        ..spec(Crop::Cherry, "korsbar", Form::Leaf, [3, 4], 50.0, 10.0, false, 0.50, [2, 4])
    },
];

impl Crop {
    pub fn spec(self) -> &'static Spec {
        &SPECS[self as usize]
    }

    /// Culture dont `item` est la graine (le produit sert de semence).
    pub fn of_item(item: &str) -> Option<Crop> {
        SPECS.iter().find(|s| s.item == item).map(|s| s.crop)
    }
}

fn in_window(month: usize, w: [u8; 2]) -> bool {
    nb_data::behavior::in_window(month, w)
}

// ---------------------------------------------------------------------------------------------
// Croissance
// ---------------------------------------------------------------------------------------------

/// Heures de jour à la date `year_fraction` et à la latitude (1 − T₀).
pub fn daylight_hours(year_fraction: f64, latitude: f32) -> f32 {
    DayLight::at(year_fraction, latitude).hours()
}

/// Conditions du moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conditions {
    pub hours: f32,
    /// Température de l'air (échelle de T₀).
    pub air: f32,
    pub rain: f32,
}

impl Conditions {
    pub fn warmth(&self) -> f32 {
        self.air - freezing_point() as f32
    }
}

/// `true` si le gel tue cette culture.
pub fn frost_kills(spec: &Spec, c: &Conditions) -> bool {
    !spec.hardy && c.warmth() < 0.0
}

/// Croissance par jour de jeu (1 : une culture entière en `spec.days` jours).
///
/// * lumière : nulle sous `spec.light` heures, pleine quatre heures plus tard, et un bonus
///   jusqu'à +40 % aux jours de plus de seize heures (soleil de minuit) ;
/// * chaleur : nulle au gel, pleine à 0,14 au-dessus (seule la céréale rustique pousse un peu
///   au froid : 20 %) ;
/// * fertilité : 0,5 + 0,5 × fertilité ;
/// * latitude : sans effet en deçà de `max_latitude`, la culture ne mûrit pas au-delà.
pub fn growth_rate(spec: &Spec, c: &Conditions, fertility: f32, latitude: f32) -> f32 {
    if latitude > spec.max_latitude {
        return 0.0;
    }
    let light = smooth(spec.light, spec.light + 4.0, c.hours);
    let bonus = 1.0 + 0.4 * ((c.hours - 16.0) / 8.0).clamp(0.0, 1.0);
    let heat = {
        let w = c.warmth();
        let h = smooth(0.0, 0.14, w);
        if spec.hardy {
            h.max(0.2 * smooth(-0.08, 0.0, w))
        } else {
            h
        }
    };
    let soil = 0.5 + 0.5 * fertility.clamp(0.0, MAX_FERTILITY);
    light * bonus * heat * soil / spec.days
}

// ---------------------------------------------------------------------------------------------
// Champs
// ---------------------------------------------------------------------------------------------

/// Nature du sol d'une case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Soil {
    /// Terre labourée.
    Tilled,
    /// Cendre de brûlis, avec les jours de jeu qui lui restent.
    Ash { days_left: f32 },
    /// Épuisé : ne se sème plus ; reboise après `days_left` jours.
    Exhausted { days_left: f32 },
}

/// Une case de champ.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plot {
    pub soil: Soil,
    pub crop: Option<Crop>,
    /// Croissance de 0 à 1 (mûre à 1).
    pub growth: f32,
    pub fertility: f32,
    /// Latitude relevée au semis.
    pub latitude: f32,
}

impl Plot {
    pub fn new(soil: Soil) -> Self {
        Self {
            soil,
            crop: None,
            growth: 0.0,
            fertility: START_FERTILITY,
            latitude: DEFAULT_LATITUDE,
        }
    }

    pub fn ripe(&self) -> bool {
        self.crop.is_some() && self.growth >= 1.0
    }

    /// Étape du bloc affiché : 0 pousse, 1 en croissance, 2 mûr.
    pub fn stage(&self) -> usize {
        if self.growth >= 1.0 {
            2
        } else if self.growth >= 0.35 {
            1
        } else {
            0
        }
    }
}

/// Champs du monde, par case de sol.
pub type FieldState = Arc<Mutex<HashMap<IVec3, Plot>>>;

/// Identifiants des blocs de culture.
#[derive(Debug, Clone)]
pub struct FarmBlocks {
    pub tilled: BlockId,
    pub ash: BlockId,
    pub exhausted: BlockId,
    pub fence: BlockId,
    pub withered: BlockId,
    pub air: BlockId,
    /// `[forme][étape]`.
    pub stages: [[BlockId; 3]; 3],
    /// Blocs que la houe laboure.
    pub tillable: Vec<BlockId>,
    /// Bloc posé quand un sol épuisé reboise.
    pub reforest: BlockId,
}

impl FarmBlocks {
    pub fn from_registry(reg: &BlockRegistry) -> Option<Self> {
        let id = |n: &str| block_id(reg, n);
        let mut stages = [[0 as BlockId; 3]; 3];
        for (f, form) in [Form::Grain, Form::Root, Form::Leaf].into_iter().enumerate() {
            for (s, slot) in stages[f].iter_mut().enumerate() {
                *slot = id(&format!("{}_{}", form.prefix(), s))?;
            }
        }
        Some(Self {
            tilled: id("akerjord")?,
            ash: id("askaker")?,
            exhausted: id("utmarkt")?,
            fence: id("gardsgard")?,
            withered: id("vissen")?,
            air: reg.ids().air,
            stages,
            tillable: ["gras", "jord", "moran", "rodmull", "lovforna", "aska"]
                .iter()
                .filter_map(|n| id(n))
                .collect(),
            reforest: id("aska")?,
        })
    }

    pub fn stage_block(&self, form: Form, stage: usize) -> BlockId {
        let f = match form {
            Form::Grain => 0,
            Form::Root => 1,
            Form::Leaf => 2,
        };
        self.stages[f][stage.min(2)]
    }

    /// Le bloc est-il une plante de champ (flétrie comprise) ?
    pub fn is_crop(&self, b: BlockId) -> bool {
        b == self.withered || self.stages.iter().flatten().any(|&s| s == b)
    }

    fn soil_block(&self, soil: Soil) -> BlockId {
        match soil {
            Soil::Tilled => self.tilled,
            Soil::Ash { .. } => self.ash,
            Soil::Exhausted { .. } => self.exhausted,
        }
    }
}

/// Laboure `pos` : la case doit être un sol labourable avec de l'air au-dessus.
pub fn till(
    blocks: &mut dyn BlockAccess,
    fields: &FieldState,
    fb: &FarmBlocks,
    pos: IVec3,
) -> bool {
    let b = blocks.block(pos.x, pos.y, pos.z);
    if !fb.tillable.contains(&b) || blocks.block(pos.x, pos.y + 1, pos.z) != fb.air {
        return false;
    }
    blocks.set_block(pos.x, pos.y, pos.z, fb.tilled);
    fields.lock().unwrap().insert(pos, Plot::new(Soil::Tilled));
    true
}

/// Raisons d'un semis refusé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SowError {
    NoField,
    Occupied,
    Exhausted,
    OutOfSeason,
    TooNorth,
    Blocked,
}

/// Sème `crop` sur le champ `pos` au mois `month` ; `latitude` = 1 − T₀ du lieu.
pub fn sow(
    blocks: &mut dyn BlockAccess,
    fields: &FieldState,
    fb: &FarmBlocks,
    pos: IVec3,
    crop: Crop,
    month: usize,
    latitude: f32,
) -> Result<(), SowError> {
    let mut f = fields.lock().unwrap();
    let plot = f.get_mut(&pos).ok_or(SowError::NoField)?;
    if matches!(plot.soil, Soil::Exhausted { .. }) {
        return Err(SowError::Exhausted);
    }
    if plot.crop.is_some() {
        return Err(SowError::Occupied);
    }
    let spec = crop.spec();
    if !in_window(month, spec.sow) {
        return Err(SowError::OutOfSeason);
    }
    if latitude > spec.max_latitude {
        return Err(SowError::TooNorth);
    }
    if blocks.block(pos.x, pos.y + 1, pos.z) != fb.air {
        return Err(SowError::Blocked);
    }
    plot.crop = Some(crop);
    plot.growth = 0.0;
    plot.latitude = latitude;
    blocks.set_block(pos.x, pos.y + 1, pos.z, fb.stage_block(spec.form, 0));
    Ok(())
}

/// Engrais : le fumier (+0,4) ou le goémon (+0,3).
pub fn manure(fields: &FieldState, pos: IVec3, amount: f32) -> bool {
    match fields.lock().unwrap().get_mut(&pos) {
        Some(p) if !matches!(p.soil, Soil::Exhausted { .. }) => {
            p.fertility = (p.fertility + amount).min(MAX_FERTILITY);
            true
        }
        _ => false,
    }
}

pub const MANURE: f32 = 0.4;
pub const SEAWEED: f32 = 0.3;

/// Produit d'une récolte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Harvest {
    pub item: &'static str,
    pub qty: u32,
}

/// Récolte une culture mûre. Rendement = tirage × (0,5 + fertilité/2) × 3 sur cendre. La culture
/// annuelle laisse un champ vide et un peu moins fertile ; une vivace redevient à moitié mûre.
pub fn harvest(
    blocks: &mut dyn BlockAccess,
    fields: &FieldState,
    fb: &FarmBlocks,
    pos: IVec3,
    rng: &mut JavaRandom,
) -> Option<Harvest> {
    let mut f = fields.lock().unwrap();
    let plot = f.get_mut(&pos)?;
    if !plot.ripe() {
        return None;
    }
    let spec = plot.crop?.spec();
    let span = i32::from(spec.yield_[1] - spec.yield_[0]) + 1;
    let base = f32::from(spec.yield_[0]) + rng.next_int_bound(span) as f32;
    let ash = if matches!(plot.soil, Soil::Ash { .. }) {
        ASH_YIELD
    } else {
        1.0
    };
    let qty = (base * (0.5 + 0.5 * plot.fertility) * ash).round().max(1.0) as u32;
    plot.fertility = (plot.fertility - HARVEST_DRAIN).max(0.0);
    if spec.perennial {
        plot.growth = 0.5;
        blocks.set_block(pos.x, pos.y + 1, pos.z, fb.stage_block(spec.form, 1));
    } else {
        plot.crop = None;
        plot.growth = 0.0;
        blocks.set_block(pos.x, pos.y + 1, pos.z, fb.air);
    }
    Some(Harvest {
        item: spec.item,
        qty,
    })
}

// ---------------------------------------------------------------------------------------------
// Brûlis
// ---------------------------------------------------------------------------------------------

/// Abattis qui sèche : troncs abattus et avancement du séchage (en jours).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Slash {
    pub logs: u32,
    pub dry_days: f32,
}

impl Slash {
    pub fn dry(&self) -> bool {
        self.dry_days >= SLASH_DRY_DAYS
    }
}

pub type SlashState = Arc<Mutex<HashMap<IVec3, Slash>>>;

/// Ajoute `logs` troncs à l'abattis de `pos` ; ce qui est ajouté est frais : le séchage retombe à
/// la moyenne pondérée.
pub fn fell(slash: &SlashState, pos: IVec3, logs: u32) {
    let mut s = slash.lock().unwrap();
    let e = s.entry(pos).or_default();
    let total = e.logs + logs;
    if total > 0 {
        e.dry_days = e.dry_days * e.logs as f32 / total as f32;
    }
    e.logs = total;
}

/// Rayon du champ de cendre que donne un abattis.
pub fn burn_radius(logs: u32) -> i32 {
    (1 + logs as i32 / 6).clamp(1, BURN_MAX_RADIUS)
}

/// Raisons d'un brûlis refusé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurnError {
    NoSlash,
    NotDry,
}

/// Brûle l'abattis de `pos` : le sol labourable du disque devient cendre. `year_days` = jours de
/// jeu par an. Renvoie le nombre de cases.
pub fn burn(
    blocks: &mut dyn BlockAccess,
    fields: &FieldState,
    slash: &SlashState,
    fb: &FarmBlocks,
    pos: IVec3,
    year_days: f32,
) -> Result<usize, BurnError> {
    let s = *slash.lock().unwrap().get(&pos).ok_or(BurnError::NoSlash)?;
    if !s.dry() {
        return Err(BurnError::NotDry);
    }
    let r = burn_radius(s.logs);
    let mut n = 0;
    let mut f = fields.lock().unwrap();
    for dx in -r..=r {
        for dz in -r..=r {
            if dx * dx + dz * dz > r * r {
                continue;
            }
            let (x, z) = (pos.x + dx, pos.z + dz);
            if !blocks.is_chunk_loaded(x, z) {
                continue;
            }
            for y in (pos.y - 3..=pos.y + 2).rev() {
                let b = blocks.block(x, y, z);
                if fb.tillable.contains(&b) {
                    blocks.set_block(x, y, z, fb.ash);
                    // Le feu emporte ce qui poussait au-dessus.
                    blocks.set_block(x, y + 1, z, fb.air);
                    f.insert(
                        IVec3::new(x, y, z),
                        Plot::new(Soil::Ash {
                            days_left: ASH_YEARS * year_days,
                        }),
                    );
                    n += 1;
                    break;
                }
            }
        }
    }
    slash.lock().unwrap().remove(&pos);
    Ok(n)
}

// ---------------------------------------------------------------------------------------------
// Clôtures
// ---------------------------------------------------------------------------------------------

/// La case `pos` (le sol du champ) est-elle enfermée dans des clôtures `fence` (dans [`FENCE_RADIUS`]) ?
/// Un remplissage horizontal à hauteur de la plante ne doit pas s'échapper.
pub fn enclosed(blocks: &dyn BlockAccess, fence: BlockId, pos: IVec3) -> bool {
    let y = pos.y + 1;
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![(pos.x, pos.z)];
    while let Some((x, z)) = stack.pop() {
        if !seen.insert((x, z)) {
            continue;
        }
        if (x - pos.x).abs() > FENCE_RADIUS || (z - pos.z).abs() > FENCE_RADIUS {
            return false;
        }
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            if blocks.block(nx, y, nz) == fence {
                continue;
            }
            stack.push((nx, nz));
        }
    }
    true
}

/// Profils d'animaux qui broutent les champs.
const GRAZERS: [&str; 4] = ["betare", "hjord", "husdjur", "smavilt"];

// ---------------------------------------------------------------------------------------------
// Système
// ---------------------------------------------------------------------------------------------

/// Fait pousser les champs, sécher les abattis, vieillir la cendre et brouter les bêtes.
pub struct FarmSystem {
    fields: FieldState,
    slash: SlashState,
    blocks: Option<FarmBlocks>,
    timer: f32,
}

impl FarmSystem {
    pub fn new(fields: FieldState, slash: SlashState, reg: &BlockRegistry) -> Self {
        Self {
            fields,
            slash,
            blocks: FarmBlocks::from_registry(reg),
            timer: 0.0,
        }
    }
}

impl System for FarmSystem {
    fn name(&self) -> &'static str {
        "Farm"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        self.timer += dt;
        if self.timer < INTERVAL {
            return;
        }
        let step = std::mem::take(&mut self.timer);
        let (Some(fb), Some(env)) = (self.blocks.as_ref(), ctx.environment) else {
            return;
        };
        let day = ctx.config.day_length_seconds.max(1.0);
        let days = step / day;
        let year_days = (12 * ctx.config.days_per_month) as f32;
        let clock = Clock::of(ctx.config, None);

        // Abattis : ils sèchent par temps sec et hors gel.
        let dries = env.rain < 0.05
            && env.snow < 0.05
            && f64::from(env.air) > freezing_point();
        if dries {
            for s in self.slash.lock().unwrap().values_mut() {
                s.dry_days += days;
            }
        }

        // Bêtes en liberté : positions des brouteurs.
        let grazers: Vec<DVec3> = world
            .query_ids::<(&MobType, &Transform)>()
            .into_iter()
            .filter_map(|id| {
                let mob = world.get::<MobType>(id)?;
                let b = ctx.mobs.get(&mob.mob_id)?.behavior.as_deref()?;
                GRAZERS
                    .contains(&b)
                    .then(|| world.copy::<Transform>(id).map(|t| t.pos))
                    .flatten()
            })
            .collect();
        let mut fields = self.fields.lock().unwrap();
        let mut gone: Vec<IVec3> = Vec::new();
        for (&pos, plot) in fields.iter_mut() {
            if !ctx.blocks.is_chunk_loaded(pos.x, pos.z) {
                continue;
            }
            // Le joueur a cassé ou remplacé le sol : le champ n'existe plus.
            let soil_block = ctx.blocks.block(pos.x, pos.y, pos.z);
            if soil_block != fb.soil_block(plot.soil) {
                gone.push(pos);
                continue;
            }
            // Cendre et sol épuisé.
            match &mut plot.soil {
                Soil::Ash { days_left } => {
                    *days_left -= days;
                    if *days_left <= 0.0 {
                        plot.soil = Soil::Exhausted {
                            days_left: REFOREST_YEARS * year_days,
                        };
                        plot.crop = None;
                        plot.growth = 0.0;
                        ctx.blocks.set_block(pos.x, pos.y, pos.z, fb.exhausted);
                        ctx.blocks.set_block(pos.x, pos.y + 1, pos.z, fb.air);
                    }
                }
                Soil::Exhausted { days_left } => {
                    *days_left -= days;
                    if *days_left <= 0.0 {
                        ctx.blocks.set_block(pos.x, pos.y, pos.z, fb.reforest);
                        gone.push(pos);
                    }
                    continue;
                }
                Soil::Tilled => {}
            }
            let Some(crop) = plot.crop else { continue };
            let above = ctx.blocks.block(pos.x, pos.y + 1, pos.z);
            if !fb.is_crop(above) {
                // Plante arrachée.
                plot.crop = None;
                plot.growth = 0.0;
                continue;
            }
            let spec = crop.spec();

            // Brouteurs : une bête à moins d'une case, champ non clos.
            let eaten = grazers.iter().any(|g| {
                (g.x - (f64::from(pos.x) + 0.5)).abs() < 1.2
                    && (g.z - (f64::from(pos.z) + 0.5)).abs() < 1.2
                    && (g.y - f64::from(pos.y + 1)).abs() < 2.0
            }) && !enclosed(ctx.blocks, fb.fence, pos);
            if eaten {
                plot.crop = None;
                plot.growth = 0.0;
                ctx.blocks.set_block(pos.x, pos.y + 1, pos.z, fb.air);
                ctx.events.push(EcsEvent::CropWithered { pos });
                continue;
            }

            let cond = Conditions {
                hours: daylight_hours(clock.year_fraction, plot.latitude),
                air: env.air,
                rain: env.rain,
            };
            if frost_kills(spec, &cond) && plot.growth > 0.0 {
                plot.crop = None;
                plot.growth = 0.0;
                ctx.blocks.set_block(pos.x, pos.y + 1, pos.z, fb.withered);
                ctx.events.push(EcsEvent::CropWithered { pos });
                continue;
            }
            if plot.ripe() {
                continue;
            }
            let before = plot.stage();
            plot.growth =
                (plot.growth + growth_rate(spec, &cond, plot.fertility, plot.latitude) * days)
                    .min(1.0);
            if plot.stage() != before {
                let block = fb.stage_block(spec.form, plot.stage());
                ctx.blocks.set_block(pos.x, pos.y + 1, pos.z, block);
                ctx.events.push(EcsEvent::CropGrew {
                    pos: pos + IVec3::Y,
                    block,
                });
            }
        }
        for p in gone {
            fields.remove(&p);
        }
    }
}

/// Identifiant du bloc nommé `name` (`None` s'il n'existe pas dans ce registre).
pub fn block_id(reg: &BlockRegistry, name: &str) -> Option<BlockId> {
    let b = reg.get(name);
    (b.name == name).then_some(b.id)
}
