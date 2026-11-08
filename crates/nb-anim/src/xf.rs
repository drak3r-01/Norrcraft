//! Transformations d'os : composantes locales (`Xf`) et matrices de peau compactes (`Mat34`).

use bytemuck::{Pod, Zeroable};
use glam::{Affine3A, Quat, Vec3};

/// Transformation locale d'un os : rotation, translation (en blocs) et échelle, appliquées dans
/// l'ordre `T · R · S` autour du pivot de l'os.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xf {
    pub q: Quat,
    pub t: Vec3,
    pub s: Vec3,
}

impl Xf {
    pub const IDENTITY: Self = Self {
        q: Quat::IDENTITY,
        t: Vec3::ZERO,
        s: Vec3::ONE,
    };

    /// Mélange `a → b` (rotation par nlerp, plus courte voie).
    #[inline]
    pub fn lerp(a: &Xf, b: &Xf, w: f32) -> Xf {
        let qb = if a.q.dot(b.q) < 0.0 { -b.q } else { b.q };
        Xf {
            q: a.q.lerp(qb, w).normalize(),
            t: a.t.lerp(b.t, w),
            s: a.s.lerp(b.s, w),
        }
    }
}

impl Default for Xf {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Matrice affine 3×4 (trois lignes de quatre flottants) : ce que le vertex shader lit pour
/// « peau »-er un sommet (`p' = (r0·p, r1·p, r2·p)` avec `p = (x, y, z, 1)`). 48 octets.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Mat34(pub [f32; 12]);

impl Mat34 {
    pub const IDENTITY: Self = Self([
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0,
    ]);

    #[inline]
    pub fn from_affine(a: &Affine3A) -> Self {
        let m = &a.matrix3;
        let t = a.translation;
        Self([
            m.x_axis.x, m.y_axis.x, m.z_axis.x, t.x, //
            m.x_axis.y, m.y_axis.y, m.z_axis.y, t.y, //
            m.x_axis.z, m.y_axis.z, m.z_axis.z, t.z,
        ])
    }

    /// Image d'un point (même calcul que le vertex shader).
    #[inline]
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let m = &self.0;
        Vec3::new(
            m[0] * p.x + m[1] * p.y + m[2] * p.z + m[3],
            m[4] * p.x + m[5] * p.y + m[6] * p.z + m[7],
            m[8] * p.x + m[9] * p.y + m[10] * p.z + m[11],
        )
    }
}

impl Default for Mat34 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mat34_matches_affine() {
        let a = Affine3A::from_scale_rotation_translation(
            Vec3::new(1.0, 2.0, 0.5),
            Quat::from_rotation_y(0.7) * Quat::from_rotation_x(-0.3),
            Vec3::new(3.0, -1.0, 2.0),
        );
        let m = Mat34::from_affine(&a);
        let p = Vec3::new(0.3, -0.2, 1.1);
        assert!((m.transform_point(p) - a.transform_point3(p)).length() < 1e-5);
        assert_eq!(std::mem::size_of::<Mat34>(), 48);
    }

    #[test]
    fn lerp_takes_short_way() {
        let a = Xf {
            q: Quat::from_rotation_z(0.1),
            ..Xf::IDENTITY
        };
        let b = Xf {
            q: -Quat::from_rotation_z(0.3),
            ..Xf::IDENTITY
        };
        let m = Xf::lerp(&a, &b, 0.5);
        assert!((m.q.angle_between(Quat::from_rotation_z(0.2))) < 1e-4);
    }
}
