//! Tests d'acceptation du relief Norrcraft (session 1.2 ; GDD doc 02 §5.1 à §5.4 et §13).

use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::{HeightFields, NorrConfig, NorrPreset, Relief};
use nb_world::{fnv1a, Chunk, WorldProfile};

const SEEDS: [i64; 3] = [12345, -98765, 20261111];
const S: f64 = 40.0;

fn profile(seed: i64, preset: NorrPreset) -> WorldProfile {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    WorldProfile::norrcraft(seed, NorrConfig::with_preset(preset), &reg)
}

/// Empreinte de 64 chunks générés (grille 8 × 8 dispersée autour de l'origine) : tous les blocs.
fn chunk_fingerprint(seed: i64) -> u64 {
    let gen = profile(seed, NorrPreset::Norrland).new_generator();
    let mut bytes = Vec::with_capacity(64 * 8);
    for cz in -4..4 {
        for cx in -4..4 {
            let mut chunk = Chunk::new(ChunkPos::new(cx * 37, cz * 41));
            gen.generate_chunk(&mut chunk);
            let v: Vec<u8> = chunk
                .voxels()
                .iter()
                .flat_map(|b| b.to_le_bytes())
                .collect();
            bytes.extend_from_slice(&fnv1a(&v).to_le_bytes());
        }
    }
    fnv1a(&bytes)
}

/// Empreintes de référence du terrain (Norrland, 64 chunks, 3 graines). Toute modification de la
/// fonction de hauteur, des champs ou du remplissage les change : à mettre à jour consciemment,
/// avec une ligne dans le compte rendu de session.
const FINGERPRINTS: [(i64, u64); 3] = [
    (12345, 0x9f7d757f74b2a9e9),
    (-98765, 0x8c2477e2eb5c04d8),
    (20261111, 0xbfda8287cacf5c6e),
];

#[test]
fn terrain_fingerprints_are_frozen() {
    let actual: Vec<(i64, u64)> = FINGERPRINTS
        .iter()
        .map(|&(seed, _)| (seed, chunk_fingerprint(seed)))
        .collect();
    assert_eq!(
        actual.as_slice(),
        FINGERPRINTS.as_slice(),
        "empreintes : {}",
        actual
            .iter()
            .map(|(s, h)| format!("({s}, {h:#x})"))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// Relief exact sur une grille de `2 · half / step` points de côté autour de l'origine.
fn survey(seed: i64, preset: NorrPreset, half: i32, step: i32) -> Vec<Relief> {
    let f = HeightFields::new(seed, NorrConfig::with_preset(preset));
    let n = half / step;
    let mut out = Vec::new();
    for j in -n..n {
        for i in -n..n {
            out.push(f.relief_exact(f64::from(i * step), f64::from(j * step)));
        }
    }
    out
}

fn share(v: &[Relief], pred: impl Fn(&Relief) -> bool) -> f64 {
    v.iter().filter(|r| pred(r)).count() as f64 / v.len() as f64
}

/// Doc 02 §7.5 : 30 % de mers sur 16 × 16 km autour de l'origine (ici, hors lacs et rivières,
/// qui viennent en 1.4 : la terre doit tourner autour de 70 %).
#[test]
fn land_share_is_about_seventy_percent() {
    for seed in SEEDS {
        let s = survey(seed, NorrPreset::Norrland, 8192, 128);
        let land = share(&s, |r| r.h.floor() >= S);
        eprintln!("graine {seed} : terre {:.1} %", land * 100.0);
        assert!(
            (0.62..=0.80).contains(&land),
            "graine {seed} : {:.1} % de terre",
            land * 100.0
        );
    }
}

#[test]
fn skargard_has_more_sea_than_norrland() {
    let n = survey(12345, NorrPreset::Norrland, 8192, 256);
    let k = survey(12345, NorrPreset::Skargard, 8192, 256);
    let land = |v: &[Relief]| share(v, |r| r.h.floor() >= S);
    assert!(land(&k) < land(&n) - 0.03, "{} / {}", land(&k), land(&n));
}

/// Doc 02 §2 et §5.3 : pas de sommet au-dessus de S + 84 = 124, des fonds de fjord vers S − 30,
/// une vraie chaîne (sommets au-dessus de S + 60) près de l'origine.
#[test]
fn heights_span_fjords_to_summits() {
    for seed in SEEDS {
        let s = survey(seed, NorrPreset::Norrland, 8192, 32);
        let max = s.iter().map(|r| r.h).fold(f64::MIN, f64::max);
        let min = s.iter().map(|r| r.h).fold(f64::MAX, f64::min);
        eprintln!("graine {seed} : h de {min:.1} à {max:.1}");
        assert!(
            max < S + 84.0 && max > S + 55.0,
            "graine {seed} : max {max}"
        );
        assert!(min >= 8.0, "graine {seed} : min {min}");
    }
}

/// Doc 02 §13 : des fjords, profonds (25 blocs sous la mer au moins), à moins de 8 km.
#[test]
fn deep_fjords_exist_near_the_origin() {
    for seed in SEEDS {
        let s = survey(seed, NorrPreset::Norrland, 8192, 32);
        let fjord = share(&s, |r| r.fjord && r.h < S);
        let deep = s.iter().filter(|r| r.fjord && r.h <= S - 25.0).count();
        eprintln!(
            "graine {seed} : fjords {:.2} %, {deep} points à 25 blocs de fond",
            fjord * 100.0
        );
        assert!(
            fjord > 0.002,
            "graine {seed} : {:.3} % de fjords",
            fjord * 100.0
        );
        assert!(deep > 5, "graine {seed} : {deep} points profonds");
    }
}

/// Le fond des fjords est sous la mer, et leurs parois dépassent de loin le niveau de la mer :
/// on mesure la hauteur avant creusement le long des fjords profonds.
#[test]
fn fjord_walls_are_high() {
    let s = survey(12345, NorrPreset::Norrland, 8192, 32);
    let tall = s
        .iter()
        .filter(|r| r.fjord && r.h <= S - 20.0 && r.before_valleys >= S + 30.0)
        .count();
    assert!(tall > 0, "aucun fjord encaissé de 50 blocs");
}

#[test]
fn hogfjall_has_plateaus_and_more_high_ground() {
    let n = survey(12345, NorrPreset::Norrland, 8192, 64);
    let h = survey(12345, NorrPreset::Hogfjall, 8192, 64);
    let high = |v: &[Relief]| share(v, |r| r.h >= S + 44.0);
    assert!(high(&h) > high(&n) * 1.5, "{} / {}", high(&h), high(&n));
    assert!(share(&h, |r| r.plateau > 0.5) > 0.01);
}

/// Doc 02 §13 : `|h(x + 1, z) − h(x, z)| ≤ 3` hors falaises déclarées (parois d'auge).
#[test]
fn surface_is_continuous_outside_declared_walls() {
    for seed in SEEDS {
        let f = HeightFields::new(seed, NorrConfig::default());
        let at = |x: i32| f.relief_exact(f64::from(x), f64::from(300 + x / 5));
        let mut prev = at(-12_000);
        let (mut steps, mut walls) = (0, 0);
        for x in -11_999..12_000 {
            let r = at(x);
            let d = (r.h.floor() - prev.h.floor()).abs();
            if d > 3.0 {
                if r.wall() || prev.wall() {
                    walls += 1;
                } else {
                    steps += 1;
                    eprintln!("graine {seed}, x = {x} : marche de {d} ({prev:?} → {r:?})");
                }
            }
            prev = r;
        }
        eprintln!("graine {seed} : {walls} marches sur parois d'auge");
        assert_eq!(steps, 0, "graine {seed} : marches hors parois");
    }
}

/// Doc 02 §13 (étanchéité) : sous le niveau de la mer, aucune eau n'a d'air pour voisin latéral,
/// y compris d'un chunk à l'autre.
#[test]
fn sea_water_is_sealed_across_chunks() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let water = reg.get("water").id;
    let gen = profile(12345, NorrPreset::Norrland).new_generator();
    // Premier trait de côte rencontré en venant de l'océan de l'ouest, le long de z = 0.
    let f = HeightFields::new(12345, NorrConfig::default());
    let coast = (-9000..0)
        .step_by(4)
        .find(|&x| f.column(x, 0).h >= S)
        .expect("une côte à l'ouest");
    let (bx, bz) = (coast.div_euclid(16) - 2, -2);
    let n = 4;
    let mut chunks = Vec::new();
    for cz in 0..n {
        for cx in 0..n {
            let mut c = Chunk::new(ChunkPos::new(bx + cx, bz + cz));
            gen.generate_chunk(&mut c);
            chunks.push(c);
        }
    }
    let at = |x: i32, y: i32, z: i32| -> Option<u8> {
        if x < 0 || z < 0 || x >= 16 * n || z >= 16 * n {
            return None;
        }
        let c = &chunks[(z / 16 * n + x / 16) as usize];
        Some(c.block(x % 16, y, z % 16))
    };
    let mut water_count = 0;
    for y in 0..S as i32 {
        for z in 0..16 * n {
            for x in 0..16 * n {
                if at(x, y, z) != Some(water) {
                    continue;
                }
                water_count += 1;
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    assert_ne!(at(x + dx, y, z + dz), Some(0), "fuite en ({x}, {y}, {z})");
                }
            }
        }
    }
    assert!(water_count > 0, "la zone testée doit contenir de la mer");
}

/// Le relief d'une colonne isolée (hauteur, départ du joueur) coïncide avec le chunk généré.
#[test]
fn surface_height_matches_generated_chunks_everywhere() {
    let p = profile(-98765, NorrPreset::Hogfjall);
    let gen = p.new_generator();
    let water = BlockRegistry::for_kind(WorldKind::Norrcraft)
        .get("water")
        .id;
    for (cx, cz) in [(-1, -1), (17, -230), (-311, 64)] {
        let mut chunk = Chunk::new(ChunkPos::new(cx, cz));
        gen.generate_chunk(&mut chunk);
        for z in 0..16 {
            for x in 0..16 {
                let h = p.generator().surface_height(cx * 16 + x, cz * 16 + z);
                assert_ne!(chunk.block(x, h - 1, z), 0);
                // Au-dessus du fond : de l'air, ou de l'eau (mer, lac, rivière : session 1.4).
                let above = chunk.block(x, h, z);
                assert!(above == 0 || above == water);
            }
        }
    }
}

/// Budget du doc 02 §12 : 2,5 ms par chunk sur un thread. Mesure indicative, à lancer en
/// `--release` : `cargo test --release -p nb-world --test norrcraft_relief -- --ignored --nocapture`.
#[test]
#[ignore]
fn chunk_generation_time() {
    let gen = profile(12345, NorrPreset::Norrland).new_generator();
    let t = std::time::Instant::now();
    let mut n = 0;
    for cz in -12..12 {
        for cx in -12..12 {
            let mut chunk = Chunk::new(ChunkPos::new(cx * 5 - 100, cz * 3));
            gen.generate_chunk(&mut chunk);
            n += 1;
        }
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    eprintln!("{ms:.2} ms par chunk ({n} chunks)");
}
