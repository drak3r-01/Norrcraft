//! Émulation des formats numériques Java utilisés par les commandes (spec §16.3).
//!
//! - [`java_parse_float`] = `Float.parseFloat` ;
//! - [`java_float_to_string`] = `Float.toString` ;
//! - [`java_format_fixed`] = `String.format("%.nf", (double) v)`, **toujours avec un point**
//!   décimal (décision de Gabriel : pas de dépendance à la locale).

/// `String.trim()` de Java : retire les caractères `≤ U+0020` aux deux extrémités.
pub fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// `Float.parseFloat(s)` ; `None` = `NumberFormatException`.
///
/// Accepte blancs de tête/queue (`≤ U+0020`), signe, `NaN`, `Infinity`, décimal avec exposant,
/// suffixe `f/F/d/D`, hexadécimal (`0x1.8p3`). Refuse `inf`, `nan` (minuscules), la chaîne vide.
pub fn java_parse_float(s: &str) -> Option<f32> {
    let t = java_trim(s);
    let (neg, body) = match t.as_bytes().first()? {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    let sign = |v: f32| if neg { -v } else { v };
    match body {
        "NaN" => return Some(f32::NAN),
        "Infinity" => return Some(sign(f32::INFINITY)),
        _ => {}
    }
    if body.starts_with("0x") || body.starts_with("0X") {
        return parse_hex(&body[2..]).map(sign);
    }
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut digits = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    let number = &body[..i];
    match &body[i..] {
        "" | "f" | "F" | "d" | "D" => {}
        _ => return None,
    }
    number.parse::<f32>().ok().map(sign)
}

/// Partie hexadécimale après `0x` : `h*[.h*]p[±]d+[fFdD]`.
///
/// Calcul via `f64` puis conversion en `f32` (double arrondi possible pour des mantisses de plus
/// de 53 bits : cas sans intérêt pratique).
fn parse_hex(s: &str) -> Option<f32> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut mant: u64 = 0;
    let mut exp_adj: i64 = 0;
    let mut digits = 0;
    let push = |d: u64, frac: bool, mant: &mut u64, exp_adj: &mut i64| {
        if *mant >> 56 == 0 {
            *mant = (*mant << 4) | d;
            if frac {
                *exp_adj -= 4;
            }
        } else {
            if d != 0 {
                *mant |= 1; // bit collant
            }
            if !frac {
                *exp_adj += 4;
            }
        }
    };
    while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
        push(
            u64::from((bytes[i] as char).to_digit(16)?),
            false,
            &mut mant,
            &mut exp_adj,
        );
        i += 1;
        digits += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
            push(
                u64::from((bytes[i] as char).to_digit(16)?),
                true,
                &mut mant,
                &mut exp_adj,
            );
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 || i >= bytes.len() || (bytes[i] != b'p' && bytes[i] != b'P') {
        return None;
    }
    i += 1;
    let mut exp_neg = false;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        exp_neg = bytes[i] == b'-';
        i += 1;
    }
    let start = i;
    let mut exp: i64 = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        exp = (exp * 10 + i64::from(bytes[i] - b'0')).min(100_000);
        i += 1;
    }
    if i == start {
        return None;
    }
    match &s[i..] {
        "" | "f" | "F" | "d" | "D" => {}
        _ => return None,
    }
    let e = if exp_neg { -exp } else { exp } + exp_adj;
    let mut v = mant as f64;
    // Mise à l'échelle par puissances de deux exactes.
    let mut e = e.clamp(-2000, 2000) as i32;
    while e > 0 {
        let step = e.min(1000);
        v *= 2f64.powi(step);
        e -= step;
    }
    while e < 0 {
        let step = (-e).min(1000);
        v /= 2f64.powi(step);
        e += step;
    }
    Some(v as f32)
}

/// Chiffres significatifs (sans zéros de tête) et exposant décimal du premier chiffre, à partir
/// de la notation `{:e}` de Rust (représentation la plus courte qui se relit à l'identique).
fn sci_parts(s: &str) -> (Vec<u8>, i32) {
    let (mant, exp) = s.split_once('e').expect("notation scientifique");
    let digits: Vec<u8> = mant.bytes().filter(u8::is_ascii_digit).collect();
    (digits, exp.parse().expect("exposant"))
}

/// `Float.toString(f)` : représentation la plus courte ; notation `d.dddE±n` hors de
/// `[1e-3, 1e7)` ; toujours au moins un chiffre après le point.
pub fn java_float_to_string(v: f32) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let neg = v.is_sign_negative();
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if v == 0.0 {
        out.push_str("0.0");
        return out;
    }
    let (digits, exp) = sci_parts(&format!("{:e}", v.abs()));
    let ds: String = digits.iter().map(|&d| d as char).collect();
    if (-3..7).contains(&exp) {
        if exp >= 0 {
            let int_len = exp as usize + 1;
            if ds.len() <= int_len {
                out.push_str(&ds);
                out.push_str(&"0".repeat(int_len - ds.len()));
                out.push_str(".0");
            } else {
                out.push_str(&ds[..int_len]);
                out.push('.');
                out.push_str(&ds[int_len..]);
            }
        } else {
            out.push_str("0.");
            out.push_str(&"0".repeat((-exp - 1) as usize));
            out.push_str(&ds);
        }
    } else {
        out.push_str(&ds[..1]);
        out.push('.');
        if ds.len() > 1 {
            out.push_str(&ds[1..]);
        } else {
            out.push('0');
        }
        out.push('E');
        out.push_str(&exp.to_string());
    }
    out
}

/// `String.format("%.{prec}f", v)` de Java : arrondi **HALF_UP** appliqué à la représentation
/// décimale la plus courte du `double`, séparateur toujours `.`.
pub fn java_format_fixed(v: f64, prec: usize) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let neg = v.is_sign_negative();
    // Chiffres du nombre arrondi, exprimé en unités de 10^-prec.
    let mut kept: Vec<u8> = if v == 0.0 {
        vec![0]
    } else {
        let (digits, exp) = sci_parts(&format!("{:e}", v.abs()));
        let keep = exp + 1 + prec as i32;
        if keep < 0 {
            vec![0]
        } else {
            let keep = keep as usize;
            let mut k: Vec<u8> = (0..keep)
                .map(|i| digits.get(i).map_or(0, |d| d - b'0'))
                .collect();
            let round_up = digits.get(keep).is_some_and(|&d| d >= b'5');
            if round_up {
                let mut i = k.len();
                loop {
                    if i == 0 {
                        k.insert(0, 1);
                        break;
                    }
                    i -= 1;
                    if k[i] == 9 {
                        k[i] = 0;
                    } else {
                        k[i] += 1;
                        break;
                    }
                }
            }
            if k.is_empty() {
                k.push(0);
            }
            k
        }
    };
    while kept.len() < prec + 1 {
        kept.insert(0, 0);
    }
    // Retire les zéros de tête superflus de la partie entière.
    while kept.len() > prec + 1 && kept[0] == 0 {
        kept.remove(0);
    }
    let split = kept.len() - prec;
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.extend(kept[..split].iter().map(|d| (b'0' + d) as char));
    if prec > 0 {
        out.push('.');
        out.extend(kept[split..].iter().map(|d| (b'0' + d) as char));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_float_accepts_java_grammar() {
        assert_eq!(java_parse_float("1.5"), Some(1.5));
        assert_eq!(java_parse_float("  -2  "), Some(-2.0));
        assert_eq!(java_parse_float("+3."), Some(3.0));
        assert_eq!(java_parse_float(".25"), Some(0.25));
        assert_eq!(java_parse_float("1e3"), Some(1000.0));
        assert_eq!(java_parse_float("1.5E-1f"), Some(0.15));
        assert_eq!(java_parse_float("2d"), Some(2.0));
        assert_eq!(java_parse_float("0x1p3"), Some(8.0));
        assert_eq!(java_parse_float("0x1.8P1"), Some(3.0));
        assert_eq!(java_parse_float("-0X.8p0"), Some(-0.5));
        assert_eq!(java_parse_float("Infinity"), Some(f32::INFINITY));
        assert_eq!(java_parse_float("-Infinity"), Some(f32::NEG_INFINITY));
        assert!(java_parse_float("NaN").unwrap().is_nan());
        assert_eq!(java_parse_float("1e99"), Some(f32::INFINITY));
    }

    #[test]
    fn parse_float_rejects_non_java() {
        for s in [
            "", "   ", "inf", "nan", "infinity", "abc", "1,5", ".", "e5", "1e", "1.0x", "0x1",
            "0xp1", "1 2", "--1", "1ff",
        ] {
            assert_eq!(java_parse_float(s), None, "{s:?}");
        }
    }

    #[test]
    fn float_to_string_matches_java() {
        assert_eq!(java_float_to_string(600.0), "600.0");
        assert_eq!(java_float_to_string(1.0), "1.0");
        assert_eq!(java_float_to_string(-2.5), "-2.5");
        assert_eq!(java_float_to_string(4.317), "4.317");
        assert_eq!(java_float_to_string(0.001), "0.001");
        assert_eq!(java_float_to_string(1.0e-4), "1.0E-4");
        assert_eq!(java_float_to_string(1.0e7), "1.0E7");
        assert_eq!(java_float_to_string(9_999_999.0), "9999999.0");
        assert_eq!(java_float_to_string(1.234_567_9e8), "1.2345679E8");
        assert_eq!(java_float_to_string(0.0), "0.0");
        assert_eq!(java_float_to_string(-0.0), "-0.0");
        assert_eq!(java_float_to_string(f32::NAN), "NaN");
        assert_eq!(java_float_to_string(f32::INFINITY), "Infinity");
        assert_eq!(java_float_to_string(0.1), "0.1");
        assert_eq!(java_float_to_string(123.456), "123.456");
    }

    #[test]
    fn format_fixed_half_up_with_point() {
        assert_eq!(java_format_fixed(1.0, 1), "1.0");
        assert_eq!(java_format_fixed(-32.0, 1), "-32.0");
        assert_eq!(java_format_fixed(f64::from(1.0f32), 2), "1.00");
        assert_eq!(java_format_fixed(f64::from(4.317f32), 3), "4.317");
        assert_eq!(java_format_fixed(f64::from(5.612f32), 3), "5.612");
        assert_eq!(java_format_fixed(0.125, 2), "0.13");
        assert_eq!(java_format_fixed(1.005, 2), "1.01");
        assert_eq!(java_format_fixed(0.05, 1), "0.1");
        assert_eq!(java_format_fixed(0.04, 1), "0.0");
        assert_eq!(java_format_fixed(-0.04, 1), "-0.0");
        assert_eq!(java_format_fixed(9.96, 1), "10.0");
        assert_eq!(java_format_fixed(99.95, 1), "100.0");
        assert_eq!(java_format_fixed(0.0004, 3), "0.000");
        assert_eq!(java_format_fixed(0.0005, 3), "0.001");
        assert_eq!(java_format_fixed(123456.0, 0), "123456");
        assert_eq!(java_format_fixed(1e20, 1), "100000000000000000000.0");
        assert_eq!(java_format_fixed(f64::NAN, 1), "NaN");
        assert_eq!(java_format_fixed(f64::NEG_INFINITY, 2), "-Infinity");
        // Pas de virgule, quelle que soit la locale.
        assert_eq!(java_format_fixed(f64::from(8.4f32 * 2.0), 1), "16.8");
    }

    #[test]
    fn trim_matches_java() {
        assert_eq!(java_trim("\t a b \n"), "a b");
        assert_eq!(java_trim("\u{1}x\u{20}"), "x");
    }
}
