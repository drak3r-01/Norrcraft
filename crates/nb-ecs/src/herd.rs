//! Hardes persistantes côté ECS (doc 03 §4.4, session 5.3).
//!
//! Le modèle (position calendaire, effectif) est dans `nb_world::herds`. Ce système ne fait que
//! le lier aux entités : quand un joueur approche d'une harde, elle **se matérialise** en
//! individus portant [`HerdMember`] ; tant qu'elle est matérialisée, son effectif suit les
//! individus vivants (les morts sont des pertes) et ses membres reçoivent comme destination la
//! position calendaire ; quand tous les joueurs s'éloignent, les survivants sont **réabsorbés**.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use nb_core::glam::DVec3;
use nb_data::{MobConfig, Persistence};
use nb_world::calendar::Calendar;
use nb_world::herds::{self, HerdBook, Query};

use crate::brain::Brain;
use crate::components::{EntityId, Transform};
use crate::density::{self, Viewer};
use crate::schedule::{EcsEvent, System, TickCtx};
use crate::spawn::{feet_y, spawn_mob, Spawner};
use crate::world::EntityWorld;

/// Secondes entre deux passes.
pub const INTERVAL: f32 = 1.0;
/// Marge entre la distance de matérialisation et celle de réabsorption.
pub const RELEASE_MARGIN: f64 = 32.0;
/// Cellules examinées au plus par passe qui n'ont jamais été sondées (la recherche des sites
/// lit une soixantaine de biomes).
const SCANS_PER_PASS: usize = 2;
/// Essais de placement d'un individu.
const PLACE_TRIES: usize = 8;

/// Marque un individu comme membre de la harde `(species, cx, cz)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerdMember {
    pub species: String,
    pub cx: i32,
    pub cz: i32,
}

/// Registre des hardes partagé entre le système et la sauvegarde.
pub type HerdHandle = Arc<Mutex<HerdBook>>;

type Key = (String, i32, i32);

/// Une harde matérialisée.
#[derive(Debug, Default)]
struct Live {
    /// Individus qui n'ont pas trouvé de place : ils comptent dans l'effectif et reviendront.
    missing: i32,
}

#[derive(Debug)]
pub struct HerdSystem {
    timer: f32,
    seed: i64,
    book: HerdHandle,
    live: HashMap<Key, Live>,
}

impl HerdSystem {
    pub fn new(seed: i64, book: HerdHandle) -> Self {
        Self {
            timer: INTERVAL,
            seed,
            book,
            live: HashMap::new(),
        }
    }

    pub fn handle(&self) -> HerdHandle {
        self.book.clone()
    }

    /// Nombre de hardes matérialisées (tests, débogage).
    pub fn live_count(&self) -> usize {
        self.live.len()
    }
}

/// Individus vivants de chaque harde.
fn members(world: &EntityWorld) -> HashMap<Key, Vec<EntityId>> {
    let mut out: HashMap<Key, Vec<EntityId>> = HashMap::new();
    for id in world.query_ids::<&HerdMember>() {
        if let Some(m) = world.get::<HerdMember>(id) {
            out.entry((m.species.clone(), m.cx, m.cz))
                .or_default()
                .push(id);
        }
    }
    out
}

/// Remplit à `want` individus autour de `center` ; renvoie les ids créés et le nombre manquant.
fn place(
    world: &mut EntityWorld,
    ctx: &mut TickCtx<'_>,
    cfg: &MobConfig,
    key: &Key,
    center: (f64, f64),
    want: i32,
) -> (Vec<EntityId>, i32) {
    let spread = 5 + (2.0 * f64::from(want.max(1)).sqrt()) as i32;
    let radius = ctx
        .config
        .mob_spawn_radius
        .max(density::MIN_DISTANCE as i32 + 1);
    let (cx, cz) = (center.0.floor() as i32, center.1.floor() as i32);
    let mut points: Vec<DVec3> = Vec::new();
    {
        let spawner = Spawner {
            blocks: &*ctx.blocks,
            reg: ctx.block_reg,
            origin_x: cx,
            origin_z: cz,
            radius,
        };
        let rng = &mut ctx.rngs.spawn;
        for _ in 0..want {
            for _ in 0..PLACE_TRIES {
                let x = cx + rng.next_int_bound(spread * 2 + 1) - spread;
                let z = cz + rng.next_int_bound(spread * 2 + 1) - spread;
                if !spawner.blocks.is_chunk_loaded(x, z) {
                    continue;
                }
                let gy = spawner.find_spawn_y(cfg, x, z);
                if gy < 0 {
                    continue;
                }
                points.push(DVec3::new(
                    f64::from(x) + 0.5,
                    feet_y(cfg, spawner.blocks, spawner.reg, x, gy, z),
                    f64::from(z) + 0.5,
                ));
                break;
            }
        }
    }
    let placed = points.len() as i32;
    let mut ids = Vec::with_capacity(points.len());
    for p in points {
        let id = spawn_mob(world, cfg, p.x, p.y, p.z, &mut ctx.rngs.spawn);
        world.insert(
            id,
            HerdMember {
                species: key.0.clone(),
                cx: key.1,
                cz: key.2,
            },
        );
        ctx.events.push(EcsEvent::MobSpawned { entity: id });
        ids.push(id);
    }
    (ids, want - placed)
}

fn nearest(viewers: &[Viewer], x: f64, z: f64) -> f64 {
    viewers
        .iter()
        .map(|v| (v.pos.x - x).hypot(v.pos.z - z))
        .fold(f64::MAX, f64::min)
}

impl System for HerdSystem {
    fn name(&self) -> &'static str {
        "Herd"
    }

    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        self.timer += dt;
        if self.timer < INTERVAL {
            return;
        }
        self.timer = 0.0;
        let cfgs: Vec<&MobConfig> = ctx
            .mobs
            .all()
            .iter()
            .filter(|c| {
                c.max_count > 0
                    && c.spawn
                        .as_ref()
                        .is_some_and(|s| s.persistence == Persistence::Herd && s.herd.is_some())
            })
            .collect();
        let viewers = density::viewers(world);
        if cfgs.is_empty() || viewers.is_empty() {
            return;
        }

        // Instant : fraction d'année et années écoulées (monotone, pour la repousse).
        let cal = Calendar::new(ctx.config.days_per_month);
        let a = cal.year_fraction(ctx.config.calendar_day, ctx.config.time_of_day);
        let time = cal.elapsed_years(ctx.config.calendar_day, ctx.config.time_of_day);
        let days_per_year = f64::from(cal.days_per_year());

        let mut alive = members(world);
        // Cellules à examiner : voisinage 3 × 3 de chaque joueur, plus les hardes matérialisées.
        let mut cells: Vec<Key> = Vec::new();
        for cfg in &cfgs {
            let rules = cfg
                .spawn
                .as_ref()
                .and_then(|s| s.herd.as_ref())
                .expect("filtré");
            let mut near: Vec<(f64, Key)> = Vec::new();
            let mut seen: HashSet<(i32, i32)> = HashSet::new();
            for v in &viewers {
                let (vx, vz) = herds::cell_of(rules, v.pos.x, v.pos.z);
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        let (cx, cz) = (vx + dx, vz + dz);
                        if seen.insert((cx, cz)) {
                            let c = f64::from(rules.cell);
                            let mid = ((f64::from(cx) + 0.5) * c, (f64::from(cz) + 0.5) * c);
                            near.push((nearest(&viewers, mid.0, mid.1), (cfg.id.clone(), cx, cz)));
                        }
                    }
                }
            }
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            cells.extend(near.into_iter().map(|(_, k)| k));
        }
        for k in self.live.keys() {
            if !cells.contains(k) {
                cells.push(k.clone());
            }
        }

        let mut scans = SCANS_PER_PASS;
        for key in cells {
            let Some(cfg) = cfgs.iter().find(|c| c.id == key.0) else {
                continue;
            };
            let rules = cfg
                .spawn
                .as_ref()
                .and_then(|s| s.herd.as_ref())
                .expect("filtré");
            let herd = {
                let mut book = self.book.lock().unwrap_or_else(|e| e.into_inner());
                let known = book.is_scanned(&key.0, key.1, key.2);
                if !known {
                    if scans == 0 {
                        continue;
                    }
                    scans -= 1;
                }
                let q = Query {
                    seed: self.seed,
                    species: &cfg.id,
                    rules,
                    year_fraction: a,
                    time,
                    days_per_year,
                };
                let blocks = &*ctx.blocks;
                let herd = book.herd_at(&q, key.1, key.2, &|x, z| {
                    blocks.biome_at(x, z).map(|b| b.name.clone())
                });
                herd
            };
            let ids = alive.remove(&key).unwrap_or_default();

            if let Some(live) = self.live.get_mut(&key) {
                // Matérialisée : l'effectif suit les vivants ; destination = position calendaire.
                let count = ids.len() as i32 + live.missing;
                let far = ids
                    .iter()
                    .filter_map(|&id| world.copy::<Transform>(id))
                    .map(|t| nearest(&viewers, t.pos.x, t.pos.z))
                    .fold(f64::MAX, f64::min);
                let release = herd.is_none() || far > f64::from(rules.materialize) + RELEASE_MARGIN;
                if release {
                    let keep = ids.len() as i32 + live.missing;
                    {
                        let mut book = self.book.lock().unwrap_or_else(|e| e.into_inner());
                        book.set_count(&key.0, key.1, key.2, keep as f32, time);
                    }
                    for id in ids {
                        world.destroy(id);
                        ctx.events.push(EcsEvent::MobDespawned { entity: id });
                    }
                    self.live.remove(&key);
                    continue;
                }
                let h = herd.expect("non relâchée");
                {
                    let mut book = self.book.lock().unwrap_or_else(|e| e.into_inner());
                    book.set_count(&key.0, key.1, key.2, count as f32, time);
                }
                let leash = 18.0 + 3.0 * f64::from(count.max(1)).sqrt();
                for &id in &ids {
                    let Some(pos) = world.copy::<Transform>(id).map(|t| t.pos) else {
                        continue;
                    };
                    let off = (pos.x - h.pos.0).hypot(pos.z - h.pos.1) > leash;
                    if let Some(b) = world.get_mut::<Brain>(id) {
                        b.goal = off.then(|| DVec3::new(h.pos.0, pos.y, h.pos.1));
                    }
                }
                // Les individus sans place au premier essai retentent leur chance.
                if live.missing > 0 && !ids.is_empty() {
                    let at = world
                        .copy::<Transform>(ids[0])
                        .map_or(h.pos, |t| (t.pos.x, t.pos.z));
                    live.missing = place(world, ctx, cfg, &key, at, live.missing).1;
                }
                continue;
            }

            // Pas matérialisée : approche d'un joueur ?
            let Some(h) = herd else {
                continue;
            };
            if h.count <= 0 {
                continue;
            }
            let near = nearest(&viewers, h.pos.0, h.pos.1);
            if near > f64::from(rules.materialize) {
                continue;
            }
            let (cx, cz) = (h.pos.0.floor() as i32, h.pos.1.floor() as i32);
            let centre = DVec3::new(h.pos.0, 0.0, h.pos.1);
            let seen = density::in_view(&viewers, centre);
            if seen || !ctx.blocks.is_chunk_loaded(cx, cz) {
                continue;
            }
            let (placed, missing) = place(world, ctx, cfg, &key, h.pos, h.count);
            if placed.is_empty() {
                continue;
            }
            self.live.insert(key, Live { missing });
        }
    }
}
