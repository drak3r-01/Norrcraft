//! Petites fonctions mathématiques au comportement Java (`Math.max`/`Math.min` propagent NaN,
//! `lerp` clampé des systèmes, `(int) Math.floor`).

/// `Math.max(float, float)` : NaN si l'un des deux est NaN ; `max(-0.0, 0.0) = 0.0`.
#[inline]
pub fn jmax(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        return f32::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { b } else { a };
    }
    if a >= b {
        a
    } else {
        b
    }
}

/// `Math.min(float, float)` : NaN si l'un des deux est NaN ; `min(-0.0, 0.0) = -0.0`.
#[inline]
pub fn jmin(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        return f32::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { a } else { b };
    }
    if a <= b {
        a
    } else {
        b
    }
}

/// `lerp(a, b, t) = a + (b − a) · max(0, min(1, t))` (en `f32`, NaN propagé).
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * jmax(0.0, jmin(1.0, t))
}

/// `(int) Math.floor(v)` (saturant comme Java).
#[inline]
pub fn floor_i(v: f64) -> i32 {
    v.floor() as i32
}

/// `(int) Math.floor(v)` sur un `float`.
#[inline]
pub fn floor_f(v: f32) -> i32 {
    v.floor() as i32
}

/// `Math.signum(double)`.
#[inline]
pub fn signum(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        v
    }
}

/// `(float) Math.toRadians(deg)` pour un angle en degrés `float`.
#[inline]
pub fn to_radians_f(deg: f32) -> f32 {
    f64::from(deg).to_radians() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_min_propagate_nan() {
        assert!(jmax(f32::NAN, 1.0).is_nan());
        assert!(jmin(0.0, f32::NAN).is_nan());
        assert_eq!(jmax(-0.0, 0.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(jmin(0.0, -0.0).to_bits(), (-0.0f32).to_bits());
        assert_eq!(jmax(2.0, 3.0), 3.0);
        assert_eq!(jmin(2.0, 3.0), 2.0);
    }

    #[test]
    fn lerp_is_clamped() {
        assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
        assert_eq!(lerp(0.0, 10.0, 2.0), 10.0);
        assert_eq!(lerp(0.0, 10.0, -1.0), 0.0);
        assert!(lerp(0.0, 10.0, f32::NAN).is_nan());
    }

    #[test]
    fn floors() {
        assert_eq!(floor_i(-0.5), -1);
        assert_eq!(floor_i(1.999), 1);
        assert_eq!(floor_f(-1.0), -1);
        assert_eq!(signum(-3.0), -1.0);
    }
}
