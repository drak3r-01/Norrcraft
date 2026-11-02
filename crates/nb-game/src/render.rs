//! Données de rendu d'une frame, calculées sans GPU : caméra, horloge, environnement
//! (`MasterRenderer.render` §8), bloc visé, mobs et particules.
//!
//! Les mobs et particules ne sont pas encore dessinés par nb-gfx : [`RenderView::mobs`] et
//! [`RenderView::particles`] sont la façade que le futur rendu consommera.

use nb_core::glam::{DVec3, IVec3, Vec3};
use nb_data::BlockRegistry;
use nb_ecs::{CameraState, EntityId, MobType, Particle, Player, Simulation, Transform};
use nb_gfx::{heat_from_nearest_lava_dist_sq, Camera, EyeFluid, FrameInput, SkyClock};
use nb_world::World;

use crate::state::GameState;

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
    pub particles: &'a [Particle],
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
        }
    }
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
    let (cx, cy, cz) = (
        cam.x.floor() as i32,
        cam.y.floor() as i32,
        cam.z.floor() as i32,
    );
    let mut min = 256.0f32;
    for x in -8..=8 {
        for y in -8..=8 {
            for z in -8..=8 {
                let (bx, by, bz) = (cx + x, cy + y, cz + z);
                if world.block(bx, by, bz) == lava {
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

/// Construit la vue de rendu.
pub fn build_view<'a>(
    state: GameState,
    world: &World,
    sim: &'a Simulation,
    reg: &BlockRegistry,
    sky: &SkyClock,
    aspect: f32,
) -> RenderView<'a> {
    let player = sim.player.and_then(|p| sim.entities.copy::<Player>(p));
    let cam = player.map(|p| p.camera).unwrap_or_else(|| {
        CameraState::new(
            Vec3::from(nb_mesh::camera::DEFAULT_POSITION),
            nb_ecs::player::default_fov(),
        )
    });
    let camera = gfx_camera(&cam, aspect);
    let fluid = player.map_or(reg.ids().air, |p| p.fluid_at_eye_level);
    let heat =
        heat_from_nearest_lava_dist_sq(nearest_lava_dist_sq(world, reg.ids().lava, cam.position));
    let highlight = if state.shows_highlight() {
        player.and_then(|p| p.targeted_block)
    } else {
        None
    };
    RenderView {
        state,
        camera,
        sky: *sky,
        eye_fluid: eye_fluid(world, reg, fluid, cam.position),
        heat_intensity: heat,
        highlight_block: highlight,
        mobs: mob_instances(sim),
        particles: sim.particles.particles(),
    }
}
