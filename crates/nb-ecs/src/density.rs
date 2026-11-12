//! Apparition par densité cible (doc 03 §4.2, session 5.2).
//!
//! ```text
//! cible(e) = Σ_biomes surface_b · densité(e, b) / 4096
//!            × saison × heure × météo × lumière × cycle × naturfrid
//! ```
//!
//! À chaque passe (1 Hz, tour de rôle entre les joueurs) on relève les biomes de la zone active
//! (disque de rayon R autour du joueur, un échantillon tous les 8 blocs), on compte les individus
//! présents de chaque espèce à règles `spawn`, et on tire jusqu'à [`ATTEMPTS`] points : l'espèce
//! est choisie au prorata de son **déficit** `max(0, cible − présents)`, le point est accepté avec
//! la probabilité `densité(biome) / densité max`, puis validé par les prédicats d'habitat du
//! milieu (air, eau, amphibie, glace, terre), de couvert, de trait de décor, d'évitement et de
//! visibilité (hors du cône de vue du joueur à moins de 80 blocs, jamais à moins de 24).

use std::collections::{HashMap, HashSet};

use nb_core::glam::DVec3;
use nb_core::JavaRandom;
use nb_data::behavior::{BehaviorBook, Stimulus};
use nb_data::{BlockId, BlockRegistry, MobConfig, Persistence, Sky, SpawnRules};
use nb_world::weather::Environment;

use crate::blocks::BlockAccess;
use crate::brain::clock::{self, Clock};
use crate::brain::Emitter;
use crate::components::{MobType, Player, RemotePlayer, Transform};
use crate::schedule::{EcsEvent, TickCtx};
use crate::spawn::{feet_y, spawn_mob, Spawner};
use crate::world::EntityWorld;

/// Points tirés par passe.
pub const ATTEMPTS: usize = 4;
/// Distance minimale au joueur d'un point d'apparition.
pub const MIN_DISTANCE: f64 = 24.0;
/// Au-delà, un point n'est plus soumis au cône de vue.
pub const VIEW_DISTANCE: f64 = 80.0;
/// Demi-ouverture (radians) du cône de vue interdit : 65°.
const VIEW_HALF_ANGLE: f64 = 1.134;
/// Pas du relevé des biomes (un échantillon représente 8 × 8 blocs).
pub const SURVEY_STEP: i32 = 8;
/// Probabilité par seconde qu'un individu éphémère hors saison hors de vue s'en aille.
pub(crate) const VANISH_CHANCE: f32 = 0.25;
/// Rayon de la recherche d'un trait de décor.
const FEATURE_REACH: i32 = 6;
/// Hauteur de recherche d'un couvert.
const SHELTER_REACH: i32 = 10;

/// Surface (en blocs carrés) de chaque biome dans la zone active.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Survey {
    pub area: HashMap<String, f32>,
}

impl Survey {
    /// Relève les colonnes chargées du disque de rayon `radius` autour de `(ox, oz)`.
    pub fn take(blocks: &dyn BlockAccess, ox: i32, oz: i32, radius: i32) -> Self {
        let mut area: HashMap<String, f32> = HashMap::new();
        let cell = (SURVEY_STEP * SURVEY_STEP) as f32;
        let r2 = i64::from(radius) * i64::from(radius);
        let half = SURVEY_STEP / 2;
        let mut dx = -radius;
        while dx < radius {
            let mut dz = -radius;
            while dz < radius {
                let (cx, cz) = (dx + half, dz + half);
                if i64::from(cx) * i64::from(cx) + i64::from(cz) * i64::from(cz) <= r2 {
                    let (x, z) = (ox.wrapping_add(cx), oz.wrapping_add(cz));
                    if blocks.is_chunk_loaded(x, z) {
                        if let Some(b) = blocks.biome_at(x, z) {
                            *area.entry(b.name.clone()).or_insert(0.0) += cell;
                        }
                    }
                }
                dz += SURVEY_STEP;
            }
            dx += SURVEY_STEP;
        }
        Self { area }
    }

    pub fn is_empty(&self) -> bool {
        self.area.is_empty()
    }

    /// Σ surface_b · densité(e, b) / 4096 : effectif visé avant les facteurs du moment.
    pub fn base_target(&self, rules: &SpawnRules) -> f32 {
        self.area
            .iter()
            .map(|(b, a)| a * rules.density(b))
            .sum::<f32>()
            / 4096.0
    }
}

/// Instant et ciel vus par les règles.
#[derive(Debug, Clone, Copy)]
pub struct Moment {
    pub clock: Clock,
    pub sky: Sky,
}

impl Moment {
    pub fn of(cfg: &nb_world::WorldConfig, env: Option<&Environment>) -> Self {
        Self {
            clock: Clock::of(cfg, env.map(|e| e.sun)),
            sky: env.map(sky_of).unwrap_or_default(),
        }
    }
}

/// Ciel dérivé de la météo vécue.
pub fn sky_of(e: &Environment) -> Sky {
    Sky {
        rain: e.rain,
        snow: e.snow,
        fog: e.fog,
        wind: e.wind.speed,
    }
}

/// Produit des facteurs du moment : saison × heure × météo × lumière × cycle × naturfrid.
pub fn climate_factor(book: &BehaviorBook, cfg: &MobConfig, rules: &SpawnRules, m: &Moment) -> f32 {
    let season = rules.season_factor(m.clock.year_fraction);
    let hour = clock::activity(rules.time_of_day, m.clock.sun);
    let weather = rules.weather_factor(m.sky);
    let light = match rules.light {
        Some(max) if m.clock.sun > max => 0.0,
        _ => 1.0,
    };
    let cycle = match (
        &rules.cycle,
        cfg.behavior.as_deref().and_then(|n| book.get(n)),
    ) {
        (Some(c), _) => book.ref_factor(c, m.clock.year),
        (None, Some(p)) => book.cycle_factor(p, m.clock.year),
        (None, None) => 1.0,
    };
    season * hour * weather * light * cycle * naturfrid(rules)
}

/// Réputation du joueur (doc 04 §7) : pas encore branchée, neutre pour l'instant.
fn naturfrid(_rules: &SpawnRules) -> f32 {
    1.0
}

/// Cible complète de l'espèce dans la zone relevée.
pub fn target(
    book: &BehaviorBook,
    cfg: &MobConfig,
    rules: &SpawnRules,
    survey: &Survey,
    m: &Moment,
) -> f32 {
    let t = survey.base_target(rules) * climate_factor(book, cfg, rules, m);
    if rules.persistence == Persistence::Unique {
        t.min(1.0)
    } else {
        t
    }
}

/// Un joueur : position des pieds et lacet.
#[derive(Debug, Clone, Copy)]
pub struct Viewer {
    pub pos: DVec3,
    pub yaw: f32,
}

pub fn viewers(world: &EntityWorld) -> Vec<Viewer> {
    let mut ids = world.query_ids::<(&Player, &Transform)>();
    ids.extend(world.query_ids::<(&RemotePlayer, &Transform)>());
    ids.into_iter()
        .filter_map(|id| world.copy::<Transform>(id))
        .map(|t| Viewer {
            pos: t.pos,
            yaw: t.yaw,
        })
        .collect()
}

/// Le point est-il à portée de la vue d'un joueur (cône de 130° à moins de 80 blocs, ou à
/// moins de 24 blocs dans toutes les directions) ?
pub fn in_view(viewers: &[Viewer], p: DVec3) -> bool {
    viewers.iter().any(|v| {
        let (dx, dz) = (p.x - v.pos.x, p.z - v.pos.z);
        let d = (dx * dx + dz * dz).sqrt();
        if d < MIN_DISTANCE {
            return true;
        }
        if d > VIEW_DISTANCE {
            return false;
        }
        // Avant du joueur : (sin lacet, cos lacet), voir `yaw_from_heading`.
        let (fx, fz) = (f64::from(v.yaw).sin(), f64::from(v.yaw).cos());
        let cos = (dx * fx + dz * fz) / d;
        cos > VIEW_HALF_ANGLE.cos()
    })
}

/// Espèces à règles dont le facteur du moment est nul (hibernant, migrateur parti, cycle au
/// plus bas) : leurs éphémères hors de vue s'en vont.
pub fn extinct(ctx: &TickCtx<'_>, m: &Moment) -> HashSet<String> {
    let book = BehaviorBook::builtin();
    ctx.mobs
        .all()
        .iter()
        .filter_map(|c| {
            let r = c.spawn.as_ref()?;
            (r.persistence == Persistence::Ephemeral && climate_factor(&book, c, r, m) <= 0.0)
                .then(|| c.id.clone())
        })
        .collect()
}

/// Une passe d'apparition par densité pour le joueur dont le point de référence est `origin`.
pub fn run(world: &mut EntityWorld, ctx: &mut TickCtx<'_>, origin: (i32, i32), players: i32) {
    let cfgs: Vec<&MobConfig> = ctx
        .mobs
        .all()
        .iter()
        .filter(|c| {
            c.max_count > 0
                && c.spawn
                    .as_ref()
                    .is_some_and(|s| s.persistence != Persistence::Herd)
        })
        .collect();
    if cfgs.is_empty() {
        return;
    }
    let book = BehaviorBook::builtin();
    let radius = ctx.config.mob_spawn_radius.max(MIN_DISTANCE as i32 + 1);
    let moment = Moment::of(ctx.config, ctx.environment);
    let survey = Survey::take(&*ctx.blocks, origin.0, origin.1, radius);
    if survey.is_empty() {
        return;
    }

    // Effectifs : dans la zone active, et dans tout le monde (plafond).
    let mut near: HashMap<&str, i32> = HashMap::new();
    let mut total: HashMap<&str, i32> = HashMap::new();
    let r2 = f64::from(radius) * f64::from(radius);
    for id in world.query_ids::<(&MobType, &Transform)>() {
        let (Some(m), Some(t)) = (world.get::<MobType>(id), world.copy::<Transform>(id)) else {
            continue;
        };
        let Some(cfg) = cfgs.iter().find(|c| c.id == m.mob_id) else {
            continue;
        };
        *total.entry(cfg.id.as_str()).or_insert(0) += 1;
        let (dx, dz) = (t.pos.x - f64::from(origin.0), t.pos.z - f64::from(origin.1));
        if dx * dx + dz * dz <= r2 {
            *near.entry(cfg.id.as_str()).or_insert(0) += 1;
        }
    }

    let mut deficits: Vec<(&MobConfig, f32)> = Vec::new();
    for &cfg in &cfgs {
        let rules = cfg.spawn.as_ref().expect("filtré");
        let have_total = total.get(cfg.id.as_str()).copied().unwrap_or(0);
        let cap = cfg.max_count.saturating_mul(players);
        let unique_taken = rules.persistence == Persistence::Unique && have_total >= 1;
        if have_total >= cap || unique_taken {
            continue;
        }
        let want = target(&book, cfg, rules, &survey, &moment);
        let have = near.get(cfg.id.as_str()).copied().unwrap_or(0) as f32;
        let deficit = want - have;
        if deficit > 0.0 {
            deficits.push((cfg, deficit));
        }
    }
    if deficits.is_empty() {
        return;
    }

    let viewers = viewers(world);
    let emitters: Vec<(Stimulus, f32, DVec3)> = world
        .query_ids::<(&Emitter, &Transform)>()
        .into_iter()
        .filter_map(|id| {
            let e = world.copy::<Emitter>(id)?;
            Some((e.kind, e.radius, world.copy::<Transform>(id)?.pos))
        })
        .collect();
    let spawner = Spawner {
        blocks: &*ctx.blocks,
        reg: ctx.block_reg,
        origin_x: origin.0,
        origin_z: origin.1,
        radius,
    };
    let site = Site {
        spawner: &spawner,
        viewers: &viewers,
        emitters: &emitters,
    };

    let mut spawned: Vec<(&MobConfig, Vec<DVec3>)> = Vec::new();
    for _ in 0..ATTEMPTS {
        let sum: f32 = deficits.iter().map(|d| d.1).sum();
        if sum <= 0.0 {
            break;
        }
        let rng = &mut ctx.rngs.spawn;
        let mut pick = rng.next_float() * sum;
        let mut chosen = deficits.len() - 1;
        for (i, d) in deficits.iter().enumerate() {
            if pick < d.1 {
                chosen = i;
                break;
            }
            pick -= d.1;
        }
        let cfg = deficits[chosen].0;
        let room = cfg.max_count.saturating_mul(players)
            - total.get(cfg.id.as_str()).copied().unwrap_or(0)
            - spawned
                .iter()
                .filter(|s| s.0.id == cfg.id)
                .map(|s| s.1.len() as i32)
                .sum::<i32>();
        if let Some(points) = site.try_place(cfg, room, rng) {
            deficits[chosen].1 -= points.len() as f32;
            if deficits[chosen].1 <= 0.0 {
                deficits.remove(chosen);
            }
            spawned.push((cfg, points));
        }
        if deficits.is_empty() {
            break;
        }
    }

    for (cfg, points) in spawned {
        for p in points {
            let id = spawn_mob(world, cfg, p.x, p.y, p.z, &mut ctx.rngs.spawn);
            ctx.events.push(EcsEvent::MobSpawned { entity: id });
        }
    }
}

/// Contexte de validation d'un point d'apparition.
struct Site<'a> {
    spawner: &'a Spawner<'a>,
    viewers: &'a [Viewer],
    emitters: &'a [(Stimulus, f32, DVec3)],
}

impl Site<'_> {
    /// Tire un point puis un groupe ; renvoie les positions (pieds) des individus, ou `None`.
    fn try_place(&self, cfg: &MobConfig, room: i32, rng: &mut JavaRandom) -> Option<Vec<DVec3>> {
        let rules = cfg.spawn.as_ref()?;
        let s = self.spawner;
        let r = s.radius;
        let ox = rng.next_int_bound(r.saturating_mul(2)) - r;
        let oz = rng.next_int_bound(r.saturating_mul(2)) - r;
        let dist = f64::from(ox).hypot(f64::from(oz));
        if !(MIN_DISTANCE..=f64::from(r)).contains(&dist) {
            return None;
        }
        let (cx, cz) = (s.origin_x.wrapping_add(ox), s.origin_z.wrapping_add(oz));
        if !s.blocks.is_chunk_loaded(cx, cz) {
            return None;
        }
        // Acceptation par la densité du biome du point.
        let density = rules.density(&s.blocks.biome_at(cx, cz)?.name);
        let top = rules.max_density();
        if density <= 0.0 || top <= 0.0 || rng.next_float() * top > density {
            return None;
        }
        let first = self.valid_point(cfg, rules, cx, cz)?;
        let range = (cfg.group_size_max - cfg.group_size_min).max(0);
        let room = room.max(0);
        if room == 0 {
            return None;
        }
        let want =
            (cfg.group_size_min + rng.next_int_bound((range + 1).min(room).max(1))).clamp(1, room);
        let scatter = cfg.spawn_scatter_radius;
        let mut out = vec![first];
        for _ in 1..want {
            let gx = cx
                + if scatter > 0 {
                    rng.next_int_bound(scatter * 2 + 1) - scatter
                } else {
                    0
                };
            let gz = cz
                + if scatter > 0 {
                    rng.next_int_bound(scatter * 2 + 1) - scatter
                } else {
                    0
                };
            if let Some(p) = self.valid_point(cfg, rules, gx, gz) {
                out.push(p);
            }
        }
        Some(out)
    }

    /// Position des pieds si la colonne `(x, z)` convient à l'espèce, sinon `None`.
    fn valid_point(&self, cfg: &MobConfig, rules: &SpawnRules, x: i32, z: i32) -> Option<DVec3> {
        let s = self.spawner;
        let gy = s.find_spawn_y(cfg, x, z);
        if gy < 0 {
            return None;
        }
        let p = DVec3::new(
            f64::from(x) + 0.5,
            feet_y(cfg, s.blocks, s.reg, x, gy, z),
            f64::from(z) + 0.5,
        );
        if in_view(self.viewers, p) {
            return None;
        }
        if rules.shelter && !sheltered(s.blocks, s.reg, x, gy, z) {
            return None;
        }
        if let Some(name) = &rules.near_feature {
            if !near_block(s.blocks, s.reg, name, x, gy, z) {
                return None;
            }
        }
        if self.avoided(rules, p) {
            return None;
        }
        Some(p)
    }

    fn avoided(&self, rules: &SpawnRules, p: DVec3) -> bool {
        let a = rules.avoid;
        if a.player > 0.0
            && self
                .viewers
                .iter()
                .any(|v| v.pos.distance(p) < f64::from(a.player))
        {
            return true;
        }
        self.emitters.iter().any(|&(kind, _, at)| {
            let r = match kind {
                Stimulus::Fire => a.fire,
                Stimulus::Bell => a.bell,
                _ => 0.0,
            };
            r > 0.0 && at.distance(p) < f64::from(r)
        })
    }
}

/// Un bloc plein ou un feuillage dans les 10 blocs au-dessus de la tête.
fn sheltered(blocks: &dyn BlockAccess, reg: &BlockRegistry, x: i32, y: i32, z: i32) -> bool {
    (1..=SHELTER_REACH).any(|dy| {
        let b = reg.from_id(blocks.block(x, y + 1 + dy, z));
        b.solid || b.name.ends_with("leaves") || b.name.ends_with("_lov")
    })
}

/// Le bloc nommé se trouve-t-il à 6 blocs ou moins (3 en hauteur) ?
fn near_block(
    blocks: &dyn BlockAccess,
    reg: &BlockRegistry,
    name: &str,
    x: i32,
    y: i32,
    z: i32,
) -> bool {
    let want: BlockId = reg.get(name).id;
    if want == reg.ids().air {
        return false;
    }
    for dx in -FEATURE_REACH..=FEATURE_REACH {
        for dz in -FEATURE_REACH..=FEATURE_REACH {
            for dy in -3..=3 {
                if blocks.block(x + dx, y + dy, z + dz) == want {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer(yaw: f32) -> Viewer {
        Viewer {
            pos: DVec3::ZERO,
            yaw,
        }
    }

    #[test]
    fn the_view_cone_follows_the_yaw() {
        // Lacet 0 : l'avant est +z.
        let v = [viewer(0.0)];
        assert!(in_view(&v, DVec3::new(0.0, 64.0, 40.0)), "devant");
        assert!(!in_view(&v, DVec3::new(0.0, 64.0, -40.0)), "derrière");
        assert!(!in_view(&v, DVec3::new(40.0, 64.0, 0.0)), "sur le côté");
        // Tout près : toujours vu.
        assert!(in_view(&v, DVec3::new(0.0, 64.0, -10.0)));
        // Trop loin pour voir.
        assert!(!in_view(&v, DVec3::new(0.0, 64.0, 100.0)));
        // Lacet π/2 : l'avant est +x.
        let v = [viewer(std::f32::consts::FRAC_PI_2)];
        assert!(in_view(&v, DVec3::new(40.0, 64.0, 0.0)));
        assert!(!in_view(&v, DVec3::new(0.0, 64.0, 40.0)));
    }
}
