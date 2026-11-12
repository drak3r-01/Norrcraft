//! Neige et glisse (session 6.2, doc 04 §4.3 et §4.4).
//!
//! * **Neige** : marcher dans plus de 3 couches (huitièmes de bloc) ralentit, jusqu'à −60 % à 8
//!   couches ([`snow_slow`]). Une colonne piétinée 3 fois devient « neige tassée » (`packad_sno`).
//! * **Équipement** : le joueur *tient* skis, raquettes, patins ou luge à pied dans la main ; le
//!   [`Gear`] choisi donne un multiplicateur de vitesse et un frottement ([`modifier`]). Bâtons
//!   (`skidstav`) et fart au goudron (`skidtjara`) comptent dès qu'ils sont dans l'inventaire.
//! * **Glisse** : à skis, en patins ou en luge, la pente accélère ([`slope_accel`]).
//! * **Véhicules** : [`Vehicle`] (pulka, ackja, traîneau, char à glace) et leur vitesse
//!   ([`team_speed`], [`ice_yacht_speed`]), leur charge sur la glace ([`Vehicle::ice_load`]) ; la
//!   monture au tölt vient de la fiche d'élevage ([`mount_speed`]).
//!
//! [`GlideSystem`] pose le composant [`Locomotion`] sur le joueur ; `PlayerInputSystem` le lit.
//! Les avalanches sont dans [`crate::avalanche`].

use std::collections::HashMap;

use nb_core::glam::{DVec3, IVec3};
use nb_data::husbandry::Gait;
use nb_data::{BlockId, BlockRegistry, ItemId};

use crate::blocks::BlockAccess;
use crate::components::{InventoryComp, Player, Transform, Velocity};
use crate::ice::IceKind;
use crate::schedule::{System, TickCtx};
use crate::world::EntityWorld;

/// Passages qui tassent la neige d'une colonne.
pub const TRAMPLE_STEPS: u8 = 3;
/// Couches à partir desquelles la neige ralentit (au-delà de 3).
pub const SLOW_FROM: u8 = 3;
/// Ralentissement maximal de la neige profonde (à 8 couches).
pub const SLOW_MAX: f32 = 0.6;
/// Accélération de la glisse par bloc de dénivelé (blocs/s² par bloc/bloc).
pub const GLIDE_G: f32 = 24.0;
/// Accélération maximale de la glisse.
pub const GLIDE_MAX: f32 = 32.0;
/// Vitesse horizontale maximale gagnée par la pente.
pub const GLIDE_CAP: f32 = 14.0;
/// Colonnes mémorisées avant d'oublier les passages.
const TRAMPLE_MEMORY: usize = 4096;

// ---------------------------------------------------------------------------------------------
// Surface
// ---------------------------------------------------------------------------------------------

/// Ce qui est sous (et autour de) un pied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Surface {
    /// Neige : `layers` huitièmes dans la cellule des pieds (0 : on marche sur un bloc de neige).
    Snow { layers: u8 },
    /// Neige tassée.
    Packed,
    /// Glace (lac ou mer).
    Ice(IceKind),
    /// Autre chose.
    Bare,
}

/// Couches de neige d'un bloc `snotacke_n` ; `None` si ce n'en est pas un.
pub fn layers_of(reg: &BlockRegistry, id: BlockId) -> Option<u8> {
    reg.from_id(id)
        .name
        .strip_prefix("snotacke_")
        .and_then(|n| n.parse().ok())
}

/// Le bloc est-il de la neige tassée ?
pub fn is_packed(reg: &BlockRegistry, id: BlockId) -> bool {
    reg.from_id(id).name == "packad_sno"
}

/// Surface sous les pieds `feet` (cellule où se tiennent les pieds).
pub fn surface_at(blocks: &dyn BlockAccess, reg: &BlockRegistry, feet: IVec3) -> Surface {
    let here = blocks.block_at(feet);
    if let Some(n) = layers_of(reg, here) {
        return Surface::Snow { layers: n };
    }
    if is_packed(reg, here) {
        return Surface::Packed;
    }
    let below = blocks.block_at(feet - IVec3::Y);
    if let Some(k) = IceKind::of(reg, below) {
        return Surface::Ice(k);
    }
    if reg.from_id(below).name == "sno" {
        return Surface::Snow { layers: 0 };
    }
    // Sur le bord d'une marche, les pieds sont encore au-dessus de la cellule voisine plus
    // basse : sa neige compte, pour ne pas perdre l'élan à chaque marche.
    if is_packed(reg, below) {
        return Surface::Packed;
    }
    if let Some(n) = layers_of(reg, below) {
        return Surface::Snow { layers: n };
    }
    Surface::Bare
}

/// Facteur de vitesse d'un marcheur sans équipement dans `layers` couches : 1 jusqu'à 3, puis
/// −12 % par couche en plus (−60 % à 8).
pub fn snow_slow(layers: u8) -> f32 {
    if layers <= SLOW_FROM {
        return 1.0;
    }
    let span = f32::from(8 - SLOW_FROM);
    1.0 - SLOW_MAX * (f32::from(layers - SLOW_FROM) / span).min(1.0)
}

// ---------------------------------------------------------------------------------------------
// Équipement
// ---------------------------------------------------------------------------------------------

/// Ce que le joueur a aux pieds ou à la main.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gear {
    #[default]
    None,
    /// Skis (`skidor`), `poles` : avec bâtons, `waxed` : farté au goudron.
    Skis { poles: bool, waxed: bool },
    /// Raquettes (`snoskor`).
    Snowshoes,
    /// Patins de randonnée (`langfardsskridskor`).
    Skates,
    /// Luge à pied (`spark`).
    Spark,
}

/// Effet d'un équipement sur une surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mod {
    /// Multiplicateur de vitesse.
    pub speed: f32,
    /// Frottement au sol imposé (`None` : celui de la marche, 15).
    pub friction: Option<f32>,
    /// La pente accélère-t-elle ?
    pub glide: bool,
}

impl Mod {
    const fn plain(speed: f32) -> Self {
        Self {
            speed,
            friction: None,
            glide: false,
        }
    }
}

/// Table du doc 04 §4.4 (et §4.3 pour la neige profonde).
pub fn modifier(gear: Gear, surface: Surface) -> Mod {
    use Surface::{Bare, Ice, Packed, Snow};
    match (gear, surface) {
        (Gear::None, Snow { layers }) => Mod::plain(snow_slow(layers)),
        (Gear::None, _) => Mod::plain(1.0),
        (Gear::Skis { poles, waxed }, Snow { .. } | Packed) => Mod {
            speed: if poles { 1.6 } else { 1.3 },
            friction: Some(if waxed { 3.0 } else { 4.0 }),
            glide: true,
        },
        (Gear::Skis { .. }, Ice(_)) => Mod {
            speed: 1.0,
            friction: Some(2.5),
            glide: true,
        },
        (Gear::Skis { .. }, Bare) => Mod::plain(0.6),
        (Gear::Snowshoes, Snow { .. } | Packed) => Mod::plain(1.0),
        (Gear::Snowshoes, Ice(_)) => Mod::plain(0.8),
        (Gear::Snowshoes, Bare) => Mod::plain(0.9),
        (Gear::Skates, Ice(_)) => Mod {
            speed: 2.5,
            friction: Some(2.0),
            glide: true,
        },
        (Gear::Skates, Snow { .. }) => Mod::plain(0.4),
        (Gear::Skates, Packed) => Mod::plain(0.6),
        (Gear::Skates, Bare) => Mod::plain(0.5),
        (Gear::Spark, Ice(_) | Packed) => Mod {
            speed: 2.0,
            friction: Some(3.0),
            glide: true,
        },
        (Gear::Spark, Snow { layers }) => Mod::plain(0.7 * snow_slow(layers)),
        (Gear::Spark, Bare) => Mod::plain(0.7),
    }
}

/// Identifiants des objets de la glisse (absents = `None`).
#[derive(Debug, Clone, Copy, Default)]
pub struct GearIds {
    pub skis: Option<ItemId>,
    pub poles: Option<ItemId>,
    pub wax: Option<ItemId>,
    pub snowshoes: Option<ItemId>,
    pub skates: Option<ItemId>,
    pub spark: Option<ItemId>,
}

impl GearIds {
    pub fn from_names(find: impl Fn(&str) -> Option<ItemId>) -> Self {
        Self {
            skis: find("skidor"),
            poles: find("skidstav"),
            wax: find("skidtjara"),
            snowshoes: find("snoskor"),
            skates: find("langfardsskridskor"),
            spark: find("spark"),
        }
    }

    /// Équipement correspondant à l'objet tenu `held`, bâtons et fart pris dans `has`.
    pub fn gear(&self, held: Option<ItemId>, has: impl Fn(ItemId) -> bool) -> Gear {
        let Some(h) = held else {
            return Gear::None;
        };
        let is = |x: Option<ItemId>| x == Some(h);
        if is(self.skis) {
            Gear::Skis {
                poles: self.poles.is_some_and(&has),
                waxed: self.wax.is_some_and(&has),
            }
        } else if is(self.snowshoes) {
            Gear::Snowshoes
        } else if is(self.skates) {
            Gear::Skates
        } else if is(self.spark) {
            Gear::Spark
        } else {
            Gear::None
        }
    }
}

/// Composant du joueur lu par `PlayerInputSystem` : vitesse et frottement du moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Locomotion {
    pub speed: f32,
    pub friction: Option<f32>,
    pub gear: Gear,
    pub surface: Surface,
}

impl Default for Locomotion {
    fn default() -> Self {
        Self {
            speed: 1.0,
            friction: None,
            gear: Gear::None,
            surface: Surface::Bare,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pente
// ---------------------------------------------------------------------------------------------

/// Dessus du sol (cellule libre au-dessus du dernier bloc solide) de la colonne, cherché de
/// `near + 2` vers le bas.
pub fn ground_top(blocks: &dyn BlockAccess, reg: &BlockRegistry, x: i32, z: i32, near: i32) -> Option<i32> {
    ((near - 4)..=(near + 2)).rev().find_map(|y| {
        let solid = reg.from_id(blocks.block(x, y, z)).solid;
        let free = !reg.from_id(blocks.block(x, y + 1, z)).solid;
        (solid && free).then_some(y + 1)
    })
}

/// Accélération (x, z) due à la pente sous `feet` : vers le bas de la pente, proportionnelle au
/// dénivelé par bloc, plafonnée à [`GLIDE_MAX`].
pub fn slope_accel(blocks: &dyn BlockAccess, reg: &BlockRegistry, feet: IVec3) -> (f32, f32) {
    let h = |dx: i32, dz: i32| {
        ground_top(blocks, reg, feet.x + dx, feet.z + dz, feet.y).unwrap_or(feet.y) as f32
    };
    let gx = (h(-1, 0) - h(1, 0)) * 0.5;
    let gz = (h(0, -1) - h(0, 1)) * 0.5;
    let (mut ax, mut az) = (gx * GLIDE_G, gz * GLIDE_G);
    let len = (ax * ax + az * az).sqrt();
    if len > GLIDE_MAX {
        ax *= GLIDE_MAX / len;
        az *= GLIDE_MAX / len;
    }
    (ax, az)
}

// ---------------------------------------------------------------------------------------------
// Véhicules
// ---------------------------------------------------------------------------------------------

/// Véhicules de neige et de glace du doc 04 §4.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vehicle {
    /// Luge à pied : un coffre, poussée à la main.
    Spark,
    /// Coffre tiré derrière le skieur.
    Pulka,
    /// Traîneau à renne : rapide, deux coffres.
    Ackja,
    /// Traîneau à cheval ou à chiens.
    Sleigh,
    /// Voilier à patins.
    IceYacht,
}

impl Vehicle {
    /// Coffres embarqués.
    pub fn chests(self) -> u8 {
        match self {
            Self::Spark | Self::Pulka => 1,
            Self::Ackja => 2,
            Self::Sleigh => 4,
            Self::IceYacht => 1,
        }
    }

    /// Bêtes d'attelage pleines.
    pub fn team(self) -> u8 {
        match self {
            Self::Ackja => 2,
            Self::Sleigh => 2,
            _ => 0,
        }
    }

    /// Charge qu'il pose sur la glace, `loaded` : au moins un coffre plein (doc 04 §4.1 : le
    /// traîneau chargé veut de la glace épaisse).
    pub fn ice_load(self, loaded: bool) -> crate::ice::Load {
        use crate::ice::Load;
        match (self, loaded) {
            (Self::Spark | Self::Pulka, _) => Load::Prone,
            (Self::IceYacht, _) => Load::LoadedSledge,
            (_, true) => Load::LoadedSledge,
            (_, false) => Load::Sledge,
        }
    }
}

/// Vitesse (blocs/s) d'un attelage de `animals` bêtes (vitesse de marche `walk`) tirant
/// `vehicle` chargé à `load` (0 à 1) sur `surface`.
pub fn team_speed(vehicle: Vehicle, walk: f32, animals: u8, load: f32, surface: Surface) -> f32 {
    let need = f32::from(vehicle.team().max(1));
    let pull = 0.4 + 0.6 * (f32::from(animals) / need).min(1.0);
    let heavy = 1.0 - 0.5 * load.clamp(0.0, 1.0);
    let ground = match surface {
        Surface::Ice(_) => 1.4,
        Surface::Packed => 1.3,
        Surface::Snow { layers } => 0.9 * snow_slow(layers).max(0.5),
        Surface::Bare => 0.5,
    };
    walk * pull * heavy * ground
}

/// Vitesse du char à glace (blocs/s) par vent `wind` (blocs/s) et angle `angle` entre le cap et
/// le vent (0 : vent debout, π : vent arrière). Le travers rapide (π/2) donne le maximum, trois
/// fois le vent, plafonné à 40 : le véhicule le plus rapide du jeu. Hors glace : à l'arrêt.
pub fn ice_yacht_speed(wind: f32, angle: f32, surface: Surface) -> f32 {
    if !matches!(surface, Surface::Ice(_)) {
        return 0.0;
    }
    // Aucune voile ne remonte le vent de face : zone morte de 30 degrés.
    if angle < 0.52 {
        return 0.0;
    }
    let beam = angle.sin().abs();
    (wind * (1.0 + 2.0 * beam)).min(40.0)
}

/// Vitesse (blocs/s) d'une monture de vitesse de marche `walk` à l'allure `gait`.
pub fn mount_speed(walk: f32, gait: &Gait) -> f32 {
    walk * gait.speed
}

// ---------------------------------------------------------------------------------------------
// Système
// ---------------------------------------------------------------------------------------------

/// Pose [`Locomotion`], fait glisser sur les pentes et tasse la neige piétinée.
pub struct GlideSystem {
    ids: GearIds,
    steps: HashMap<(i32, i32), u8>,
    last: Option<(i32, i32)>,
}

impl GlideSystem {
    pub fn new(ids: GearIds) -> Self {
        Self {
            ids,
            steps: HashMap::new(),
            last: None,
        }
    }
}

fn cell(pos: DVec3) -> IVec3 {
    IVec3::new(pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32)
}

impl System for GlideSystem {
    fn name(&self) -> &'static str {
        "Glide"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let Some(&id) = world.query_ids::<(&Player, &Transform, &Velocity)>().first() else {
            return;
        };
        let (pos, grounded) = {
            let (Some(t), Some(v)) = (world.get::<Transform>(id), world.get::<Velocity>(id))
            else {
                return;
            };
            (t.pos, v.grounded)
        };
        let feet = cell(pos);
        if !ctx.blocks.is_chunk_loaded(feet.x, feet.z) {
            return;
        }
        let gear = world
            .get::<InventoryComp>(id)
            .map(|inv| {
                let held = inv.0.slot(i32::from(ctx.input.selected_slot)).map(|s| s.item);
                self.ids.gear(held, |it| inv.0.count_of(it) > 0)
            })
            .unwrap_or_default();
        let surface = surface_at(&*ctx.blocks, ctx.block_reg, feet);
        let m = modifier(gear, surface);
        world.insert(
            id,
            Locomotion {
                speed: m.speed,
                friction: m.friction,
                gear,
                surface,
            },
        );

        if m.glide && grounded {
            let (ax, az) = slope_accel(&*ctx.blocks, ctx.block_reg, feet);
            if let Some(v) = world.get_mut::<Velocity>(id) {
                let (nx, nz) = (v.v.x + ax * dt, v.v.z + az * dt);
                let len = (nx * nx + nz * nz).sqrt();
                let k = if len > GLIDE_CAP { GLIDE_CAP / len } else { 1.0 };
                v.v.x = nx * k;
                v.v.z = nz * k;
            }
        }

        // Piétinement : un passage = entrer dans une nouvelle colonne de neige.
        let col = (feet.x, feet.z);
        if grounded && self.last != Some(col) {
            self.last = Some(col);
            if let Surface::Snow { layers } = surface {
                if layers > 0 && layers_of(ctx.block_reg, ctx.blocks.block_at(feet)).is_some() {
                    if self.steps.len() > TRAMPLE_MEMORY {
                        self.steps.clear();
                    }
                    let n = self.steps.entry(col).or_insert(0);
                    *n += 1;
                    if *n >= TRAMPLE_STEPS {
                        self.steps.remove(&col);
                        let packed = ctx.block_reg.get("packad_sno").id;
                        ctx.blocks.set_block(feet.x, feet.y, feet.z, packed);
                    }
                }
            }
        }
    }
}
