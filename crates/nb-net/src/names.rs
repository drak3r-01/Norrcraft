//! Noms de joueur : valides (1–16 caractères visibles) et uniques dans la partie.

/// Longueur maximale d'un nom (en caractères).
pub const MAX_NAME_LEN: usize = 16;

/// Nettoie un nom saisi : retire les caractères de contrôle, réduit les espaces, tronque.
/// Vide → `"Player"`.
pub fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cut: String = cleaned.chars().take(MAX_NAME_LEN).collect();
    let cut = cut.trim().to_owned();
    if cut.is_empty() {
        "Player".to_owned()
    } else {
        cut
    }
}

/// Rend `name` unique parmi `taken` (insensible à la casse) en ajoutant ` 2`, ` 3`…
pub fn unique_name<'a>(name: &str, taken: impl IntoIterator<Item = &'a str> + Clone) -> String {
    let is_taken = |n: &str| taken.clone().into_iter().any(|t| t.eq_ignore_ascii_case(n));
    let base = sanitize_name(name);
    if !is_taken(&base) {
        return base;
    }
    for i in 2.. {
        let suffix = format!(" {i}");
        let keep = MAX_NAME_LEN.saturating_sub(suffix.chars().count());
        let candidate = format!(
            "{}{}",
            base.chars().take(keep).collect::<String>().trim(),
            suffix
        );
        if !is_taken(&candidate) {
            return candidate;
        }
    }
    unreachable!("boucle infinie bornée par le nombre de joueurs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_name("  Gabriel \n"), "Gabriel");
        assert_eq!(sanitize_name(""), "Player");
        assert_eq!(sanitize_name("\u{7}\u{8}"), "Player");
        assert_eq!(sanitize_name("a   b"), "a b");
        assert_eq!(sanitize_name(&"x".repeat(40)).chars().count(), MAX_NAME_LEN);
    }

    #[test]
    fn unique() {
        let taken = ["Gab", "gab 2"];
        assert_eq!(unique_name("Léa", taken), "Léa");
        assert_eq!(unique_name("GAB", taken), "GAB 3");
        let long = "x".repeat(16);
        let u = unique_name(&long, [long.as_str()]);
        assert!(u.chars().count() <= MAX_NAME_LEN && u.ends_with(" 2"));
    }
}
