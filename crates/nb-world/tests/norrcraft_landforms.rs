//! Tests d'acceptation des formes de relief Norrcraft (session 1.3 ; GDD doc 02 §5.5 à §5.8 et
//! §13) : présence près de l'origine, continuité, blocs posés par le générateur.

use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::{Coast, Forms, HeightFields, NorrConfig, NorrPreset, Relief};
use nb_world::{Chunk, WorldProfile};

const SEEDS: [i64; 3] = [12345, -98765, 20261111];
const S: f64 = 40.0;

/// Une forme de relief et son prédicat (sur la colonne entière).
type Probe = (&'static str, fn(&Relief) -> bool);

/// Formes attendues près de l'origine. Le champ de lave, rare et « loin au large » (doc 02 §1),
/// a son propre test.
const PROBES: [Probe; 12] = [
    ("îlot de l'archipel", |r| {
        r.forms.archipelago > 0.3 && r.h >= S
    }),
    ("klint", |r| r.forms.coast == Coast::Klint),
    ("côte soulevée", |r| r.forms.coast == Coast::Raised),
    ("dunes", |r| r.forms.coast == Coast::Dunes),
    ("pré salé", |r| r.forms.coast == Coast::SaltMeadow),
    ("esker", |r| r.forms.esker > 3.0),
    ("marmite", |r| r.forms.kettle > 1.0),
    ("drumlin", |r| r.forms.drumlin > 2.0),
    ("tourbière", |r| r.forms.mire > 0.5),
    ("palse", |r| r.forms.palsa > 2.0),
    ("glacier", |r| r.forms.ice > 3.0),
    ("moraine", |r| r.forms.moraine > 1.5),
];

/// Cœur de la province de lave la plus forte (Gt maximal, au moins 0,5) sur une grille de pas
/// 200 jusqu'à `reach` blocs de l'origine.
fn lava_spot(f: &HeightFields, reach: i32) -> Option<(i32, i32, f64)> {
    let m = f.macro_fields();
    let n = reach / 200;
    (-n..=n)
        .flat_map(|j| (-n..=n).map(move |i| (i * 200, j * 200)))
        .map(|(x, z)| (x, z, m.sample(f64::from(x), f64::from(z)).gt))
        .filter(|&(_, _, gt)| gt > 0.5)
        .max_by(|a, b| a.2.total_cmp(&b.2))
}

/// Doc 02 §5.8 : là où Gt est fort, une plaine de basalte au-dessus de la mer (île volcanique),
/// présente pour les 3 graines à moins de 30 km.
#[test]
fn lava_fields_rise_where_geothermy_is_strong() {
    for seed in SEEDS {
        let f = HeightFields::new(seed, NorrConfig::default());
        let (x, z, gt) = lava_spot(&f, 30_000).expect("province de lave à moins de 30 km");
        let (mut lava, mut above) = (0, 0);
        for j in -10..10 {
            for i in -10..10 {
                let r = f.relief_exact(f64::from(x + i * 3), f64::from(z + j * 3));
                lava += u32::from(r.forms.lava > 0.5);
                above += u32::from(r.h >= S);
            }
        }
        eprintln!("graine {seed} : lave en ({x}, {z}), Gt {gt:.2}, {lava} / 400 de lave, {above} / 400 au-dessus de la mer");
        assert!(lava > 300, "graine {seed} : {lava} / 400 de lave");
        assert!(
            above > 300,
            "graine {seed} : {above} / 400 au-dessus de la mer"
        );
        // Continuité (doc 02 §13) du cœur de la province jusqu'à la haute mer, dans les quatre
        // directions : flanc de l'île, cratères ; seules les fissures sont des falaises.
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut prev = f.relief_exact(f64::from(x), f64::from(z));
            for k in 1..2500 {
                let r = f.relief_exact(f64::from(x + k * dx), f64::from(z + k * dz));
                let d = (r.h.floor() - prev.h.floor()).abs();
                assert!(
                    d <= 3.0 || r.wall() || prev.wall(),
                    "graine {seed} : marche de {d} à {k} blocs de ({x}, {z}) vers ({dx}, {dz})"
                );
                prev = r;
            }
        }
    }
}

/// Relief exact sur une grille de `2 · half / step` points de côté autour de `(cx, cz)`.
fn survey(seed: i64, preset: NorrPreset, half: i32, step: i32) -> Vec<(i32, i32, Relief)> {
    let f = HeightFields::new(seed, NorrConfig::with_preset(preset));
    let n = half / step;
    let mut out = Vec::new();
    for j in -n..n {
        for i in -n..n {
            let (x, z) = (i * step, j * step);
            out.push((x, z, f.relief_exact(f64::from(x), f64::from(z))));
        }
    }
    out
}

/// Doc 02 §13 (« chaque biome présent à moins de 6 km de l'origine », ici étendu aux formes de
/// relief à 12 km, le carré de 24 km) : toutes les formes existent pour les 3 graines.
#[test]
fn every_landform_exists_near_the_origin() {
    for seed in SEEDS {
        let s = survey(seed, NorrPreset::Norrland, 12_000, 40);
        let mut missing = Vec::new();
        let mut report = Vec::new();
        for (name, pred) in PROBES {
            let n = s.iter().filter(|(_, _, r)| pred(r)).count();
            report.push(format!("{name} {:.2} %", n as f64 * 100.0 / s.len() as f64));
            if n == 0 {
                missing.push(name);
            }
        }
        eprintln!("graine {seed} : {}", report.join(" · "));
        assert!(missing.is_empty(), "graine {seed} : absents {missing:?}");
    }
}

/// Skärgård (doc 02 §11 : K × 2) a bien plus d'îlots que Norrland.
#[test]
fn skargard_preset_has_more_islands() {
    let islands = |p| {
        survey(12345, p, 12_000, 48)
            .iter()
            .filter(|(_, _, r)| PROBES[0].1(r))
            .count()
    };
    let (n, k) = (islands(NorrPreset::Norrland), islands(NorrPreset::Skargard));
    eprintln!("îlots : Norrland {n}, Skärgård {k}");
    assert!(k * 2 > n * 3, "Norrland {n}, Skärgård {k}");
}

/// Doc 02 §13 : `|h(x + 1, z) − h(x, z)| ≤ 3` (et de même en z) hors falaises déclarées, sur des
/// carrés de 48 × 48 colonnes centrés sur des exemples de chaque forme.
#[test]
fn landforms_are_continuous_outside_declared_cliffs() {
    for seed in SEEDS {
        let f = HeightFields::new(seed, NorrConfig::default());
        let s = survey(seed, NorrPreset::Norrland, 12_000, 40);
        let mut steps = 0;
        for (name, pred) in PROBES {
            // Jusqu'à 3 exemples par forme, pris loin les uns des autres.
            let picks: Vec<_> = s
                .iter()
                .filter(|(_, _, r)| pred(r))
                .step_by(97)
                .take(3)
                .collect();
            for &&(cx, cz, _) in &picks {
                let half = 24;
                let side = (2 * half + 1) as usize;
                let grid: Vec<Relief> = (-half..=half)
                    .flat_map(|j| (-half..=half).map(move |i| (cx + i, cz + j)))
                    .map(|(x, z)| f.relief_exact(f64::from(x), f64::from(z)))
                    .collect();
                let at = |i: usize, j: usize| &grid[j * side + i];
                for j in 0..side {
                    for i in 0..side {
                        let a = at(i, j);
                        for b in [
                            (i + 1 < side).then(|| at(i + 1, j)),
                            (j + 1 < side).then(|| at(i, j + 1)),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            let d = (a.h.floor() - b.h.floor()).abs();
                            if d > 3.0 && !a.wall() && !b.wall() {
                                steps += 1;
                                if steps <= 10 {
                                    eprintln!(
                                        "graine {seed}, {name} près de ({cx}, {cz}) : marche de {d}\n  {:?}\n  {:?}",
                                        a.forms, b.forms
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(steps, 0, "graine {seed} : {steps} marches hors falaises");
    }
}

/// La glace ne se pose qu'en montagne, au-dessus de la limite des arbres, et la surface glace
/// comprise reste dans le monde.
#[test]
fn glaciers_sit_high_in_the_mountains() {
    let s = survey(12345, NorrPreset::Hogfjall, 12_000, 40);
    let iced: Vec<_> = s.iter().filter(|(_, _, r)| r.forms.ice > 0.5).collect();
    assert!(!iced.is_empty());
    for (x, z, r) in iced {
        assert!(r.h > S + 15.0, "glace basse en ({x}, {z}) : {}", r.h);
        assert!(r.h + r.forms.ice < 127.0, "glace trop haute en ({x}, {z})");
    }
}

fn first_column(
    seed: i64,
    preset: NorrPreset,
    pred: impl Fn(&Forms) -> bool,
) -> Option<(i32, i32)> {
    survey(seed, preset, 12_000, 20)
        .into_iter()
        .find(|(_, _, r)| pred(&r.forms))
        .map(|(x, z, _)| (x, z))
}

/// Bloc attendu, préréglage du monde, et prédicat de la colonne où le chercher.
type BlockCase = (&'static str, NorrPreset, fn(&Forms) -> bool);

/// Le générateur pose les blocs des formes : glace bleue, tourbe, permafrost, sable, galets,
/// basalte.
#[test]
fn generator_places_landform_blocks() {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let cases: [BlockCase; 5] = [
        ("blais", NorrPreset::Hogfjall, |f| f.ice > 4.0),
        ("torv", NorrPreset::Norrland, |f| {
            f.mire > 0.9 && f.palsa == 0.0
        }),
        ("permafrost", NorrPreset::Norrland, |f| f.palsa > 2.5),
        ("flygsand", NorrPreset::Norrland, |f| f.coast == Coast::Dunes),
        ("klappersten", NorrPreset::Norrland, |f| {
            f.coast == Coast::Raised
        }),
    ];
    let mut spots: Vec<(&str, NorrPreset, (i32, i32))> = cases
        .into_iter()
        .map(|(block, preset, pred)| {
            let at = first_column(12345, preset, pred)
                .unwrap_or_else(|| panic!("pas de colonne pour {block}"));
            (block, preset, at)
        })
        .collect();
    let f = HeightFields::new(12345, NorrConfig::default());
    let (lx, lz, _) = lava_spot(&f, 30_000).expect("province de lave");
    spots.push(("basaltsten", NorrPreset::Norrland, (lx, lz)));
    for (block, preset, (x, z)) in spots {
        let id = reg.get(block).id;
        let profile = WorldProfile::norrcraft(12345, NorrConfig::with_preset(preset), &reg);
        let gen = profile.new_generator();
        let mut chunk = Chunk::new(ChunkPos::new(x.div_euclid(16), z.div_euclid(16)));
        gen.generate_chunk(&mut chunk);
        let found = chunk.voxels().iter().filter(|&&b| b == id).count();
        assert!(found > 0, "{block} absent du chunk de ({x}, {z})");
    }
}
