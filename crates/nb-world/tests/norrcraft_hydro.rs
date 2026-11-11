//! Tests d'acceptation de l'hydrographie Norrcraft (session 1.4 ; GDD doc 02 §6 et §13).

use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::hydro::seal;
use nb_world::norrcraft::{Hydrography, NorrConfig, NorrPreset, WaterColumn, WaterKind};
use nb_world::{Chunk, WorldProfile};

const S: i32 = 40;

fn profile(seed: i64, preset: NorrPreset) -> WorldProfile {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    WorldProfile::norrcraft(seed, NorrConfig::with_preset(preset), &reg)
}

/// Bilan d'une zone de `n × n` chunks générés.
#[derive(Default, Debug)]
struct Area {
    water: usize,
    above_sea: usize,
}

/// Génère `n × n` chunks à partir du chunk `(bx, bz)` et vérifie que toute l'eau est étanche :
/// aucun bloc d'eau n'a d'air pour voisin latéral (doc 02 §13), ni d'air dessous (pas de nappe
/// flottante), y compris d'un chunk à l'autre.
fn check_area(seed: i64, preset: NorrPreset, bx: i32, bz: i32, n: i32) -> Area {
    let water = BlockRegistry::for_kind(WorldKind::Norrcraft)
        .get("water")
        .id;
    let gen = profile(seed, preset).new_generator();
    let mut chunks = Vec::new();
    for cz in 0..n {
        for cx in 0..n {
            let mut c = Chunk::new(ChunkPos::new(bx + cx, bz + cz));
            gen.generate_chunk(&mut c);
            chunks.push(c);
        }
    }
    let at = |x: i32, y: i32, z: i32| -> Option<u8> {
        if x < 0 || z < 0 || x >= 16 * n || z >= 16 * n || y < 0 {
            return None;
        }
        let c = &chunks[(z / 16 * n + x / 16) as usize];
        Some(c.block(x % 16, y, z % 16))
    };
    let mut area = Area::default();
    for y in 0..127 {
        for z in 0..16 * n {
            for x in 0..16 * n {
                if at(x, y, z) != Some(water) {
                    continue;
                }
                area.water += 1;
                if y >= S {
                    area.above_sea += 1;
                }
                let (wx, wz) = (bx * 16 + x, bz * 16 + z);
                assert_ne!(
                    at(x, y - 1, z),
                    Some(0),
                    "graine {seed} : eau flottante en ({wx}, {y}, {wz})"
                );
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    assert_ne!(
                        at(x + dx, y, z + dz),
                        Some(0),
                        "graine {seed} : fuite en ({wx}, {y}, {wz}) vers ({}, {})",
                        wx + dx,
                        wz + dz
                    );
                }
            }
        }
    }
    area
}

/// Doc 02 §6 et §13 : l'eau est étanche partout, lacs et rivières au-dessus de la mer compris. Les
/// zones couvrent l'origine (lacs, rivières, marches), une côte à fjords, la chaîne (auges,
/// lacs de vallée) et les terres basses de l'est.
#[test]
fn water_is_sealed_everywhere() {
    let mut zones = vec![
        (12345, NorrPreset::Norrland, -60, 40),
        (12345, NorrPreset::Norrland, -352, 16),
        (20261111, NorrPreset::Skargard, 200, 0),
    ];
    // Zones choisies par le relevé : une marche de rivière (rapide, barrage), un lac du Worley,
    // un lac de creux ou de vallée au-dessus de la mer, pour chaque graine.
    for seed in [12345, -98765, 20261111] {
        for (x, z) in interesting_points(seed) {
            zones.push((
                seed,
                NorrPreset::Norrland,
                x.div_euclid(16) - 3,
                z.div_euclid(16) - 3,
            ));
        }
    }
    let (mut above, mut total) = (0, 0);
    for (seed, preset, bx, bz) in zones {
        let a = check_area(seed, preset, bx, bz, 6);
        eprintln!("graine {seed} {preset:?} ({bx}, {bz}) : {a:?}");
        above += a.above_sea;
        total += a.water;
    }
    assert!(
        total > 0 && above > 0,
        "trop peu d'eau dans les zones testées"
    );
}

/// Points remarquables à moins de 3 km de l'origine : une marche de rivière, le centre d'un lac
/// du Worley, un lac de creux au-dessus de la mer.
fn interesting_points(seed: i64) -> Vec<(i32, i32)> {
    let hy = Hydrography::new(seed, NorrConfig::default());
    let half = 3000;
    let lakes = hy.lakes(-half, -half, half + 8, half + 8);
    let mut out = Vec::new();
    if let Some(l) = lakes.lakes().find(|l| l.radius > 30.0) {
        out.push((l.center[0] as i32, l.center[1] as i32));
    }
    let (mut step, mut pond) = (None, None);
    'scan: for j in (-half..half).step_by(8) {
        for i in (-half..half).step_by(8) {
            let col = |x: i32, z: i32| hy.column_exact(f64::from(x), f64::from(z), &lakes).2;
            let c = col(i, j);
            if !c.is_wet() || c.level <= S {
                continue;
            }
            if step.is_none() && c.kind == WaterKind::River {
                let next = col(i + 8, j);
                if next.is_wet() && next.level != c.level {
                    step = Some((i, j));
                }
            }
            if pond.is_none() && c.kind == WaterKind::Lake && c.level == c.stair {
                pond = Some((i, j));
            }
            if step.is_some() && pond.is_some() {
                break 'scan;
            }
        }
    }
    eprintln!("graine {seed} : marche {step:?}, étang {pond:?}");
    out.extend(step);
    out.extend(pond);
    out
}

/// Relevé des colonnes brutes (exactes) sur une grille de `2 · half / step` points de côté.
fn survey(seed: i64, half: i32, step: i32) -> (Vec<WaterColumn>, usize) {
    let hy = Hydrography::new(seed, NorrConfig::default());
    let lakes = hy.lakes(-half, -half, half, half);
    let n = half / step;
    let mut out = Vec::new();
    for j in -n..n {
        for i in -n..n {
            let (x, z) = (f64::from(i * step), f64::from(j * step));
            out.push(hy.column_exact(x, z, &lakes).2);
        }
    }
    let big = lakes.lakes().filter(|l| l.radius >= 40.0).count();
    (out, big)
}

/// Doc 02 §6 et §13 : des lacs (dont au moins un de plus de 80 blocs) et des rivières à moins de
/// 5 km de l'origine ; la mer reste autour de 30 % ; les anciennes « mers intérieures » au-dessus
/// de S sont devenues des lacs.
#[test]
fn lakes_and_rivers_near_the_origin() {
    for seed in [12345, -98765, 20261111] {
        let (cols, big) = survey(seed, 5000, 20);
        let share =
            |k: WaterKind| cols.iter().filter(|c| c.kind == k).count() as f64 / cols.len() as f64;
        let (sea, lake, river) = (
            share(WaterKind::Sea),
            share(WaterKind::Lake),
            share(WaterKind::River),
        );
        eprintln!(
            "graine {seed} : mer {:.1} %, lacs {:.2} %, rivières {:.2} %, {big} grands lacs",
            sea * 100.0,
            lake * 100.0,
            river * 100.0
        );
        assert!(big > 0, "graine {seed} : aucun lac de plus de 80 blocs");
        assert!(lake > 0.01, "graine {seed} : {:.2} % de lacs", lake * 100.0);
        assert!(
            river > 0.003,
            "graine {seed} : {:.2} % de rivières",
            river * 100.0
        );
        assert!(sea < 0.45, "graine {seed} : {:.1} % de mer", sea * 100.0);
        // Toute la mer est au niveau S ; toute eau au-dessus de S est un lac ou une rivière.
        for c in cols.iter().filter(|c| c.is_wet()) {
            if c.kind == WaterKind::Sea {
                assert_eq!(c.level, S);
            }
            if c.level > S {
                assert!(matches!(c.kind, WaterKind::Lake | WaterKind::River));
            }
        }
    }
}

/// Doc 02 §6.1 : les nappes se calent sur l'escalier W (marches de 6 blocs au-dessus de S).
#[test]
fn rivers_and_ponds_follow_the_stair() {
    let (cols, _) = survey(12345, 4000, 16);
    let mut levels = std::collections::BTreeSet::new();
    for c in cols
        .iter()
        .filter(|c| c.is_wet() && c.kind != WaterKind::Lake)
    {
        assert_eq!((c.level - S).rem_euclid(6), 0, "{c:?}");
        assert_eq!(c.level, c.stair);
        levels.insert(c.level);
    }
    eprintln!("niveaux d'eau : {levels:?}");
    assert!(levels.len() >= 3, "trop peu de marches : {levels:?}");
}

/// Doc 02 §13 : continuité de la surface visible (sol ou eau), `|Δ| ≤ 3` hors falaises déclarées
/// (parois d'auge, marches de W : barrages et barres rocheuses).
#[test]
fn visible_surface_is_continuous_outside_declared_steps() {
    for seed in [12345, -98765] {
        let hy = Hydrography::new(seed, NorrConfig::default());
        let (x0, x1, z) = (-3000, 3000, 211);
        let lakes = hy.lakes(x0 - 1, z - 1, x1 + 1, z + 1);
        let raw = |x: i32, z: i32| hy.raw_column(x, z, &lakes);
        let column = |x: i32| {
            let n = [raw(x - 1, z), raw(x + 1, z), raw(x, z - 1), raw(x, z + 1)];
            seal(raw(x, z), n, 128)
        };
        let wall = |x: i32| hy.heights().column(x, z).wall();
        let mut prev = column(x0);
        let (mut steps, mut declared, mut wet) = (0, 0, 0);
        for x in x0 + 1..=x1 {
            let c = column(x);
            wet += usize::from(c.is_wet());
            if (c.surface() - prev.surface()).abs() > 3 {
                if c.dam || prev.dam || wall(x) || wall(x - 1) {
                    declared += 1;
                } else {
                    steps += 1;
                    eprintln!("graine {seed}, x = {x} : {prev:?} → {c:?}");
                }
            }
            prev = c;
        }
        eprintln!("graine {seed} : {declared} marches déclarées, {steps} non déclarées, {wet} colonnes mouillées");
        // Rares exceptions tolérées : contact entre deux lacs voisins, embouchure d'une rivière
        // dans un lac de niveau différent.
        assert!(steps <= 6, "graine {seed} : {steps} marches non déclarées");
    }
}

/// La colonne isolée (départ, hauteur) coïncide avec le chunk généré, eau comprise.
#[test]
fn isolated_columns_match_generated_chunks() {
    let p = profile(12345, NorrPreset::Norrland);
    let hy = Hydrography::new(12345, NorrConfig::default());
    let gen = p.new_generator();
    let water = BlockRegistry::for_kind(WorldKind::Norrcraft)
        .get("water")
        .id;
    for (cx, cz) in [(0, 0), (-3, 2), (5, 7)] {
        let mut chunk = Chunk::new(ChunkPos::new(cx, cz));
        gen.generate_chunk(&mut chunk);
        for z in 0..16 {
            for x in 0..16 {
                let c = hy.column(cx * 16 + x, cz * 16 + z);
                assert_eq!(
                    p.generator().surface_height(cx * 16 + x, cz * 16 + z),
                    c.top
                );
                assert_ne!(chunk.block(x, c.top - 1, z), 0);
                for y in c.top..c.level {
                    assert_eq!(chunk.block(x, y, z), water);
                }
                assert_eq!(chunk.block(x, c.surface(), z), 0);
            }
        }
    }
}
