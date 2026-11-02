//! Police `stb_easy_font` (utilisée par `ChatRenderer` via LWJGL) : glyphes en segments
//! horizontaux / verticaux, un quad uni par segment. Port exact des tables et de
//! `stb_easy_font_print` / `stb_easy_font_width` (domaine public, Sean Barrett).

/// Capacité du tampon Java (`MAX_CHARS = 1000` quads de 64 octets).
pub const MAX_QUADS: usize = 1000;
/// Hauteur de ligne (`'\n'` avance de 12).
pub const LINE_HEIGHT: f32 = 12.0;

const CHARINFO: [(u8, u8, u8); 96] = [
    (6, 0, 0),
    (3, 0, 0),
    (5, 1, 1),
    (7, 1, 4),
    (7, 3, 7),
    (7, 6, 12),
    (7, 8, 19),
    (4, 16, 21),
    (4, 17, 22),
    (4, 19, 23),
    (23, 21, 24),
    (23, 22, 31),
    (20, 23, 34),
    (22, 23, 36),
    (19, 24, 36),
    (21, 25, 36),
    (6, 25, 39),
    (6, 27, 43),
    (6, 28, 45),
    (6, 30, 49),
    (6, 33, 53),
    (6, 34, 57),
    (6, 40, 58),
    (6, 46, 59),
    (6, 47, 62),
    (6, 55, 64),
    (19, 57, 68),
    (20, 59, 68),
    (21, 61, 69),
    (22, 66, 69),
    (21, 68, 69),
    (7, 73, 69),
    (9, 75, 74),
    (6, 78, 81),
    (6, 80, 85),
    (6, 83, 90),
    (6, 85, 91),
    (6, 87, 95),
    (6, 90, 96),
    (7, 92, 97),
    (6, 96, 102),
    (5, 97, 106),
    (6, 99, 107),
    (6, 100, 110),
    (6, 100, 115),
    (7, 101, 116),
    (6, 101, 121),
    (6, 101, 125),
    (6, 102, 129),
    (7, 103, 133),
    (6, 104, 140),
    (6, 105, 145),
    (7, 107, 149),
    (6, 108, 151),
    (7, 109, 155),
    (7, 109, 160),
    (7, 109, 165),
    (7, 118, 167),
    (6, 118, 172),
    (4, 120, 176),
    (6, 122, 177),
    (4, 122, 181),
    (23, 124, 182),
    (22, 129, 182),
    (4, 130, 182),
    (22, 131, 183),
    (6, 133, 187),
    (22, 135, 191),
    (6, 137, 192),
    (22, 139, 196),
    (6, 144, 197),
    (22, 147, 198),
    (6, 150, 202),
    (19, 151, 206),
    (21, 152, 207),
    (6, 155, 209),
    (3, 160, 210),
    (23, 160, 211),
    (22, 164, 216),
    (22, 165, 220),
    (22, 167, 224),
    (22, 169, 228),
    (21, 171, 232),
    (21, 173, 233),
    (5, 178, 233),
    (22, 179, 234),
    (23, 180, 238),
    (23, 180, 243),
    (23, 180, 248),
    (22, 189, 248),
    (22, 191, 252),
    (5, 196, 252),
    (3, 203, 252),
    (5, 203, 253),
    (22, 210, 253),
    (0, 214, 253),
];

#[rustfmt::skip]
const HSEG: [u8; 214] = [
    97,37,69,84,28,51,2,18,10,49,98,41,65,25,81,105,33,9,97,1,97,37,37,36,
    81,10,98,107,3,100,3,99,58,51,4,99,58,8,73,81,10,50,98,8,73,81,4,10,50,
    98,8,25,33,65,81,10,50,17,65,97,25,33,25,49,9,65,20,68,1,65,25,49,41,
    11,105,13,101,76,10,50,10,50,98,11,99,10,98,11,50,99,11,50,11,99,8,57,
    58,3,99,99,107,10,10,11,10,99,11,5,100,41,65,57,41,65,9,17,81,97,3,107,
    9,97,1,97,33,25,9,25,41,100,41,26,82,42,98,27,83,42,98,26,51,82,8,41,
    35,8,10,26,82,114,42,1,114,8,9,73,57,81,41,97,18,8,8,25,26,26,82,26,82,
    26,82,41,25,33,82,26,49,73,35,90,17,81,41,65,57,41,65,25,81,90,114,20,
    84,73,57,41,49,25,33,65,81,9,97,1,97,25,33,65,81,57,33,25,41,25,
];

#[rustfmt::skip]
const VSEG: [u8; 253] = [
    4,2,8,10,15,8,15,33,8,15,8,73,82,73,57,41,82,10,82,18,66,10,21,29,1,65,
    27,8,27,9,65,8,10,50,97,74,66,42,10,21,57,41,29,25,14,81,73,57,26,8,8,
    26,66,3,8,8,15,19,21,90,58,26,18,66,18,105,89,28,74,17,8,73,57,26,21,
    8,42,41,42,8,28,22,8,8,30,7,8,8,26,66,21,7,8,8,29,7,7,21,8,8,8,59,7,8,
    8,15,29,8,8,14,7,57,43,10,82,7,7,25,42,25,15,7,25,41,15,21,105,105,29,
    7,57,57,26,21,105,73,97,89,28,97,7,57,58,26,82,18,57,57,74,8,30,6,8,8,
    14,3,58,90,58,11,7,74,43,74,15,2,82,2,42,75,42,10,67,57,41,10,7,2,42,
    74,106,15,2,35,8,8,29,7,8,8,59,35,51,8,8,15,35,30,35,8,8,30,7,8,8,60,
    36,8,45,7,7,36,8,43,8,44,21,8,8,44,35,8,8,43,23,8,8,43,35,8,8,31,21,15,
    20,8,8,28,18,58,89,58,26,21,89,73,89,29,20,8,8,30,7,
];

/// Rectangle d'un segment, en unités de police (avant `translate·scale`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Octet affiché pour `c` : ASCII imprimable tel quel, `'\n'` conservé, le reste → `'?'`
/// ([DÉVIATION] : comportement indéfini côté stb / LWJGL).
fn byte_of(c: char) -> u8 {
    match c {
        '\n' => b'\n',
        ' '..='~' => c as u8,
        _ => b'?',
    }
}

fn draw_segs(x: f32, y: f32, segs: &[u8], vertical: bool, out: &mut Vec<GlyphQuad>) {
    let mut x = x;
    for &s in segs {
        let len = (s & 7) as f32;
        x += ((s >> 3) & 1) as f32;
        if s & 7 != 0 && out.len() < MAX_QUADS {
            let y0 = y + (s >> 4) as f32;
            let (w, h) = if vertical { (1.0, len) } else { (len, 1.0) };
            out.push(GlyphQuad { x, y: y0, w, h });
        }
    }
}

/// `stb_easy_font_print(x, y, text)` : quads des segments, au plus [`MAX_QUADS`].
pub fn print(x: f32, y: f32, text: &str) -> Vec<GlyphQuad> {
    let mut out = Vec::new();
    let start_x = x;
    let (mut x, mut y) = (x, y);
    for c in text.chars() {
        if out.len() >= MAX_QUADS {
            break;
        }
        let b = byte_of(c);
        if b == b'\n' {
            y += LINE_HEIGHT;
            x = start_x;
            continue;
        }
        let i = (b - 32) as usize;
        let (advance, h_seg, v_seg) = CHARINFO[i];
        let (_, h_next, v_next) = CHARINFO[i + 1];
        let y_ch = if advance & 16 != 0 { y + 1.0 } else { y };
        draw_segs(
            x,
            y_ch,
            &HSEG[h_seg as usize..h_next as usize],
            false,
            &mut out,
        );
        draw_segs(
            x,
            y_ch,
            &VSEG[v_seg as usize..v_next as usize],
            true,
            &mut out,
        );
        x += (advance & 15) as f32;
    }
    out
}

/// `stb_easy_font_width(text)` (ligne la plus longue, arrondie au-dessus).
pub fn width(text: &str) -> i32 {
    let (mut len, mut max_len) = (0.0f32, 0.0f32);
    for c in text.chars() {
        let b = byte_of(c);
        if b == b'\n' {
            max_len = max_len.max(len);
            len = 0.0;
        } else {
            len += (CHARINFO[(b - 32) as usize].0 & 15) as f32;
        }
    }
    max_len.max(len).ceil() as i32
}

/// `ChatRenderer.measureWidth(text, scale)` : 0 pour un texte vide.
pub fn measure_width(text: &str, scale: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    width(text) as f32 * scale
}

/// `stb_easy_font_height(text)`.
pub fn height(text: &str) -> i32 {
    let mut y = 0.0f32;
    let mut nonempty = false;
    for c in text.chars() {
        if c == '\n' {
            y += LINE_HEIGHT;
            nonempty = false;
        } else {
            nonempty = true;
        }
    }
    (y + if nonempty { LINE_HEIGHT } else { 0.0 }).ceil() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_match_stb() {
        assert_eq!(width(""), 0);
        assert_eq!(width(" "), 6);
        assert_eq!(width("A"), 6);
        assert_eq!(width("i"), 3); // advance 19 & 15
        assert_eq!(width("PLAY"), 6 + 6 + 6 + 7);
        assert_eq!(width("ab\nabc"), width("abc"));
        assert_eq!(measure_width("", 2.0), 0.0);
        assert_eq!(measure_width("PLAY", 2.0), 50.0);
    }

    #[test]
    fn heights() {
        assert_eq!(height(""), 0);
        assert_eq!(height("a"), 12);
        assert_eq!(height("a\nb"), 24);
        assert_eq!(height("a\n"), 12);
    }

    #[test]
    fn segment_counts() {
        // Espace : aucun segment.
        assert!(print(0.0, 0.0, " ").is_empty());
        // « 1 » : hseg[27] = 107 (len 3) + vseg[43..45] = 25, 14 (len 1, 6).
        let q = print(0.0, 0.0, "1");
        assert_eq!(q.len(), 3);
        assert_eq!((q[0].w, q[0].h), (3.0, 1.0));
        assert_eq!((q[2].w, q[2].h), (1.0, 6.0));
    }

    #[test]
    fn exclamation_mark_geometry() {
        // hseg[0] = 97 : point (len 1) à y 6 ; vseg[0] = 4 : barre de 4 à y 0.
        let q = print(10.0, 20.0, "!");
        assert_eq!(
            q,
            vec![
                GlyphQuad {
                    x: 10.0,
                    y: 26.0,
                    w: 1.0,
                    h: 1.0
                },
                GlyphQuad {
                    x: 10.0,
                    y: 20.0,
                    w: 1.0,
                    h: 4.0
                },
            ]
        );
    }

    #[test]
    fn lowercase_shifted_down() {
        // 'a' a le bit 16 de advance : y + 1.
        let q = print(0.0, 0.0, "a");
        assert!(q.iter().all(|g| g.y >= 1.0));
    }

    #[test]
    fn newline_resets_x() {
        let a = print(5.0, 0.0, "A");
        let b = print(5.0, 0.0, "\nA");
        assert_eq!(a.len(), b.len());
        for (p, q) in a.iter().zip(&b) {
            assert_eq!(p.x, q.x);
            assert_eq!(p.y + 12.0, q.y);
        }
    }

    #[test]
    fn quad_cap() {
        let long = "#".repeat(500);
        assert_eq!(print(0.0, 0.0, &long).len(), MAX_QUADS);
    }

    #[test]
    fn non_ascii_replaced() {
        assert_eq!(width("é"), width("?"));
        assert_eq!(print(0.0, 0.0, "é"), print(0.0, 0.0, "?"));
    }
}
