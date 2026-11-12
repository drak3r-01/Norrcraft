//! Avalanches (session 6.2, doc 04 §4.3).
//!
//! Sur une pente d'au moins 2 pour 1 portant au moins 5 couches de neige, un bruit fort (bloc de
//! pierre cassé) peut déclencher une coulée : un « whoumf » sourd prévient, puis le manteau de la
//! bande de pente descend le long du gradient et s'accumule en bas. Ce qui est sur le trajet est
//! emporté, blessé et enseveli ([`Buried`]) ; on se dégage en cassant la neige au-dessus de soi.
//!
//! Seuls les biomes de haute pente (Fjord, Kalfjäll, Fjällhed) en connaissent ; le jeu n'en
//! déclenche qu'après une chute de neige ou au redoux ([`Instability`]).

use std::collections::HashSet;

use nb_core::glam::{DVec3, IVec3};
use nb_data::{BlockId, BlockRegistry};
use nb_world::weather::{freezing_point, Environment};

use crate::blocks::BlockAccess;
use crate::components::{EntityId, Health, Transform};
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::world::EntityWorld;

/// Pente minimale au départ : 2 blocs de dénivelé pour 1 bloc de distance.
pub const MIN_DROP: f32 = 2.0;
/// Pente minimale pour que la coulée continue.
pub const KEEP_DROP: f32 = 0.5;
/// Neige minimale au départ, en huitièmes de bloc (5 couches).
pub const MIN_SNOW: i32 = 5;
/// Longueur maximale de la coulée en colonnes.
pub const MAX_RUN: usize = 40;
/// Colonnes d'étalement au pied de la pente.
pub const RUNOUT: usize = 4;
/// Délai entre le « whoumf » et la coulée.
pub const WARNING_SECS: f32 = 2.0;
/// Rayon (cases) autour du bruit où l'on cherche un manteau instable.
pub const NOISE_RADIUS: i32 = 8;
/// Dégâts de l'emportement.
pub const SWEEP_DAMAGE: f32 = 6.0;
/// Secondes de réserve d'air sous la neige.
pub const BURIED_GRACE: f32 = 8.0;
/// Intervalle des dégâts d'étouffement.
pub const SUFFOCATE_EVERY: f32 = 1.5;
/// Chance qu'un bruit fort déclenche la coulée.
pub const TRIGGER_CHANCE: f32 = 0.5;
/// Durée pendant laquelle une chute de neige laisse le manteau instable (secondes de jeu).
pub const UNSTABLE_SECS: f32 = 1800.0;

/// Blocs de neige du monde.
#[derive(Debug, Clone, Copy)]
pub struct SnowIds {
    pub layers: [BlockId; 7],
    pub full: BlockId,
    pub packed: BlockId,
    pub air: BlockId,
}

impl SnowIds {
    pub fn new(reg: &BlockRegistry) -> Self {
        let id = |n: &str| reg.get(n).id;
        Self {
            layers: std::array::from_fn(|i| id(&format!("snotacke_{}", i + 1))),
            full: id("sno"),
            packed: id("packad_sno"),
            air: reg.ids().air,
        }
    }

    /// Huitièmes de neige d'un bloc (0 si ce n'est pas de la neige).
    pub fn eighths(&self, b: BlockId) -> i32 {
        if b == self.full {
            8
        } else if b == self.packed {
            2
        } else {
            self.layers
                .iter()
                .position(|&l| l == b)
                .map_or(0, |i| i as i32 + 1)
        }
    }

    pub fn is_snow(&self, b: BlockId) -> bool {
        self.eighths(b) > 0
    }
}

/// Profil d'une colonne : sol et neige posée dessus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    /// Cellule libre au-dessus du sol.
    pub ground: i32,
    /// Huitièmes de neige au-dessus.
    pub snow: i32,
}

impl Column {
    /// Hauteur de la surface de neige, en blocs.
    pub fn surface(&self) -> f32 {
        self.ground as f32 + self.snow as f32 / 8.0
    }
}

/// Lit le profil de la colonne `(x, z)` ; `None` dans l'eau ou hors monde.
pub fn column(blocks: &dyn BlockAccess, reg: &BlockRegistry, ids: &SnowIds, x: i32, z: i32) -> Option<Column> {
    if !blocks.is_chunk_loaded(x, z) {
        return None;
    }
    let mut buf = [ids.air; 128];
    blocks.read_column(x, z, &mut buf);
    let mut snow = 0;
    for y in (0..128).rev() {
        let b = buf[y];
        if b == ids.air {
            continue;
        }
        let e = ids.eighths(b);
        if e > 0 {
            snow += e;
            continue;
        }
        let info = reg.from_id(b);
        if info.solid {
            return Some(Column {
                ground: y as i32 + 1,
                snow,
            });
        }
        if b == reg.ids().water {
            return None;
        }
        // Plantes et feuillages : ignorés (la neige qui les couvre est comptée plus haut).
    }
    None
}

const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Une coulée résolue : colonnes balayées et colonnes d'étalement.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub start: IVec3,
    /// Colonnes balayées (trajet et bande latérale), dans l'ordre de descente.
    pub swept: Vec<(i32, i32)>,
    /// Colonnes où la neige s'accumule.
    pub runout: Vec<(i32, i32)>,
    /// Neige emportée, en huitièmes.
    pub volume: i32,
}

/// La colonne `(x, z)` est-elle un départ possible : 5 couches et une pente de 2 pour 1 ?
pub fn can_start(blocks: &dyn BlockAccess, reg: &BlockRegistry, ids: &SnowIds, x: i32, z: i32) -> bool {
    let Some(c) = column(blocks, reg, ids, x, z) else {
        return false;
    };
    if c.snow < MIN_SNOW {
        return false;
    }
    DIRS.iter().any(|(dx, dz)| {
        column(blocks, reg, ids, x + dx, z + dz).is_some_and(|n| c.surface() - n.surface() >= MIN_DROP)
    })
}

/// Trace la coulée à partir de `(x, z)` : descente par le plus fort gradient.
pub fn plan(blocks: &dyn BlockAccess, reg: &BlockRegistry, ids: &SnowIds, x: i32, z: i32) -> Option<Run> {
    if !can_start(blocks, reg, ids, x, z) {
        return None;
    }
    let mut path = vec![(x, z)];
    let mut seen: HashSet<(i32, i32)> = path.iter().copied().collect();
    let (mut cx, mut cz) = (x, z);
    for _ in 0..MAX_RUN {
        let here = column(blocks, reg, ids, cx, cz)?.surface();
        let mut best: Option<((i32, i32), f32)> = None;
        for (dx, dz) in DIRS {
            let p = (cx + dx, cz + dz);
            if seen.contains(&p) {
                continue;
            }
            let Some(n) = column(blocks, reg, ids, p.0, p.1) else {
                continue;
            };
            let drop = here - n.surface();
            if drop >= KEEP_DROP && best.is_none_or(|b| drop > b.1) {
                best = Some((p, drop));
            }
        }
        let Some((p, _)) = best else {
            break;
        };
        seen.insert(p);
        path.push(p);
        (cx, cz) = p;
    }
    // Bande de trois colonnes : le trajet et ses voisines latérales (même tracé de pente).
    let mut swept = Vec::new();
    let mut runout = Vec::new();
    let split = path.len().saturating_sub(RUNOUT).max(1);
    for (i, &(px, pz)) in path.iter().enumerate() {
        let target = if i >= split { &mut runout } else { &mut swept };
        if !target.contains(&(px, pz)) {
            target.push((px, pz));
        }
        if i + 1 < path.len() {
            let (nx, nz) = path[i + 1];
            let (lx, lz) = (nz - pz, nx - px);
            for s in [-1, 1] {
                let side = (px + lx.abs() * s, pz + lz.abs() * s);
                if side != (px, pz)
                    && !seen.contains(&side)
                    && !target.contains(&side)
                    && column(blocks, reg, ids, side.0, side.1).is_some_and(|c| c.snow > 0)
                {
                    target.push(side);
                }
            }
        }
    }
    let volume = swept
        .iter()
        .filter_map(|&(sx, sz)| column(blocks, reg, ids, sx, sz))
        .map(|c| c.snow)
        .sum();
    let y = column(blocks, reg, ids, x, z)?.ground;
    Some(Run {
        start: IVec3::new(x, y, z),
        swept,
        runout,
        volume,
    })
}

/// Retire toute la neige d'une colonne.
fn clear(blocks: &mut dyn BlockAccess, reg: &BlockRegistry, ids: &SnowIds, x: i32, z: i32) {
    if let Some(c) = column(&*blocks, reg, ids, x, z) {
        let mut y = c.ground;
        while ids.is_snow(blocks.block(x, y, z)) {
            blocks.set_block(x, y, z, ids.air);
            y += 1;
        }
    }
}

/// Pose `eighths` huitièmes de neige sur le sol de la colonne (blocs pleins puis une couche).
fn lay(blocks: &mut dyn BlockAccess, ids: &SnowIds, x: i32, ground: i32, z: i32, eighths: i32) {
    let mut y = ground;
    let mut left = eighths;
    while left >= 8 && y < 126 {
        blocks.set_block(x, y, z, ids.full);
        y += 1;
        left -= 8;
    }
    if left > 0 && y < 127 {
        blocks.set_block(x, y, z, ids.layers[(left.min(7) - 1) as usize]);
    }
}

/// Applique la coulée : la neige des colonnes balayées est retirée et s'accumule au pied.
/// Renvoie le volume déposé (le reste se disperse).
pub fn apply(blocks: &mut dyn BlockAccess, reg: &BlockRegistry, ids: &SnowIds, run: &Run) -> i32 {
    for &(x, z) in &run.swept {
        clear(blocks, reg, ids, x, z);
    }
    if run.runout.is_empty() {
        return 0;
    }
    let share = run.volume / run.runout.len() as i32;
    for &(x, z) in &run.runout {
        let Some(c) = column(&*blocks, reg, ids, x, z) else {
            continue;
        };
        let total = (c.snow + share).min(8 * 6);
        clear(blocks, reg, ids, x, z);
        lay(blocks, ids, x, c.ground, z, total);
    }
    share * run.runout.len() as i32
}

// ---------------------------------------------------------------------------------------------
// Ensevelissement
// ---------------------------------------------------------------------------------------------

/// Entité prise sous la neige : le bloc de neige au-dessus de sa tête l'étouffe peu à peu.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Buried {
    pub secs: f32,
    pub since_hurt: f32,
}

/// Emporte les entités des colonnes balayées jusqu'au pied de la pente et les y ensevelit.
/// Renvoie les entités ensevelies.
pub fn sweep_entities(
    world: &mut EntityWorld,
    blocks: &mut dyn BlockAccess,
    reg: &BlockRegistry,
    ids: &SnowIds,
    run: &Run,
) -> Vec<EntityId> {
    let swept: HashSet<(i32, i32)> = run.swept.iter().copied().collect();
    let Some(&(fx, fz)) = run.runout.last() else {
        return Vec::new();
    };
    let mut caught = Vec::new();
    for id in world.query_ids::<&Transform>() {
        let Some(pos) = world.get::<Transform>(id).map(|t| t.pos) else {
            continue;
        };
        let (px, pz) = (pos.x.floor() as i32, pos.z.floor() as i32);
        if !swept.contains(&(px, pz)) {
            continue;
        }
        caught.push(id);
    }
    let mut buried = Vec::new();
    for id in caught {
        if let Some(h) = world.get_mut::<Health>(id) {
            h.health -= SWEEP_DAMAGE;
            if h.health <= 0.0 {
                h.health = 0.0;
                h.alive = false;
            }
        }
        let Some(c) = column(&*blocks, reg, ids, fx, fz) else {
            continue;
        };
        // Poche d'air de deux cellules sous un bouchon de neige pleine.
        for dy in 0..2 {
            blocks.set_block(fx, c.ground + dy, fz, ids.air);
        }
        blocks.set_block(fx, c.ground + 2, fz, ids.full);
        if let Some(t) = world.get_mut::<Transform>(id) {
            t.pos = DVec3::new(f64::from(fx) + 0.5, f64::from(c.ground), f64::from(fz) + 0.5);
        }
        world.insert(id, Buried::default());
        buried.push(id);
    }
    buried
}

// ---------------------------------------------------------------------------------------------
// Système
// ---------------------------------------------------------------------------------------------

/// Manteau instable : neige fraîche ou redoux.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instability {
    /// Secondes d'instabilité restantes.
    pub left: f32,
}

impl Instability {
    /// Met à jour d'après la météo : une chute de neige remet le compteur à plein, un redoux
    /// (air au-dessus du point de gel) aussi.
    pub fn observe(&mut self, env: Option<&Environment>, dt: f32) {
        match env {
            None => self.left = UNSTABLE_SECS,
            Some(e) => {
                if e.snow > 0.05 || f64::from(e.air) > freezing_point() {
                    self.left = UNSTABLE_SECS;
                } else {
                    self.left = (self.left - dt).max(0.0);
                }
            }
        }
    }

    pub fn active(&self) -> bool {
        self.left > 0.0
    }
}

/// Biomes de haute pente.
pub fn slope_biome(name: &str) -> bool {
    matches!(name, "FJORD" | "KALFJALL" | "FJALLHED")
}

/// Bruits forts, « whoumf », coulées et étouffement.
pub struct AvalancheSystem {
    ids: Option<SnowIds>,
    instability: Instability,
    /// Coulées annoncées : délai restant et départ.
    pending: Vec<(f32, i32, i32)>,
    /// Le biome compte : sans biome connu (tests), toute pente convient.
    pub gated: bool,
}

impl Default for AvalancheSystem {
    fn default() -> Self {
        Self {
            ids: None,
            instability: Instability { left: 0.0 },
            pending: Vec::new(),
            gated: true,
        }
    }
}

impl AvalancheSystem {
    /// Sans condition de biome : toute pente suffit (essais, mondes sans biomes).
    pub fn ungated() -> Self {
        Self {
            gated: false,
            ..Self::default()
        }
    }

    /// Cherche, autour de `at`, le meilleur départ (le plus de neige) d'un manteau instable.
    pub fn find_start(
        blocks: &dyn BlockAccess,
        reg: &BlockRegistry,
        ids: &SnowIds,
        at: IVec3,
        gated: bool,
    ) -> Option<(i32, i32)> {
        let mut best: Option<((i32, i32), i32)> = None;
        for dx in -NOISE_RADIUS..=NOISE_RADIUS {
            for dz in -NOISE_RADIUS..=NOISE_RADIUS {
                let (x, z) = (at.x + dx, at.z + dz);
                if gated {
                    if let Some(b) = blocks.biome_at(x, z) {
                        if !slope_biome(&b.name) {
                            continue;
                        }
                    }
                }
                if !can_start(blocks, reg, ids, x, z) {
                    continue;
                }
                let s = column(blocks, reg, ids, x, z).map_or(0, |c| c.snow);
                if best.is_none_or(|b| s > b.1) {
                    best = Some(((x, z), s));
                }
            }
        }
        best.map(|b| b.0)
    }
}

impl System for AvalancheSystem {
    fn name(&self) -> &'static str {
        "Avalanche"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        let reg = ctx.block_reg;
        let ids = *self.ids.get_or_insert_with(|| SnowIds::new(reg));
        self.instability.observe(ctx.environment, dt);

        // Bruits de ce tick : un bloc non neigeux cassé est un coup sourd.
        if self.instability.active() {
            let noises: Vec<IVec3> = ctx
                .events
                .iter()
                .filter_map(|e| match e {
                    EcsEvent::BlockBroken { pos, block } if !ids.is_snow(*block) => Some(*pos),
                    _ => None,
                })
                .collect();
            for pos in noises {
                if ctx.rngs.ambient.next_float() >= TRIGGER_CHANCE {
                    continue;
                }
                let Some((x, z)) = Self::find_start(&*ctx.blocks, reg, &ids, pos, self.gated) else {
                    continue;
                };
                if self.pending.iter().any(|p| (p.1, p.2) == (x, z)) {
                    continue;
                }
                self.pending.push((WARNING_SECS, x, z));
                let y = column(&*ctx.blocks, reg, &ids, x, z).map_or(pos.y, |c| c.ground);
                ctx.events.push(EcsEvent::Whoumf {
                    pos: IVec3::new(x, y, z),
                });
            }
        }

        // Coulées dont le délai est écoulé.
        let mut due = Vec::new();
        self.pending.retain_mut(|p| {
            p.0 -= dt;
            if p.0 <= 0.0 {
                due.push((p.1, p.2));
                false
            } else {
                true
            }
        });
        for (x, z) in due {
            let Some(run) = plan(&*ctx.blocks, reg, &ids, x, z) else {
                continue;
            };
            let dropped = apply(ctx.blocks, reg, &ids, &run);
            // Les poches des ensevelis sont creusées après le dépôt, dans la neige accumulée.
            let _ = sweep_entities(world, ctx.blocks, reg, &ids, &run);
            ctx.events.push(EcsEvent::Avalanche {
                pos: run.start,
                columns: run.swept.len() as u32,
                volume: dropped.max(0) as u32,
            });
        }

        // Étouffement des ensevelis.
        for id in world.query_ids::<&Buried>() {
            let Some(pos) = world.get::<Transform>(id).map(|t| t.pos) else {
                continue;
            };
            let cap = IVec3::new(pos.x.floor() as i32, pos.y.floor() as i32 + 2, pos.z.floor() as i32);
            if !ids.is_snow(ctx.blocks.block_at(cap)) {
                world.remove::<Buried>(id);
                continue;
            }
            let hurt = {
                let Some(b) = world.get_mut::<Buried>(id) else {
                    continue;
                };
                b.secs += dt;
                if b.secs < BURIED_GRACE {
                    false
                } else {
                    b.since_hurt += dt;
                    if b.since_hurt >= SUFFOCATE_EVERY {
                        b.since_hurt = 0.0;
                        true
                    } else {
                        false
                    }
                }
            };
            if hurt {
                if let Some(h) = world.get_mut::<Health>(id) {
                    h.health -= 1.0;
                    if h.health <= 0.0 {
                        h.health = 0.0;
                        h.alive = false;
                    }
                }
            }
        }
    }
}
