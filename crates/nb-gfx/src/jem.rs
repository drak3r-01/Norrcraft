//! Parseur JEM minimal (OptiFine CEM) de `MobModelLoader`, reproduit tel quel (spec §14.5).
//!
//! Ce n'est **pas** un parseur JSON : recherches de sous-chaînes, premier tableau après une clé,
//! bornes d'objet approximatives ([JAVA-BUG] : la sous-chaîne d'une boîte déborde sur la boîte
//! suivante). Pur CPU, testable sans fichier.

use image::RgbaImage;

use crate::mob_model::{add_color_box, add_tex_box, MobModelCpu, MobVertex};

/// `Float.parseFloat` Java (sous-ensemble utile) : espaces de tête et de queue (`≤ ' '`)
/// ignorés, suffixe `f`/`F`/`d`/`D` accepté, `NaN` / `Infinity` signés. Les flottants
/// hexadécimaux Java ne sont pas reconnus ([INFO] : absents des fichiers CEM).
pub(crate) fn java_parse_float(s: &str) -> Option<f32> {
    let s = s.trim_matches(|c: char| c <= ' ');
    let (sign, body) = match s.as_bytes().first() {
        Some(b'+') => (1.0, &s[1..]),
        Some(b'-') => (-1.0, &s[1..]),
        _ => (1.0, s),
    };
    match body {
        "NaN" => return Some(f32::NAN),
        "Infinity" => return Some(sign * f32::INFINITY),
        _ => {}
    }
    let body = body
        .strip_suffix(['f', 'F', 'd', 'D'])
        .filter(|b| !b.is_empty())
        .unwrap_or(body);
    // Rust accepterait aussi « inf », « nan »… que Java refuse.
    if body.is_empty()
        || !body
            .bytes()
            .all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        return None;
    }
    body.parse::<f32>().ok().map(|v| sign * v)
}

/// Erreur de parsing d'un nombre : tout le fichier est rejeté (exception Java attrapée).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ParseError;

/// `extractFloatArrayAt` : texte entre le premier `[` à partir de `pos` et le `]` suivant,
/// découpé sur `,` (les chaînes vides **finales** sont retirées, comme `String.split`).
pub(crate) fn float_array_at(json: &str, pos: usize) -> Result<Option<Vec<f32>>, ParseError> {
    let Some(start) = json[pos..].find('[').map(|i| i + pos) else {
        return Ok(None);
    };
    let Some(end) = json[start..].find(']').map(|i| i + start) else {
        return Ok(None);
    };
    let content = json[start + 1..end].trim_matches(|c: char| c <= ' ');
    if content.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let mut parts: Vec<&str> = content.split(',').collect();
    while parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    parts
        .iter()
        .map(|p| java_parse_float(p).ok_or(ParseError))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// `extractFirstFloatArray` : premier tableau après la première occurrence de `"key"`.
pub(crate) fn first_float_array(json: &str, key: &str) -> Result<Option<Vec<f32>>, ParseError> {
    match json.find(&format!("\"{key}\"")) {
        Some(idx) => float_array_at(json, idx),
        None => Ok(None),
    }
}

/// `extractString` : après la première occurrence de `"key"`, premier `:` puis texte entre les
/// deux guillemets suivants (aucun échappement).
pub(crate) fn extract_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let idx = json.find(&format!("\"{key}\""))?;
    let colon = json[idx..].find(':')? + idx;
    let q1 = json[colon + 1..].find('"')? + colon + 1;
    let q2 = json[q1 + 1..].find('"')? + q1 + 1;
    Some(&json[q1 + 1..q2])
}

/// `findObjectStart` : `{` ouvrant englobant (recherche arrière avec profondeur), sinon 0.
pub(crate) fn object_start(json: &str, ref_pos: usize) -> usize {
    let mut depth = 0i32;
    for i in (0..ref_pos).rev() {
        match json.as_bytes()[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    0
}

/// `findObjectEnd` : ignore les `}` rencontrés avant le premier `{`, puis s'arrête à la fin de
/// l'objet ouvert par ce `{` ([JAVA-BUG] : c'est l'objet **suivant** la boîte courante).
pub(crate) fn object_end(json: &str, ref_pos: usize) -> usize {
    let mut entered = false;
    let mut depth = 0i32;
    for (i, &c) in json.as_bytes().iter().enumerate().skip(ref_pos) {
        match c {
            b'{' => {
                depth += 1;
                entered = true;
            }
            b'}' => {
                depth -= 1;
                if entered && depth <= 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    json.len()
}

/// `guessMobId` : nom de fichier sans extension, suffixe `_(temperate|cold|warm|baby)` retiré.
pub(crate) fn guess_mob_id(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let mut parts: Vec<&str> = path.split('/').collect();
    while parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    let mut last = (*parts.last()?).to_owned();
    if let Some(dot) = last.rfind('.').filter(|&d| d > 0) {
        last.truncate(dot);
    }
    for suffix in ["_temperate", "_cold", "_warm", "_baby"] {
        if let Some(s) = last.strip_suffix(suffix) {
            last = s.to_owned();
            break;
        }
    }
    (!last.is_empty()).then_some(last)
}

/// `parseJem` : construit le modèle, ou `None` (textureSize absent / invalide, nombre illisible,
/// aucun sommet). `resolve_texture` reçoit la valeur brute de `"texture"` (non vide).
pub fn parse_jem(
    json: &str,
    resolve_texture: &mut dyn FnMut(&str) -> Option<RgbaImage>,
) -> Option<MobModelCpu> {
    parse(json, resolve_texture).ok().flatten()
}

fn parse(
    json: &str,
    resolve_texture: &mut dyn FnMut(&str) -> Option<RgbaImage>,
) -> Result<Option<MobModelCpu>, ParseError> {
    let Some(ts) = first_float_array(json, "textureSize")?.filter(|t| t.len() >= 2) else {
        return Ok(None);
    };
    let (tw, th) = (ts[0], ts[1]);
    let texture = extract_string(json, "texture")
        .filter(|p| !p.is_empty())
        .and_then(resolve_texture);
    let has_texture = texture.is_some();

    let mut opaque: Vec<MobVertex> = Vec::new();
    let mut trans: Vec<MobVertex> = Vec::new();
    let mut pos = 0;
    while let Some(idx) = json[pos..].find("\"coordinates\"").map(|i| i + pos) {
        pos = idx + 1;
        let Some(c) = float_array_at(json, idx)?.filter(|c| c.len() >= 6) else {
            continue;
        };
        let start = object_start(json, idx);
        let end = object_end(json, idx);
        let box_json = &json[start..end];
        let offset = first_float_array(box_json, "textureOffset")?;
        let color = first_float_array(box_json, "color")?;
        let b = [c[0], c[1], c[2], c[3], c[4], c[5]];
        match (color, offset) {
            (Some(col), _) if col.len() >= 4 => {
                let out = if col[3] < 1.0 {
                    &mut trans
                } else {
                    &mut opaque
                };
                add_color_box(out, &b, [col[0], col[1], col[2], col[3]]);
            }
            (_, Some(o)) if o.len() >= 2 && has_texture => {
                add_tex_box(
                    &mut opaque,
                    &[b[0], b[1], b[2], b[3], b[4], b[5], o[0], o[1]],
                    tw,
                    th,
                );
            }
            _ => {}
        }
    }
    Ok(MobModelCpu::from_lists(opaque, trans, texture))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_floats() {
        assert_eq!(java_parse_float(" 1.5 "), Some(1.5));
        assert_eq!(java_parse_float("-2"), Some(-2.0));
        assert_eq!(java_parse_float("3f"), Some(3.0));
        assert_eq!(java_parse_float("1e2"), Some(100.0));
        assert_eq!(java_parse_float("-Infinity"), Some(f32::NEG_INFINITY));
        assert!(java_parse_float("NaN").unwrap().is_nan());
        for bad in ["", "inf", "nan", "abc", "1,2", "f", "\"1\""] {
            assert_eq!(java_parse_float(bad), None, "{bad}");
        }
    }

    #[test]
    fn float_arrays_follow_java_split() {
        let j = r#""a": [1, 2, 3], "b": [ ], "c": [1,2,,], "d": [,1], "e": [x]"#;
        assert_eq!(
            first_float_array(j, "a").unwrap(),
            Some(vec![1.0, 2.0, 3.0])
        );
        assert_eq!(first_float_array(j, "b").unwrap(), Some(vec![]));
        // Chaînes vides finales retirées par `split`.
        assert_eq!(first_float_array(j, "c").unwrap(), Some(vec![1.0, 2.0]));
        assert_eq!(first_float_array(j, "d"), Err(ParseError));
        assert_eq!(first_float_array(j, "e"), Err(ParseError));
        assert_eq!(first_float_array(j, "zz").unwrap(), None);
        // Le premier `[` après la clé, même s'il appartient à une autre clé.
        assert_eq!(
            first_float_array(r#""k": 5, "z": [7]"#, "k").unwrap(),
            Some(vec![7.0])
        );
    }

    #[test]
    fn strings_and_ids() {
        let j = r#"{"textureSize": [64, 32], "texture" : "textures/entity/cow/cow.png"}"#;
        assert_eq!(
            extract_string(j, "texture"),
            Some("textures/entity/cow/cow.png")
        );
        assert_eq!(extract_string(j, "nope"), None);
        assert_eq!(
            guess_mob_id("textures/entity/cow/cow_temperate.png").as_deref(),
            Some("cow")
        );
        assert_eq!(guess_mob_id("a\\b\\pig_baby.png").as_deref(), Some("pig"));
        assert_eq!(guess_mob_id("x/.hidden").as_deref(), Some(".hidden"));
        assert_eq!(guess_mob_id("dir/"), Some("dir".to_owned()));
        assert_eq!(guess_mob_id("///"), None);
        assert_eq!(guess_mob_id("_cold.png"), None);
    }

    #[test]
    fn object_bounds_quirk() {
        let j = r#"{"boxes":[{"coordinates":[0,0,0,1,1,1]},{"coordinates":[2,0,0,1,1,1],"textureOffset":[8,4]}]}"#;
        let first = j.find("\"coordinates\"").unwrap();
        let s = object_start(j, first);
        assert_eq!(&j[s..s + 2], "{\"");
        assert_eq!(j.as_bytes()[s - 1], b'[');
        // La sous-chaîne de la 1re boîte s'étend jusqu'à la fin de la 2e.
        let e = object_end(j, first);
        assert!(j[s..e].contains("textureOffset"));
        assert!(j[s..e].ends_with("[8,4]}"));
        assert_eq!(object_end("no braces", 0), 9);
        assert_eq!(object_start("x", 1), 0);
    }

    fn solid(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]))
    }

    #[test]
    fn parse_textured_and_color_boxes() {
        // Boîte couleur d'abord : sa sous-chaîne déborde sur la boîte texturée, mais `color`
        // est prioritaire. (Dans l'ordre inverse, la boîte texturée deviendrait une boîte
        // couleur, [JAVA-BUG] reproduit.)
        let j = r#"{
            "texture": "textures/entity/foo.png",
            "textureSize": [64, 32],
            "models": [
                {"part": "b", "boxes": [{"coordinates": [0, 8, 0, 2, 2, 2], "color": [1, 0, 0, 0.5]}]},
                {"part": "a", "boxes": [{"coordinates": [-4, 0, -4, 8, 8, 8], "textureOffset": [0, 0]}]}
            ]
        }"#;
        let mut asked = Vec::new();
        let m = parse_jem(j, &mut |p| {
            asked.push(p.to_owned());
            Some(solid(64, 32))
        })
        .unwrap();
        assert_eq!(asked, ["textures/entity/foo.png"]);
        assert_eq!((m.opaque_count, m.transparent_count), (36, 36));
        assert!(m.texture.is_some());
        // Boîte couleur transparente en fin de tampon, UV (0,0), teinte du fichier.
        let last = m.vertices.last().unwrap();
        assert_eq!(last.uv, [0.0, 0.0]);
        assert_eq!(last.tint, [1.0, 0.0, 0.0, 0.5]);
        // Boîte texturée : UV de la face +Z = (uB, vB) au 1er sommet.
        assert_eq!(m.vertices[0].uv, [8.0 / 64.0, 1.0 - 16.0 / 32.0]);
    }

    #[test]
    fn parse_failures_and_fallbacks() {
        let none = &mut |_: &str| None;
        // textureSize absent / trop court / illisible.
        assert!(parse_jem(r#"{"coordinates":[0,0,0,1,1,1],"color":[1,1,1,1]}"#, none).is_none());
        assert!(parse_jem(r#"{"textureSize":[64]}"#, none).is_none());
        assert!(parse_jem(
            r#"{"textureSize":[64,32],"boxes":[{"coordinates":[0,0,0,1,1,x],"color":[1,1,1,1]}]}"#,
            none
        )
        .is_none());
        // Texture non résolue : la boîte texturée est ignorée → aucun sommet → échec.
        assert!(parse_jem(
            r#"{"texture":"t.png","textureSize":[64,32],"boxes":[{"coordinates":[0,0,0,1,1,1],"textureOffset":[0,0]}]}"#,
            none
        )
        .is_none());
        // < 6 coordonnées : boîte ignorée ; boîte couleur sans texture → mode couleur.
        let m = parse_jem(
            r#"{"textureSize":[64,32],"boxes":[{"coordinates":[1,2]},{"coordinates":[0,0,0,1,1,1],"color":[0,1,0,1]}]}"#,
            none,
        )
        .unwrap();
        assert_eq!((m.opaque_count, m.transparent_count), (36, 0));
        assert!(m.texture.is_none());
    }

    #[test]
    fn box_without_offset_borrows_the_next_one() {
        // [JAVA-BUG] reproduit : la 1re boîte n'a pas de textureOffset mais sa sous-chaîne
        // déborde sur la 2e → elle est quand même texturée (avec l'offset de la 2e).
        let j = r#"{"texture":"t.png","textureSize":[16,16],"boxes":[{"coordinates":[0,0,0,1,1,1]},{"coordinates":[4,0,0,1,1,1],"textureOffset":[2,2]}]}"#;
        let m = parse_jem(j, &mut |_| Some(solid(16, 16))).unwrap();
        assert_eq!(m.opaque_count, 72);
        assert_eq!(m.vertices[0].uv, m.vertices[36].uv);
        // Une boîte texturée suivie d'une boîte couleur devient elle-même une boîte couleur.
        let j = r#"{"texture":"t.png","textureSize":[16,16],"boxes":[{"coordinates":[0,0,0,1,1,1],"textureOffset":[0,0]},{"coordinates":[4,0,0,1,1,1],"color":[0,0,1,1]}]}"#;
        let m = parse_jem(j, &mut |_| Some(solid(16, 16))).unwrap();
        assert_eq!(m.opaque_count, 72);
        assert!(m.vertices.iter().all(|v| v.tint == [0.0, 0.0, 1.0, 1.0]));
    }
}
