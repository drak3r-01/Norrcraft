//! Caméra et matrices (spec §2.2, §18.1). Pur CPU.
//!
//! Les matrices caméra GL viennent de `nb_mesh::camera` (répliques bit à bit de JOML) ; la
//! correction Vulkan `C` est appliquée uniquement aux matrices envoyées au GPU.

use glam::{Mat4, Vec3, Vec4};
use nb_mesh::camera as jcam;

/// Caméra du joueur (`Camera`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub position: Vec3,
    /// Degrés, borné à ±89 par [`Camera::rotate`].
    pub pitch_deg: f32,
    /// Degrés, non borné.
    pub yaw_deg: f32,
    /// FOV vertical en radians.
    pub fov_y_rad: f32,
    /// Largeur / hauteur du framebuffer effectivement rendu.
    pub aspect: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::from(jcam::DEFAULT_POSITION),
            pitch_deg: 0.0,
            yaw_deg: 0.0,
            fov_y_rad: (90f64).to_radians() as f32,
            aspect: 1280.0 / 720.0,
        }
    }
}

impl Camera {
    pub const NEAR: f32 = jcam::DEFAULT_NEAR;
    pub const FAR: f32 = jcam::DEFAULT_FAR;

    /// `rotate(dx, dy)` : `pitch = clamp(pitch + dx, -89, 89)`, `yaw += dy`.
    pub fn rotate(&mut self, d_pitch: f32, d_yaw: f32) {
        self.pitch_deg = jcam::clamp_pitch(self.pitch_deg + d_pitch);
        self.yaw_deg += d_yaw;
    }

    /// `setFov` (degrés → radians, comme `Math.toRadians`).
    pub fn set_fov_deg(&mut self, fov_deg: f32) {
        self.fov_y_rad = (fov_deg as f64).to_radians() as f32;
    }

    /// Matrice de vue `V`.
    pub fn view(&self) -> Mat4 {
        Mat4::from_cols_array(&jcam::view_matrix(
            self.position.into(),
            self.pitch_deg,
            self.yaw_deg,
        ))
    }

    /// Projection en convention GL `P_gl` (z ∈ [-1, 1]).
    pub fn proj_gl(&self) -> Mat4 {
        Mat4::from_cols_array(&self.proj_gl_array())
    }

    fn proj_gl_array(&self) -> [f32; 16] {
        jcam::perspective_gl(self.fov_y_rad, self.aspect, Self::NEAR, Self::FAR)
    }

    /// `P_gl · V` calculée comme JOML (`mulPerspectiveAffine`), pour le culling.
    pub fn frustum_matrix_gl(&self) -> [f32; 16] {
        jcam::mul_perspective_affine(
            &self.proj_gl_array(),
            &jcam::view_matrix(self.position.into(), self.pitch_deg, self.yaw_deg),
        )
    }
}

/// Matrice de correction GL → Vulkan (y inversé, z ramené dans [0, 1]) : `P_vk = C · P_gl`.
pub const VK_CLIP_CORRECTION: Mat4 = Mat4::from_cols(
    Vec4::new(1.0, 0.0, 0.0, 0.0),
    Vec4::new(0.0, -1.0, 0.0, 0.0),
    Vec4::new(0.0, 0.0, 0.5, 0.0),
    Vec4::new(0.0, 0.0, 0.5, 1.0),
);

/// Projection orthographique de la lumière `Lp_gl` (`ortho(-120,120,-120,120,1,400)`).
pub fn light_projection_gl() -> Mat4 {
    Mat4::orthographic_rh_gl(-120.0, 120.0, -120.0, 120.0, 1.0, 400.0)
}

/// Distance de l'œil de la lumière à la caméra (en remontant la lumière).
pub const LIGHT_DISTANCE: f32 = 150.0;

/// `Lv = lookAt(camPos - shadowLightDir·150, camPos, +Y)` (§6 étape 6, §10).
pub fn light_view(camera_pos: Vec3, shadow_light_dir: Vec3) -> Mat4 {
    let light_pos = camera_pos + shadow_light_dir * -LIGHT_DISTANCE;
    Mat4::look_at_rh(light_pos, camera_pos, Vec3::Y)
}

/// `Lp_gl · Lv` (convention GL, pour le culling de la passe d'ombre).
pub fn light_space_gl(camera_pos: Vec3, shadow_light_dir: Vec3) -> Mat4 {
    light_projection_gl() * light_view(camera_pos, shadow_light_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_clamps_pitch_only() {
        let mut c = Camera::default();
        c.rotate(100.0, 400.0);
        assert_eq!(c.pitch_deg, 89.0);
        assert_eq!(c.yaw_deg, 400.0);
        c.rotate(-200.0, -10.0);
        assert_eq!(c.pitch_deg, -89.0);
    }

    #[test]
    fn looks_down_minus_z_at_rest() {
        let c = Camera {
            position: Vec3::ZERO,
            ..Camera::default()
        };
        let p = c.proj_gl() * c.view() * Vec4::new(0.0, 0.0, -10.0, 1.0);
        assert!(p.x.abs() < 1e-6 && p.y.abs() < 1e-6 && p.w > 0.0);
    }

    #[test]
    fn matches_glam_reference() {
        let c = Camera {
            position: Vec3::new(3.0, 70.0, -5.0),
            pitch_deg: 20.0,
            yaw_deg: 135.0,
            fov_y_rad: 1.3,
            aspect: 1.5,
        };
        let v = Mat4::from_rotation_x(20f32.to_radians())
            * Mat4::from_rotation_y(135f32.to_radians())
            * Mat4::from_translation(-c.position);
        assert!(c.view().abs_diff_eq(v, 1e-4));
        let p = Mat4::perspective_rh_gl(1.3, 1.5, 0.05, 1000.0);
        assert!(c.proj_gl().abs_diff_eq(p, 1e-4));
        let pv = Mat4::from_cols_array(&c.frustum_matrix_gl());
        assert!(pv.abs_diff_eq(p * v, 1e-2));
    }

    #[test]
    fn vulkan_correction_maps_depth_like_gl_window() {
        let c = Camera::default();
        let pgl = c.proj_gl();
        let pvk = VK_CLIP_CORRECTION * pgl;
        let pt = Vec4::new(1.0, 2.0, -30.0, 1.0);
        let (g, k) = (pgl * pt, pvk * pt);
        let (gz, kz) = (g.z / g.w, k.z / k.w);
        assert!((kz - (0.5 * gz + 0.5)).abs() < 1e-6);
        assert!((k.y / k.w + g.y / g.w).abs() < 1e-6);
        assert!((k.x - g.x).abs() < 1e-6);
    }

    #[test]
    fn light_space_keeps_camera_in_view() {
        let dir = crate::sky::sky_params(900.0).shadow_light_dir;
        let cam = Vec3::new(10.0, 80.0, 10.0);
        let p = light_space_gl(cam, dir) * cam.extend(1.0);
        assert!(p.x.abs() < 1e-3 && p.y.abs() < 1e-3);
        // Caméra à 150 unités de l'œil : z_ndc = (2·150 − 401) / 399.
        assert!((p.z - (300.0 - 401.0) / 399.0).abs() < 1e-4);
    }
}
