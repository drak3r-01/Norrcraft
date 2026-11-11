//! Tests d'acceptation des briques mathématiques et des champs macroscopiques Norrcraft
//! (session 1.1 ; GDD doc 02 §2, §3, §4 et §13).

use nb_world::norrcraft::{
    ChunkFields, ClimateZone, MacroFields, MacroSample, NorrConfig, NorrPreset, COAST_C,
};

/// FNV-1a 64 bits sur des flottants (par leurs bits).
fn fnv_f64(hash: &mut u64, v: f64) {
    for b in v.to_bits().to_le_bytes() {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
}

/// Empreinte de 64 chunks (grille 8 × 8 autour de l'origine) : tous les champs des 256 colonnes,
/// lus par le treillis de chunk.
fn chunk_fingerprint(seed: i64, preset: NorrPreset) -> u64 {
    let fields = MacroFields::new(seed, NorrConfig::with_preset(preset));
    let mut hash = 0xCBF2_9CE4_8422_2325u64;
    for cz in -4..4 {
        for cx in -4..4 {
            let chunk = ChunkFields::new(&fields, cx * 37, cz * 41);
            for lz in 0..16 {
                for lx in 0..16 {
                    let s = chunk.at(lx, lz);
                    for v in [
                        s.c, s.o, s.t0, s.hm, s.u, s.e, s.lk, s.gt, s.vr, s.theta[0], s.theta[1],
                    ] {
                        fnv_f64(&mut hash, v);
                    }
                }
            }
        }
    }
    hash
}

/// Empreintes de référence (Norrland, 64 chunks, 3 graines). Un changement de formule, de constante
/// ou de graine de champ les modifie : à mettre à jour consciemment, avec une ligne dans le
/// compte rendu de session. Le calcul n'utilise que `+ − × ÷ sqrt floor` : mêmes valeurs sous Linux
/// et Windows.
const FINGERPRINTS: [(i64, u64); 3] = [
    (12345, 0xcddab82e37fc38ff),
    (-98765, 0xa681ebc4e0bfa4c3),
    (20261111, 0x2e260cc8767dd1d0),
];

#[test]
fn field_fingerprints_are_frozen() {
    for (seed, expected) in FINGERPRINTS {
        let actual = chunk_fingerprint(seed, NorrPreset::Norrland);
        assert_eq!(actual, expected, "graine {seed} : {actual:#x}");
    }
}

#[test]
fn presets_have_distinct_fingerprints() {
    let a = chunk_fingerprint(12345, NorrPreset::Norrland);
    let b = chunk_fingerprint(12345, NorrPreset::Hogfjall);
    assert_ne!(a, b, "Högfjäll change U et T₀");
    // Skärgård ne touche que la hauteur (sessions suivantes) : champs identiques à Norrland.
    assert_eq!(a, chunk_fingerprint(12345, NorrPreset::Skargard));
}

/// Aucune fonction transcendante dans le générateur (doc 02 §2) : un résultat bit à bit identique
/// sur toutes les plateformes exige de n'utiliser que `+ − × ÷ sqrt floor` et des comparaisons.
#[test]
fn norrcraft_module_has_no_transcendental_calls() {
    let forbidden = [
        ".sin(",
        ".cos(",
        ".tan(",
        ".asin(",
        ".acos(",
        ".atan(",
        ".atan2(",
        ".sinh(",
        ".cosh(",
        ".tanh(",
        ".exp(",
        ".exp2(",
        ".exp_m1(",
        ".ln(",
        ".ln_1p(",
        ".log(",
        ".log2(",
        ".log10(",
        ".powf(",
        ".powi(",
        ".cbrt(",
        ".hypot(",
        ".sin_cos(",
        ".mul_add(",
    ];
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/norrcraft");
    let mut files = 0;
    for entry in std::fs::read_dir(&dir).expect("dossier norrcraft") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        files += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for f in forbidden {
                assert!(
                    !code.contains(f),
                    "{}:{} appelle {f} (interdit : résultat non portable)",
                    path.display(),
                    n + 1
                );
            }
        }
    }
    assert!(files >= 4);
}

fn survey(seed: i64, preset: NorrPreset, half_width: i32, step: i32) -> Vec<MacroSample> {
    let f = MacroFields::new(seed, NorrConfig::with_preset(preset));
    let n = half_width / step;
    let mut out = Vec::new();
    for j in -n..n {
        for i in -n..n {
            out.push(f.sample_column(i * step, j * step));
        }
    }
    out
}

/// Répartition terre/mer sur 16 × 16 km autour de l'origine, trois graines : le doc 02 §7.5 vise
/// 30 % de mers (tolérance ±40 % relatifs, ici élargie : les champs seuls ne contiennent ni lacs
/// ni rivières, la mer vient de C).
#[test]
fn land_share_is_plausible_for_three_seeds() {
    for seed in [12345i64, -98765, 20261111] {
        let s = survey(seed, NorrPreset::Norrland, 8192, 128);
        let land = s.iter().filter(|s| s.c >= COAST_C).count() as f64 / s.len() as f64;
        eprintln!("graine {seed} : terre {:.1} %", land * 100.0);
        assert!(
            (0.50..=0.80).contains(&land),
            "graine {seed} : {:.1} % de terre sur 16 km",
            land * 100.0
        );
    }
}

/// Le départ du joueur doit pouvoir être en zone hémiboréale ou némorale (doc 02 §13) : il en existe
/// beaucoup, sur terre, au sud du monde.
#[test]
fn temperate_land_exists_for_three_seeds() {
    for seed in [12345i64, -98765, 20261111] {
        let s = survey(seed, NorrPreset::Norrland, 8192, 128);
        let temperate = s
            .iter()
            .filter(|s| s.is_land() && ClimateZone::from_t0(s.t0) >= ClimateZone::Hemiboreal)
            .count() as f64
            / s.len() as f64;
        assert!(
            temperate > 0.10,
            "graine {seed} : {:.1} % de terre tempérée",
            temperate * 100.0
        );
    }
}

#[test]
fn every_climate_zone_but_arctic_appears_within_16_km() {
    for seed in [12345i64, -98765, 20261111] {
        let s = survey(seed, NorrPreset::Norrland, 8192, 128);
        for z in [
            ClimateZone::Subarctic,
            ClimateZone::Boreal,
            ClimateZone::Hemiboreal,
            ClimateZone::Nemoral,
        ] {
            let share = s.iter().filter(|s| ClimateZone::from_t0(s.t0) == z).count();
            assert!(share > 0, "graine {seed} : aucune zone {}", z.name());
        }
    }
}

#[test]
fn hogfjall_is_colder_and_more_mountainous() {
    let n = survey(12345, NorrPreset::Norrland, 8192, 256);
    let h = survey(12345, NorrPreset::Hogfjall, 8192, 256);
    let mean = |v: &[MacroSample], f: fn(&MacroSample) -> f64| {
        v.iter().map(f).sum::<f64>() / v.len() as f64
    };
    assert!(mean(&h, |s| s.t0) < mean(&n, |s| s.t0) - 0.1);
    assert!(mean(&h, |s| s.u) > mean(&n, |s| s.u) + 0.15);
}

/// Les champs lents sont continus : un pas d'un bloc ne les fait jamais sauter (aucune marche aux
/// frontières de zone, doc 02 §7.4).
#[test]
fn fields_have_no_steps_along_a_long_transect() {
    let f = MacroFields::new(-98765, NorrConfig::default());
    let mut prev = f.sample_column(-12000, 500 + (-12000) / 7);
    for x in -11_999..12_000 {
        let s = f.sample_column(x, 500 + x / 7);
        for (a, b) in [
            (s.c, prev.c),
            (s.o, prev.o),
            (s.t0, prev.t0),
            (s.hm, prev.hm),
            (s.u, prev.u),
            (s.e, prev.e),
            (s.lk, prev.lk),
            (s.vr, prev.vr),
        ] {
            assert!((a - b).abs() < 0.03, "saut de {} à x = {x}", (a - b).abs());
        }
        prev = s;
    }
}
