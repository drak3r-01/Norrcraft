//! Matrices caméra en convention OpenGL (spec §15.4), répliques des chemins JOML 1.10.5
//! effectivement empruntés par `Camera` (sans FMA : `joml.useMathFma` est désactivé par défaut).
//!
//! Toutes les matrices sont `[f32; 16]` **column-major** : `m[c*4 + r]` = colonne `c`, ligne `r`
//! (le `mCR` de JOML).

/// Champ de vision vertical de `Main` (radians de 70°).
pub fn default_fov_y() -> f32 {
    70f64.to_radians() as f32
}
/// Plan proche de `Main`.
pub const DEFAULT_NEAR: f32 = 0.05;
/// Plan lointain de `Main`.
pub const DEFAULT_FAR: f32 = 1000.0;
/// Position initiale de la caméra.
pub const DEFAULT_POSITION: [f32; 3] = [0.0, 100.0, 0.0];

const PI_F: f32 = std::f32::consts::PI;
const PI2_F: f32 = PI_F * 2.0;
const PI_HALF_F: f32 = std::f32::consts::FRAC_PI_2;

/// Matrice identité.
pub const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

#[inline]
fn sin_f(a: f32) -> f32 {
    (a as f64).sin() as f32
}

/// `org.joml.Math.cosFromSin` (chemin non « fastmath »).
fn cos_from_sin(sin: f32, angle: f32) -> f32 {
    let cos = ((1.0f32 - sin * sin) as f64).sqrt() as f32;
    let a = angle + PI_HALF_F;
    let mut b = a - ((a / PI2_F) as i32) as f32 * PI2_F;
    if b < 0.0 {
        b += PI2_F;
    }
    if b >= PI_F {
        -cos
    } else {
        cos
    }
}

/// `clamp` du pitch de `Camera.rotate` : `max(-89, min(89, p))`.
pub fn clamp_pitch(p: f32) -> f32 {
    p.clamp(-89.0, 89.0)
}

/// `Camera.getViewMatrix` : `identity().rotateX(rad(pitch)).rotateY(rad(yaw)).translate(-pos)`.
pub fn view_matrix(pos: [f32; 3], pitch_deg: f32, yaw_deg: f32) -> [f32; 16] {
    let ax = (pitch_deg as f64).to_radians() as f32;
    let ay = (yaw_deg as f64).to_radians() as f32;

    // rotationX (la matrice est l'identité).
    let mut m = IDENTITY;
    let sx = sin_f(ax);
    let cx = cos_from_sin(sx, ax);
    m[5] = cx; // m11
    m[6] = sx; // m12
    m[9] = -sx; // m21
    m[10] = cx; // m22

    // rotateYInternal.
    let sy = sin_f(ay);
    let cy = cos_from_sin(sy, ay);
    let mut r = m;
    for k in 0..4 {
        r[k] = m[k] * cy + m[8 + k] * -sy; // nm0k = fma(m0k, cos, m2k * -sin)
        r[8 + k] = m[k] * sy + m[8 + k] * cy; // m2k = fma(m0k, sin, m2k * cos)
    }
    let m = r;

    // translateGeneric(-pos).
    let (x, y, z) = (-pos[0], -pos[1], -pos[2]);
    let mut t = m;
    for k in 0..4 {
        t[12 + k] = m[k] * x + (m[4 + k] * y + (m[8 + k] * z + m[12 + k]));
    }
    t
}

/// `setPerspective(fovy, aspect, near, far, zZeroToOne = false)` (convention GL, z ∈ [-1, 1]).
pub fn perspective_gl(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let mut m = [0.0f32; 16];
    let h = ((fov_y_rad * 0.5) as f64).tan() as f32;
    m[0] = 1.0 / (h * aspect);
    m[5] = 1.0 / h;
    let far_inf = far > 0.0 && far.is_infinite();
    let near_inf = near > 0.0 && near.is_infinite();
    if far_inf {
        let e = 1e-6f32;
        m[10] = e - 1.0;
        m[14] = (e - 2.0) * near;
    } else if near_inf {
        let e = 1e-6f32;
        m[10] = 1.0 - e;
        m[14] = (2.0 - e) * far;
    } else {
        m[10] = (far + near) / (near - far);
        m[14] = (far + far) * near / (near - far);
    }
    m[11] = -1.0;
    m
}

/// `mulPerspectiveAffine(view)` : le chemin pris par `new Matrix4f(proj).mul(view)` quand `proj`
/// est une perspective et `view` une transformation affine (cas de la caméra).
pub fn mul_perspective_affine(p: &[f32; 16], v: &[f32; 16]) -> [f32; 16] {
    let (p00, p11, p22, p23, p32) = (p[0], p[5], p[10], p[11], p[14]);
    [
        p00 * v[0],
        p11 * v[1],
        p22 * v[2],
        p23 * v[2],
        p00 * v[4],
        p11 * v[5],
        p22 * v[6],
        p23 * v[6],
        p00 * v[8],
        p11 * v[9],
        p22 * v[10],
        p23 * v[10],
        p00 * v[12],
        p11 * v[13],
        p22 * v[14] + p32,
        p23 * v[14],
    ]
}

/// Produit générique `a × b` (`mulGeneric` de JOML, sans FMA).
pub fn mul_mat4(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for c in 0..4 {
        for row in 0..4 {
            let b0 = b[c * 4];
            let b1 = b[c * 4 + 1];
            let b2 = b[c * 4 + 2];
            let b3 = b[c * 4 + 3];
            r[c * 4 + row] = a[row] * b0 + (a[4 + row] * b1 + (a[8 + row] * b2 + a[12 + row] * b3));
        }
    }
    r
}

/// Matrice de culling de la caméra : `proj · view` (via [`mul_perspective_affine`]).
pub fn camera_frustum_matrix(
    pos: [f32; 3],
    pitch_deg: f32,
    yaw_deg: f32,
    fov_y_rad: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> [f32; 16] {
    let proj = perspective_gl(fov_y_rad, aspect, near, far);
    let view = view_matrix(pos, pitch_deg, yaw_deg);
    mul_perspective_affine(&proj, &view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_view_at_origin() {
        let v = view_matrix([0.0, 0.0, 0.0], 0.0, 0.0);
        assert_eq!(v, IDENTITY);
        let v = view_matrix([1.0, 2.0, 3.0], 0.0, 0.0);
        assert_eq!(&v[12..16], &[-1.0, -2.0, -3.0, 1.0]);
    }

    #[test]
    fn yaw_90_maps_x_to_minus_z() {
        // Ry(90°) appliqué à +X : colonne 0 = (c, 0, -s) = (~0, 0, -1).
        let v = view_matrix([0.0, 0.0, 0.0], 0.0, 90.0);
        assert!(v[0].abs() < 1e-6);
        assert_eq!(v[2], -1.0);
        assert_eq!(v[8], 1.0);
    }

    #[test]
    fn perspective_terms() {
        let p = perspective_gl(default_fov_y(), 1280.0 / 720.0, DEFAULT_NEAR, DEFAULT_FAR);
        assert_eq!(p[11], -1.0);
        assert_eq!(p[15], 0.0);
        assert_eq!(p[10], (1000.0f32 + 0.05) / (0.05 - 1000.0));
        let generic = mul_mat4(&p, &view_matrix(DEFAULT_POSITION, 10.0, 30.0));
        let fast = mul_perspective_affine(&p, &view_matrix(DEFAULT_POSITION, 10.0, 30.0));
        for (a, b) in generic.iter().zip(fast) {
            assert!((a - b).abs() <= 1e-4 * a.abs().max(1.0));
        }
    }

    #[test]
    fn pitch_clamp() {
        assert_eq!(clamp_pitch(120.0), 89.0);
        assert_eq!(clamp_pitch(-95.0), -89.0);
        assert_eq!(clamp_pitch(12.5), 12.5);
    }
}
