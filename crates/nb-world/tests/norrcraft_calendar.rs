//! Latitude et durée du jour sur le terrain Norrcraft (session 3.1 ; GDD doc 04 §2.2).

use nb_data::{BlockRegistry, WorldKind};
use nb_world::calendar::{DayLight, Polar};
use nb_world::norrcraft::{NorrBlocks, NorrConfig, NorrPreset, NorrcraftTerrain};
use nb_world::TerrainGenerator;

const SEEDS: [i64; 3] = [12345, -98765, 20261111];

fn terrain(seed: i64, preset: NorrPreset) -> NorrcraftTerrain {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    NorrcraftTerrain::new(
        seed,
        NorrConfig::with_preset(preset),
        NorrBlocks::from_registry(&reg),
    )
}

fn latitude(t: &NorrcraftTerrain, x: i32, z: i32) -> f64 {
    t.climate_at(x, z).unwrap().latitude.expect("latitude")
}

/// Latitudes d'une grille de 41 × 41 points espacés de 1000 blocs.
fn survey(t: &NorrcraftTerrain) -> Vec<f64> {
    let mut v = Vec::new();
    for j in -20..=20 {
        for i in -20..=20 {
            v.push(latitude(t, i * 1000, j * 1000));
        }
    }
    v
}

#[test]
fn latitude_is_in_range_and_varies_across_the_map() {
    for seed in SEEDS {
        for preset in [
            NorrPreset::Norrland,
            NorrPreset::Skargard,
            NorrPreset::Hogfjall,
        ] {
            let v = survey(&terrain(seed, preset));
            let lo = v.iter().cloned().fold(f64::MAX, f64::min);
            let hi = v.iter().cloned().fold(f64::MIN, f64::max);
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            eprintln!("seed {seed} {preset:?} : λ {lo:.2}..{hi:.2}, moyenne {mean:.2}");
            assert!(lo >= 0.0 && hi <= 1.0);
            assert!(hi - lo > 0.15, "latitude presque constante : {lo}..{hi}");
        }
    }
}

#[test]
fn latitude_is_smooth_between_neighbouring_columns() {
    let t = terrain(SEEDS[0], NorrPreset::Norrland);
    for i in 0..400 {
        let x = i * 37 - 7000;
        let (a, b) = (latitude(&t, x, 123), latitude(&t, x + 1, 123));
        assert!((a - b).abs() < 0.01, "saut de {} en x = {x}", (a - b).abs());
    }
}

#[test]
fn the_north_has_midnight_sun_and_the_south_does_not() {
    // Au moins une région de chaque preset passe le cercle polaire (λ > 0,77) ou s'en approche ;
    // le sud tempéré garde un cycle jour-nuit complet en été.
    let mut north_hits = 0;
    let mut south_ok = 0;
    for seed in SEEDS {
        let t = terrain(seed, NorrPreset::Hogfjall);
        for &lat in &survey(&t) {
            let summer = DayLight::at(0.5, lat as f32);
            if summer.polar == Polar::MidnightSun {
                north_hits += 1;
            }
            if lat < 0.4 && summer.polar == Polar::None && summer.fraction < 0.8 {
                south_ok += 1;
            }
        }
    }
    eprintln!("soleil de minuit : {north_hits} points, sud : {south_ok}");
    assert!(south_ok > 0, "aucune région tempérée");
    assert!(north_hits > 0, "aucune région au-delà du cercle polaire");
}
