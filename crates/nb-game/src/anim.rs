//! Animation des mobs et des avatars : fait le lien entre la simulation (positions, santé, IA) et
//! le moteur d'animation `nb-anim`.
//!
//! Tout est **déduit du mouvement affiché** (positions déjà interpolées pour le rendu) : vitesse,
//! rotation, saut, plus quelques événements lus dans l'ECS quand ils existent (dégâts reçus,
//! attaque d'un hostile). Les mobs d'un client LAN, qui ne sont que des positions répliquées,
//! s'animent donc exactement comme ceux de l'hôte, sans rien ajouter au protocole.
//!
//! Coût maîtrisé :
//! * l'état d'animation (`Animator::update`) avance chaque frame pour tous les mobs (quelques
//!   flottants) ;
//! * la palette de matrices (`Animator::evaluate`) n'est recalculée que pour les mobs proches et
//!   dans le champ de la caméra, à cadence réduite avec la distance, et réutilisée sinon (elle
//!   est exprimée dans le repère du modèle : le déplacement du mob ne l'invalide pas) ;
//! * aucune allocation par frame : palettes persistantes par mob, tampons de travail partagés.

use std::collections::HashMap;
use std::sync::Arc;

use nb_anim::{Action, AnimInput, Animator, Mat34, Rig, Scratch};
use nb_core::glam::{DVec3, Vec3};
use nb_data::{MobMedium, MobRegistry};
use nb_ecs::EntityId;

use crate::net::Avatar;

/// Distance (blocs) jusqu'à laquelle la pose est recalculée à chaque frame.
const NEAR: f32 = 28.0;
/// Au-delà : une frame sur 2 jusqu'à `MID`, sur 4 jusqu'à `FAR`, jamais après.
const MID: f32 = 56.0;
const FAR: f32 = 112.0;
/// En deçà, un mob est toujours évalué, même hors du champ de la caméra (ombres, reflets).
const ALWAYS: f32 = 6.0;
/// Déplacement (blocs) au-delà duquel on parle de téléportation : pas de vitesse déduite.
const TELEPORT: f64 = 4.0;

/// Identité d'un acteur animé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimKey {
    Mob(EntityId),
    Player(u16),
}

/// Ce que la simulation sait d'un mob cette frame.
#[derive(Clone, Copy, Debug)]
pub struct MobSample<'a> {
    pub entity: EntityId,
    pub mob_id: &'a str,
    /// Pieds, position affichée (interpolée).
    pub pos: DVec3,
    pub yaw: f32,
    pub grounded: bool,
    pub health: Option<f32>,
    /// Délai de recharge d'attaque d'un hostile (`AggressiveAi`), s'il en a un.
    pub attack_cooldown: Option<f32>,
}

/// Caméra utilisée pour le niveau de détail et le regard.
#[derive(Clone, Copy, Debug)]
pub struct AnimCamera {
    pub eye: Vec3,
    pub forward: Vec3,
    /// Demi-diagonale du champ de vision (rad).
    pub half_diag: f32,
}

struct Entry {
    rig: Arc<Rig>,
    anim: Animator,
    palette: Vec<Mat34>,
    last_pos: DVec3,
    last_yaw: f32,
    last_health: Option<f32>,
    last_attack: Option<f32>,
    /// Temps accumulé depuis la dernière évaluation de la pose.
    stale: f32,
    /// Pose jamais évaluée.
    fresh: bool,
    seen: u64,
    primed: bool,
    water: bool,
}

/// Compteurs de la dernière mise à jour (mesures `NB_PERF`, tests).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimStats {
    pub actors: usize,
    pub evaluated: usize,
}

/// Système d'animation des mobs et des avatars.
pub struct MobAnimSystem {
    /// Rigs compilés par mob, avec la description d'origine (recompilé si elle change).
    rigs: HashMap<String, (Arc<nb_anim::RigDef>, Arc<Rig>)>,
    player_rig: Arc<Rig>,
    entries: HashMap<AnimKey, Entry>,
    scratch: Box<Scratch>,
    frame: u64,
    stats: AnimStats,
}

impl Default for MobAnimSystem {
    fn default() -> Self {
        Self::new()
    }
}

fn wrap_pi(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a + PI).rem_euclid(TAU) - PI
}

impl MobAnimSystem {
    pub fn new() -> Self {
        let player_rig =
            Rig::compile(&nb_anim::humanoid::humanoid()).expect("rig de l'avatar invalide");
        Self {
            rigs: HashMap::new(),
            player_rig: Arc::new(player_rig),
            entries: HashMap::new(),
            scratch: Scratch::new(),
            frame: 0,
            stats: AnimStats::default(),
        }
    }

    /// Statistiques de la dernière mise à jour.
    pub fn stats(&self) -> AnimStats {
        self.stats
    }

    /// Palette de peau d'un acteur (vide : pas de rig, rendu statique).
    pub fn pose(&self, key: AnimKey) -> &[[f32; 12]] {
        self.entries
            .get(&key)
            .map_or(&[][..], |e| bytemuck::cast_slice(&e.palette))
    }

    /// Rig compilé d'un mob (compilé à la première demande ; `None` : modèle statique ou rig
    /// invalide, signalé une fois sur stderr).
    fn rig_of(&mut self, reg: &MobRegistry, mob_id: &str) -> Option<Arc<Rig>> {
        let def = reg.get(mob_id)?.rig.as_ref()?;
        if let Some((d, r)) = self.rigs.get(mob_id) {
            if Arc::ptr_eq(d, def) {
                return Some(Arc::clone(r));
            }
        }
        match Rig::compile(def) {
            Ok(r) => {
                let r = Arc::new(r);
                self.rigs
                    .insert(mob_id.to_owned(), (Arc::clone(def), Arc::clone(&r)));
                Some(r)
            }
            Err(e) => {
                eprintln!("rig de {mob_id} ignoré : {e}");
                None
            }
        }
    }

    /// Avance l'animation de `dt` secondes pour les mobs et avatars donnés.
    pub fn update(
        &mut self,
        dt: f32,
        cam: &AnimCamera,
        reg: &MobRegistry,
        mobs: &[MobSample<'_>],
        avatars: &[Avatar],
    ) {
        self.frame += 1;
        let frame = self.frame;
        let mut stats = AnimStats::default();
        for m in mobs {
            let key = AnimKey::Mob(m.entity);
            if !self.entries.contains_key(&key) {
                let Some(rig) = self.rig_of(reg, m.mob_id) else {
                    continue;
                };
                let n = rig.bone_count();
                let e = Entry {
                    anim: Animator::new(m.entity.0 as u32),
                    palette: vec![Mat34::IDENTITY; n],
                    rig,
                    last_pos: m.pos,
                    last_yaw: m.yaw,
                    last_health: m.health,
                    last_attack: m.attack_cooldown,
                    stale: 0.0,
                    fresh: true,
                    seen: frame,
                    primed: false,
                    water: false,
                };
                self.entries.insert(key, e);
            }
            let cfg = reg.get(m.mob_id);
            let Some(e) = self.entries.get_mut(&key) else {
                continue;
            };
            e.seen = frame;
            e.water = cfg.is_some_and(|c| c.medium == MobMedium::Water);
            // Événements.
            if let (Some(h), Some(last)) = (m.health, e.last_health) {
                if h < last - 0.01 {
                    e.anim.trigger(&e.rig, Action::Hurt);
                }
            }
            e.last_health = m.health;
            if let (Some(cd), Some(last)) = (m.attack_cooldown, e.last_attack) {
                if cd > last + 0.01 {
                    e.anim.trigger(&e.rig, Action::Attack);
                }
            }
            e.last_attack = m.attack_cooldown;
            let height = cfg.map_or(1.0, |c| c.height);
            let aggressive =
                cfg.is_some_and(|c| c.behavior_type == nb_data::MobBehaviorType::Aggressive);
            Self::step(
                e,
                &mut self.scratch,
                &mut stats,
                frame,
                m.entity.0 as u32,
                dt,
                cam,
                m.pos,
                m.yaw,
                m.grounded,
                None,
                height,
                if aggressive { 1.5 } else { 1.0 },
            );
        }
        for a in avatars {
            let key = AnimKey::Player(a.id);
            let yaw = std::f32::consts::PI - a.yaw_deg.to_radians();
            let e = self.entries.entry(key).or_insert_with(|| Entry {
                rig: Arc::clone(&self.player_rig),
                anim: Animator::new(u32::from(a.id) + 1000),
                palette: vec![Mat34::IDENTITY; self.player_rig.bone_count()],
                last_pos: a.pos,
                last_yaw: yaw,
                last_health: None,
                last_attack: None,
                stale: 0.0,
                fresh: true,
                seen: frame,
                primed: false,
                water: false,
            });
            e.seen = frame;
            // Les avatars regardent là où le joueur regarde : la tête suit la hauteur de visée.
            let look = Some([0.0, -a.pitch_deg.to_radians()]);
            Self::step(
                e,
                &mut self.scratch,
                &mut stats,
                frame,
                u32::from(a.id),
                dt,
                cam,
                a.pos,
                yaw,
                true,
                look,
                1.8,
                1.0,
            );
        }
        // Acteurs disparus de la frame (mob mort, déchargé, joueur parti).
        self.entries.retain(|_, e| e.seen == frame);
        stats.actors = self.entries.len();
        self.stats = stats;
    }

    #[allow(clippy::too_many_arguments)]
    fn step(
        e: &mut Entry,
        scratch: &mut Scratch,
        stats: &mut AnimStats,
        frame: u64,
        id: u32,
        dt: f32,
        cam: &AnimCamera,
        pos: DVec3,
        yaw: f32,
        grounded: bool,
        forced_look: Option<[f32; 2]>,
        height: f32,
        look_range_mul: f32,
    ) {
        // Mouvement observé.
        let d = pos - e.last_pos;
        let moved = d.length();
        let (speed, vspeed, turn) = if !e.primed || moved > TELEPORT || dt <= 1e-5 {
            (0.0, 0.0, 0.0)
        } else {
            (
                ((d.x * d.x + d.z * d.z).sqrt() as f32) / dt,
                d.y as f32 / dt,
                wrap_pi(yaw - e.last_yaw) / dt,
            )
        };
        e.primed = true;
        e.last_pos = pos;
        e.last_yaw = yaw;
        // Regard : vers le joueur, devant le mob et à l'arrêt ou presque.
        let to_eye = cam.eye - pos.as_vec3();
        let dist = to_eye.length();
        let mut look = forced_look;
        if look.is_none() {
            if let Some(l) = &e.rig.look {
                if speed < 1.6 && dist < l.range * look_range_mul && dist > 0.6 {
                    let rel = wrap_pi(to_eye.x.atan2(to_eye.z) - yaw);
                    if rel.abs() < 1.9 {
                        let flat = (to_eye.x * to_eye.x + to_eye.z * to_eye.z).sqrt().max(0.1);
                        let up = (cam.eye.y - (pos.y as f32 + height * 0.8)).atan2(flat);
                        look = Some([rel, up]);
                    }
                }
            }
        }
        let inp = AnimInput {
            speed,
            vspeed,
            turn,
            grounded,
            in_water: e.water,
            look,
        };
        e.anim.update(&e.rig, &inp, dt);
        // Pose : cadence selon la distance, et seulement si l'acteur est (presque) visible.
        e.stale += dt;
        let period = if dist <= NEAR {
            1
        } else if dist <= MID {
            2
        } else if dist <= FAR {
            4
        } else {
            0
        };
        let visible = dist <= ALWAYS || {
            let dir = to_eye / dist.max(1e-3);
            // Angle entre l'axe de visée et la direction (caméra → mob) ; marge = rayon du mob.
            let cos = -dir.dot(cam.forward);
            let margin = (2.5 / dist.max(1.0)).min(1.0).asin();
            cos > (cam.half_diag + margin).min(std::f32::consts::PI).cos()
        };
        let phase = (frame + u64::from(id)) % period.max(1);
        let due = period != 0 && phase == 0;
        if visible && period != 0 && (due || e.fresh || e.stale > 0.25) {
            e.anim.evaluate(&e.rig, scratch, &mut e.palette);
            e.stale = 0.0;
            e.fresh = false;
            stats.evaluated += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> AnimCamera {
        AnimCamera {
            eye: Vec3::new(0.0, 70.0, 0.0),
            forward: Vec3::Z,
            half_diag: 1.2,
        }
    }

    fn sample(id: i32, mob: &str, x: f64, z: f64) -> MobSample<'_> {
        MobSample {
            entity: EntityId(id),
            mob_id: mob,
            pos: DVec3::new(x, 70.0, z),
            yaw: 0.0,
            grounded: true,
            health: Some(5.0),
            attack_cooldown: None,
        }
    }

    #[test]
    fn walking_mob_gets_a_moving_pose_and_cleans_up() {
        let reg = MobRegistry::builtin();
        let mut sys = MobAnimSystem::new();
        let mut max_dev = 0.0f32;
        for i in 0..240 {
            // Marche à 1,5 blocs/s vers +Z, devant la caméra.
            let z = 10.0 + 1.5 * f64::from(i) / 60.0;
            let s = sample(1, "reindeer", 0.0, z);
            sys.update(1.0 / 60.0, &cam(), &reg, &[s], &[]);
            let p = sys.pose(AnimKey::Mob(EntityId(1)));
            assert!(!p.is_empty());
            for m in p {
                for (a, b) in m.iter().zip(Mat34::IDENTITY.0) {
                    max_dev = max_dev.max((a - b).abs());
                }
            }
        }
        assert!(max_dev > 0.1, "les pattes bougent : {max_dev}");
        assert_eq!(sys.stats().actors, 1);
        // Plus d'entité : l'entrée disparaît.
        sys.update(1.0 / 60.0, &cam(), &reg, &[], &[]);
        assert!(sys.pose(AnimKey::Mob(EntityId(1))).is_empty());
    }

    #[test]
    fn far_mobs_are_not_evaluated_and_unknown_mobs_are_static() {
        let reg = MobRegistry::builtin();
        let mut sys = MobAnimSystem::new();
        for _ in 0..30 {
            let far = sample(1, "reindeer", 0.0, 400.0);
            let unknown = sample(2, "inconnu", 0.0, 10.0);
            sys.update(1.0 / 60.0, &cam(), &reg, &[far, unknown], &[]);
        }
        assert_eq!(sys.stats().evaluated, 0, "trop loin pour être évalué");
        assert!(
            sys.pose(AnimKey::Mob(EntityId(2))).is_empty(),
            "mob inconnu : statique"
        );
    }

    #[test]
    fn behind_the_camera_is_skipped_but_recovers() {
        let reg = MobRegistry::builtin();
        let mut sys = MobAnimSystem::new();
        let mut evaluated = 0;
        for _ in 0..20 {
            // 20 blocs derrière la caméra.
            sys.update(
                1.0 / 60.0,
                &cam(),
                &reg,
                &[sample(1, "reindeer", 0.0, -20.0)],
                &[],
            );
            evaluated += sys.stats().evaluated;
        }
        assert_eq!(evaluated, 0);
        sys.update(
            1.0 / 60.0,
            &cam(),
            &reg,
            &[sample(1, "reindeer", 0.0, 20.0)],
            &[],
        );
        assert_eq!(
            sys.stats().evaluated,
            1,
            "revenu devant : évalué tout de suite"
        );
    }

    #[test]
    fn damage_and_attack_trigger_reactions() {
        let reg = MobRegistry::builtin();
        let mut sys = MobAnimSystem::new();
        let mut s = sample(1, "reindeer", 0.0, 10.0);
        sys.update(1.0 / 60.0, &cam(), &reg, &[s], &[]);
        s.health = Some(3.0);
        sys.update(1.0 / 60.0, &cam(), &reg, &[s], &[]);
        let e = &sys.entries[&AnimKey::Mob(EntityId(1))];
        assert!(
            e.anim.is_playing_oneshot(),
            "une blessure déclenche l'animation"
        );
    }

    #[test]
    fn avatars_are_animated_too() {
        let reg = MobRegistry::builtin();
        let mut sys = MobAnimSystem::new();
        let mut dev = 0.0f32;
        for i in 0..120 {
            let a = Avatar {
                id: 3,
                name: "Lea".into(),
                pos: DVec3::new(0.0, 70.0, 8.0 + 4.3 * f64::from(i) / 60.0),
                yaw_deg: 180.0,
                pitch_deg: 20.0,
            };
            sys.update(1.0 / 60.0, &cam(), &reg, &[], &[a]);
            for m in sys.pose(AnimKey::Player(3)) {
                dev = dev.max(
                    m.iter()
                        .zip(Mat34::IDENTITY.0)
                        .map(|(a, b)| (a - b).abs())
                        .fold(0.0, f32::max),
                );
            }
        }
        assert!(dev > 0.1);
    }

    /// Mesure (ignorée par défaut) : 400 mobs variés autour de la caméra, coût CPU d'une frame
    /// d'animation. `cargo test --release -p nb-game anim::tests::bench -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_400_mobs() {
        let reg = MobRegistry::builtin();
        let ids: Vec<String> = reg.all().iter().map(|c| c.id.clone()).collect();
        let mut sys = MobAnimSystem::new();
        let frames = 600;
        let t0 = std::time::Instant::now();
        let mut evaluated = 0;
        for f in 0..frames {
            let samples: Vec<MobSample<'_>> = (0..400)
                .map(|i| {
                    let a = i as f64 * 0.37;
                    let r = 8.0 + (i % 40) as f64 * 2.0; // 8 → 86 blocs
                    let t = f64::from(f) / 60.0;
                    MobSample {
                        entity: EntityId(i),
                        mob_id: &ids[i as usize % ids.len()],
                        pos: DVec3::new(a.sin() * r + t * 1.4, 70.0, a.cos() * r + t * 0.9),
                        yaw: a as f32,
                        grounded: true,
                        health: Some(5.0),
                        attack_cooldown: None,
                    }
                })
                .collect();
            sys.update(1.0 / 60.0, &cam(), &reg, &samples, &[]);
            evaluated += sys.stats().evaluated;
        }
        let per = t0.elapsed().as_secs_f64() / f64::from(frames) * 1e6;
        eprintln!(
            "400 mobs : {per:.0} µs/frame (dont allocation des échantillons), {} poses évaluées/frame en moyenne",
            evaluated / frames as usize
        );
    }
}
