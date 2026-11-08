//! Effets visuels du jeu : pont entre la simulation (événements ECS, joueur, monde) et le moteur
//! `nb-particles`. Tourne **à la cadence de rendu** (appelé par [`crate::game::Game::animate`]),
//! indépendamment du pas fixe de la simulation.

use std::collections::HashMap;
use std::sync::Arc;

use nb_core::glam::{DVec3, Vec3};
use nb_data::BlockRegistry;
use nb_ecs::{
    BlockAccess, EcsEvent, EntityId, Player, Simulation, Transform, Velocity, WorldBlocks,
};
use nb_gfx::{SkyClock, TextureArrayCpu};
use nb_particles::{
    presets, Ambient, AmbientCtx, BlockLook, Cell, Emit, GpuParticle, ParticleSystem,
    ParticleWorld, View,
};
use nb_world::World;

/// Capacité du pool (32 octets d'instance par particule : 1 Mio de tampon GPU).
pub const POOL_CAPACITY: usize = 32768;
/// Distance de rendu des particules.
const MAX_DISTANCE: f32 = 72.0;

/// Monde vu par les particules : une table `BlockId → Cell` précalculée.
struct FxWorld<'a> {
    world: &'a World,
    cells: &'a [Cell; 256],
}

impl ParticleWorld for FxWorld<'_> {
    #[inline]
    fn cell(&self, x: i32, y: i32, z: i32) -> Cell {
        self.cells[usize::from(self.world.block(x, y, z))]
    }

    fn leaf_tint(&self, x: i32, z: i32) -> [f32; 3] {
        let t = WorldBlocks(self.world).leaves_tint(x, z);
        [t[0] as f32, t[1] as f32, t[2] as f32]
    }
}

/// Mémoire du suivi du joueur (pas, atterrissage, entrée dans l'eau).
#[derive(Default)]
struct Track {
    grounded: bool,
    in_water: bool,
    vy: f32,
    stride: f32,
    last_eye: Option<Vec3>,
}

pub struct Fx {
    sys: ParticleSystem,
    ambient: Ambient,
    cells: Arc<[Cell; 256]>,
    looks: Box<[BlockLook; 256]>,
    instances: Vec<GpuParticle>,
    rain: f32,
    snow: f32,
    rain_target: f32,
    snow_target: f32,
    track: Track,
    /// Dernière position connue des mobs (pour les effets de coup et de mort, l'entité détruite
    /// n'étant plus lisible).
    mob_pos: HashMap<EntityId, DVec3>,
    eye: Vec3,
    forward: Vec3,
    enabled: bool,
}

impl std::fmt::Debug for Fx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fx")
            .field("alive", &self.sys.len())
            .finish_non_exhaustive()
    }
}

impl Fx {
    pub fn new(reg: &BlockRegistry) -> Self {
        let mut cells = [Cell::Air; 256];
        let ids = reg.ids();
        for b in reg.iter() {
            cells[usize::from(b.id)] = if b.id == ids.air {
                Cell::Air
            } else if b.id == ids.water {
                Cell::Water
            } else if b.id == ids.lava {
                Cell::Lava
            } else if b.id == ids.magma {
                Cell::Magma
            } else if b.name.contains("leaves") {
                Cell::Leaves
            } else if b.solid {
                Cell::Solid
            } else {
                Cell::Air
            };
        }
        Self {
            sys: ParticleSystem::new(POOL_CAPACITY, 0x4e6f_7272),
            ambient: Ambient::new(),
            cells: Arc::new(cells),
            looks: Box::new(
                [BlockLook {
                    layer: 0,
                    tint: [0.6; 3],
                }; 256],
            ),
            instances: Vec::with_capacity(4096),
            rain: 0.0,
            snow: 0.0,
            rain_target: 0.0,
            snow_target: 0.0,
            track: Track::default(),
            mob_pos: HashMap::new(),
            eye: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            enabled: true,
        }
    }

    /// Calcule, pour chaque bloc, la couche de l'atlas et la teinte moyenne de sa texture
    /// (éclats et poussière). À rappeler après un changement de pack de ressources.
    pub fn set_block_atlas(&mut self, reg: &BlockRegistry, atlas: &TextureArrayCpu) {
        for b in reg.iter() {
            // Face latérale (le dessus d'un bloc d'herbe serait vert : la poussière de terre
            // est brune).
            let layer = atlas.layer(&b.texture_names[0]);
            let rgba = atlas.layers.get(layer as usize);
            let mut sum = [0.0f32; 3];
            let mut n = 0.0f32;
            if let Some(px) = rgba {
                for p in px.chunks_exact(4).filter(|p| p[3] > 128) {
                    for (s, c) in sum.iter_mut().zip(p) {
                        *s += f32::from(*c) / 255.0;
                    }
                    n += 1.0;
                }
            }
            let tint = if n > 0.0 {
                [sum[0] / n, sum[1] / n, sum[2] / n]
            } else {
                [0.6; 3]
            };
            self.looks[usize::from(b.id)] = BlockLook {
                layer: layer as u16,
                tint,
            };
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.sys.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.sys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sys.is_empty()
    }

    pub fn system(&self) -> &ParticleSystem {
        &self.sys
    }

    pub fn system_mut(&mut self) -> &mut ParticleSystem {
        &mut self.sys
    }

    /// Instances à dessiner (calculées par la dernière [`Self::frame`]).
    pub fn instances(&self) -> &[GpuParticle] {
        &self.instances
    }

    pub fn clear(&mut self) {
        self.sys.clear();
        self.instances.clear();
        self.track = Track::default();
    }

    /// Météo cible (0..1) ; l'intensité réelle la rejoint en quelques secondes.
    pub fn set_weather(&mut self, rain: f32, snow: f32) {
        self.rain_target = rain.clamp(0.0, 1.0);
        self.snow_target = snow.clamp(0.0, 1.0);
    }

    pub fn weather(&self) -> (f32, f32) {
        (self.rain_target, self.snow_target)
    }

    /// Positions des mobs de la frame (effets de coup / de mort).
    pub fn set_mob_positions(&mut self, mobs: impl Iterator<Item = (EntityId, DVec3)>) {
        self.mob_pos.clear();
        self.mob_pos.extend(mobs);
    }

    fn look(&self, block: u8) -> BlockLook {
        self.looks[usize::from(block)]
    }

    /// Réagit aux événements des ticks écoulés.
    pub fn on_events(&mut self, events: &[EcsEvent]) {
        if !self.enabled {
            return;
        }
        for e in events {
            match *e {
                EcsEvent::BlockBroken { pos, block } => {
                    let c = pos.as_vec3() + Vec3::splat(0.5);
                    let look = self.look(block);
                    self.sys.block_break(c, look, 1.0);
                    if self.cells[usize::from(block)] == Cell::Leaves {
                        let t = look.tint;
                        let e = Emit::at(c, 5).tint([t[0] * 1.6, t[1] * 1.6, t[2] * 1.6]);
                        self.sys.spawn(&presets::LEAF, &e);
                    }
                }
                EcsEvent::BlockPlaced { pos, block } => {
                    let c = pos.as_vec3() + Vec3::splat(0.5);
                    let look = self.look(block);
                    self.sys.block_place(c, look);
                }
                EcsEvent::MobHit { entity } => {
                    if let Some(p) = self.mob_pos.get(&entity) {
                        self.sys.mob_hit(p.as_vec3() + Vec3::Y * 0.8);
                    }
                }
                EcsEvent::MobDied { entity } => {
                    if let Some(p) = self.mob_pos.get(&entity) {
                        self.sys.mob_death(p.as_vec3() + Vec3::Y * 0.7);
                    }
                }
                _ => {}
            }
        }
    }

    /// Avance les particules de `dt` (temps réel) et prépare les instances de la frame.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        dt: f32,
        world: &World,
        sim: &Simulation,
        eye: Vec3,
        forward: Vec3,
        sky: &SkyClock,
        local_light: f32,
        eye_in_water: bool,
    ) {
        if !self.enabled {
            self.instances.clear();
            return;
        }
        let dt = dt.clamp(0.0, 0.1);
        // Téléportation / changement de monde : on repart de zéro.
        if self
            .track
            .last_eye
            .is_some_and(|p| (p - eye).length_squared() > 64.0 * 64.0)
        {
            self.clear();
        }
        self.track.last_eye = Some(eye);
        self.eye = eye;
        self.forward = forward;
        let cells = Arc::clone(&self.cells);
        let fw = FxWorld {
            world,
            cells: &cells,
        };
        // Météo : l'intensité suit la cible (≈ 4 s de fondu).
        let k = (dt * 0.25).min(1.0);
        self.rain += (self.rain_target - self.rain) * k;
        self.snow += (self.snow_target - self.snow) * k;
        if self.rain_target == 0.0 && self.rain < 0.01 {
            self.rain = 0.0;
        }
        if self.snow_target == 0.0 && self.snow < 0.01 {
            self.snow = 0.0;
        }

        self.track_player(dt, &fw, sim);

        let params = sky.params();
        let ctx = AmbientCtx {
            eye,
            forward,
            rain: self.rain,
            snow: self.snow,
            night: params.night,
            eye_in_water,
        };
        self.ambient.update(&mut self.sys, &fw, &ctx, dt);
        self.sys.update(dt, &fw);

        // Lumière des particules non émissives : ciel × lumière du lieu, jamais totalement noire.
        let sun = (params.ambient + params.light_color * 0.6).min(Vec3::ONE);
        let l = local_light.clamp(0.0, 1.0);
        let light = [
            (sun.x * l).max(0.12),
            (sun.y * l).max(0.12),
            (sun.z * l).max(0.14),
        ];
        self.sys.write_instances(
            &View {
                eye,
                forward,
                max_distance: MAX_DISTANCE,
                light,
            },
            &mut self.instances,
        );
    }

    /// Pas, atterrissage et éclaboussures du joueur.
    fn track_player(&mut self, dt: f32, fw: &FxWorld<'_>, sim: &Simulation) {
        let Some(pid) = sim.player else { return };
        let e = &sim.entities;
        let (Some(t), Some(v), Some(p)) = (
            e.copy::<Transform>(pid),
            e.copy::<Velocity>(pid),
            e.copy::<Player>(pid),
        ) else {
            return;
        };
        let feet = t.pos.as_vec3();
        let below = fw.world.block(
            feet.x.floor() as i32,
            (feet.y - 0.2).floor() as i32,
            feet.z.floor() as i32,
        );
        let walkable = matches!(fw.cells[usize::from(below)], Cell::Solid | Cell::Leaves);
        let tr = &mut self.track;
        if v.grounded && !tr.grounded && tr.vy < -5.0 && walkable && !p.flying {
            let look = self.looks[usize::from(below)];
            self.sys.landing(feet, look, -tr.vy);
        }
        // Pas : une foulée tous les ~2 blocs au sol.
        let speed = Vec3::new(v.v.x, 0.0, v.v.z).length();
        if v.grounded && !p.flying && walkable && speed > 0.8 {
            tr.stride += speed * dt;
            if tr.stride > 2.0 {
                tr.stride = 0.0;
                let look = self.looks[usize::from(below)];
                self.sys.footstep(feet, look, Vec3::new(v.v.x, 0.0, v.v.z));
            }
        } else if !v.grounded {
            tr.stride = 0.0;
        }
        // Entrée dans l'eau.
        let waist = (feet.y + 0.5).floor() as i32;
        let wet = fw.cell(feet.x.floor() as i32, waist, feet.z.floor() as i32) == Cell::Water;
        if wet && !tr.in_water && tr.vy < -2.5 {
            let surface = Vec3::new(feet.x, waist as f32 + 0.9, feet.z);
            self.sys.water_splash(surface, -tr.vy);
        }
        tr.in_water = wet;
        tr.grounded = v.grounded;
        tr.vy = v.v.y;
    }

    /// Fait apparaître l'effet `name` (`presets::by_name`) à 3 blocs devant la caméra.
    pub fn spawn_named(&mut self, name: &str, count: u32) -> Result<(), String> {
        let def = presets::by_name(name).ok_or_else(|| format!("particule inconnue : {name}"))?;
        let at = self.eye + self.forward * 3.0;
        self.sys.spawn(def, &Emit::at(at, count.max(1)));
        Ok(())
    }

    /// Commandes de chat `/weather` et `/fx`. `None` : ce n'est pas pour nous.
    pub fn command(&mut self, text: &str) -> Option<String> {
        let mut it = text.split_whitespace();
        match it.next()? {
            "/weather" => Some(match it.next() {
                Some("clear") => {
                    self.set_weather(0.0, 0.0);
                    "Météo : ciel dégagé".to_owned()
                }
                Some("rain") => {
                    self.set_weather(1.0, 0.0);
                    "Météo : pluie".to_owned()
                }
                Some("snow") => {
                    self.set_weather(0.0, 1.0);
                    "Météo : neige".to_owned()
                }
                _ => "Usage : /weather <clear|rain|snow>".to_owned(),
            }),
            "/fx" => Some(match it.next() {
                Some("stats") => {
                    let s = self.sys.stats();
                    format!(
                        "Particules : {} vivantes / {}, {} créées, {} refusées",
                        s.alive,
                        self.sys.capacity(),
                        s.spawned,
                        s.dropped
                    )
                }
                Some("clear") => {
                    self.sys.clear();
                    "Particules effacées".to_owned()
                }
                Some(name) => {
                    let n = it.next().and_then(|n| n.parse().ok()).unwrap_or(20);
                    match self.spawn_named(name, n) {
                        Ok(()) => format!("{n} × {name}"),
                        Err(e) => e,
                    }
                }
                None => "Usage : /fx <effet> [n] | stats | clear".to_owned(),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_table_from_registry() {
        let reg = BlockRegistry::builtin();
        let ids = *reg.ids();
        let fx = Fx::new(&reg);
        assert_eq!(fx.cells[usize::from(ids.air)], Cell::Air);
        assert_eq!(fx.cells[usize::from(ids.stone)], Cell::Solid);
        assert_eq!(fx.cells[usize::from(ids.water)], Cell::Water);
        assert_eq!(fx.cells[usize::from(ids.lava)], Cell::Lava);
        assert_eq!(fx.cells[usize::from(ids.leaves)], Cell::Leaves);
        assert_eq!(fx.cells[usize::from(ids.magma)], Cell::Magma);
        // Plante en croix : traversée.
        assert_eq!(fx.cells[usize::from(ids.tall_grass)], Cell::Air);
    }

    #[test]
    fn block_atlas_gives_tint_and_layer() {
        let reg = BlockRegistry::builtin();
        let ids = *reg.ids();
        let atlas = nb_gfx::SceneAssets::builtin_particle_atlas();
        let mut fx = Fx::new(&reg);
        fx.set_block_atlas(&reg, &atlas);
        // L'atlas des particules ne connaît pas `stone` : couche manquante, teinte définie.
        let l = fx.look(ids.stone);
        assert_eq!(u32::from(l.layer), atlas.missing_layer);
    }

    #[test]
    fn events_spawn_effects() {
        let reg = BlockRegistry::builtin();
        let ids = *reg.ids();
        let mut fx = Fx::new(&reg);
        fx.on_events(&[
            EcsEvent::BlockBroken {
                pos: nb_core::glam::IVec3::new(1, 70, 1),
                block: ids.stone,
            },
            EcsEvent::BlockPlaced {
                pos: nb_core::glam::IVec3::new(2, 70, 1),
                block: ids.dirt,
            },
        ]);
        assert!(fx.len() > 10);
    }

    #[test]
    fn commands() {
        let reg = BlockRegistry::builtin();
        let mut fx = Fx::new(&reg);
        assert!(fx.command("/time day").is_none());
        assert!(fx.command("/weather rain").unwrap().contains("pluie"));
        assert_eq!(fx.weather(), (1.0, 0.0));
        assert!(fx.command("/fx smoke 5").is_some());
        assert_eq!(fx.len(), 5);
        assert!(fx.command("/fx nope").unwrap().contains("inconnue"));
        fx.command("/fx clear");
        assert!(fx.is_empty());
    }
}
