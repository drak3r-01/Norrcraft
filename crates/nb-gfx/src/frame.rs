//! Réglages, entrées d'une frame et bloc d'uniformes (spec §6–§9, §18.2, §19). Pur CPU.

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat4, Vec3};
pub use nb_data::ShadowQuality;

use crate::camera::{light_space_gl, Camera, VK_CLIP_CORRECTION};
use crate::sky::{sky_params, SkyClock, SkyParams};

/// Réglages graphiques (sous-ensemble de `GameSettings`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxSettings {
    pub vsync: bool,
    pub fullscreen: bool,
    pub shadow_quality: ShadowQuality,
    pub bloom: bool,
    pub fov_y_deg: f32,
    /// Déviation désactivée par défaut : seuils 200000/100000 dans `shadow.vert` au lieu des
    /// 2000/1000 du Java (feuilles et croix projetteraient alors des ombres découpées).
    pub fix_shadow_layer_thresholds: bool,
}

impl Default for GfxSettings {
    fn default() -> Self {
        Self {
            vsync: true,
            fullscreen: false,
            shadow_quality: ShadowQuality::Low,
            bloom: true,
            fov_y_deg: 90.0,
            fix_shadow_layer_thresholds: true,
        }
    }
}

impl GfxSettings {
    /// Depuis les réglages du joueur.
    pub fn from_game(s: &nb_data::GameSettings) -> Self {
        Self {
            vsync: s.vsync_enabled,
            fullscreen: s.fullscreen,
            shadow_quality: s.shadow_quality,
            bloom: s.bloom_enabled,
            fov_y_deg: s.fov,
            fix_shadow_layer_thresholds: true,
        }
    }
}

/// Résolution de la shadow map : `LOW → 1024`, `HIGH → 4096`, `OFF → None` (inchangée).
pub fn shadow_resolution(q: ShadowQuality) -> Option<u32> {
    match q {
        ShadowQuality::Off => None,
        ShadowQuality::Low => Some(1024),
        ShadowQuality::High => Some(4096),
    }
}

/// Fluide à hauteur d'œil (§8.1). La couleur d'eau du biome est résolue par l'appelant.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum EyeFluid {
    #[default]
    None,
    Water {
        color: Vec3,
    },
    Lava,
}

impl EyeFluid {
    /// `(fluidState, (r,g,b))` : couleur d'effacement HDR et `uFogColor`.
    pub fn state_and_color(self, base_fog: Vec3) -> (i32, Vec3) {
        match self {
            EyeFluid::None => (0, base_fog),
            EyeFluid::Water { color } => (1, color),
            EyeFluid::Lava => (2, Vec3::new(0.8, 0.1, 0.0)),
        }
    }
}

/// Mob à dessiner (§14.1). L'appelant ne transmet que les entités dont la config existe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MobInstance<'a> {
    /// Id du mob ; modèle absent → cube manquant.
    pub model_id: &'a str,
    pub position: Vec3,
    /// Valeurs brutes de `TransformComponent` (radians).
    pub yaw_rad: f32,
    pub pitch_rad: f32,
    /// `(widthX, height, widthZ)` de la config (échelle du cube manquant).
    pub missing_scale: Vec3,
    /// Niveau de lumière du lieu, 0..1 (1 = ciel ouvert), appliqué à l'éclairage du mob.
    pub light: f32,
    /// Palette de peau (une matrice 3×4 par os, voir `nb_anim::Mat34`) calculée par le moteur
    /// d'animation ; vide : pose de repos (modèle statique ou pas encore animé).
    pub pose: &'a [[f32; 12]],
}

/// Particule à dessiner : l instance GPU du moteur `nb-particles` (32 octets, un seul draw).
pub use nb_particles::GpuParticle as ParticleInstance;

/// Entrées d'une frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameInput<'a> {
    pub camera: &'a Camera,
    /// `uTime` (secondes depuis le démarrage de la plateforme).
    pub time_seconds: f32,
    pub sky_clock: &'a SkyClock,
    pub eye_fluid: EyeFluid,
    /// §8.2, calculée par l'appelant (`heat_from_nearest_lava_dist_sq`).
    pub heat_intensity: f32,
    /// Mobs (ordre de la requête ECS, non trié).
    pub mobs: &'a [MobInstance<'a>],
    /// Particules (ordre du gestionnaire, non trié).
    pub particles: &'a [ParticleInstance],
    /// Coin min du bloc visé ; `None` hors PLAYING/PAUSED/INVENTORY.
    pub highlight_block: Option<IVec3>,
}

/// Bloc d'uniformes par frame, miroir std140 de `shaders/glsl450/frame.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct FrameUniforms {
    pub proj_vk: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub light_space_vk: [[f32; 4]; 4],
    pub inverse_view: [[f32; 4]; 4],
    pub inverse_proj_gl: [[f32; 4]; 4],
    /// xyz = caméra, w = uTime.
    pub camera_pos: [f32; 4],
    pub light_dir: [f32; 4],
    pub ambient_color: [f32; 4],
    pub light_color: [f32; 4],
    pub fog_color: [f32; 4],
    pub zenith_color: [f32; 4],
    pub horizon_color: [f32; 4],
    pub sun_color: [f32; 4],
    /// x = `P_gl[2][2]`, y = `P_gl[3][2]`, z = W, w = H.
    pub proj_gl: [f32; 4],
    /// x = auroraIntensity, y = auroraPhase, z = heatIntensity, w = nuit (étoiles, lune).
    pub env: [f32; 4],
    /// x = fluidState, y = correctif des seuils d'ombre, z = 1 si la lune éclaire (sinon soleil),
    /// w = couche d'atlas de la lave (−1 si absente ; renseignée par le renderer de scène).
    pub flags: [i32; 4],
}

const _: () = assert!(std::mem::size_of::<FrameUniforms>() == 496);

fn v4(v: Vec3, w: f32) -> [f32; 4] {
    [v.x, v.y, v.z, w]
}

/// Tout ce qui est dérivé de l'entrée pour une frame donnée.
#[derive(Clone, Copy, Debug)]
pub struct FrameDerived {
    /// Caméra avec l'aspect de l'extent rendu (§3.3).
    pub camera: Camera,
    pub sky: SkyParams,
    pub fluid_state: i32,
    /// Couleur d'effacement des deux attachements HDR (§6.1).
    pub clear_color: Vec3,
    pub proj_gl: Mat4,
    pub view: Mat4,
    /// `Lp_gl · Lv`.
    pub light_space_gl: Mat4,
    pub uniforms: FrameUniforms,
}

impl FrameDerived {
    pub fn new(input: &FrameInput<'_>, extent: (u32, u32), settings: &GfxSettings) -> Self {
        let mut camera = *input.camera;
        if extent.0 > 0 && extent.1 > 0 {
            camera.aspect = extent.0 as f32 / extent.1 as f32;
        }
        let sky = sky_params(input.sky_clock.time_of_day);
        let (fluid_state, clear_color) = input.eye_fluid.state_and_color(sky.base_fog);
        let proj_gl = camera.proj_gl();
        let view = camera.view();
        let light_space_gl = light_space_gl(camera.position, sky.shadow_light_dir);
        let uniforms = FrameUniforms {
            proj_vk: (VK_CLIP_CORRECTION * proj_gl).to_cols_array_2d(),
            view: view.to_cols_array_2d(),
            light_space_vk: (VK_CLIP_CORRECTION * light_space_gl).to_cols_array_2d(),
            inverse_view: view.inverse().to_cols_array_2d(),
            inverse_proj_gl: proj_gl.inverse().to_cols_array_2d(),
            camera_pos: v4(camera.position, input.time_seconds),
            light_dir: v4(sky.shadow_light_dir, 0.0),
            ambient_color: v4(sky.ambient, 0.0),
            light_color: v4(sky.light_color, 0.0),
            fog_color: v4(clear_color, 0.0),
            zenith_color: v4(sky.zenith, 0.0),
            horizon_color: v4(sky.horizon, 0.0),
            sun_color: v4(sky.sun_color, 0.0),
            proj_gl: [
                proj_gl.z_axis.z,
                proj_gl.w_axis.z,
                extent.0 as f32,
                extent.1 as f32,
            ],
            env: [
                sky.aurora_intensity,
                input.sky_clock.aurora_phase,
                input.heat_intensity,
                sky.night,
            ],
            flags: [
                fluid_state,
                i32::from(settings.fix_shadow_layer_thresholds),
                i32::from(sky.moon_up),
                0,
            ],
        };
        Self {
            camera,
            sky,
            fluid_state,
            clear_color,
            proj_gl,
            view,
            light_space_gl,
            uniforms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniforms_follow_spec() {
        let cam = Camera {
            position: Vec3::new(1.0, 70.0, 2.0),
            ..Camera::default()
        };
        let clock = SkyClock::new();
        let input = FrameInput {
            camera: &cam,
            time_seconds: 12.5,
            sky_clock: &clock,
            eye_fluid: EyeFluid::Lava,
            heat_intensity: 0.25,
            mobs: &[],
            particles: &[],
            highlight_block: None,
        };
        let d = FrameDerived::new(&input, (800, 400), &GfxSettings::default());
        assert_eq!(d.camera.aspect, 2.0);
        assert_eq!(d.fluid_state, 2);
        assert_eq!(d.clear_color, Vec3::new(0.8, 0.1, 0.0));
        let u = d.uniforms;
        assert_eq!(u.camera_pos, [1.0, 70.0, 2.0, 12.5]);
        assert_eq!(u.flags, [2, 1, 0, 0]);
        assert_eq!(u.env[2], 0.25);
        // getLinearDepth(gl_FragCoord.z) avec ces coefficients = distance à l'œil.
        let (m22, m32) = (u.proj_gl[0], u.proj_gl[1]);
        let n = Camera::NEAR;
        let f = Camera::FAR;
        for z in [1.0f32, 10.0, 300.0] {
            let z_ndc = (-(f + n) / (f - n) * -z - 2.0 * f * n / (f - n)) / z;
            let depth = 0.5 * z_ndc + 0.5;
            let lin = m32 / ((depth * 2.0 - 1.0) + m22);
            assert!((lin - z).abs() / z < 1e-3, "{lin} vs {z}");
        }
        let water = EyeFluid::Water {
            color: Vec3::new(0.1, 0.2, 0.3),
        };
        assert_eq!(
            water.state_and_color(Vec3::ONE),
            (1, Vec3::new(0.1, 0.2, 0.3))
        );
        assert_eq!(EyeFluid::None.state_and_color(Vec3::ONE), (0, Vec3::ONE));
    }

    #[test]
    fn shadow_resolutions() {
        assert_eq!(shadow_resolution(ShadowQuality::Low), Some(1024));
        assert_eq!(shadow_resolution(ShadowQuality::High), Some(4096));
        assert_eq!(shadow_resolution(ShadowQuality::Off), None);
        let g = GfxSettings::from_game(&nb_data::GameSettings::default());
        assert_eq!(g, GfxSettings::default());
    }
}
