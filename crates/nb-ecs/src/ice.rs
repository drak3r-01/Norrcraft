//! Glace jouable (session 6.1, doc 04 §4.1 et §4.2).
//!
//! * **États** : [`IceKind`] lit le bloc (`nyis`, `is`, `tjockis`, `havsis`, `blais`) ; [`Load`]
//!   dit ce qui marche dessus ; [`IceKind::carries`] décide si la glace porte, et [`fragile`]
//!   en retire un rang près de l'eau libre (trous, chenaux de débâcle, rivières).
//! * **Chute** : [`IceSystem`] fait craquer la glace sous une charge qu'elle ne porte pas
//!   ([`Strain`], `EcsEvent::IceCrack`), puis la fait céder (`EcsEvent::IceBroke`).
//! * **Remontée** : [`climb_out`] hisse un nageur sur le bord ; sans poinçons (`isdubbar`) le
//!   bord casse deux fois sur trois.
//! * **Trous et pêche** : [`drill`] (tarière, `isborr`), lignes [`Line`] (`pimpel` à la main,
//!   `angeldon` dormante à drapeau) dans [`IceState`], senne sous la glace [`seine`] (`isnot`,
//!   deux trous, deux équipiers).
//! * **Récolte** : [`saw`] (scie à glace, `issag`) donne des `isblock` ; [`Icehouse`] les garde
//!   sous la sciure et dit combien il en reste en été.
//!
//! La débâcle elle-même est dans la surcouche saisonnière (`nb-world`, `is_channel`) : les
//! plaques et chenaux qu'elle pose sont ici de la glace *fragile*.

use std::sync::{Arc, Mutex};

use nb_core::glam::{DVec3, IVec3};
use nb_core::JavaRandom;
use nb_data::{BlockId, BlockRegistry, ItemId, MobMedium};

use crate::blocks::BlockAccess;
use crate::components::{EntityId, InventoryComp, MobType, Player, Transform, Velocity};
use crate::input::GameAction;
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;

/// Secondes entre le craquement d'avertissement et la rupture sous un marcheur.
pub const COLLAPSE_SECS: f32 = 1.6;
/// Idem sous un animal lourd : presque immédiat.
pub const COLLAPSE_HEAVY_SECS: f32 = 0.5;
/// Secondes sans charge pour que la tension retombe.
pub const RELAX_SECS: f32 = 1.0;
/// Rayon (cases) de la glace qui cède autour du pied.
pub const COLLAPSE_RADIUS: i32 = 1;
/// Rayon (cases) dans lequel l'eau libre rend la glace fragile.
pub const FRAGILE_RADIUS: i32 = 2;
/// Chance de remonter sur le bord sans poinçons.
pub const CLIMB_CHANCE_BARE: f32 = 1.0 / 3.0;
/// Délai entre deux tentatives de remontée.
pub const CLIMB_COOLDOWN: f32 = 0.8;
/// Masse (largeur × largeur × hauteur) en dessous de laquelle un animal est léger.
pub const LIGHT_VOLUME: f32 = 0.35;
/// Masse à partir de laquelle un animal est lourd.
pub const HEAVY_VOLUME: f32 = 1.6;
/// Fenêtre (s) pendant laquelle un poisson reste à la touche d'un pimpel.
pub const BITE_WINDOW: f32 = 3.0;

// ---------------------------------------------------------------------------------------------
// États de la glace
// ---------------------------------------------------------------------------------------------

/// Les glaces du doc 04 §4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IceKind {
    /// `nyis` : sombre, lisse, elle chante.
    New,
    /// `is` : portante (kärnis).
    Bearing,
    /// `tjockis` : épaisse, blanche.
    Thick,
    /// `havsis` : de mer, plus fragile près des courants.
    Sea,
    /// `blais` : banquise, porte tout.
    Pack,
}

/// Ce qui pèse sur la glace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Load {
    /// Un joueur à plat ventre (accroupi tant que ramper n'existe pas) ou à skis.
    Prone,
    /// Un joueur debout, un renne.
    Walker,
    /// Un petit animal.
    Light,
    /// Un animal lourd (cheval, élan).
    Heavy,
    /// Un traîneau vide.
    Sledge,
    /// Un traîneau chargé (6.2).
    LoadedSledge,
}

impl IceKind {
    /// Glace du bloc `id`, `None` si ce n'en est pas.
    pub fn of(reg: &BlockRegistry, id: BlockId) -> Option<Self> {
        Some(match reg.from_id(id).name.as_str() {
            "nyis" => Self::New,
            "is" => Self::Bearing,
            "tjockis" => Self::Thick,
            "havsis" => Self::Sea,
            "blais" => Self::Pack,
            _ => return None,
        })
    }

    /// Glace d'un rang moindre, pour les abords de l'eau libre.
    pub fn weakened(self) -> Self {
        match self {
            Self::Thick => Self::Bearing,
            Self::Bearing | Self::Sea | Self::New => Self::New,
            Self::Pack => Self::Pack,
        }
    }

    /// La glace porte-t-elle cette charge ? `fragile` : elle perd un rang.
    pub fn carries(self, load: Load, fragile: bool) -> bool {
        let k = if fragile { self.weakened() } else { self };
        match k {
            Self::Pack | Self::Thick => true,
            Self::Bearing | Self::Sea => !matches!(load, Load::Heavy | Load::LoadedSledge),
            Self::New => matches!(load, Load::Prone | Load::Light),
        }
    }
}

/// Glace fragile en `pos` : de l'eau libre à moins de [`FRAGILE_RADIUS`] cases (trou, chenal,
/// embouchure) au niveau de la glace.
pub fn fragile(blocks: &dyn BlockAccess, reg: &BlockRegistry, pos: IVec3) -> bool {
    let water = reg.ids().water;
    for dx in -FRAGILE_RADIUS..=FRAGILE_RADIUS {
        for dz in -FRAGILE_RADIUS..=FRAGILE_RADIUS {
            if (dx != 0 || dz != 0) && blocks.block(pos.x + dx, pos.y, pos.z + dz) == water {
                return true;
            }
        }
    }
    false
}

/// La glace en `pos` porte-t-elle `load` ? `true` si ce n'est pas de la glace.
pub fn bears(blocks: &dyn BlockAccess, reg: &BlockRegistry, pos: IVec3, load: Load) -> bool {
    match IceKind::of(reg, blocks.block_at(pos)) {
        Some(k) => k.carries(load, fragile(blocks, reg, pos)),
        None => true,
    }
}

/// Charge d'une entité : joueur (accroupi = à plat ventre) ou animal selon sa masse.
pub fn load_of(world: &EntityWorld, ctx: &TickCtx<'_>, id: EntityId) -> Option<Load> {
    if world.has::<Player>(id) {
        return Some(if ctx.input.pressed.contains(GameAction::Crouch) {
            Load::Prone
        } else {
            Load::Walker
        });
    }
    let mob = world.get::<MobType>(id)?;
    let c = ctx.mobs.get(&mob.mob_id)?;
    // Les animaux de banquise, de l'air et de l'eau ne pèsent pas sur la glace.
    if c.medium != MobMedium::Land && c.medium != MobMedium::Amphibian {
        return None;
    }
    let vol = c.width_x * c.width_z * c.height;
    Some(if vol < LIGHT_VOLUME {
        Load::Light
    } else if vol >= HEAVY_VOLUME {
        Load::Heavy
    } else {
        Load::Walker
    })
}

// ---------------------------------------------------------------------------------------------
// Rupture et remontée
// ---------------------------------------------------------------------------------------------

/// Tension d'une entité sur une glace qui ne la porte pas.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Strain {
    pub secs: f32,
    /// Temps écoulé hors charge excessive.
    pub relaxed: f32,
}

/// Fait céder la glace autour de `at` : les cellules de glace (hors banquise) deviennent de l'eau.
/// Renvoie le nombre de cellules rompues.
pub fn collapse(blocks: &mut dyn BlockAccess, reg: &BlockRegistry, at: IVec3) -> usize {
    let water = reg.ids().water;
    let mut n = 0;
    for dx in -COLLAPSE_RADIUS..=COLLAPSE_RADIUS {
        for dz in -COLLAPSE_RADIUS..=COLLAPSE_RADIUS {
            let p = at + IVec3::new(dx, 0, dz);
            match IceKind::of(reg, blocks.block_at(p)) {
                Some(IceKind::Pack) | None => {}
                Some(_) => {
                    blocks.set_block(p.x, p.y, p.z, water);
                    n += 1;
                }
            }
        }
    }
    n
}

/// Résultat d'une tentative de remontée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Climb {
    /// Hissé sur le bord, en `to` (dessus du bloc de glace).
    Out { to: IVec3 },
    /// Le bord a cassé sous la main.
    EdgeBroke { at: IVec3 },
    /// Aucun bord de glace à portée.
    NoEdge,
}

/// Tente de remonter sur la glace depuis `feet` (cellule d'eau, hauteur de la poitrine). Un bord
/// est un bloc de glace voisin dont le dessus est libre. Avec `picks` (poinçons), on réussit
/// toujours ; sans, `1/3` seulement et le bord casse sinon.
pub fn climb_out(
    blocks: &mut dyn BlockAccess,
    reg: &BlockRegistry,
    feet: IVec3,
    picks: bool,
    rng: &mut JavaRandom,
) -> Climb {
    let ids = *reg.ids();
    const DIRS: [IVec3; 4] = [
        IVec3::new(1, 0, 0),
        IVec3::new(-1, 0, 0),
        IVec3::new(0, 0, 1),
        IVec3::new(0, 0, -1),
    ];
    for d in DIRS {
        // Poitrine à `feet`, le bord peut être à ce niveau ou juste au-dessous.
        for dy in [0, -1] {
            let p = feet + d + IVec3::new(0, dy, 0);
            let Some(k) = IceKind::of(reg, blocks.block_at(p)) else {
                continue;
            };
            let up = p + IVec3::Y;
            let air_above = blocks.block_at(up) == ids.air && blocks.block_at(up + IVec3::Y) == ids.air;
            if !air_above {
                continue;
            }
            if picks || rng.next_float() < CLIMB_CHANCE_BARE {
                return Climb::Out { to: up };
            }
            if k != IceKind::Pack {
                blocks.set_block(p.x, p.y, p.z, ids.water);
            }
            return Climb::EdgeBroke { at: p };
        }
    }
    Climb::NoEdge
}

// ---------------------------------------------------------------------------------------------
// Trous, scie, pêche
// ---------------------------------------------------------------------------------------------

/// Outils de la glace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IceTool {
    /// `isborr`, la tarière.
    Auger,
    /// `issag`, la scie à glace.
    Saw,
    /// `pimpel`, la canne courte.
    Jig,
    /// `angeldon`, la ligne dormante à drapeau.
    TipUp,
    /// `isnot`, la senne.
    Seine,
    /// `isdubbar`, les poinçons.
    Picks,
}

impl IceTool {
    pub fn from_item(name: &str) -> Option<Self> {
        Some(match name {
            "isborr" => Self::Auger,
            "issag" => Self::Saw,
            "pimpel" => Self::Jig,
            "angeldon" => Self::TipUp,
            "isnot" => Self::Seine,
            "isdubbar" => Self::Picks,
            _ => return None,
        })
    }
}

/// Un trou de pêche : de l'eau à l'air libre, entourée de glace sur au moins trois côtés.
pub fn is_hole(blocks: &dyn BlockAccess, reg: &BlockRegistry, pos: IVec3) -> bool {
    let ids = reg.ids();
    if blocks.block_at(pos) != ids.water || blocks.block_at(pos + IVec3::Y) != ids.air {
        return false;
    }
    let around = [
        IVec3::X,
        IVec3::NEG_X,
        IVec3::Z,
        IVec3::NEG_Z,
    ]
    .into_iter()
    .filter(|d| IceKind::of(reg, blocks.block_at(pos + *d)).is_some())
    .count();
    around >= 3
}

/// Perce la glace en `pos` (tarière) : il faut de l'eau dessous et de l'air dessus, et une glace
/// qui n'est pas la banquise. Le trou regèle de lui-même au prochain pas de saison (c'est de
/// l'eau comme dans la génération).
pub fn drill(blocks: &mut dyn BlockAccess, reg: &BlockRegistry, pos: IVec3) -> bool {
    let ids = *reg.ids();
    match IceKind::of(reg, blocks.block_at(pos)) {
        Some(IceKind::Pack) | None => return false,
        Some(_) => {}
    }
    if blocks.block_at(pos - IVec3::Y) != ids.water || blocks.block_at(pos + IVec3::Y) != ids.air {
        return false;
    }
    blocks.set_block(pos.x, pos.y, pos.z, ids.water);
    true
}

/// Scie un bloc de glace en `pos` : il faut une glace assez épaisse (pas la neuve). Rend les
/// `isblock` récoltés (deux pour l'épaisse). Le bloc devient de l'eau.
pub fn saw(
    blocks: &mut dyn BlockAccess,
    reg: &BlockRegistry,
    pos: IVec3,
) -> Option<(&'static str, u32)> {
    let ids = *reg.ids();
    let n = match IceKind::of(reg, blocks.block_at(pos))? {
        IceKind::New | IceKind::Pack => return None,
        IceKind::Bearing | IceKind::Sea => 1,
        IceKind::Thick => 2,
    };
    blocks.set_block(pos.x, pos.y, pos.z, ids.water);
    Some(("isblock", n))
}

/// Les poissons des lacs gelés : `(objet, poids, mois ouverts)` ; mois 0 = janvier.
const FISH: [(&str, u32, [bool; 12]); 5] = [
    ("abborre", 40, [true; 12]),
    ("gadda", 25, [true; 12]),
    ("sik", 20, [true; 12]),
    (
        "roding",
        12,
        [true, true, true, true, false, false, false, false, false, true, true, true],
    ),
    (
        "harr",
        8,
        [true, true, true, false, false, false, false, false, true, true, true, true],
    ),
];

/// Une prise sous la glace au mois `month` : `(objet, nombre)`.
pub fn catch(month: usize, rng: &mut JavaRandom) -> (&'static str, u32) {
    let total: u32 = FISH.iter().filter(|f| f.2[month % 12]).map(|f| f.1).sum();
    let mut r = rng.next_int_bound(total as i32) as u32;
    for (name, w, open) in FISH {
        if !open[month % 12] {
            continue;
        }
        if r < w {
            let n = if name == "abborre" && rng.next_float() < 0.35 {
                2
            } else {
                1
            };
            return (name, n);
        }
        r -= w;
    }
    ("abborre", 1)
}

/// Senne sous la glace (`isnot`) : deux trous, deux équipiers, 4 à 24 cases d'écart. Rend une
/// grosse prise, plus grosse quand les trous sont éloignés.
pub fn seine(
    blocks: &dyn BlockAccess,
    reg: &BlockRegistry,
    a: IVec3,
    b: IVec3,
    crew: u32,
    month: usize,
    rng: &mut JavaRandom,
) -> Option<Vec<(&'static str, u32)>> {
    if crew < 2 || !is_hole(blocks, reg, a) || !is_hole(blocks, reg, b) {
        return None;
    }
    let d = ((a.x - b.x).abs() + (a.z - b.z).abs()) as u32;
    if !(4..=24).contains(&d) {
        return None;
    }
    let mut out: Vec<(&'static str, u32)> = Vec::new();
    for _ in 0..(4 + d / 3) {
        let (name, n) = catch(month, rng);
        match out.iter_mut().find(|o| o.0 == name) {
            Some(o) => o.1 += n,
            None => out.push((name, n)),
        }
    }
    Some(out)
}

/// Sorte de ligne posée dans un trou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Canne courte tenue : il faut ferrer pendant la fenêtre de touche.
    Jig,
    /// Ligne dormante : un drapeau se lève, la prise attend qu'on la relève.
    TipUp,
}

/// Une ligne dans un trou.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub id: u32,
    pub hole: IVec3,
    pub kind: LineKind,
    /// Secondes avant la prochaine touche.
    pub wait: f32,
    /// Un poisson est à la ligne (drapeau levé / fenêtre ouverte).
    pub bite: bool,
    /// Temps de fenêtre restant (canne tenue).
    pub window: f32,
}

/// Lignes posées sur la glace.
#[derive(Debug, Default)]
pub struct IceState {
    lines: Vec<Line>,
    next: u32,
}

/// Partagé entre le jeu et le système.
pub type IceHandle = Arc<Mutex<IceState>>;

impl IceState {
    /// Pose une ligne dans le trou `hole`.
    pub fn set_line(&mut self, kind: LineKind, hole: IVec3, rng: &mut JavaRandom) -> u32 {
        self.next += 1;
        let (lo, span) = match kind {
            LineKind::Jig => (5.0, 20.0),
            LineKind::TipUp => (40.0, 140.0),
        };
        self.lines.push(Line {
            id: self.next,
            hole,
            kind,
            wait: lo + rng.next_float() * span,
            bite: false,
            window: 0.0,
        });
        self.next
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn line(&self, id: u32) -> Option<&Line> {
        self.lines.iter().find(|l| l.id == id)
    }

    /// Ferre une touche (canne) ou relève une ligne dormante : la prise si un poisson y est.
    /// Une canne ferrée trop tôt effraie le poisson (l'attente repart) ; la dormante est
    /// relevée et reposée. `None` s'il n'y a pas de prise.
    pub fn reel(
        &mut self,
        id: u32,
        month: usize,
        rng: &mut JavaRandom,
    ) -> Option<(&'static str, u32)> {
        let l = self.lines.iter_mut().find(|l| l.id == id)?;
        let (lo, span) = match l.kind {
            LineKind::Jig => (5.0, 20.0),
            LineKind::TipUp => (40.0, 140.0),
        };
        let had = l.bite;
        l.bite = false;
        l.window = 0.0;
        l.wait = lo + rng.next_float() * span;
        had.then(|| catch(month, rng))
    }

    /// Retire une ligne.
    pub fn remove(&mut self, id: u32) -> bool {
        let n = self.lines.len();
        self.lines.retain(|l| l.id != id);
        self.lines.len() != n
    }

    /// Fait avancer les lignes de `dt` secondes ; renvoie les trous où un poisson vient de mordre.
    /// Une ligne dont le trou a regelé est retirée (chunk non chargé : conservée).
    pub fn advance(
        &mut self,
        dt: f32,
        blocks: &dyn BlockAccess,
        reg: &BlockRegistry,
        rng: &mut JavaRandom,
    ) -> Vec<IVec3> {
        let mut bites = Vec::new();
        self.lines.retain_mut(|l| {
            if blocks.is_chunk_loaded(l.hole.x, l.hole.z) && !is_hole(blocks, reg, l.hole) {
                return false;
            }
            if l.bite {
                if l.kind == LineKind::Jig {
                    l.window -= dt;
                    if l.window <= 0.0 {
                        // Raté : le poisson part.
                        l.bite = false;
                        l.wait = 5.0 + rng.next_float() * 20.0;
                    }
                }
                return true;
            }
            l.wait -= dt;
            if l.wait <= 0.0 {
                l.bite = true;
                l.window = BITE_WINDOW;
                bites.push(l.hole);
            }
            true
        });
        bites
    }
}

// ---------------------------------------------------------------------------------------------
// Glacière
// ---------------------------------------------------------------------------------------------

/// Une glacière (ishus) : des blocs de glace conservés sous la sciure pour l'été (§6.3).
#[derive(Debug, Clone, PartialEq)]
pub struct Icehouse {
    /// Blocs restants (fractionnaires : la fonte est continue).
    pub blocks: f32,
    /// Recouvert de sciure.
    pub sawdust: bool,
}

impl Icehouse {
    pub fn new(blocks: u32, sawdust: bool) -> Self {
        Self {
            blocks: blocks as f32,
            sawdust,
        }
    }

    /// Part qui fond par jour, `warmth` entre 0 (gel) et 1 (canicule d'été).
    pub fn melt_per_day(sawdust: bool, warmth: f32) -> f32 {
        let w = warmth.clamp(0.0, 1.0);
        if sawdust {
            0.0015 + 0.0035 * w
        } else {
            0.02 + 0.05 * w
        }
    }

    /// Laisse passer `days` jours à chaleur `warmth`.
    pub fn tick_days(&mut self, days: f32, warmth: f32) {
        let k = Self::melt_per_day(self.sawdust, warmth);
        self.blocks *= (1.0 - k).powf(days.max(0.0));
    }

    /// Prend jusqu'à `n` blocs entiers.
    pub fn take(&mut self, n: u32) -> u32 {
        let got = (self.blocks.floor() as u32).min(n);
        self.blocks -= got as f32;
        got
    }
}

// ---------------------------------------------------------------------------------------------
// Système
// ---------------------------------------------------------------------------------------------

/// Craquements, ruptures, remontée et lignes de pêche, une fois par tick.
pub struct IceSystem {
    handle: IceHandle,
    /// Poinçons (`isdubbar`) : si le joueur en porte, la remontée réussit toujours.
    picks: Option<ItemId>,
    climb_cd: f32,
}

impl IceSystem {
    pub fn new(handle: IceHandle, picks: Option<ItemId>) -> Self {
        Self {
            handle,
            picks,
            climb_cd: 0.0,
        }
    }

    pub fn handle(&self) -> IceHandle {
        self.handle.clone()
    }
}

fn cell(pos: DVec3) -> IVec3 {
    IVec3::new(pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32)
}

impl System for IceSystem {
    fn name(&self) -> &'static str {
        "Ice"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        // Seules les chunks qui ont de la glace nous intéressent : on regarde le bloc sous les pieds.
        let ids = *ctx.block_reg.ids();
        for id in world.query_ids::<(&Transform, &Velocity)>() {
            let (pos, grounded) = {
                let (Some(t), Some(v)) = (world.get::<Transform>(id), world.get::<Velocity>(id))
                else {
                    continue;
                };
                (t.pos, v.grounded)
            };
            let below = cell(pos - DVec3::new(0.0, 0.1, 0.0));
            if !ctx.blocks.is_chunk_loaded(below.x, below.z) {
                continue;
            }
            let on_ice = grounded
                && IceKind::of(ctx.block_reg, ctx.blocks.block_at(below)).is_some();
            if !on_ice {
                if let Some(s) = world.get_mut::<Strain>(id) {
                    s.relaxed += dt;
                    if s.relaxed >= RELAX_SECS {
                        world.remove::<Strain>(id);
                    }
                }
                continue;
            }
            let Some(load) = load_of(world, ctx, id) else {
                continue;
            };
            if bears(&*ctx.blocks, ctx.block_reg, below, load) {
                world.remove::<Strain>(id);
                continue;
            }
            let limit = if load == Load::Heavy {
                COLLAPSE_HEAVY_SECS
            } else {
                COLLAPSE_SECS
            };
            let first = !world.has::<Strain>(id);
            if first {
                world.insert(id, Strain::default());
                ctx.events.push(EcsEvent::IceCrack { pos: below });
            }
            let Some(s) = world.get_mut::<Strain>(id) else {
                continue;
            };
            s.secs += dt;
            s.relaxed = 0.0;
            if s.secs >= limit {
                world.remove::<Strain>(id);
                if collapse(ctx.blocks, ctx.block_reg, below) > 0 {
                    ctx.events.push(EcsEvent::IceBroke { pos: below });
                }
            }
        }

        // Remontée du joueur tombé à l'eau : saut tenu près d'un bord de glace.
        self.climb_cd = (self.climb_cd - dt).max(0.0);
        if self.climb_cd == 0.0 && ctx.input.pressed.contains(GameAction::Jump) {
            if let Some(&pid) = world.query_ids::<(&Player, &Transform)>().first() {
                let pos = world.get::<Transform>(pid).map(|t| t.pos);
                if let Some(pos) = pos {
                    let chest = cell(pos + DVec3::new(0.0, 0.9, 0.0));
                    if ctx.blocks.block_at(chest) == ids.water
                        || ctx.blocks.block_at(cell(pos)) == ids.water
                    {
                        let picks = self.picks.is_some_and(|it| {
                            world
                                .get::<InventoryComp>(pid)
                                .is_some_and(|inv| inv.0.count_of(it) > 0)
                        });
                        let feet = chest;
                        let r = climb_out(ctx.blocks, ctx.block_reg, feet, picks, &mut ctx.rngs.ambient);
                        match r {
                            Climb::Out { to } => {
                                if let Some(t) = world.get_mut::<Transform>(pid) {
                                    t.pos = DVec3::new(
                                        f64::from(to.x) + 0.5,
                                        f64::from(to.y),
                                        f64::from(to.z) + 0.5,
                                    );
                                }
                                if let Some(v) = world.get_mut::<Velocity>(pid) {
                                    v.v = nb_core::glam::Vec3::ZERO;
                                }
                                self.climb_cd = CLIMB_COOLDOWN;
                            }
                            Climb::EdgeBroke { at } => {
                                ctx.events.push(EcsEvent::IceBroke { pos: at });
                                self.climb_cd = CLIMB_COOLDOWN;
                            }
                            Climb::NoEdge => {}
                        }
                    }
                }
            }
        }

        // Lignes de pêche.
        let bites = {
            let mut st = self.handle.lock().expect("ice state");
            if st.lines.is_empty() {
                Vec::new()
            } else {
                st.advance(dt, &*ctx.blocks, ctx.block_reg, &mut ctx.rngs.ambient)
            }
        };
        for h in bites {
            ctx.events.push(EcsEvent::FishBite {
                pos: DVec3::new(f64::from(h.x) + 0.5, f64::from(h.y) + 0.9, f64::from(h.z) + 0.5),
            });
        }
    }
}
