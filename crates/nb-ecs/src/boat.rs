//! Bateaux, voile et vent, mouillage, fuite et mise au sec (session 6.3, doc 04 §5.1 et §5.2).
//!
//! * **Bateaux** : [`BoatKind`] (radeau, eka, snipa, knarr, långskepp) avec équipage, coffres,
//!   bétail, propulsion (perche, rames, voile aurique ou carrée).
//! * **Voile** : [`sail_efficiency`] et [`sail_speed`] : nulle à moins de 45° du lit du vent
//!   (70° pour la voile carrée), maximale au largue, un peu moins vent arrière.
//! * **Mer** : [`swell`] naît du vent et de la longueur d'eau libre qu'il a parcourue
//!   ([`fetch`]) : pas de houle au fond d'un fjord.
//! * **Coque** : [`Boat::water`] monte par la fuite d'une coque non goudronnée et par la houle ;
//!   on écope avec « saut », au-delà de 1 le bateau coule.
//! * **Hiver** : une coque prise dans la glace est écrasée ; on tire les petits bateaux au sec
//!   (`ashore`), les gros restent à quai.
//! * **Morue d'hiver** : [`bottom_fishing`], séchage sur claies [`Rack`] / [`RackSystem`] :
//!   le stockfisch ne se gâte jamais ([`NEVER_SPOILS`]).
//!
//! [`BoatSystem`] est inséré après `PlayerInputSystem` : un joueur embarqué ([`Aboard`]) est
//! porté par le bateau, dont il règle la barre (gauche, droite), les rames (avancer, reculer),
//! la voile (sprint), le mouillage (clic droit), l'écope (saut), et qu'il quitte en s'accroupissant.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use nb_core::glam::{DVec3, IVec3};
use nb_data::{BlockId, BlockRegistry};

use crate::blocks::BlockAccess;
use crate::components::{EntityId, Player, Transform, Velocity};
use crate::glide::ground_top;
use crate::ice::IceKind;
use crate::input::GameAction;
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;

/// Blocs/s qu'un vent de module 1 pousse.
pub const WIND_BLOCKS: f32 = 12.0;
/// Distance (blocs) pour monter à bord.
pub const BOARD_REACH: f64 = 2.5;
/// Vitesse de halage à terre (blocs/s).
pub const HAUL_SPEED: f32 = 1.2;
/// Vitesse au-dessus de laquelle on ne s'échoue pas doucement : choc de coque.
pub const BEACH_MAX_SPEED: f32 = 1.5;
/// Longueur d'eau libre (blocs) donnant la houle complète.
pub const FULL_FETCH: f32 = 48.0;
/// Fuite par seconde d'une coque goudronnée.
pub const LEAK_TARRED: f32 = 0.001;
/// Fuite par seconde d'une coque nue (environ 80 s pour couler sans écoper).
pub const LEAK_BARE: f32 = 0.012;
/// Eau écopée par seconde.
pub const BAIL_RATE: f32 = 0.06;
/// Écrasement par seconde d'une coque prise dans la glace.
pub const ICE_CRUSH: f32 = 0.015;
/// Objets qui ne se gâtent jamais (lus par la conservation, session 6.5).
pub const NEVER_SPOILS: &[&str] = &["stockfisk"];

// ---------------------------------------------------------------------------------------------
// Bateaux
// ---------------------------------------------------------------------------------------------

/// Les bateaux du doc 04 §5.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoatKind {
    /// Radeau de rondins : une perche.
    Raft,
    /// Eka : barque à clins, deux rameurs.
    Eka,
    /// Snipa : voile aurique et rames.
    Snipa,
    /// Knarr : navire de charge à voile carrée.
    Knarr,
    /// Långskepp : voile et six rames, le plus rapide.
    Longship,
}

/// Gréement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sail {
    /// Voile aurique : remonte à 45° du vent.
    Gaff,
    /// Voile carrée : 70° du vent, excellente aux allures portantes.
    Square,
}

impl Sail {
    /// Angle mort (radians) de part et d'autre du lit du vent.
    pub fn dead_zone(self) -> f32 {
        match self {
            Self::Gaff => 45f32.to_radians(),
            Self::Square => 70f32.to_radians(),
        }
    }

    /// Angle (cap/provenance du vent) du rendement maximal : le largue.
    pub fn peak(self) -> f32 {
        match self {
            Self::Gaff => 120f32.to_radians(),
            Self::Square => 150f32.to_radians(),
        }
    }
}

impl BoatKind {
    pub const ALL: [BoatKind; 5] = [
        Self::Raft,
        Self::Eka,
        Self::Snipa,
        Self::Knarr,
        Self::Longship,
    ];

    /// Nom de l'objet qui le met à l'eau.
    pub fn item(self) -> &'static str {
        match self {
            Self::Raft => "flotte",
            Self::Eka => "eka",
            Self::Snipa => "snipa",
            Self::Knarr => "knarr",
            Self::Longship => "langskepp",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.item() == name || k.label() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Raft => "raft",
            Self::Eka => "eka",
            Self::Snipa => "snipa",
            Self::Knarr => "knarr",
            Self::Longship => "longship",
        }
    }

    /// Équipage maximal.
    pub fn crew(self) -> u8 {
        match self {
            Self::Raft => 1,
            Self::Eka => 2,
            Self::Snipa => 3,
            Self::Knarr => 6,
            Self::Longship => 8,
        }
    }

    /// Coffres embarqués.
    pub fn chests(self) -> u8 {
        match self {
            Self::Raft => 0,
            Self::Eka => 1,
            Self::Snipa => 3,
            Self::Knarr => 8,
            Self::Longship => 2,
        }
    }

    /// Le bateau transporte-t-il du bétail ?
    pub fn livestock(self) -> bool {
        self == Self::Knarr
    }

    /// Vitesse (blocs/s) à la perche ou aux rames, pleine force.
    pub fn oar_speed(self) -> f32 {
        match self {
            Self::Raft => 1.6,
            Self::Eka => 3.2,
            Self::Snipa => 3.0,
            Self::Knarr => 0.0,
            Self::Longship => 4.5,
        }
    }

    pub fn sail(self) -> Option<Sail> {
        match self {
            Self::Snipa => Some(Sail::Gaff),
            Self::Knarr | Self::Longship => Some(Sail::Square),
            _ => None,
        }
    }

    /// Puissance du gréement (part du vent transformée en avance).
    pub fn sail_power(self) -> f32 {
        match self {
            Self::Snipa => 0.8,
            Self::Knarr => 0.7,
            Self::Longship => 1.0,
            _ => 0.0,
        }
    }

    /// Vitesse maximale (blocs/s), rames et voile ensemble.
    pub fn top_speed(self) -> f32 {
        match self {
            Self::Raft => 1.6,
            Self::Eka => 3.4,
            Self::Snipa => 8.0,
            Self::Knarr => 7.5,
            Self::Longship => 11.0,
        }
    }

    /// Constante de temps (s) de l'élan : les lourds répondent lentement.
    pub fn inertia(self) -> f32 {
        match self {
            Self::Raft => 0.8,
            Self::Eka => 0.9,
            Self::Snipa => 1.4,
            Self::Knarr => 2.4,
            Self::Longship => 1.8,
        }
    }

    /// Un radeau de rondins ne prend pas l'eau.
    pub fn leaks(self) -> bool {
        self != Self::Raft
    }

    /// Exposition à la houle (les grandes coques sont plus marines).
    pub fn swell_exposure(self) -> f32 {
        match self {
            Self::Raft => 0.5,
            Self::Eka => 1.2,
            Self::Snipa => 1.0,
            Self::Knarr => 0.6,
            Self::Longship => 0.5,
        }
    }

    /// Peut-il être tiré sur la grève ? Knarr et långskepp restent à quai.
    pub fn haulable(self) -> bool {
        matches!(self, Self::Raft | Self::Eka | Self::Snipa)
    }
}

/// Composant du bateau (avec `Transform` ; le cap est `Transform::yaw` en degrés, comme la
/// caméra : l'avant est `(sin yaw, −cos yaw)`).
#[derive(Debug, Clone)]
pub struct Boat {
    pub kind: BoatKind,
    /// Vitesse le long du cap (blocs/s).
    pub speed: f32,
    /// Voile établie.
    pub sail: bool,
    pub anchored: bool,
    /// Tiré au sec.
    pub ashore: bool,
    /// Coque goudronnée.
    pub tarred: bool,
    /// Eau dans la coque, de 0 à 1.
    pub water: f32,
    /// Charge de 0 (vide) à 1 (pleine).
    pub load: f32,
    pub pilot: Option<EntityId>,
    swamped: bool,
    haul: f32,
}

impl Boat {
    pub fn new(kind: BoatKind, tarred: bool) -> Self {
        Self {
            kind,
            speed: 0.0,
            sail: false,
            anchored: false,
            ashore: false,
            tarred,
            water: 0.0,
            load: 0.0,
            pilot: None,
            swamped: false,
            haul: 0.0,
        }
    }
}

/// Le joueur est à bord de ce bateau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aboard(pub EntityId);

// ---------------------------------------------------------------------------------------------
// Voile et mer
// ---------------------------------------------------------------------------------------------

/// Cap `yaw` (degrés) → vecteur avant `(x, z)`.
pub fn heading(yaw_deg: f32) -> (f32, f32) {
    let y = yaw_deg.to_radians();
    (y.sin(), -y.cos())
}

/// Angle (0 à π) entre le cap et la provenance du vent : 0, le vent debout ; π, vent arrière.
pub fn wind_angle(yaw_deg: f32, wind_dir: [f32; 2]) -> f32 {
    let (hx, hz) = heading(yaw_deg);
    let dot = -(hx * wind_dir[0] + hz * wind_dir[1]);
    dot.clamp(-1.0, 1.0).acos()
}

/// Rendement de la voile (0 à 1) à l'angle `a` du vent : rien dans l'angle mort, maximal au
/// largue, trois quarts vent arrière.
pub fn sail_efficiency(sail: Sail, a: f32) -> f32 {
    let (dead, peak) = (sail.dead_zone(), sail.peak());
    if a <= dead {
        return 0.0;
    }
    if a < peak {
        let t = (a - dead) / (peak - dead);
        return t * t * (3.0 - 2.0 * t);
    }
    1.0 - 0.25 * ((a - peak) / (std::f32::consts::PI - peak)).clamp(0.0, 1.0)
}

/// Vitesse (blocs/s) que la voile donne au cap `yaw_deg` dans un vent `wind_dir`/`wind_speed`.
pub fn sail_speed(kind: BoatKind, yaw_deg: f32, wind_dir: [f32; 2], wind_speed: f32) -> f32 {
    let Some(sail) = kind.sail() else { return 0.0 };
    let eff = sail_efficiency(sail, wind_angle(yaw_deg, wind_dir));
    (wind_speed * WIND_BLOCKS * eff * kind.sail_power()).min(kind.top_speed())
}

/// Houle (0 à 1) d'un vent de module `wind` après `fetch` blocs d'eau libre.
pub fn swell(wind: f32, fetch: f32) -> f32 {
    wind.clamp(0.0, 1.0) * (fetch / FULL_FETCH).clamp(0.0, 1.0)
}

/// Cellule d'eau libre (surface) de la colonne, cherchée autour de `near_y` ; renvoie la
/// hauteur de la surface (dessus de la cellule d'eau).
pub fn water_top(blocks: &dyn BlockAccess, water: BlockId, x: i32, near_y: i32, z: i32) -> Option<i32> {
    ((near_y - 2)..=(near_y + 1)).rev().find_map(|y| {
        (blocks.block(x, y, z) == water && blocks.block(x, y + 1, z) != water).then_some(y + 1)
    })
}

/// Longueur (blocs, au plus 64) d'eau libre en remontant le vent depuis `pos`.
pub fn fetch(blocks: &dyn BlockAccess, water: BlockId, pos: DVec3, wind_dir: [f32; 2]) -> f32 {
    let near = pos.y.floor() as i32;
    let mut n = 0;
    for i in 1..=64 {
        let x = (pos.x - f64::from(wind_dir[0]) * f64::from(i)).floor() as i32;
        let z = (pos.z - f64::from(wind_dir[1]) * f64::from(i)).floor() as i32;
        if water_top(blocks, water, x, near, z).is_none() {
            break;
        }
        n = i;
    }
    n as f32
}

// ---------------------------------------------------------------------------------------------
// Milieu sous la proue
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Medium {
    Water(f64),
    Land(f64),
    Blocked,
}

fn medium(blocks: &dyn BlockAccess, reg: &BlockRegistry, x: f64, y: f64, z: f64) -> Medium {
    let (cx, cz) = (x.floor() as i32, z.floor() as i32);
    if !blocks.is_chunk_loaded(cx, cz) {
        return Medium::Blocked;
    }
    let water = reg.ids().water;
    let near = y.floor() as i32;
    if let Some(top) = water_top(blocks, water, cx, near, cz) {
        if !reg.from_id(blocks.block(cx, top, cz)).solid
            && !reg.from_id(blocks.block(cx, top + 1, cz)).solid
        {
            return Medium::Water(f64::from(top));
        }
        return Medium::Blocked;
    }
    match ground_top(blocks, reg, cx, cz, near) {
        Some(g) if (f64::from(g) - y).abs() <= 1.0 => Medium::Land(f64::from(g)),
        _ => Medium::Blocked,
    }
}

fn iced_around(blocks: &dyn BlockAccess, reg: &BlockRegistry, pos: DVec3) -> usize {
    let y = pos.y.floor() as i32 - 1;
    [(1.5, 0.0), (-1.5, 0.0), (0.0, 1.5), (0.0, -1.5)]
        .into_iter()
        .filter(|(dx, dz)| {
            let id = blocks.block((pos.x + dx).floor() as i32, y, (pos.z + dz).floor() as i32);
            IceKind::of(reg, id).is_some()
        })
        .count()
}

// ---------------------------------------------------------------------------------------------
// Création, embarquement
// ---------------------------------------------------------------------------------------------

/// Met un bateau sur l'eau en `pos` (surface), cap `yaw`.
pub fn spawn_boat(world: &mut EntityWorld, kind: BoatKind, pos: DVec3, yaw: f32, tarred: bool) -> EntityId {
    let id = world.create();
    let mut t = Transform::new(pos.x, pos.y, pos.z);
    t.yaw = yaw;
    world.insert(id, t);
    world.insert(id, Boat::new(kind, tarred));
    id
}

/// Embarque `player` sur `boat` si la place est libre.
pub fn board(world: &mut EntityWorld, player: EntityId, boat: EntityId) -> bool {
    let Some(b) = world.get_mut::<Boat>(boat) else { return false };
    if b.pilot.is_some() {
        return false;
    }
    b.pilot = Some(player);
    world.insert(player, Aboard(boat));
    true
}

/// Le bateau libre le plus proche à portée.
pub fn nearest_free(world: &EntityWorld, pos: DVec3) -> Option<EntityId> {
    world
        .query_ids::<(&Boat, &Transform)>()
        .into_iter()
        .filter_map(|id| {
            let b = world.get::<Boat>(id)?;
            let t = world.get::<Transform>(id)?;
            let near = (t.pos.x - pos.x).hypot(t.pos.z - pos.z) <= BOARD_REACH
                && (t.pos.y - pos.y).abs() < 2.0;
            (b.pilot.is_none() && near).then(|| (id, (t.pos - pos).length()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Débarque : sur la terre ferme voisine si elle existe, sinon dans l'eau.
pub fn dismount(world: &mut EntityWorld, blocks: &dyn BlockAccess, reg: &BlockRegistry, player: EntityId) {
    let Some(Aboard(boat)) = world.remove::<Aboard>(player) else { return };
    let Some(bpos) = world.get::<Transform>(boat).map(|t| t.pos) else { return };
    if let Some(b) = world.get_mut::<Boat>(boat) {
        b.pilot = None;
        b.speed = 0.0;
    }
    let mut spot = None;
    'ring: for r in 1..=3i32 {
        for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs().max(dz.abs()) != r {
                    continue;
                }
                let (x, z) = (bpos.x.floor() as i32 + dx, bpos.z.floor() as i32 + dz);
                if let Some(g) = ground_top(blocks, reg, x, z, bpos.y.floor() as i32) {
                    if (f64::from(g) - bpos.y).abs() <= 1.5 {
                        spot = Some(DVec3::new(f64::from(x) + 0.5, f64::from(g), f64::from(z) + 0.5));
                        break 'ring;
                    }
                }
            }
        }
    }
    if let Some(t) = world.get_mut::<Transform>(player) {
        t.pos = spot.unwrap_or(bpos);
    }
}

// ---------------------------------------------------------------------------------------------
// Système
// ---------------------------------------------------------------------------------------------

/// Mène les bateaux et porte leur pilote.
#[derive(Debug, Default)]
pub struct BoatSystem;

impl System for BoatSystem {
    fn name(&self) -> &'static str {
        "Boat"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let boats = world.query_ids::<(&Boat, &Transform)>();
        let player = world.query_ids::<(&Player, &Transform, &Velocity)>().first().copied();
        let input = *ctx.input;
        let just = |a| input.is_just_pressed(a);
        let (wdir, wspd) = ctx
            .environment
            .map_or(([1.0, 0.0], 0.0), |e| (e.wind.dir, e.wind.speed));

        // Embarquer : clic droit près d'un bateau libre.
        if let Some(p) = player {
            if !world.has::<Aboard>(p) && just(GameAction::Secondary) {
                let at = world.get::<Transform>(p).map(|t| t.pos);
                if let Some(boat) = at.and_then(|at| nearest_free(world, at)) {
                    board(world, p, boat);
                }
            } else if world.has::<Aboard>(p) && just(GameAction::Crouch) {
                dismount(world, &*ctx.blocks, ctx.block_reg, p);
            }
        }

        for id in boats {
            let Some(mut b) = world.get::<Boat>(id).map(|b| Boat::clone(&b)) else { continue };
            let Some(mut t) = world.copy::<Transform>(id) else { continue };
            // Le pilote a-t-il disparu (mort, déconnecté) ?
            if let Some(p) = b.pilot {
                if !world.contains(p) || world.get::<Aboard>(p).map(|a| a.0) != Some(id) {
                    b.pilot = None;
                }
            }
            let piloted = b.pilot.is_some();
            let (mut row, mut turn, mut bail) = (0.0f32, 0.0f32, false);
            if piloted {
                if input.is_pressed(GameAction::Forward) {
                    row += 1.0;
                }
                if input.is_pressed(GameAction::Backward) {
                    row -= 0.5;
                }
                if input.is_pressed(GameAction::Left) {
                    turn -= 1.0;
                }
                if input.is_pressed(GameAction::Right) {
                    turn += 1.0;
                }
                bail = input.is_pressed(GameAction::Jump);
                if just(GameAction::Sprint) && b.kind.sail().is_some() {
                    b.sail = !b.sail;
                }
                if just(GameAction::Secondary) {
                    b.anchored = !b.anchored;
                }
            }

            // Barre : plus on avance, mieux on tourne, mais une rame suffit à pivoter.
            let grip = 0.4 + 0.6 * (b.speed.abs() / 2.0).min(1.0);
            t.yaw += turn * 70.0 * grip * dt;

            let sail = if piloted && b.sail && !b.ashore {
                sail_speed(b.kind, t.yaw, wdir, wspd)
            } else {
                0.0
            };
            let oar = b.kind.oar_speed() * row;
            let mut target = if row > 0.0 { oar + sail } else { oar.min(0.0) + sail };
            target = target.clamp(-b.kind.top_speed() * 0.4, b.kind.top_speed());
            target *= 1.0 - 0.4 * b.load;
            target *= 1.0 - 0.5 * b.water.min(1.0);
            if b.anchored {
                target = 0.0;
            }
            if b.ashore {
                target = row.clamp(-1.0, 1.0) * HAUL_SPEED * (1.0 - 0.5 * b.load);
            }
            b.speed += (target - b.speed) * (1.0 - (-dt / b.kind.inertia()).exp());
            if b.speed.abs() < 0.01 && target == 0.0 {
                b.speed = 0.0;
            }

            // Dérive sous le vent d'un bateau sans pilote ni ancre.
            let (fx, fz) = heading(t.yaw);
            let mut vx = f64::from(fx * b.speed);
            let mut vz = f64::from(fz * b.speed);
            if !b.ashore && !b.anchored && !piloted {
                vx += f64::from(wdir[0] * wspd * WIND_BLOCKS * 0.25);
                vz += f64::from(wdir[1] * wspd * WIND_BLOCKS * 0.25);
            }
            self.advance(&mut b, &mut t, ctx, (vx, vz), row, dt);

            // Coque.
            if !b.ashore {
                let sw = swell(wspd, fetch(&*ctx.blocks, ctx.block_reg.ids().water, t.pos, wdir));
                let leak = if b.kind.leaks() {
                    if b.tarred { LEAK_TARRED } else { LEAK_BARE }
                } else {
                    0.0
                };
                let mut rate = leak + sw * b.kind.swell_exposure() * 0.012;
                if iced_around(&*ctx.blocks, ctx.block_reg, t.pos) >= 2 {
                    rate += ICE_CRUSH;
                }
                if bail {
                    rate -= BAIL_RATE;
                }
                b.water = (b.water + rate * dt).max(0.0);
                if b.water > 0.7 && !b.swamped {
                    b.swamped = true;
                    ctx.events.push(EcsEvent::BoatSwamped { boat: id });
                } else if b.water < 0.5 {
                    b.swamped = false;
                }
            }

            if b.water >= 1.0 {
                ctx.events.push(EcsEvent::BoatSank { boat: id });
                if let Some(p) = b.pilot {
                    world.remove::<Aboard>(p);
                }
                world.destroy(id);
                continue;
            }

            // Le pilote suit le bateau.
            if let Some(p) = b.pilot {
                if let Some(pt) = world.get_mut::<Transform>(p) {
                    pt.pos = DVec3::new(t.pos.x, t.pos.y + 0.25, t.pos.z);
                }
                if let Some(v) = world.get_mut::<Velocity>(p) {
                    v.v = Default::default();
                    v.grounded = true;
                }
            }
            if let Some(slot) = world.get_mut::<Boat>(id) {
                *slot = b;
            }
            if let Some(slot) = world.get_mut::<Transform>(id) {
                *slot = t;
            }
        }
    }
}

impl BoatSystem {
    /// Avance par axes ; mise à flot, échouage doux ou choc selon le milieu rencontré.
    fn advance(&self, b: &mut Boat, t: &mut Transform, ctx: &mut TickCtx<'_>, v: (f64, f64), row: f32, dt: f32) {
        let (hx, hz) = heading(t.yaw);
        let bow = 0.9;
        let mut touched = false;
        let dt = f64::from(dt);
        for (dx, dz) in [(v.0 * dt, 0.0), (0.0, v.1 * dt)] {
            if dx == 0.0 && dz == 0.0 {
                continue;
            }
            let np = DVec3::new(t.pos.x + dx, t.pos.y, t.pos.z + dz);
            let probe = (
                np.x + f64::from(hx) * bow * f64::from(b.speed.signum()),
                np.z + f64::from(hz) * bow * f64::from(b.speed.signum()),
            );
            let m = medium(&*ctx.blocks, ctx.block_reg, probe.0, t.pos.y, probe.1);
            match (b.ashore, m) {
                (false, Medium::Water(top)) => {
                    t.pos = DVec3::new(np.x, top, np.z);

                }
                (true, Medium::Land(top)) => t.pos = DVec3::new(np.x, top, np.z),
                (true, Medium::Water(top)) => {
                    b.ashore = false;
                    t.pos = DVec3::new(np.x, top, np.z);
                }
                (false, Medium::Land(top)) if b.kind.haulable() && row > 0.0 && b.speed > 0.0 => {
                    // On accoste doucement : la vitesse retombe, puis la coque est tirée au sec.
                    b.speed = b.speed.min(BEACH_MAX_SPEED * 0.9);
                    touched = true;
                    b.haul += dt as f32;
                    if b.haul >= 0.6 {
                        b.ashore = true;
                        b.anchored = false;
                        t.pos = DVec3::new(np.x, top, np.z);
                    }
                }
                _ => {
                    if b.speed.abs() > 3.0 {
                        b.water += (b.speed.abs() - 3.0) * 0.03;
                    }
                    b.speed = 0.0;
                }
            }
        }
        if !touched {
            b.haul = 0.0;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Morue d'hiver et stockfisch
// ---------------------------------------------------------------------------------------------

/// Ce que ramène la ligne de fond.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Catch {
    /// Morue d'hiver (`skrei`), février à avril.
    Skrei,
    /// Flétan (`hälleflundra`), en eau profonde.
    Halibut,
    /// Requin du Groenland, très profond et rare.
    GreenlandShark,
}

impl Catch {
    pub fn item(self) -> &'static str {
        match self {
            Self::Skrei => "skrei",
            Self::Halibut => "helleflundra",
            Self::GreenlandShark => "gronlandshaj",
        }
    }
}

/// Saison du skrei : de février à avril (`month` de 1 à 12).
pub fn skrei_season(month: u32) -> bool {
    (2..=4).contains(&month)
}

/// Pêche de fond : `depth` blocs d'eau sous la ligne, mois réel, `roll` de 0 à 1. La morue ne
/// mord que de février à avril à 6 blocs de fond au moins ; flétan à 14, requin à 26.
pub fn bottom_fishing(depth: u32, month: u32, roll: f32) -> Option<Catch> {
    if depth >= 26 && roll < 0.08 {
        return Some(Catch::GreenlandShark);
    }
    if depth >= 14 && roll < 0.35 {
        return Some(Catch::Halibut);
    }
    (depth >= 6 && skrei_season(month) && roll < 0.8).then_some(Catch::Skrei)
}

/// Poissons au plus sur une claie.
pub const RACK_CAPACITY: u8 = 12;
/// Jours de bon temps du séchage.
pub const DRY_DAYS: f32 = 4.0;
/// Jours de temps doux et humide avant que le poisson ne pourrisse.
pub const ROT_DAYS: f32 = 1.5;

/// Temps de séchage : froid, sec et venté. `Good`, `Wet` (pluie ou neige : pause) ou `Warm`
/// (le poisson tourne).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DryingWeather {
    Good,
    Idle,
    Warm,
}

pub fn drying_weather(env: &nb_world::weather::Environment) -> DryingWeather {
    let cold = f64::from(env.air) < nb_world::weather::freezing_point() + 0.12;
    if !cold {
        return DryingWeather::Warm;
    }
    if env.rain > 0.05 || env.snow > 0.05 || env.fog > 0.3 || env.wind.speed < 0.15 {
        return DryingWeather::Idle;
    }
    DryingWeather::Good
}

/// Claie (hjell) chargée de morue.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rack {
    pub fish: u8,
    /// Séchage, de 0 à 1.
    pub dried: f32,
    /// Avancement de la pourriture, de 0 à 1.
    pub rot: f32,
}

impl Rack {
    /// Suspend `n` poissons frais ; renvoie ceux qui n'ont pas de place. Un ajout ramène le
    /// séchage à la moyenne pondérée.
    pub fn hang(&mut self, n: u8) -> u8 {
        let room = RACK_CAPACITY.saturating_sub(self.fish).min(n);
        let total = f32::from(self.fish + room);
        if total > 0.0 {
            self.dried = self.dried * f32::from(self.fish) / total;
            self.rot = self.rot * f32::from(self.fish) / total;
        }
        self.fish += room;
        n - room
    }

    /// Un pas de `dt` secondes, `day` secondes par jour de jeu.
    pub fn step(&mut self, weather: DryingWeather, dt: f32, day: f32) {
        if self.fish == 0 || self.rot >= 1.0 {
            return;
        }
        match weather {
            DryingWeather::Good => self.dried = (self.dried + dt / (DRY_DAYS * day)).min(1.0),
            DryingWeather::Warm if self.dried < 1.0 => {
                self.rot = (self.rot + dt / (ROT_DAYS * day)).min(1.0);
            }
            _ => {}
        }
    }

    /// Décroche : du stockfisch si sec, du poisson pourri si la pourriture l'a emporté, sinon
    /// rien (trop tôt) ; renvoie `(stockfisk, pourri)`.
    pub fn take(&mut self) -> (u8, u8) {
        let n = self.fish;
        if self.rot >= 1.0 {
            *self = Rack::default();
            return (0, n);
        }
        if self.dried >= 1.0 {
            *self = Rack::default();
            return (n, 0);
        }
        (0, 0)
    }
}

/// Claies posées, par cellule.
pub type RackState = Arc<Mutex<HashMap<IVec3, Rack>>>;

/// Fait sécher les claies avec le temps du joueur (partout où elles se trouvent).
#[derive(Debug, Default)]
pub struct RackSystem {
    racks: RackState,
}

impl RackSystem {
    pub fn new(racks: RackState) -> Self {
        Self { racks }
    }
}

impl System for RackSystem {
    fn name(&self) -> &'static str {
        "Racks"
    }

    fn run(&mut self, _world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let Some(env) = ctx.environment else { return };
        let weather = drying_weather(env);
        let day = ctx.config.day_length_seconds.max(1.0);
        let mut racks = self.racks.lock().unwrap();
        for r in racks.values_mut() {
            r.step(weather, dt, day);
        }
    }
}
