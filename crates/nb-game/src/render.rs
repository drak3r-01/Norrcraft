//! Données de rendu d'une frame, calculées sans GPU : caméra, horloge, environnement
//! (`MasterRenderer.render` §8), bloc visé, mobs et particules.
//!
//! [`RenderView::mobs`] / [`RenderView::particles`] gardent les données de l'ECS ;
//! [`RenderView::frame_input`] les passe à nb-gfx converties ([`gfx_mobs`], [`gfx_particles`]).

use std::collections::HashMap;

use nb_core::glam::{DVec3, IVec3, Vec3};
use nb_data::{BlockRegistry, MobRegistry};
use nb_ecs::{CameraState, EntityId, MobType, Particle, Player, Simulation, Transform};
use nb_gfx::{
    heat_from_nearest_lava_dist_sq, Camera, EyeFluid, FrameInput, ParticleInstance, SkyClock,
    TextureArrayCpu,
};
use nb_world::World;

use crate::anim::{AnimKey, MobAnimSystem};
use crate::net::Avatar;
use crate::state::GameState;

/// Interpolation de rendu entre deux ticks de simulation (60 Hz).
///
/// La simulation avance par pas fixes ; sans interpolation, l'image affichée ne change qu'à
/// chaque tick : sur un écran à 75 / 120 / 144 Hz des images consécutives montrent la même
/// position puis un saut (saccades régulières), et la souris n'est lue qu'à 60 Hz. Ici la
/// caméra et les mobs sont placés entre l'état du tick précédent et celui du tick courant
/// (`alpha` = reliquat de l'accumulateur / pas), et la rotation de la souris non encore
/// consommée par un tick est appliquée immédiatement. Désactivée par défaut (tests,
/// captures hors écran : rendu exact de l'état de simulation).
#[derive(Clone, Debug)]
pub struct RenderInterp {
    pub enabled: bool,
    /// 0 = état du tick précédent, 1 = état du tick courant.
    pub alpha: f32,
    pub prev_cam_pos: Option<Vec3>,
    pub prev_mobs: HashMap<EntityId, (DVec3, f32, f32)>,
}

impl Default for RenderInterp {
    fn default() -> Self {
        Self {
            enabled: false,
            alpha: 1.0,
            prev_cam_pos: None,
            prev_mobs: HashMap::new(),
        }
    }
}

/// Au-delà, le déplacement entre deux ticks est une téléportation : pas d'interpolation.
const TELEPORT_DIST_SQ: f32 = 16.0;

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let d = (b - a + PI).rem_euclid(TAU) - PI;
    a + d * t
}

/// Un mob à dessiner (`MobRenderer.render` : entités `(Transform, MobType)`, ordre ECS).
#[derive(Clone, Debug, PartialEq)]
pub struct MobInstance {
    pub entity: EntityId,
    /// Clé du `MobRegistry` (config introuvable → le rendu l'ignore).
    pub mob_id: String,
    /// Pieds (bas de l'AABB).
    pub position: DVec3,
    /// Radians.
    pub yaw: f32,
    pub pitch: f32,
}

/// Tout ce qu'il faut pour rendre une frame.
#[derive(Clone, Debug)]
pub struct RenderView<'a> {
    pub state: GameState,
    pub camera: Camera,
    pub sky: SkyClock,
    pub eye_fluid: EyeFluid,
    pub heat_intensity: f32,
    pub highlight_block: Option<IVec3>,
    pub mobs: Vec<MobInstance>,
    /// Autres joueurs d'une partie LAN.
    pub avatars: Vec<Avatar>,
    pub particles: &'a [Particle],
    /// [`Self::mobs`] convertis pour nb-gfx (config introuvable → ignoré).
    pub gfx_mobs: Vec<nb_gfx::MobInstance<'a>>,
    /// [`Self::particles`] converties pour nb-gfx.
    pub gfx_particles: Vec<ParticleInstance>,
}

impl RenderView<'_> {
    /// Entrée de `nb_gfx::Renderer::render_frame`.
    pub fn frame_input(&self, time_seconds: f32) -> FrameInput<'_> {
        FrameInput {
            camera: &self.camera,
            time_seconds,
            sky_clock: &self.sky,
            eye_fluid: self.eye_fluid,
            heat_intensity: self.heat_intensity,
            highlight_block: self.highlight_block,
            mobs: &self.gfx_mobs,
            particles: &self.gfx_particles,
        }
    }
}

/// Couches de l'atlas des particules (`TextureRegistry.getLayer` : insensible à la casse,
/// absent → couche manquante).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParticleLayers {
    names: HashMap<String, u32>,
    missing: u32,
}

impl ParticleLayers {
    pub fn from_atlas(atlas: &TextureArrayCpu) -> Self {
        Self {
            names: atlas.names.clone(),
            missing: atlas.missing_layer,
        }
    }

    /// Atlas intégré du renderer (`loadFromClasspath("/textures/particle", …)`).
    pub fn builtin() -> Self {
        Self::from_atlas(&nb_gfx::SceneAssets::builtin_particle_atlas())
    }

    pub fn layer(&self, texture: &str) -> u32 {
        // Nom déjà en minuscules (cas courant) : pas d'allocation, cette fonction est appelée
        // pour chaque particule à chaque frame.
        let layer = if texture.bytes().any(|b| b.is_ascii_uppercase()) {
            self.names.get(&texture.to_lowercase())
        } else {
            self.names.get(texture)
        };
        layer.copied().unwrap_or(self.missing)
    }
}

/// `MobRenderer.render` : mobs dont la config existe, modèle = id de la config, échelle du
/// cube manquant `(widthX, height, widthZ)`.
pub fn gfx_mobs<'a>(
    mobs: &[MobInstance],
    reg: &'a MobRegistry,
    anim: Option<&'a MobAnimSystem>,
    light: impl Fn(DVec3) -> f32,
    water: impl Fn(DVec3) -> f32,
) -> Vec<nb_gfx::MobInstance<'a>> {
    mobs.iter()
        .filter_map(|m| {
            let c = reg.get(&m.mob_id)?;
            Some(nb_gfx::MobInstance {
                model_id: &c.id,
                position: m.position.as_vec3(),
                yaw_rad: m.yaw,
                pitch_rad: m.pitch,
                missing_scale: Vec3::new(c.width_x, c.height, c.width_z),
                light: light(m.position),
                water_surface: water(m.position),
                pose: anim.map_or(&[][..], |a| a.pose(AnimKey::Mob(m.entity))),
            })
        })
        .collect()
}

/// Niveau de lumière du ciel (0..1) à la hauteur d'un mob : 15 moins l'atténuation des blocs
/// au-dessus de sa tête (feuilles, eau, roche), comme la lumière du ciel du mailleur. Un seul
/// verrou de chunk par mob. Chunk absent → 1 (ciel ouvert).
pub fn mob_light(world: &World, reg: &BlockRegistry, pos: DVec3) -> f32 {
    use nb_core::math::{block_index, CHUNK_SIZE_Y};
    let (x, z) = (pos.x.floor() as i32, pos.z.floor() as i32);
    let y0 = pos.y.floor() as i32 + 1;
    let Some(chunk) = world
        .chunk_manager()
        .chunk(nb_core::ChunkPos::new(x.div_euclid(16), z.div_euclid(16)))
    else {
        return 1.0;
    };
    let guard = chunk.read();
    let voxels = guard.voxels();
    let (lx, lz) = (x.rem_euclid(16) as usize, z.rem_euclid(16) as usize);
    let air = reg.ids().air;
    let mut level = 15i32;
    for y in y0.max(0)..CHUNK_SIZE_Y as i32 {
        let id = voxels[block_index(lx, y as usize, lz)];
        if id != air {
            level -= i32::from(reg.from_id(id).light_attenuation as i8).max(0);
            if level <= 0 {
                return 0.0;
            }
        }
    }
    level as f32 / 15.0
}

/// Hauteur de la surface de l'eau au-dessus d'un mob qui baigne dedans (`NO_WATER` sinon) : on
/// monte depuis le bloc de son centre tant que le bloc est de l'eau ; la surface est au sommet du
/// dernier bloc d'eau moins la hauteur de bassin (0,1) du mailleur.
pub fn mob_water_surface(world: &World, reg: &BlockRegistry, pos: DVec3) -> f32 {
    use nb_core::math::{block_index, CHUNK_SIZE_Y};
    let (x, z) = (pos.x.floor() as i32, pos.z.floor() as i32);
    let Some(chunk) = world
        .chunk_manager()
        .chunk(nb_core::ChunkPos::new(x.div_euclid(16), z.div_euclid(16)))
    else {
        return nb_gfx::NO_WATER;
    };
    let guard = chunk.read();
    let voxels = guard.voxels();
    let (lx, lz) = (x.rem_euclid(16) as usize, z.rem_euclid(16) as usize);
    let water = reg.ids().water;
    // Centre du mob : un peu au-dessus des pieds (les poissons nagent à ras de sol).
    let mut y = (pos.y + 0.4).floor() as i32;
    if y < 0 || y >= CHUNK_SIZE_Y as i32 || voxels[block_index(lx, y as usize, lz)] != water {
        return nb_gfx::NO_WATER;
    }
    while y + 1 < CHUNK_SIZE_Y as i32 && voxels[block_index(lx, (y + 1) as usize, lz)] == water {
        y += 1;
    }
    (y + 1) as f32 - 0.1
}

/// Avatars des autres joueurs pour nb-gfx. Le modèle porte une couleur par numéro de joueur ;
/// le lacet de la caméra (degrés, 0 = vers −Z) devient le lacet du modèle (face vers +Z).
pub fn gfx_avatars<'a>(
    avatars: &[Avatar],
    anim: Option<&'a MobAnimSystem>,
) -> Vec<nb_gfx::MobInstance<'a>> {
    avatars
        .iter()
        .map(|a| nb_gfx::MobInstance {
            model_id: nb_gfx::player_model_id(a.id),
            position: a.pos.as_vec3(),
            yaw_rad: std::f32::consts::PI - a.yaw_deg.to_radians(),
            pitch_rad: 0.0,
            missing_scale: Vec3::new(0.6, 1.8, 0.6),
            light: 1.0,
            water_surface: nb_gfx::NO_WATER,
            pose: anim.map_or(&[][..], |s| s.pose(AnimKey::Player(a.id))),
        })
        .collect()
}

/// `ParticleRenderer.render` : position, `type.scale`, `type.textureLayer`, teinte.
pub fn gfx_particles(particles: &[Particle], layers: &ParticleLayers) -> Vec<ParticleInstance> {
    particles
        .iter()
        .map(|p| ParticleInstance {
            position: p.position,
            scale: p.kind.scale,
            layer: layers.layer(&p.kind.texture),
            color: p.color,
        })
        .collect()
}

/// Caméra de rendu depuis l'état ECS.
pub fn gfx_camera(c: &CameraState, aspect: f32) -> Camera {
    Camera {
        position: c.position,
        pitch_deg: c.pitch_deg,
        yaw_deg: c.yaw_deg,
        fov_y_rad: c.fov,
        aspect,
    }
}

/// §8.2 : distance² minimale à une lave dans le cube [-8, 8]³ autour de la caméra (256 si
/// aucune), arithmétique `float` du Java.
pub fn nearest_lava_dist_sq(world: &World, lava: u8, cam: Vec3) -> f32 {
    use nb_core::math::{block_index, CHUNK_SIZE_Y};
    let (cx, cy, cz) = (
        cam.x.floor() as i32,
        cam.y.floor() as i32,
        cam.z.floor() as i32,
    );
    let mut min = 256.0f32;
    // Le cube [-8, 8]³ touche au plus 4 chunks : on les prend une fois chacun (un verrou par
    // chunk) et on lit les voxels directement, au lieu de 4913 `World::block` qui prenaient
    // chacun deux verrous et une recherche dans la table des chunks, à chaque frame.
    let cm = world.chunk_manager();
    let (y0, y1) = ((cy - 8).max(0), (cy + 8).min(CHUNK_SIZE_Y as i32 - 1));
    for chunk_x in (cx - 8).div_euclid(16)..=(cx + 8).div_euclid(16) {
        for chunk_z in (cz - 8).div_euclid(16)..=(cz + 8).div_euclid(16) {
            let Some(chunk) = cm.chunk(nb_core::ChunkPos::new(chunk_x, chunk_z)) else {
                continue;
            };
            let guard = chunk.read();
            let voxels = guard.voxels();
            let (bx0, bz0) = (chunk_x * 16, chunk_z * 16);
            for bx in (cx - 8).max(bx0)..=(cx + 8).min(bx0 + 15) {
                for bz in (cz - 8).max(bz0)..=(cz + 8).min(bz0 + 15) {
                    for by in y0..=y1 {
                        let i = block_index((bx - bx0) as usize, by as usize, (bz - bz0) as usize);
                        if voxels[i] == lava {
                            let dx = (bx as f32 + 0.5) - cam.x;
                            let dy = (by as f32 + 0.5) - cam.y;
                            let dz = (bz as f32 + 0.5) - cam.z;
                            let d = dx * dx + dy * dy + dz * dz;
                            if d < min {
                                min = d;
                            }
                        }
                    }
                }
            }
        }
    }
    min
}

/// §8.1 : fluide à hauteur d'œil (couleur d'eau du biome sous la caméra).
pub fn eye_fluid(world: &World, reg: &BlockRegistry, fluid: u8, cam: Vec3) -> EyeFluid {
    let ids = reg.ids();
    if fluid == ids.water {
        let b = world.biome_system();
        let c = b
            .config(b.biome(f64::from(cam.x), f64::from(cam.z)))
            .water_color;
        EyeFluid::Water {
            color: Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32),
        }
    } else if fluid == ids.lava {
        EyeFluid::Lava
    } else {
        EyeFluid::None
    }
}

/// Mobs de la simulation, par id croissant.
pub fn mob_instances(sim: &Simulation) -> Vec<MobInstance> {
    let e = &sim.entities;
    e.query_ids::<(&Transform, &MobType)>()
        .into_iter()
        .filter_map(|id| {
            let t = e.copy::<Transform>(id)?;
            let m = e.get::<MobType>(id)?;
            Some(MobInstance {
                entity: id,
                mob_id: m.mob_id.clone(),
                position: t.pos,
                yaw: t.yaw,
                pitch: t.pitch,
            })
        })
        .collect()
}

/// Mobs de la simulation tels qu'affichés : interpolés entre le tick précédent et le courant
/// quand l'interpolation de rendu est active.
pub fn interpolated_mobs(sim: &Simulation, interp: &RenderInterp) -> Vec<MobInstance> {
    let mut mobs = mob_instances(sim);
    if interp.enabled {
        let t = interp.alpha.clamp(0.0, 1.0);
        for m in &mut mobs {
            if let Some(&(pp, py, pq)) = interp.prev_mobs.get(&m.entity) {
                if (m.position - pp).length_squared() < f64::from(TELEPORT_DIST_SQ) {
                    m.position = pp.lerp(m.position, f64::from(t));
                    m.yaw = lerp_angle(py, m.yaw, t);
                    m.pitch = lerp_angle(pq, m.pitch, t);
                }
            }
        }
    }
    mobs
}

/// Construit la vue de rendu.
#[allow(clippy::too_many_arguments)]
pub fn build_view<'a>(
    state: GameState,
    world: &World,
    sim: &'a Simulation,
    reg: &BlockRegistry,
    mob_reg: &'a MobRegistry,
    particle_layers: &ParticleLayers,
    sky: &SkyClock,
    aspect: f32,
    avatars: Vec<Avatar>,
    interp: &RenderInterp,
    look: Option<(f32, f32)>,
    anim: Option<&'a MobAnimSystem>,
) -> RenderView<'a> {
    let player = sim.player.and_then(|p| sim.entities.copy::<Player>(p));
    let cam = player.map(|p| p.camera).unwrap_or_else(|| {
        CameraState::new(
            Vec3::from(nb_mesh::camera::DEFAULT_POSITION),
            nb_ecs::player::default_fov(),
        )
    });
    let mut camera = gfx_camera(&cam, aspect);
    if interp.enabled && player.is_some() {
        if let Some(prev) = interp.prev_cam_pos {
            if (cam.position - prev).length_squared() < TELEPORT_DIST_SQ {
                camera.position = prev.lerp(cam.position, interp.alpha.clamp(0.0, 1.0));
            }
        }
        if let Some((d_pitch, d_yaw)) = look {
            camera.rotate(d_pitch, d_yaw);
        }
    }
    let fluid = player.map_or(reg.ids().air, |p| p.fluid_at_eye_level);
    let heat =
        heat_from_nearest_lava_dist_sq(nearest_lava_dist_sq(world, reg.ids().lava, cam.position));
    let highlight = if state.shows_highlight() {
        player.and_then(|p| p.targeted_block)
    } else {
        None
    };
    let mobs = interpolated_mobs(sim, interp);
    let particles = sim.particles.particles();
    RenderView {
        state,
        camera,
        sky: *sky,
        eye_fluid: eye_fluid(world, reg, fluid, cam.position),
        heat_intensity: heat,
        highlight_block: highlight,
        gfx_mobs: {
            let mut g = gfx_mobs(&mobs, mob_reg, anim, |p| mob_light(world, reg, p), |p| mob_water_surface(world, reg, p));
            let mut av = gfx_avatars(&avatars, anim);
            for (g, a) in av.iter_mut().zip(&avatars) {
                g.light = mob_light(world, reg, a.pos);
                g.water_surface = mob_water_surface(world, reg, a.pos);
            }
            g.extend(av);
            g
        },
        avatars,
        gfx_particles: gfx_particles(particles, particle_layers),
        mobs,
        particles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_data::ParticleRegistry;

    #[test]
    fn avatars_face_where_the_camera_looks() {
        let mk = |yaw| Avatar {
            id: 2,
            name: "Lea".into(),
            pos: DVec3::new(1.0, 70.0, 2.0),
            yaw_deg: yaw,
            pitch_deg: 30.0,
        };
        // Le modèle regarde vers +Z quand son lacet est nul : (sin, cos) du lacet = direction.
        let dir = |a: &Avatar| {
            let g = &gfx_avatars(std::slice::from_ref(a), None)[0];
            (g.yaw_rad.sin(), g.yaw_rad.cos())
        };
        let cam_dir = |yaw_deg: f32| {
            let y = yaw_deg.to_radians();
            (y.sin(), -y.cos())
        };
        for yaw in [0.0, 90.0, 180.0, -45.0, 300.0] {
            let (mx, mz) = dir(&mk(yaw));
            let (cx, cz) = cam_dir(yaw);
            assert!(
                (mx - cx).abs() < 1e-5 && (mz - cz).abs() < 1e-5,
                "yaw {yaw}"
            );
        }
        let g = gfx_avatars(&[mk(10.0)], None);
        assert_eq!(g[0].model_id, "nb_player_2");
        assert_eq!(g[0].position, Vec3::new(1.0, 70.0, 2.0));
        assert_eq!(g[0].pitch_rad, 0.0, "le corps ne s'incline pas");
    }

    #[test]
    fn mobs_and_particles_convert_for_gfx() {
        let reg = MobRegistry::builtin();
        let rein = reg.get("reindeer").expect("renne intégré");
        let mobs = [
            MobInstance {
                entity: EntityId(1),
                mob_id: "reindeer".into(),
                position: DVec3::new(1.5, 70.25, -3.0),
                yaw: 0.5,
                pitch: -0.25,
            },
            MobInstance {
                entity: EntityId(2),
                mob_id: "inconnu".into(),
                position: DVec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
            },
        ];
        let g = gfx_mobs(&mobs, &reg, None, |_| 0.5, |_| nb_gfx::NO_WATER);
        assert_eq!(g.len(), 1, "config introuvable → ignoré");
        assert_eq!(g[0].model_id, "reindeer");
        assert_eq!(g[0].position, Vec3::new(1.5, 70.25, -3.0));
        assert_eq!((g[0].yaw_rad, g[0].pitch_rad), (0.5, -0.25));
        assert_eq!(
            g[0].missing_scale,
            Vec3::new(rein.width_x, rein.height, rein.width_z)
        );

        let preg = ParticleRegistry::builtin();
        let layers = ParticleLayers::builtin();
        let leaf = preg.get("leaf").unwrap();
        let lava = preg.get("lava").unwrap();
        let ps = [
            Particle::new(leaf, Vec3::new(1.0, 2.0, 3.0), Vec3::ZERO, Vec3::splat(0.5)),
            Particle::new(lava, Vec3::ZERO, Vec3::ZERO, Vec3::X),
        ];
        let g = gfx_particles(&ps, &layers);
        assert_eq!(g[0].position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(g[0].scale, leaf.scale);
        assert_eq!(g[0].color, Vec3::splat(0.5));
        let atlas = nb_gfx::SceneAssets::builtin_particle_atlas();
        assert_eq!(g[0].layer, atlas.layer(&leaf.texture));
        assert_eq!(g[1].layer, atlas.layer(&lava.texture));
        assert_ne!(g[0].layer, g[1].layer);
        assert_ne!(g[1].layer, atlas.missing_layer, "texture de lave connue");
        assert_eq!(layers.layer("absente"), atlas.missing_layer);
    }
}
