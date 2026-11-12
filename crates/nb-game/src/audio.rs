//! Sons du jeu : traduit l'état de la simulation (événements ECS, pas, biome, heure, position
//! sous terre ou sous l'eau…) en demandes au moteur [`nb_audio`]. Sans périphérique audio
//! (tests, mode sans tête) le moteur est muet et tout ceci ne coûte presque rien.
//!
//! Compatible LAN : les autres joueurs (avatars) ont leurs pas, les créatures répliquées chez un
//! client sont traitées comme des créatures locales. Les édits de blocs distants ne produisent
//! pas encore de son côté client (le journal d'éditions sert au réseau).

use std::collections::HashMap;

use nb_audio::dsp::Rng;
use nb_audio::{
    voice_for_mob, AmbienceTargets, AudioEngine, Bus, Environment, Listener, Material, MobCall,
    MusicMode, PlayReq, Sfx, Theme, Volumes,
};
use nb_core::glam::{DVec3, IVec3, Vec3};
use nb_data::{GameSettings, MobBehaviorType, MobMedium};
use nb_ecs::{EcsEvent, EntityId, GameMode, Health, MobType, Player, Transform, Velocity};
use nb_gfx::SkyClock;
use nb_world::{BiomeAmbience, BiomeInfo};

use crate::net::Avatar;
use crate::session::{Registries, WorldSession};
use crate::state::GameState;

/// Ce que le directeur audio lit du jeu à chaque appel.
pub struct AudioCtx<'a> {
    pub session: &'a WorldSession,
    pub regs: &'a Registries,
    pub sky: &'a SkyClock,
    pub state: GameState,
}

/// Une créature suivie pour ses bruits de pas et ses cris.
struct MobSnap {
    pos: DVec3,
    voice: nb_audio::Voice,
    voice_pitch: f32,
    size: f32,
    stride: f32,
    walked: f32,
    medium: MobMedium,
    aggressive: bool,
    next_idle: f32,
    seen: u32,
}

struct AvatarSnap {
    pos: DVec3,
    walked: f32,
    seen: u32,
}

/// Durée entre deux sondages de l'environnement (biome, grotte, eau).
const PROBE_PERIOD: f32 = 0.25;
/// Portée de la plupart des sons du monde.
const WORLD_RANGE: f32 = 40.0;

/// Le chef d'orchestre sonore du jeu.
pub struct AudioDirector {
    engine: AudioEngine,
    rng: Rng,
    mats: HashMap<u8, Option<Material>>,
    // Joueur
    prev_feet: Option<DVec3>,
    walked: f32,
    prev_grounded: bool,
    prev_vy: f32,
    prev_in_water: bool,
    swim_timer: f32,
    prev_health: Option<f32>,
    // Entités
    mobs: HashMap<EntityId, MobSnap>,
    avatars: HashMap<u16, AvatarSnap>,
    generation: u32,
    // Ambiance
    probe_timer: f32,
    probe: Probe,
    theme: Theme,
    pending_theme: Theme,
    pending_theme_time: f32,
    last_volumes: Option<[f32; 5]>,
}

/// Résultat du dernier sondage de l'environnement.
#[derive(Clone, Copy, Debug)]
struct Probe {
    theme: Theme,
    ambience: BiomeAmbience,
    underwater: bool,
    cave: f32,
    roof: bool,
    y: f32,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            theme: Theme::Meadow,
            ambience: BiomeAmbience::default(),
            underwater: false,
            cave: 0.0,
            roof: false,
            y: 70.0,
        }
    }
}

impl std::fmt::Debug for AudioDirector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDirector")
            .field("engine", &self.engine)
            .field("theme", &self.theme)
            .finish_non_exhaustive()
    }
}

/// Matériau principal d'un biome pour les ambiances.
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Lumière du jour de 0 (nuit profonde) à 1 (plein jour) d'après l'horloge du ciel.
pub fn daylight(sky: &SkyClock) -> f32 {
    sky.sun_height() * 0.5 + 0.5
}

/// Thème musical d'un biome (de jour, en surface).
pub fn biome_theme(b: &BiomeInfo) -> Theme {
    theme_from_key(&b.music)
}

/// Thème musical désigné par la clé d'un [`BiomeInfo`] ; clé inconnue : prairie.
fn theme_from_key(key: &str) -> Theme {
    match key {
        "forest" => Theme::Forest,
        "jungle" => Theme::Jungle,
        "desert" => Theme::Desert,
        "snow" => Theme::Snow,
        "mountain" => Theme::Mountain,
        "ocean" => Theme::Ocean,
        "swamp" => Theme::Swamp,
        "volcano" => Theme::Volcano,
        "mystic" => Theme::Mystic,
        _ => Theme::Meadow,
    }
}

/// Intensité des ambiances d'un biome (hors grotte et eau libre).
pub fn biome_ambience(b: &BiomeInfo, daylight: f32, altitude: f32) -> AmbienceTargets {
    ambience_targets(b.ambience, daylight, altitude)
}

fn ambience_targets(a: BiomeAmbience, daylight: f32, altitude: f32) -> AmbienceTargets {
    let day = smooth(0.35, 0.65, daylight);
    let night = 1.0 - smooth(0.2, 0.5, daylight);
    let BiomeAmbience {
        wind,
        birds,
        crickets,
        frogs,
        water,
        lava,
    } = a;
    let high = smooth(90.0, 150.0, altitude) * 0.6;
    AmbienceTargets {
        wind: (wind + high).min(1.0),
        birds: birds * day,
        crickets: crickets * night,
        frogs: frogs * (0.4 + 0.6 * night),
        water,
        lava,
        cave: 0.0,
    }
}

impl AudioDirector {
    pub fn new(engine: AudioEngine) -> Self {
        Self {
            engine,
            rng: Rng::new(0x5EED_0A0D),
            mats: HashMap::new(),
            prev_feet: None,
            walked: 0.0,
            prev_grounded: true,
            prev_vy: 0.0,
            prev_in_water: false,
            swim_timer: 0.0,
            prev_health: None,
            mobs: HashMap::new(),
            avatars: HashMap::new(),
            generation: 0,
            probe_timer: 0.0,
            probe: Probe::default(),
            theme: Theme::Menu,
            pending_theme: Theme::Menu,
            pending_theme_time: 0.0,
            last_volumes: None,
        }
    }

    pub fn engine(&self) -> &AudioEngine {
        &self.engine
    }

    pub fn set_engine(&mut self, e: AudioEngine) {
        self.engine = e;
        self.last_volumes = None;
    }

    /// Thème musical courant (tests).
    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn enabled(&self) -> bool {
        self.engine.is_enabled()
    }

    /// Transmet les volumes des réglages (seulement s'ils ont changé).
    pub fn apply_settings(&mut self, s: &GameSettings) {
        let v = s.volumes();
        if self.last_volumes == Some(v) {
            return;
        }
        self.last_volumes = Some(v);
        self.engine.set_volumes(Volumes {
            master: v[0],
            music: v[1],
            ambient: v[2],
            effects: v[3],
            creatures: v[4],
        });
    }

    // ── Aides ──────────────────────────────────────────────────────────────────────────

    fn material(&mut self, regs: &Registries, id: u8) -> Option<Material> {
        *self
            .mats
            .entry(id)
            .or_insert_with(|| Material::from_block_name(&regs.blocks.from_id(id).name))
    }

    /// Matériau du sol sous `pos` (premier bloc solide dans les deux blocs sous les pieds).
    fn ground(&mut self, ctx: &AudioCtx<'_>, pos: DVec3) -> Material {
        let (x, z) = (pos.x.floor() as i32, pos.z.floor() as i32);
        let y0 = (pos.y - 0.1).floor() as i32;
        for y in [y0, y0 - 1] {
            let id = ctx.session.world.block(x, y, z);
            if ctx.regs.blocks.from_id(id).solid {
                if let Some(m) = self.material(ctx.regs, id) {
                    return m;
                }
            }
        }
        Material::Stone
    }

    fn world_sound(&self, sfx: Sfx, pos: Vec3, bus: Bus, gain: f32, pitch: f32) {
        self.engine.play(
            PlayReq::new(sfx, bus)
                .at(pos.to_array())
                .gain(gain)
                .pitch(pitch)
                .range(WORLD_RANGE),
        );
    }

    fn ui_sound(&self, sfx: Sfx, gain: f32) {
        self.engine.play(PlayReq::new(sfx, Bus::Effects).gain(gain));
    }

    // ── Interface ──────────────────────────────────────────────────────────────────────

    pub fn ui_click(&self) {
        self.ui_sound(Sfx::UiClick, 1.0);
    }

    pub fn ui_tick(&self) {
        self.ui_sound(Sfx::UiSlider, 1.0);
    }

    pub fn ui_craft(&self) {
        self.ui_sound(Sfx::Craft, 1.0);
    }

    /// Changement d'écran : ouverture / fermeture de l'inventaire ou de la pause.
    pub fn state_changed(&self, old: GameState, new: GameState) {
        let overlay = |s: GameState| matches!(s, GameState::Paused | GameState::Inventory);
        if overlay(new) && !overlay(old) {
            self.ui_sound(Sfx::UiOpen, 1.0);
        } else if overlay(old) && new == GameState::Playing {
            self.ui_sound(Sfx::UiClose, 1.0);
        }
    }

    // ── Tick de simulation ─────────────────────────────────────────────────────────────

    /// Après chaque tick : événements ECS, pas du joueur, cris et pas des créatures.
    /// `events` : événements produits par ce tick ; `avatars` : autres joueurs (LAN).
    pub fn on_tick(
        &mut self,
        ctx: &AudioCtx<'_>,
        events: &[EcsEvent],
        avatars: &[Avatar],
        dt: f32,
    ) {
        if !self.enabled() {
            return;
        }
        let in_world = matches!(
            ctx.state,
            GameState::Playing | GameState::Paused | GameState::Inventory
        );
        if !in_world {
            self.prev_feet = None;
            self.mobs.clear();
            self.avatars.clear();
            return;
        }
        let player_pos = ctx.session.camera().position;
        // Événements d'abord : les créatures mortes n'existent déjà plus dans l'ECS, mais leur
        // dernier instantané est encore là.
        for e in events {
            self.on_event(ctx, e);
        }
        if ctx.state == GameState::Playing {
            self.player_tick(ctx, dt);
        }
        self.mob_tick(ctx, dt, player_pos);
        self.avatar_tick(ctx, avatars);
    }

    fn on_event(&mut self, ctx: &AudioCtx<'_>, e: &EcsEvent) {
        match *e {
            EcsEvent::BlockBroken { pos, block } => {
                if let Some(m) = self.material(ctx.regs, block) {
                    self.world_sound(Sfx::Break(m), center(pos), Bus::Effects, 1.0, 1.0);
                }
            }
            EcsEvent::BlockPlaced { pos, block } => {
                if let Some(m) = self.material(ctx.regs, block) {
                    self.world_sound(Sfx::Place(m), center(pos), Bus::Effects, 0.9, 1.0);
                }
            }
            EcsEvent::MobHit { entity } => {
                if let Some(s) = self.mobs.get(&entity) {
                    let p = to_vec(s.pos) + Vec3::Y * (s.size * 0.5);
                    let (v, vp) = (s.voice, s.voice_pitch);
                    self.world_sound(Sfx::Hit, p, Bus::Effects, 0.9, 1.0);
                    self.world_sound(Sfx::Mob(v, MobCall::Hurt), p, Bus::Creatures, 0.9, vp);
                }
            }
            EcsEvent::MobDied { entity } => {
                if let Some(s) = self.mobs.remove(&entity) {
                    let p = to_vec(s.pos) + Vec3::Y * (s.size * 0.5);
                    self.world_sound(
                        Sfx::Mob(s.voice, MobCall::Death),
                        p,
                        Bus::Creatures,
                        1.0,
                        s.voice_pitch,
                    );
                }
            }
            EcsEvent::PlayerDied => {
                self.ui_sound(Sfx::PlayerDeath, 1.0);
                self.prev_health = None;
            }
            EcsEvent::PlayerRespawned => {
                self.ui_sound(Sfx::Respawn, 0.8);
                self.prev_health = None;
                self.prev_feet = None;
            }
            EcsEvent::StationUsed { .. } => self.ui_sound(Sfx::UiOpen, 1.0),
            // --- Survie ---
            EcsEvent::BlockMining { pos, block, .. } => {
                if let Some(m) = self.material(ctx.regs, block) {
                    self.world_sound(Sfx::Place(m), center(pos), Bus::Effects, 0.75, 1.15);
                }
            }
            EcsEvent::Gained { .. } => self.ui_sound(Sfx::Pickup, 0.55),
            EcsEvent::ItemBroke { .. } => self.ui_sound(Sfx::Break(Material::Metal), 1.0),
            EcsEvent::Equipped { .. } => self.ui_sound(Sfx::Place(Material::Metal), 0.8),
            EcsEvent::Tilled { pos } => {
                self.world_sound(Sfx::Break(Material::Dirt), center(pos), Bus::Effects, 0.8, 0.9);
            }
            EcsEvent::Planted { pos } => {
                self.world_sound(Sfx::Place(Material::Dirt), center(pos), Bus::Effects, 0.8, 1.2);
            }
            EcsEvent::Cast { pos } | EcsEvent::Reel { pos } => {
                self.world_sound(Sfx::Splash, to_vec(pos), Bus::Effects, 0.6, 1.3);
            }
            EcsEvent::FishBite { pos } => {
                self.world_sound(Sfx::Splash, to_vec(pos), Bus::Effects, 1.0, 0.9);
            }
            EcsEvent::Caught { .. } => self.ui_sound(Sfx::Pickup, 1.0),
            EcsEvent::Eating { .. } => self.ui_sound(Sfx::Step(Material::Plant), 0.9),
            EcsEvent::Ate { .. } => self.ui_sound(Sfx::Pickup, 0.7),
            EcsEvent::Hurt { .. } => self.ui_sound(Sfx::PlayerHurt, 0.5),
            EcsEvent::MobSpawned { .. }
            | EcsEvent::MobDespawned { .. }
            | EcsEvent::AttackDealt { .. }
            | EcsEvent::Notice(_)
            | EcsEvent::CropGrew { .. }
            | EcsEvent::AnimalAct { .. }
            | EcsEvent::Whoumf { .. }
            | EcsEvent::Avalanche { .. }
            | EcsEvent::IceCrack { .. }
            | EcsEvent::IceBroke { .. } => {}
        }
    }

    fn player_tick(&mut self, ctx: &AudioCtx<'_>, dt: f32) {
        let Some(pid) = ctx.session.player() else {
            return;
        };
        let e = &ctx.session.sim.entities;
        let (Some(t), Some(vel)) = (e.copy::<Transform>(pid), e.copy::<Velocity>(pid)) else {
            return;
        };
        let (flying, creative_fly) = e
            .get::<Player>(pid)
            .map(|p| (p.flying, p.game_mode == GameMode::Creative))
            .unwrap_or((false, false));
        let feet = t.pos;
        let in_water = {
            let id = ctx.session.world.block(
                feet.x.floor() as i32,
                (feet.y + 0.3).floor() as i32,
                feet.z.floor() as i32,
            );
            id == ctx.regs.blocks.ids().water
        };
        // Santé : un coup reçu fait grogner le joueur.
        if let Some(h) = e.copy::<Health>(pid) {
            if let Some(prev) = self.prev_health {
                if h.health < prev - 0.01 && h.alive {
                    self.ui_sound(Sfx::PlayerHurt, 1.0);
                }
            }
            self.prev_health = Some(h.health);
        }
        let dist_h = self.prev_feet.map_or(0.0, |p| {
            ((feet.x - p.x).powi(2) + (feet.z - p.z).powi(2)).sqrt() as f32
        });
        let teleported = self.prev_feet.is_some_and(|p| (feet - p).length() > 4.0);
        if teleported {
            self.walked = 0.0;
        }
        let speed = if dt > 0.0 { dist_h / dt } else { 0.0 };
        let fly = flying && creative_fly;
        if vel.grounded && !fly && !in_water && !teleported {
            self.walked += dist_h;
            let stride = if speed > 5.5 { 2.1 } else { 1.6 };
            if self.walked >= stride {
                self.walked -= stride;
                let m = self.ground(ctx, feet);
                let pos = to_vec(feet);
                let g = if speed > 5.5 { 0.8 } else { 0.6 };
                self.world_sound(Sfx::Step(m), pos, Bus::Effects, g, 1.0);
            }
        }
        // Atterrissage.
        if vel.grounded && !self.prev_grounded && self.prev_vy < -7.0 && !in_water {
            let m = self.ground(ctx, feet);
            let g = (self.prev_vy.abs() / 22.0).clamp(0.3, 1.0);
            self.world_sound(Sfx::Land(m), to_vec(feet), Bus::Effects, g, 1.0);
        }
        // Eau : plouf à l'entrée, brasses en nageant.
        if in_water && !self.prev_in_water && self.prev_feet.is_some() {
            let g = (self.prev_vy.abs() / 8.0).clamp(0.4, 1.0);
            self.world_sound(Sfx::Splash, to_vec(feet), Bus::Effects, g, 1.0);
        }
        self.swim_timer -= dt;
        if in_water && speed > 0.6 && self.swim_timer <= 0.0 {
            self.swim_timer = 0.75;
            self.world_sound(Sfx::Swim, to_vec(feet), Bus::Effects, 0.7, 1.0);
        }
        self.prev_in_water = in_water;
        self.prev_grounded = vel.grounded;
        self.prev_vy = vel.v.y;
        self.prev_feet = Some(feet);
    }

    fn mob_tick(&mut self, ctx: &AudioCtx<'_>, dt: f32, player: Vec3) {
        self.generation = self.generation.wrapping_add(1);
        let epoch = self.generation;
        let e = &ctx.session.sim.entities;
        for id in e.query_ids::<(&Transform, &MobType)>() {
            let Some(t) = e.copy::<Transform>(id) else {
                continue;
            };
            if !self.mobs.contains_key(&id) {
                let Some(mob_id) = e.get::<MobType>(id).map(|m| m.mob_id.clone()) else {
                    continue;
                };
                let cfg = ctx.regs.mobs.get(&mob_id);
                let (voice, vp) = voice_for_mob(&mob_id);
                let size = cfg.map_or(1.0, |c| c.height.max(0.2));
                let width = cfg.map_or(0.6, |c| c.width_x.max(c.width_z));
                // Les grands mobs sont plus graves ; les petits plus aigus.
                let voice_pitch = (vp * (1.6 / (0.6 + size))).clamp(0.45, 2.2);
                let aggressive =
                    cfg.is_some_and(|c| c.behavior_type == MobBehaviorType::Aggressive);
                let next = self.rng.range(1.0, 14.0);
                self.mobs.insert(
                    id,
                    MobSnap {
                        pos: t.pos,
                        voice,
                        voice_pitch,
                        size,
                        stride: (0.45 + width * 0.8).clamp(0.5, 2.4),
                        walked: 0.0,
                        medium: cfg.map_or(MobMedium::Land, |c| c.medium),
                        aggressive,
                        next_idle: next,
                        seen: epoch,
                    },
                );
            }
            // Distance au joueur avant de payer un accès monde.
            let pos = t.pos;
            let d2 = (to_vec(pos) - player).length_squared();
            let near = d2 < 28.0 * 28.0;
            let (step_due, idle_due, snapshot) = {
                let s = self.mobs.get_mut(&id).expect("inséré ci-dessus");
                s.seen = epoch;
                let moved = ((pos.x - s.pos.x).powi(2) + (pos.z - s.pos.z).powi(2)).sqrt() as f32;
                s.pos = pos;
                let mut step_due = false;
                if s.medium == MobMedium::Land && moved < 2.0 {
                    s.walked += moved;
                    if s.walked >= s.stride {
                        s.walked -= s.stride;
                        step_due = near && d2 < 20.0 * 20.0;
                    }
                }
                s.next_idle -= dt;
                let mut idle_due = false;
                if s.next_idle <= 0.0 {
                    let (a, b) = if s.aggressive {
                        (5.0, 14.0)
                    } else {
                        (9.0, 26.0)
                    };
                    s.next_idle = self.rng.range(a, b);
                    idle_due = near;
                }
                (
                    step_due,
                    idle_due,
                    (s.voice, s.voice_pitch, s.size, s.medium),
                )
            };
            let (voice, vpitch, size, medium) = snapshot;
            if step_due {
                let m = self.ground(ctx, pos);
                let g = (0.25 + size * 0.2).clamp(0.2, 0.7);
                let pitch = (1.5 / (0.6 + size)).clamp(0.55, 1.6);
                self.world_sound(Sfx::Step(m), to_vec(pos), Bus::Creatures, g, pitch);
            }
            if idle_due {
                let p = to_vec(pos) + Vec3::Y * (size * 0.5);
                let gain = if medium == MobMedium::Water {
                    0.45
                } else {
                    0.8
                };
                self.world_sound(
                    Sfx::Mob(voice, MobCall::Idle),
                    p,
                    Bus::Creatures,
                    gain,
                    vpitch,
                );
            }
        }
        self.mobs.retain(|_, s| s.seen == epoch);
    }

    fn avatar_tick(&mut self, ctx: &AudioCtx<'_>, avatars: &[Avatar]) {
        let epoch = self.generation;
        for a in avatars {
            let snap = self.avatars.entry(a.id).or_insert(AvatarSnap {
                pos: a.pos,
                walked: 0.0,
                seen: epoch,
            });
            snap.seen = epoch;
            let moved =
                ((a.pos.x - snap.pos.x).powi(2) + (a.pos.z - snap.pos.z).powi(2)).sqrt() as f32;
            let dy = (a.pos.y - snap.pos.y).abs();
            snap.pos = a.pos;
            if moved < 3.0 && dy < 0.5 {
                snap.walked += moved;
            }
            if snap.walked >= 1.7 {
                snap.walked -= 1.7;
                let pos = a.pos;
                let m = self.ground(ctx, pos);
                self.world_sound(Sfx::Step(m), to_vec(pos), Bus::Creatures, 0.6, 1.0);
            }
        }
        let epoch = self.generation;
        self.avatars.retain(|_, s| s.seen == epoch);
    }

    // ── Frame ──────────────────────────────────────────────────────────────────────────

    /// Une fois par image : écouteur, acoustique, ambiances et musique.
    pub fn on_frame(&mut self, ctx: &AudioCtx<'_>, dt: f32) {
        if !self.enabled() {
            return;
        }
        let cam = ctx.session.camera();
        let (yaw, pitch) = (cam.yaw_deg.to_radians(), cam.pitch_deg.to_radians());
        self.engine.set_listener(Listener {
            pos: cam.position.to_array(),
            forward: [
                yaw.sin() * pitch.cos(),
                -pitch.sin(),
                -yaw.cos() * pitch.cos(),
            ],
            up: [0.0, 1.0, 0.0],
        });
        let in_world = matches!(
            ctx.state,
            GameState::Playing | GameState::Paused | GameState::Inventory
        );
        self.probe_timer -= dt;
        if self.probe_timer > 0.0 {
            return;
        }
        self.probe_timer = PROBE_PERIOD;
        if !in_world {
            self.engine.set_environment(Environment::default());
            self.engine.set_ambience(AmbienceTargets::default());
            self.engine.set_music_mode(MusicMode::Continuous);
            self.set_theme(Theme::Menu, dt + PROBE_PERIOD, true);
            return;
        }
        self.probe = self.probe_environment(ctx);
        let p = self.probe;
        let day = daylight(ctx.sky);
        self.engine.set_environment(Environment {
            underwater: p.underwater,
            reverb: if p.underwater {
                0.3
            } else {
                p.cave.max(if p.roof { 0.2 } else { 0.0 })
            },
        });
        let mut amb = ambience_targets(p.ambience, day, p.y);
        // Météo (session 3.3) : le vent local souffle dans l'ambiance, les oiseaux se taisent
        // avant le mauvais temps. Sans météo (`SkyWeather` neutre), rien ne change.
        let w = &ctx.sky.weather;
        if w.wind > 0.0 {
            amb.wind = (amb.wind * 0.4 + w.wind * 0.9).min(1.0);
        }
        amb.birds *= 1.0 - w.birds_quiet.clamp(0.0, 1.0);
        let open = 1.0 - p.cave;
        amb.wind *= open;
        amb.birds *= open;
        amb.crickets *= open;
        amb.frogs *= open;
        amb.water *= open;
        amb.lava *= open.max(0.4);
        amb.cave = p.cave;
        if p.underwater {
            amb = AmbienceTargets {
                water: 0.9,
                ..AmbienceTargets::default()
            };
        }
        self.engine.set_ambience(amb);
        self.engine.set_music_mode(MusicMode::Sparse);
        let theme = if p.underwater {
            Theme::Ocean
        } else if p.cave > 0.5 {
            Theme::Cave
        } else if day < 0.25 && !matches!(p.theme, Theme::Desert | Theme::Volcano) {
            Theme::Night
        } else {
            p.theme
        };
        self.set_theme(theme, PROBE_PERIOD, false);
    }

    /// Changement de thème avec hystérésis (évite les allers-retours aux frontières de biomes).
    fn set_theme(&mut self, t: Theme, dt: f32, immediate: bool) {
        if t == self.theme {
            self.pending_theme = t;
            self.pending_theme_time = 0.0;
            return;
        }
        if t != self.pending_theme {
            self.pending_theme = t;
            self.pending_theme_time = 0.0;
        }
        self.pending_theme_time += dt;
        if immediate || self.pending_theme_time >= 6.0 {
            self.theme = t;
            self.engine.set_theme(t);
        }
    }

    fn probe_environment(&mut self, ctx: &AudioCtx<'_>) -> Probe {
        let cam = ctx.session.camera().position;
        let (x, y, z) = (
            cam.x.floor() as i32,
            cam.y.floor() as i32,
            cam.z.floor() as i32,
        );
        let world = &ctx.session.world;
        let blocks = &ctx.regs.blocks;
        let underwater = world.block(x, y, z) == blocks.ids().water;
        // Épaisseur du plafond : part des 24 blocs au-dessus qui sont pleins et opaques.
        let mut solid = 0;
        let mut any = false;
        for dy in 1..=24 {
            let b = blocks.from_id(world.block(x, y + dy, z));
            if b.solid && !b.transparent {
                solid += 1;
                any = true;
            }
        }
        let frac = solid as f32 / 24.0;
        let cave = smooth(0.3, 0.65, frac);
        let info = world.biome_info_at(x, z);
        Probe {
            theme: biome_theme(info),
            ambience: info.ambience,
            underwater,
            cave,
            roof: any,
            y: cam.y,
        }
    }
}

fn center(p: IVec3) -> Vec3 {
    Vec3::new(p.x as f32 + 0.5, p.y as f32 + 0.5, p.z as f32 + 0.5)
}

fn to_vec(p: DVec3) -> Vec3 {
    Vec3::new(p.x as f32, p.y as f32, p.z as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daylight_follows_the_sky_clock() {
        let mut sky = SkyClock::new();
        sky.set_time(900.0);
        assert!(daylight(&sky) > 0.95);
        sky.set_time(300.0);
        assert!(daylight(&sky) < 0.05);
    }

    /// Biomes vanilla (30) tels que les voit le moteur.
    fn vanilla_biomes() -> Vec<BiomeInfo> {
        let reg = nb_data::BlockRegistry::builtin();
        nb_world::WorldProfile::vanilla(1, std::sync::Arc::new(nb_world::WorldConfig::default()), reg.ids())
            .biomes
    }

    fn biome(name: &str) -> BiomeInfo {
        vanilla_biomes()
            .into_iter()
            .find(|b| b.name == name)
            .unwrap_or_else(|| panic!("biome {name}"))
    }

    #[test]
    fn every_biome_has_theme_and_ambience() {
        for b in vanilla_biomes() {
            let _ = biome_theme(&b);
            let a = biome_ambience(&b, 1.0, 70.0);
            for v in [
                a.wind, a.birds, a.crickets, a.frogs, a.water, a.lava, a.cave,
            ] {
                assert!((0.0..=1.0).contains(&v), "{}", b.name);
            }
        }
    }

    #[test]
    fn ambience_depends_on_time_of_day() {
        let forest = biome("FOREST");
        let day = biome_ambience(&forest, 1.0, 70.0);
        let night = biome_ambience(&forest, 0.0, 70.0);
        assert!(day.birds > 0.9 && night.birds < 0.05);
        assert!(night.crickets > 0.5 && day.crickets < 0.05);
        assert!(biome_ambience(&biome("OCEAN"), 1.0, 62.0).water > 0.9);
        let mountains = biome("MOUNTAINS");
        assert!(
            biome_ambience(&mountains, 1.0, 160.0).wind
                > biome_ambience(&mountains, 1.0, 70.0).wind
        );
    }

    #[test]
    fn themes_match_biomes() {
        assert_eq!(biome_theme(&biome("DESERT")), Theme::Desert);
        assert_eq!(biome_theme(&biome("DEEP_OCEAN")), Theme::Ocean);
        assert_eq!(biome_theme(&biome("SNOWY_TAIGA")), Theme::Snow);
    }


    #[test]
    fn theme_changes_need_to_persist() {
        let mut d = AudioDirector::new(AudioEngine::recording());
        d.set_theme(Theme::Menu, 0.0, true);
        d.set_theme(Theme::Forest, 1.0, false);
        assert_eq!(d.theme(), Theme::Menu);
        d.set_theme(Theme::Meadow, 1.0, false);
        d.set_theme(Theme::Forest, 1.0, false);
        assert_eq!(
            d.theme(),
            Theme::Menu,
            "un thème intermédiaire réinitialise l'attente"
        );
        for _ in 0..7 {
            d.set_theme(Theme::Forest, 1.0, false);
        }
        assert_eq!(d.theme(), Theme::Forest);
    }
}
