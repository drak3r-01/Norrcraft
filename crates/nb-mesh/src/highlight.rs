//! Contour du bloc ciblé (`HighlightMesh`, spec §16) : 12 segments `GL_LINES`.

/// Échelle du cube de contour.
pub const HIGHLIGHT_SCALE: f32 = 1.002;
/// Décalage du cube de contour.
pub const HIGHLIGHT_OFFSET: f32 = -0.001;

/// Les 24 sommets (positions seules, stride 12) dans l'ordre du Java. À dessiner avec
/// `model = translate(bloc ciblé)` et un test de profondeur `LEQUAL`.
pub fn highlight_lines() -> [[f32; 3]; 24] {
    let o = HIGHLIGHT_OFFSET;
    let s = HIGHLIGHT_SCALE + HIGHLIGHT_OFFSET;
    [
        // Bas.
        [o, o, o],
        [s, o, o],
        [s, o, o],
        [s, o, s],
        [s, o, s],
        [o, o, s],
        [o, o, s],
        [o, o, o],
        // Haut.
        [o, s, o],
        [s, s, o],
        [s, s, o],
        [s, s, s],
        [s, s, s],
        [o, s, s],
        [o, s, s],
        [o, s, o],
        // Verticales.
        [o, o, o],
        [o, s, o],
        [s, o, o],
        [s, s, o],
        [s, o, s],
        [s, s, s],
        [o, o, s],
        [o, s, s],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_axis_aligned_segments() {
        let v = highlight_lines();
        let s = 1.002f32 + (-0.001f32);
        assert_eq!(v[1][0], s);
        for seg in v.chunks(2) {
            let diff = (0..3).filter(|&k| seg[0][k] != seg[1][k]).count();
            assert_eq!(diff, 1, "segment non aligné : {seg:?}");
        }
    }
}
