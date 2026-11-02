//! `String.format("%.nf", v)` de Java (chiffres les plus courts du double puis arrondi
//! HALF_UP), toujours avec un point décimal (même règle que les commandes, décision
//! 2026-11-01). Même algorithme que `nb_ecs::javafmt`, que nb-ui ne peut pas importer.

/// Chiffres significatifs et exposant décimal de `format!("{:e}")`.
fn sci_parts(s: &str) -> (Vec<u8>, i32) {
    let (mant, exp) = s.split_once('e').expect("notation scientifique");
    let digits = mant.bytes().filter(u8::is_ascii_digit).collect();
    (digits, exp.parse().expect("exposant"))
}

/// `String.format("%.{prec}f", v)`.
pub fn format_fixed(v: f64, prec: usize) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
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
            if digits.get(keep).is_some_and(|&d| d >= b'5') {
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
    while kept.len() > prec + 1 && kept[0] == 0 {
        kept.remove(0);
    }
    let split = kept.len() - prec;
    let mut out = String::new();
    if v.is_sign_negative() {
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
    fn java_rounding() {
        assert_eq!(format_fixed(0.125, 2), "0.13"); // HALF_UP (Rust donnerait 0.12)
        assert_eq!(format_fixed(0.1f32 as f64, 2), "0.10");
        assert_eq!(format_fixed(0.0012, 4), "0.0012");
        assert_eq!(format_fixed(9.96, 1), "10.0");
        assert_eq!(format_fixed(0.00004, 4), "0.0000");
        assert_eq!(format_fixed(-0.0005, 3), "-0.001");
        assert_eq!(format_fixed(12.0, 1), "12.0");
        assert_eq!(format_fixed(0.0, 3), "0.000");
    }
}
