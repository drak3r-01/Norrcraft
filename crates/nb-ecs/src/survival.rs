//! Mode survie nordique : faim, froid, endurance, minage progressif, combat à l'arme, armures,
//! pêche, labour et cultures, butin des mobs.
//!
//! Tout ceci ne s'applique qu'au joueur en mode **survie** quand le contexte du tick porte des
//! [`SurvivalData`] ; en créatif (ou sans données, comme dans les tests de conformité du Java)
//! le comportement d'origine est inchangé (casser un bloc est instantané, B19).

use nb_core::glam::{DVec3, IVec3, Vec3};
use nb_data::{ArmorSlot, BlockId, ItemId, SurvivalData, ToolKind};

use crate::components::{
    EntityId, GameMode, Health, InventoryComp, MobType, Player, RemotePlayer, Transform, Velocity,
};
use crate::health::hit_mob_damage;
use crate::input::GameAction;
use crate::inventory::{ItemStack, HOTBAR_START};
use crate::physics::raycast_mob;
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;

pub const MAX_HUNGER: f32 = 20.0;
pub const MAX_WARMTH: f32 = 100.0;
pub const MAX_STAMINA: f32 = 100.0;
/// Durée pour manger une portion (s).
pub const EAT_TIME: f32 = 1.4;
/// Délai entre deux coups (s).
pub const ATTACK_COOLDOWN: f32 = 0.4;
/// Réduction de dégâts par point de défense.
pub const DEFENSE_PER_POINT: f32 = 0.03;
pub const MAX_REDUCTION: f32 = 0.8;
/// Portée de la canne à pêche.
pub const CAST_RANGE: f32 = 12.0;

/// Progression du minage d'un bloc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MiningState {
    pub pos: IVec3,
    pub block: BlockId,
    /// 0..1
    pub progress: f32,
    pub sound_timer: f32,
}

/// Ligne de pêche lancée.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fishing {
    pub bobber: DVec3,
    /// Attente avant la prochaine touche (s).
    pub wait: f32,
    /// Fenêtre de ferrage restante quand un poisson mord (s), 0 sinon.
    pub bite: f32,
}

/// État de survie du joueur (composant).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vitals {
    /// Faim, 0..=20 (20 = rassasié).
    pub hunger: f32,
    /// Réserve de saturation (consommée avant la faim).
    pub saturation: f32,
    /// Chaleur corporelle, 0..=100.
    pub warmth: f32,
    /// Endurance, 0..=100.
    pub stamina: f32,
    /// Épuisé : plus de sprint avant d'avoir récupéré.
    pub exhausted: bool,
    pub regen_timer: f32,
    pub harm_timer: f32,
    /// Température ressentie (biome + heure + altitude + humidité), pour l'affichage.
    pub ambient: f32,
    /// Isolation des vêtements (0..~0.9).
    pub insulation: f32,
    /// Chaleur des sources proches (feu de camp…).
    pub heat: f32,
    /// Réduction des dégâts de mobs par l'armure (0..0.8).
    pub reduction: f32,
    /// Somme des points de défense (affichage).
    pub defense: f32,
    /// Temps passé à manger la portion courante (s).
    pub eating: f32,
    /// Le clic droit courant a déjà servi à autre chose (planter, équiper) : pas de repas.
    pub eat_lock: bool,
    pub mining: Option<MiningState>,
    pub mine_cooldown: f32,
    pub fishing: Option<Fishing>,
    pub attack_cd: f32,
    /// Animation de coup restante (s).
    pub swing: f32,
    pub last_hit_mob: Option<EntityId>,
    /// Coups reçus depuis le dernier tick (usure de l'armure).
    pub armor_hits: u8,
    /// Le joueur court (écrit par `PlayerInputSystem`).
    pub sprinting: bool,
    /// Saut effectué ce tick.
    pub jumped: bool,
    /// Vitesse horizontale (blocs/s).
    pub speed: f32,
    /// Mouillé (dans l'eau).
    pub wet: bool,
    /// Palier de froid déjà annoncé (0 : bien, 1 : froid, 2 : gelé).
    pub cold_notice: u8,
    pub hunger_notice: bool,
    pub heat_scan: f32,
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            hunger: MAX_HUNGER,
            saturation: 5.0,
            warmth: MAX_WARMTH,
            stamina: MAX_STAMINA,
            exhausted: false,
            regen_timer: 0.0,
            harm_timer: 0.0,
            ambient: 0.0,
            insulation: 0.0,
            heat: 0.0,
            reduction: 0.0,
            defense: 0.0,
            eating: 0.0,
            eat_lock: false,
            mining: None,
            mine_cooldown: 0.0,
            fishing: None,
            attack_cd: 0.0,
            swing: 0.0,
            last_hit_mob: None,
            armor_hits: 0,
            sprinting: false,
            jumped: false,
            speed: 0.0,
            wet: false,
            cold_notice: 0,
            hunger_notice: false,
            heat_scan: 0.0,
        }
    }
}

impl Vitals {
    /// Multiplicateur de vitesse de marche (froid, faim, épuisement).
    pub fn speed_mult(&self) -> f32 {
        let mut m = 1.0;
        if self.warmth < 10.0 {
            m *= 0.72;
        } else if self.warmth < 30.0 {
            m *= 0.88;
        }
        if self.hunger <= 0.0 {
            m *= 0.85;
        }
        if self.exhausted {
            m *= 0.92;
        }
        m
    }

    /// Le joueur peut-il courir ?
    pub fn can_sprint(&self) -> bool {
        !self.exhausted && self.hunger > 6.0
    }

    /// Dépense de faim (consomme la saturation d'abord).
    pub fn spend(&mut self, amount: f32) {
        let from_sat = self.saturation.min(amount);
        self.saturation -= from_sat;
        self.hunger = (self.hunger - (amount - from_sat)).max(0.0);
    }
}

/// Le joueur est-il soumis aux règles de survie ?
pub fn survival_active(world: &EntityWorld, id: EntityId, ctx: &TickCtx<'_>) -> bool {
    ctx.survival.is_some()
        && world
            .get::<Player>(id)
            .is_some_and(|p| p.game_mode == GameMode::Survival)
}

fn notice(ctx: &mut TickCtx<'_>, text: &str) {
    ctx.events.push(EcsEvent::Notice(text.to_owned()));
}

/// Retire l'usure du/des objet(s) tenu(s) ; l'objet casse à la fin de sa durabilité.
pub fn wear_held(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    data: &SurvivalData,
    id: EntityId,
    slot: i32,
    amount: u16,
) {
    let Some(inv) = world.get_mut::<InventoryComp>(id) else {
        return;
    };
    let idx = HOTBAR_START + slot;
    let Some(mut s) = inv.0.slot(idx) else { return };
    let max = data.max_durability(s.item);
    if max == 0 {
        return;
    }
    s.wear = s.wear.saturating_add(amount);
    if s.wear >= max {
        inv.0.remove_one(idx);
        ctx.events.push(EcsEvent::ItemBroke { item: s.item });
        notice(ctx, "Your tool broke!");
    } else {
        inv.0.set_slot(idx, Some(s));
    }
}

/// Ajoute des objets à l'inventaire du joueur (reliquat perdu) et annonce le gain.
pub fn give(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    id: EntityId,
    item: ItemId,
    count: i32,
) {
    if count <= 0 {
        return;
    }
    if let Some(inv) = world.get_mut::<InventoryComp>(id) {
        inv.0.add_stack(ItemStack::new(item, count));
    }
    ctx.events.push(EcsEvent::Gained { item, count });
}

/// Tire le butin d'une liste de [`nb_data::Drop`].
pub fn roll_drops(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    id: EntityId,
    drops: &[nb_data::Drop],
) {
    for d in drops {
        if ctx.rngs.ambient.next_float() >= d.chance {
            continue;
        }
        let n = d.min + ctx.rngs.ambient.next_int_bound(d.max - d.min + 1);
        give(world, ctx, id, d.item, n);
    }
}

fn held_stack(world: &EntityWorld, id: EntityId, slot: i32) -> Option<ItemStack> {
    world
        .get::<InventoryComp>(id)
        .and_then(|i| i.0.hotbar_slot(slot))
}

// ---------------------------------------------------------------------------------------------
// Actions du joueur (appelées par `PlayerInputSystem`)
// ---------------------------------------------------------------------------------------------

/// Clic gauche en survie : coup à l'arme ou minage progressif.
#[allow(clippy::too_many_arguments)]
pub(crate) fn primary(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    data: &SurvivalData,
    id: EntityId,
    slot: i32,
    origin: Vec3,
    dir: Vec3,
    targeted: Option<IVec3>,
    dt: f32,
) {
    let just = ctx.input.is_just_pressed(GameAction::Primary);
    let held = ctx.input.is_pressed(GameAction::Primary);
    let Some(mut v) = world.copy::<Vitals>(id) else {
        return;
    };
    let stack = held_stack(world, id, slot);
    let item = stack.map(|s| s.item);
    let air = ctx.block_reg.ids().air;

    // Mob visé plus près que le bloc visé ?
    let mob = raycast_mob(world, origin, dir).filter(|m| {
        let Some(mt) = world.copy::<Transform>(*m) else {
            return false;
        };
        let md = (mt.pos.as_vec3() - origin).length();
        targeted.is_none_or(|tb| md < (tb.as_vec3() + Vec3::splat(0.5) - origin).length() + 0.6)
    });

    if let Some(mob) = mob {
        v.mining = None;
        if just && v.attack_cd <= 0.0 {
            let dmg = data.attack_damage(item, ctx.config.attack_damage);
            let player = Some(id);
            hit_mob_damage(world, mob, player, dmg, ctx.mobs, &mut ctx.rngs.passive);
            ctx.events.push(EcsEvent::MobHit { entity: mob });
            ctx.events.push(EcsEvent::AttackDealt {
                entity: mob,
                damage: dmg,
            });
            v.attack_cd = ATTACK_COOLDOWN;
            v.swing = 0.28;
            v.last_hit_mob = Some(mob);
            v.stamina = (v.stamina - 4.0).max(0.0);
            v.spend(0.1);
            if item.and_then(|i| data.tool(i)).is_some() {
                world.insert(id, v);
                wear_held(world, ctx, data, id, slot, 1);
                return;
            }
        }
        world.insert(id, v);
        return;
    }

    if !held {
        v.mining = None;
        world.insert(id, v);
        return;
    }
    let Some(tb) = targeted else {
        v.mining = None;
        world.insert(id, v);
        return;
    };
    if v.mine_cooldown > 0.0 {
        world.insert(id, v);
        return;
    }
    let block = ctx.blocks.block_at(tb);
    if block == air {
        v.mining = None;
        world.insert(id, v);
        return;
    }
    let Some(rule) = data.block_rule(block) else {
        // Bloc sans règle : comportement d'origine (instantané, butin = lui-même).
        ctx.blocks.set_block(tb.x, tb.y, tb.z, air);
        if let Some(inv) = world.get_mut::<InventoryComp>(id) {
            inv.0.add_block(block, air);
        }
        ctx.events.push(EcsEvent::BlockBroken { pos: tb, block });
        v.mining = None;
        v.mine_cooldown = 0.2;
        world.insert(id, v);
        return;
    };
    let Some(m) = data.mining(block, item) else {
        // Incassable (roche mère, fluides).
        v.mining = None;
        world.insert(id, v);
        return;
    };
    let mut st = v
        .mining
        .filter(|s| s.pos == tb && s.block == block)
        .unwrap_or(MiningState {
            pos: tb,
            block,
            progress: 0.0,
            sound_timer: 0.0,
        });
    st.progress += if m.seconds <= 0.0 { 1.0 } else { dt / m.seconds };
    st.sound_timer -= dt;
    v.swing = 0.3;
    v.spend(0.004 * dt * 10.0);
    if st.sound_timer <= 0.0 && st.progress < 1.0 {
        st.sound_timer = 0.24;
        ctx.events.push(EcsEvent::BlockMining {
            pos: tb,
            block,
            progress: st.progress.min(1.0),
        });
    }
    if st.progress < 1.0 {
        v.mining = Some(st);
        world.insert(id, v);
        return;
    }
    // --- Le bloc cède ---
    ctx.blocks.set_block(tb.x, tb.y, tb.z, air);
    ctx.events.push(EcsEvent::BlockBroken { pos: tb, block });
    v.mining = None;
    v.mine_cooldown = if m.seconds > 0.2 { 0.22 } else { 0.12 };
    world.insert(id, v);
    if m.harvest {
        let drops = rule.drops.clone();
        roll_drops(world, ctx, id, &drops);
    }
    if rule.hardness > 0.0 {
        let tool = item.and_then(|i| data.tool(i));
        if tool.is_some_and(|t| t.kind != ToolKind::Rod) {
            wear_held(world, ctx, data, id, slot, 1);
        }
    }
}

/// Clic droit en survie. Retourne `true` si le clic a été consommé (équiper, labourer, planter,
/// pêcher) et ne doit pas poser de bloc.
#[allow(clippy::too_many_arguments)]
pub(crate) fn secondary(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    data: &SurvivalData,
    id: EntityId,
    slot: i32,
    origin: Vec3,
    dir: Vec3,
    targeted: Option<IVec3>,
) -> bool {
    let air = ctx.block_reg.ids().air;
    // Relever la ligne : un clic avec la canne alors qu'elle est lancée.
    if let Some(f) = world.get::<Vitals>(id).and_then(|v| v.fishing) {
        reel_in(world, ctx, data, id, slot, f);
        lock_eating(world, id);
        return true;
    }
    let Some(held) = held_stack(world, id, slot) else {
        return false;
    };
    let props = data.props(held.item).copied().unwrap_or_default();

    // Armure : équiper (échange avec la pièce déjà portée).
    if let Some(a) = props.armor {
        if let Some(inv) = world.get_mut::<InventoryComp>(id) {
            let old = inv.0.armor(a.slot);
            inv.0
                .set_armor(a.slot, Some(ItemStack::new(held.item, 1).with_wear(held.wear)));
            inv.0.set_slot(HOTBAR_START + slot, None);
            if let Some(o) = old {
                inv.0.set_slot(HOTBAR_START + slot, Some(o));
            }
        }
        ctx.events.push(EcsEvent::Equipped { item: held.item });
        lock_eating(world, id);
        return true;
    }
    // Houe : labourer.
    if let Some(t) = props.tool {
        if t.kind == ToolKind::Hoe {
            if let Some(tb) = targeted {
                let b = ctx.blocks.block_at(tb);
                let above = ctx.blocks.block_at(tb + IVec3::Y);
                if data.is_tillable(b) && above == air {
                    ctx.blocks.set_block(tb.x, tb.y, tb.z, data.farmland());
                    ctx.events.push(EcsEvent::Tilled { pos: tb });
                    wear_held(world, ctx, data, id, slot, 1);
                    lock_eating(world, id);
                    return true;
                }
            }
            return true;
        }
        if t.kind == ToolKind::Rod {
            cast(world, ctx, id, origin, dir);
            lock_eating(world, id);
            return true;
        }
    }
    // Graines, pomme de terre, carotte : planter sur de la terre labourée.
    if let (Some(crop), Some(tb)) = (data.plant_of(held.item), targeted) {
        let b = ctx.blocks.block_at(tb);
        let pt = tb + IVec3::Y;
        if b == data.farmland() && ctx.blocks.block_at(pt) == air {
            ctx.blocks.set_block(pt.x, pt.y, pt.z, crop);
            if let Some(inv) = world.get_mut::<InventoryComp>(id) {
                inv.0.remove_one(HOTBAR_START + slot);
            }
            ctx.events.push(EcsEvent::Planted { pos: pt });
            lock_eating(world, id);
            return true;
        }
    }
    false
}

fn lock_eating(world: &mut EntityWorld, id: EntityId) {
    if let Some(v) = world.get_mut::<Vitals>(id) {
        v.eat_lock = true;
        v.eating = 0.0;
    }
}

/// Lance la ligne vers l'eau visée.
fn cast(world: &mut EntityWorld, ctx: &mut TickCtx<'_>, id: EntityId, origin: Vec3, dir: Vec3) {
    let ids = *ctx.block_reg.ids();
    let mut p = origin;
    let step = dir * 0.2;
    let mut hit = None;
    for _ in 0..(CAST_RANGE / 0.2) as i32 {
        p += step;
        let b = ctx.blocks.block(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if b == ids.water {
            hit = Some(DVec3::new(
                f64::from(p.x),
                f64::from(p.y.floor()) + 0.92,
                f64::from(p.z),
            ));
            break;
        }
        if ctx.is_solid(b) {
            break;
        }
    }
    match hit {
        Some(pos) => {
            let wait = 3.0 + ctx.rngs.ambient.next_float() * 7.0;
            if let Some(v) = world.get_mut::<Vitals>(id) {
                v.fishing = Some(Fishing {
                    bobber: pos,
                    wait,
                    bite: 0.0,
                });
                v.swing = 0.4;
            }
            ctx.events.push(EcsEvent::Cast { pos });
        }
        None => notice(ctx, "Aim at open water to fish."),
    }
}

/// Relève la ligne : prise si un poisson mord, sinon on remonte à vide.
fn reel_in(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    data: &SurvivalData,
    id: EntityId,
    slot: i32,
    f: Fishing,
) {
    if let Some(v) = world.get_mut::<Vitals>(id) {
        v.fishing = None;
        v.swing = 0.4;
    }
    ctx.events.push(EcsEvent::Reel { pos: f.bobber });
    if f.bite > 0.0 {
        let r = ctx.rngs.ambient.next_float();
        if let Some(d) = data.fishing_catch(r) {
            let n = d.min + ctx.rngs.ambient.next_int_bound(d.max - d.min + 1);
            give(world, ctx, id, d.item, n);
            ctx.events.push(EcsEvent::Caught { item: d.item });
        }
        wear_held(world, ctx, data, id, slot, 1);
    }
}

/// Repas : maintenir le clic droit avec une nourriture en main (appelée à chaque tick).
pub(crate) fn eating(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    data: &SurvivalData,
    id: EntityId,
    slot: i32,
    dt: f32,
) {
    let held = ctx.input.is_pressed(GameAction::Secondary);
    let Some(mut v) = world.copy::<Vitals>(id) else {
        return;
    };
    if !held {
        v.eating = 0.0;
        v.eat_lock = false;
        world.insert(id, v);
        return;
    }
    let food = held_stack(world, id, slot).and_then(|s| data.food(s.item).map(|f| (s.item, f)));
    let Some((item, f)) = food else {
        v.eating = 0.0;
        world.insert(id, v);
        return;
    };
    if v.eat_lock || (v.hunger >= MAX_HUNGER - 0.1 && v.warmth >= MAX_WARMTH - 0.1) {
        world.insert(id, v);
        return;
    }
    let before = v.eating;
    v.eating += dt;
    if (before / 0.3).floor() != (v.eating / 0.3).floor() {
        ctx.events.push(EcsEvent::Eating { item });
    }
    if v.eating < EAT_TIME {
        world.insert(id, v);
        return;
    }
    v.eating = 0.0;
    v.hunger = (v.hunger + f.hunger).min(MAX_HUNGER);
    v.saturation = (v.saturation + f.saturation).min(v.hunger);
    v.warmth = (v.warmth + f.warmth).min(MAX_WARMTH);
    v.eat_lock = true;
    world.insert(id, v);
    if f.health > 0.0 {
        if let Some(h) = world.get_mut::<Health>(id) {
            h.health = (h.health + f.health).min(h.max_health);
        }
    }
    if let Some(inv) = world.get_mut::<InventoryComp>(id) {
        inv.0.remove_one(HOTBAR_START + slot);
    }
    ctx.events.push(EcsEvent::Ate { item });
}

// ---------------------------------------------------------------------------------------------
// Systèmes
// ---------------------------------------------------------------------------------------------

/// Faim, froid, endurance, régénération, pêche, usure de l'armure.
#[derive(Debug, Default, Clone, Copy)]
pub struct VitalsSystem;

impl System for VitalsSystem {
    fn name(&self) -> &'static str {
        "Vitals"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let Some(data) = ctx.survival else { return };
        let Some(&id) = world.query_ids::<(&Player, &Vitals)>().first() else {
            return;
        };
        let (mode, pos) = {
            let Some(p) = world.copy::<Player>(id) else {
                return;
            };
            (p.game_mode, world.copy::<Transform>(id).map(|t| t.pos))
        };
        let Some(mut v) = world.copy::<Vitals>(id) else {
            return;
        };
        v.attack_cd = (v.attack_cd - dt).max(0.0);
        v.mine_cooldown = (v.mine_cooldown - dt).max(0.0);
        v.swing = (v.swing - dt).max(0.0);
        if !ctx.input.is_pressed(GameAction::Secondary) {
            v.eating = 0.0;
        }
        if mode != GameMode::Survival {
            // Créatif : tout est plein, aucune règle.
            v.hunger = MAX_HUNGER;
            v.saturation = 5.0;
            v.warmth = MAX_WARMTH;
            v.stamina = MAX_STAMINA;
            v.exhausted = false;
            v.mining = None;
            v.fishing = None;
            v.reduction = 0.0;
            world.insert(id, v);
            return;
        }
        let Some(pos) = pos else { return };

        // --- Armure : défense, isolation, usure ---
        let (defense, insulation) = world
            .get::<InventoryComp>(id)
            .map(|inv| {
                let mut d = 0.0;
                let mut w = 0.0;
                for s in ArmorSlot::ALL {
                    if let Some(a) = inv.0.armor(s).and_then(|st| data.armor(st.item)) {
                        d += a.defense;
                        w += a.warmth;
                    }
                }
                (d, w)
            })
            .unwrap_or((0.0, 0.0));
        v.defense = defense;
        v.reduction = (defense * DEFENSE_PER_POINT).min(MAX_REDUCTION);
        v.insulation = insulation.min(0.95);
        while v.armor_hits > 0 {
            v.armor_hits -= 1;
            wear_random_armor(world, ctx, data, id);
        }

        // --- Température ---
        let ids = *ctx.block_reg.ids();
        let (bx, by, bz) = (
            pos.x.floor() as i32,
            pos.y.floor() as i32,
            pos.z.floor() as i32,
        );
        let biome = ctx
            .blocks
            .biome_at(bx, bz)
            .map_or(0.0, |b| data.biome_temp(b.name()));
        let day = {
            let angle = ctx.config.time_of_day / 1200.0 * std::f32::consts::TAU;
            (-angle.sin()).clamp(-1.0, 1.0)
        };
        let time_term = 0.12 * day - 0.1;
        let alt_term = -((by as f32 - 64.0) / 128.0 * 0.3).clamp(-0.1, 0.3);
        let body = ctx.blocks.block(bx, by + 1, bz);
        let feet = ctx.blocks.block(bx, by, bz);
        v.wet = feet == ids.water || body == ids.water;
        let wet_term = if v.wet { -0.45 } else { 0.0 };
        // Abri : un bloc solide au-dessus de la tête.
        let sheltered = (2..12).any(|k| {
            let b = ctx.blocks.block(bx, by + k, bz);
            b != ids.air && ctx.is_solid(b)
        });
        let mut ambient = biome + time_term + alt_term + wet_term;
        if sheltered {
            ambient += 0.08;
            if by < 52 {
                ambient = ambient.max(-0.12);
            }
        }
        v.ambient = ambient;
        // Sources de chaleur (balayage espacé).
        v.heat_scan -= dt;
        if v.heat_scan <= 0.0 && data.has_heat_sources() {
            v.heat_scan = 0.4;
            let mut heat = 0.0f32;
            for dx in -4..=4 {
                for dz in -4..=4 {
                    for dy in -2..=3 {
                        let h = data.heat_of(ctx.blocks.block(bx + dx, by + dy, bz + dz));
                        if h > 0.0 {
                            let d = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
                            heat = heat.max(h * (1.0 - d / 5.5).max(0.0));
                        }
                    }
                }
            }
            v.heat = heat;
        }
        let felt = ambient + v.heat * 0.95 + v.insulation;
        let before_warm = v.warmth;
        if felt >= -0.1 {
            v.warmth = (v.warmth + 6.0 * (felt + 0.1).clamp(0.2, 1.0) * dt).min(MAX_WARMTH);
        } else {
            v.warmth = (v.warmth - (-0.1 - felt) * 2.4 * dt).max(0.0);
        }
        if v.warmth < 30.0 && before_warm >= 30.0 && v.cold_notice < 1 {
            v.cold_notice = 1;
            notice(ctx, "You are getting cold. Find a fire or put on furs.");
        }
        if v.warmth < 10.0 && v.cold_notice < 2 {
            v.cold_notice = 2;
            notice(ctx, "You are freezing!");
        }
        if v.warmth > 45.0 {
            v.cold_notice = 0;
        }

        // --- Endurance ---
        if v.sprinting {
            v.stamina = (v.stamina - 14.0 * dt).max(0.0);
            if v.stamina <= 0.0 {
                v.exhausted = true;
            }
        } else {
            let rate = if v.hunger <= 0.0 { 6.0 } else { 20.0 };
            v.stamina = (v.stamina + rate * dt).min(MAX_STAMINA);
        }
        if v.jumped {
            v.stamina = (v.stamina - 6.0).max(0.0);
            v.jumped = false;
        }
        if v.exhausted && v.stamina >= 30.0 {
            v.exhausted = false;
        }

        // --- Faim ---
        let mut drain = 0.015;
        if v.sprinting {
            drain += 0.07;
        }
        if v.warmth < 30.0 {
            drain += 0.025;
        }
        v.spend(drain * dt);
        if v.hunger <= 6.0 && !v.hunger_notice {
            v.hunger_notice = true;
            notice(ctx, "Your stomach growls. You need to eat.");
        }
        if v.hunger > 10.0 {
            v.hunger_notice = false;
        }

        // --- Santé : régénération, famine, gel ---
        let mut heal = 0.0;
        let mut harm = 0.0;
        let hurt = world
            .get::<Health>(id)
            .is_some_and(|h| h.health < h.max_health);
        if v.hunger >= 15.0 && v.warmth > 30.0 && hurt {
            v.regen_timer += dt;
            let interval = if v.saturation > 0.5 { 1.3 } else { 2.6 };
            if v.regen_timer >= interval {
                v.regen_timer = 0.0;
                heal = 0.5;
                v.spend(0.35);
            }
        } else {
            v.regen_timer = 0.0;
        }
        let starving = v.hunger <= 0.0;
        let freezing = v.warmth <= 0.0;
        if starving || freezing {
            v.harm_timer += dt;
            if v.harm_timer >= 3.5 {
                v.harm_timer = 0.0;
                harm = 0.5;
            }
        } else {
            v.harm_timer = 0.0;
        }

        // --- Pêche ---
        if let Some(mut f) = v.fishing {
            let rod_held = held_stack(world, id, i32::from(ctx.input.selected_slot))
                .and_then(|s| data.tool(s.item))
                .is_some_and(|t| t.kind == ToolKind::Rod);
            let far = (f.bobber - pos).length() > f64::from(CAST_RANGE + 4.0);
            if far || !rod_held {
                v.fishing = None;
                if far {
                    notice(ctx, "The line snapped: too far from the water.");
                }
            } else if f.bite > 0.0 {
                f.bite -= dt;
                if f.bite <= 0.0 {
                    f.bite = 0.0;
                    f.wait = 3.0 + ctx.rngs.ambient.next_float() * 6.0;
                    notice(ctx, "It got away...");
                }
                v.fishing = Some(f);
            } else {
                f.wait -= dt;
                if f.wait <= 0.0 {
                    f.bite = 1.15;
                    ctx.events.push(EcsEvent::FishBite { pos: f.bobber });
                }
                v.fishing = Some(f);
            }
        }

        world.insert(id, v);
        if heal > 0.0 || harm > 0.0 {
            if let Some(h) = world.get_mut::<Health>(id) {
                if heal > 0.0 {
                    h.health = (h.health + heal).min(h.max_health);
                }
                if harm > 0.0 {
                    // La famine n'achève pas ; le gel, si.
                    let floor: f32 = if freezing { 0.0 } else { 1.0 };
                    let nh = (h.health - harm).max(floor.min(h.health));
                    if nh < h.health {
                        h.health = nh;
                        ctx.events.push(EcsEvent::Hurt { cold: freezing });
                    }
                    if h.health <= 0.0 {
                        h.alive = false;
                    }
                }
            }
        }
    }
}

fn wear_random_armor(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    data: &SurvivalData,
    id: EntityId,
) {
    let Some(inv) = world.get_mut::<InventoryComp>(id) else {
        return;
    };
    let worn: Vec<ArmorSlot> = ArmorSlot::ALL
        .into_iter()
        .filter(|s| inv.0.armor(*s).is_some())
        .collect();
    if worn.is_empty() {
        return;
    }
    let slot = worn[ctx.rngs.ambient.next_int_bound(worn.len() as i32) as usize];
    let Some(mut st) = inv.0.armor(slot) else {
        return;
    };
    st.wear = st.wear.saturating_add(1);
    if st.wear >= data.max_durability(st.item) {
        inv.0.set_armor(slot, None);
        ctx.events.push(EcsEvent::ItemBroke { item: st.item });
        notice(ctx, "A piece of your armor broke!");
    } else {
        inv.0.set_armor(slot, Some(st));
    }
}

/// Croissance des cultures près du joueur (tirages aléatoires par colonne, comme les « random
/// ticks »). N'agit que dans le monde simulé (hôte / solo).
#[derive(Debug, Default, Clone, Copy)]
pub struct CropSystem;

/// Rayon (blocs) autour du joueur où les cultures poussent.
pub const CROP_RADIUS: i32 = 24;
const SAMPLES: i32 = 14;

impl System for CropSystem {
    fn name(&self) -> &'static str {
        "Crops"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let Some(data) = ctx.survival else { return };
        let Some(&id) = world.query_ids::<(&Player, &Transform)>().first() else {
            return;
        };
        let Some(t) = world.copy::<Transform>(id) else {
            return;
        };
        let area = std::f32::consts::PI * (CROP_RADIUS * CROP_RADIUS) as f32;
        let mut col = [0u8; 128];
        for _ in 0..SAMPLES {
            let dx = ctx.rngs.ambient.next_int_bound(CROP_RADIUS * 2 + 1) - CROP_RADIUS;
            let dz = ctx.rngs.ambient.next_int_bound(CROP_RADIUS * 2 + 1) - CROP_RADIUS;
            if dx * dx + dz * dz > CROP_RADIUS * CROP_RADIUS {
                continue;
            }
            let (x, z) = (t.pos.x.floor() as i32 + dx, t.pos.z.floor() as i32 + dz);
            if !ctx.blocks.is_chunk_loaded(x, z) {
                continue;
            }
            ctx.blocks.read_column(x, z, &mut col);
            for (y, &b) in col.iter().enumerate() {
                let Some(rule) = data.crop(b) else { continue };
                let below = if y > 0 { col[y - 1] } else { 0 };
                // Une culture ne pousse que sur de la terre labourée.
                if below != data.farmland() {
                    continue;
                }
                let p = area * dt / (SAMPLES as f32 * rule.grow);
                if ctx.rngs.ambient.next_float() < p {
                    let pos = IVec3::new(x, y as i32, z);
                    ctx.blocks.set_block(x, y as i32, z, rule.next);
                    ctx.events.push(EcsEvent::CropGrew {
                        pos,
                        block: rule.next,
                    });
                }
            }
        }
    }
}

/// Butin d'un mob mort : au joueur qui l'a frappé en dernier.
pub fn loot_for_kill(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    mob: EntityId,
    mob_id: &str,
) {
    let Some(data) = ctx.survival else { return };
    let Some(&pid) = world
        .query_ids::<(&Player, &Vitals)>()
        .iter()
        .find(|p| !world.has::<RemotePlayer>(**p))
    else {
        return;
    };
    if world.copy::<Vitals>(pid).and_then(|v| v.last_hit_mob) != Some(mob) {
        return;
    }
    if !survival_active(world, pid, ctx) {
        return;
    }
    let loot = data.mob_loot(mob_id).to_vec();
    roll_drops(world, ctx, pid, &loot);
    if let Some(v) = world.get_mut::<Vitals>(pid) {
        v.last_hit_mob = None;
    }
}

/// Mob mort : identifiant de type.
pub fn mob_type_of(world: &EntityWorld, id: EntityId) -> Option<String> {
    world.get::<MobType>(id).map(|m| m.mob_id.clone())
}

/// Vitesse du joueur (pour l'endurance) : écrit par `PlayerInputSystem`.
pub fn record_motion(world: &mut EntityWorld, id: EntityId, sprinting: bool, jumped: bool) {
    let speed = world
        .copy::<Velocity>(id)
        .map_or(0.0, |v| (v.v.x * v.v.x + v.v.z * v.v.z).sqrt());
    if let Some(v) = world.get_mut::<Vitals>(id) {
        v.sprinting = sprinting && speed > 1.0;
        v.jumped |= jumped;
        v.speed = speed;
    }
}
