//! Vecteurs de référence produits par le Java d'origine (spec §16).

use std::sync::Arc;

use nb_core::ChunkPos;
use nb_data::BlockRegistry;
use nb_world::{
    find_spawn_location, fnv1a, Chunk, GeneratorKind, LightTable, WorldConfig, WorldGenerator,
    WorldType,
};

/// Les vecteurs de référence sont ceux du Java : ils valident le générateur `Classic`.
fn classic(t: WorldType) -> WorldConfig {
    let c = WorldConfig::preset_java(t);
    assert_eq!(c.generator, GeneratorKind::Classic);
    c
}

const POSITIONS: [(i32, i32); 5] = [(0, 0), (-1, 2), (5, -3), (-17, -9), (30, 30)];

fn generator(reg: &BlockRegistry, seed: i64, t: WorldType) -> WorldGenerator {
    WorldGenerator::new(seed, Arc::new(classic(t)), reg.ids()).with_java_lava_pass(true)
}

fn chunk(g: &WorldGenerator, cx: i32, cz: i32) -> Chunk {
    let mut c = Chunk::new(ChunkPos::new(cx, cz));
    g.generate_chunk(&mut c);
    c
}

/// Décompte « nom=n » dans l'ordre des identifiants (format de la spec).
fn counts(reg: &BlockRegistry, c: &Chunk) -> String {
    let mut n = [0usize; 256];
    for &b in c.voxels().iter() {
        n[usize::from(b)] += 1;
    }
    reg.iter()
        .filter(|b| n[usize::from(b.id)] > 0)
        .map(|b| format!("{}={}", b.name, n[usize::from(b.id)]))
        .collect::<Vec<_>>()
        .join(" ")
}

fn spawn(g: &WorldGenerator) -> [f64; 3] {
    find_spawn_location(g.config().sea_level, |x, z| g.compute_surface_height(x, z))
}

/// Lumière du chunk (0,0) avec ses 8 voisins fraîchement générés : (fnv, histogramme).
fn light_00(reg: &BlockRegistry, g: &WorldGenerator) -> (u64, [usize; 16]) {
    let mut chunks = Vec::new();
    for dz in -1..=1 {
        for dx in -1..=1 {
            chunks.push(chunk(g, dx, dz));
        }
    }
    let mut cache: [Option<&Chunk>; 9] = [None; 9];
    for (slot, c) in cache.iter_mut().zip(&chunks) {
        *slot = Some(c);
    }
    let light = nb_world::compute_light(&cache, &LightTable::new(reg));
    let mut hist = [0usize; 16];
    for &l in light.iter() {
        hist[usize::from(l)] += 1;
    }
    (fnv1a(light.as_slice()), hist)
}

fn check_fnvs(seed: i64, t: WorldType, spawn_ref: [f64; 3], fnvs: [u64; 5]) {
    let reg = BlockRegistry::builtin();
    let g = generator(&reg, seed, t);
    assert_eq!(spawn(&g), spawn_ref, "spawn {t:?} seed {seed}");
    for ((cx, cz), f) in POSITIONS.into_iter().zip(fnvs) {
        assert_eq!(
            chunk(&g, cx, cz).fingerprint(),
            f,
            "{t:?} seed {seed} chunk ({cx},{cz})"
        );
    }
}

#[test]
fn normal_seed_12345_chunks_counts_and_light() {
    let reg = BlockRegistry::builtin();
    let g = generator(&reg, 12345, WorldType::Normal);
    assert_eq!(spawn(&g), [-31.5, 49.0, -31.5]);
    let expected = [
        (0x3300759565bbadfe, "air=24064 stone=5215 water=2033 sand=1024 bedrock=256 clay=63 coal_ore=53 iron_ore=26 gold_ore=11 diamond_ore=7 emerald_ore=6 lava=10"),
        (0x1d923785e1bba4db, "air=24642 grass=9 water=1663 sand=1005 bedrock=256 clay=4915 tall_grass=4 lily_pad=8 lava=266"),
        (0xc9333655e4eebcf7, "air=24370 stone=4983 water=2244 sand=775 bedrock=256 coal_ore=51 iron_ore=22 gold_ore=12 diamond_ore=16 emerald_ore=7 lava=32"),
        (0x666b0b07e783bb97, "air=21585 dirt=768 grass=253 stone=9443 bedrock=256 wood=12 leaves=154 tall_grass=42 coal_ore=115 iron_ore=90 gold_ore=36 diamond_ore=7 emerald_ore=7"),
        (0x73eb580f4c326acb, "air=24990 stone=4198 water=2048 sand=744 bedrock=256 coal_ore=42 iron_ore=15 gold_ore=11 diamond_ore=7 emerald_ore=5 lava=452"),
    ];
    for ((cx, cz), (f, cnt)) in POSITIONS.into_iter().zip(expected) {
        let c = chunk(&g, cx, cz);
        assert_eq!(counts(&reg, &c), cnt, "décompte ({cx},{cz})");
        assert_eq!(c.fingerprint(), f, "fnv ({cx},{cz})");
    }
    let (f, hist) = light_00(&reg, &g);
    assert_eq!(
        hist,
        [6902, 256, 0, 256, 0, 256, 0, 256, 0, 256, 0, 256, 0, 256, 0, 24074]
    );
    assert_eq!(f, 0x3f00fe567734ed11);
}

#[test]
fn normal_other_seeds() {
    check_fnvs(
        0,
        WorldType::Normal,
        [-31.5, 46.0, 32.5],
        [
            0xd533636a3f60da83,
            0x57ce6f46fbfa3c60,
            0xb79db8dd3c12a932,
            0x9d0ac739169ab0e0,
            0x8567b76f1f58694e,
        ],
    );
    check_fnvs(
        -987654321012345,
        WorldType::Normal,
        [-15.5, 44.0, -15.5],
        [
            0xa8eb4fec7123907d,
            0x7ebe349aedb21e8c,
            0xd5925b5c99b342e3,
            0x54e41135ad2629a0,
            0x6e7ffdf6b19d6892,
        ],
    );
    check_fnvs(
        42,
        WorldType::Normal,
        [-31.5, 47.0, 0.5],
        [
            0xc38bda97bc01342e,
            0x17e0d2cb6cedcb9f,
            0x403c38ceda013d21,
            0xa132b279e363f2aa,
            0x09d7b49732612653,
        ],
    );
    let reg = BlockRegistry::builtin();
    let g0 = generator(&reg, 0, WorldType::Normal);
    assert_eq!(
        counts(&reg, &chunk(&g0, -1, 2)),
        "air=20673 dirt=768 grass=256 stone=10341 bedrock=256 wood=9 leaves=154 tall_grass=47 coal_ore=134 iron_ore=83 gold_ore=31 diamond_ore=7 emerald_ore=9"
    );
    let g42 = generator(&reg, 42, WorldType::Normal);
    assert_eq!(
        counts(&reg, &chunk(&g42, -17, -9)),
        "air=22648 dirt=756 grass=246 stone=8444 bedrock=256 acacia_wood=5 acacia_leaves=51 tall_grass=80 coal_ore=105 iron_ore=79 gold_ore=36 diamond_ore=7 emerald_ore=7 lava=48"
    );
    assert_eq!(
        counts(&reg, &chunk(&g42, 30, 30)),
        "air=23088 dirt=768 grass=256 stone=8041 bedrock=256 tall_grass=82 mushroom_brown=1 mushroom_red=1 coal_ore=87 iron_ore=42 gold_ore=19 diamond_ore=8 emerald_ore=5 lava=114"
    );
}

#[test]
fn amplified_all_seeds() {
    check_fnvs(
        12345,
        WorldType::Amplified,
        [-31.5, 85.0, -31.5],
        [
            0xe0b6b60b34d6fc42,
            0xb5111373eb7bcdd9,
            0x64ccbc434916e59a,
            0x6477f4934fe3e1b2,
            0x73eb580f4c326acb,
        ],
    );
    check_fnvs(
        0,
        WorldType::Amplified,
        [16.5, 37.0, 16.5],
        [
            0xa92175878026f14c,
            0x97fc1b79d2ffbfe2,
            0xa5dbd3f3be9ec6b6,
            0x504aad36e7fc08a5,
            0x95d90a9fd5d0fedc,
        ],
    );
    check_fnvs(
        -987654321012345,
        WorldType::Amplified,
        [-15.5, 71.0, -15.5],
        [
            0xe967695c4cf81657,
            0x119c3960184ca3df,
            0xec6a9035462ccdd6,
            0x111cd49328a00bb4,
            0x64d104252a33b1d2,
        ],
    );
    check_fnvs(
        42,
        WorldType::Amplified,
        [-15.5, 36.0, 0.5],
        [
            0x9ccaa2549ae3cd9b,
            0xe306bbee9efeaab5,
            0x03ea3e60c4cdf1a7,
            0xa2314bfe5aa5b57e,
            0xcfbe0f4391adc378,
        ],
    );
    let reg = BlockRegistry::builtin();
    let g = generator(&reg, 12345, WorldType::Amplified);
    assert_eq!(light_00(&reg, &g).0, 0x3f00fe567734ed11);
}

#[test]
fn island_all_seeds() {
    check_fnvs(
        12345,
        WorldType::Island,
        [-31.5, 49.0, -31.5],
        [
            0x3300759565bbadfe,
            0x1d923785e1bba4db,
            0xf5a064046d57e020,
            0x666b0b07e783bb97,
            0x73eb580f4c326acb,
        ],
    );
    check_fnvs(
        0,
        WorldType::Island,
        [-31.5, 46.0, 32.5],
        [
            0xd533636a3f60da83,
            0x57ce6f46fbfa3c60,
            0xb79db8dd3c12a932,
            0x9e0bf0b5fbcdd787,
            0x8567b76f1f58694e,
        ],
    );
    check_fnvs(
        -987654321012345,
        WorldType::Island,
        [-15.5, 44.0, -15.5],
        [
            0xa8eb4fec7123907d,
            0x5a66de52d7de84bc,
            0xd5925b5c99b342e3,
            0x54e41135ad2629a0,
            0x6e7ffdf6b19d6892,
        ],
    );
    check_fnvs(
        42,
        WorldType::Island,
        [-31.5, 47.0, 0.5],
        [
            0xe26b0ab65a81bce5,
            0x4b1098ea8b024360,
            0x403c38ceda013d21,
            0xa132b279e363f2aa,
            0x09d7b49732612653,
        ],
    );
    let reg = BlockRegistry::builtin();
    let g = generator(&reg, 0, WorldType::Island);
    assert_eq!(
        counts(&reg, &chunk(&g, -17, -9)),
        "air=22431 dirt=768 grass=256 stone=8291 bedrock=256 jungle_wood=22 jungle_leaves=256 fern=60 coal_ore=104 iron_ore=80 gold_ore=33 diamond_ore=7 emerald_ore=6 lava=198"
    );
}

const FLAT_STD: u64 = 0x4bd00d1336dd5925;

#[test]
fn flat_java_mode_matches_reference() {
    let reg = BlockRegistry::builtin();
    let cases: [(i64, [f64; 3], [u64; 5]); 4] = [
        (
            12345,
            [32.5, 8.0, -31.5],
            [FLAT_STD, FLAT_STD, 0x404eeb2ba8464d63, FLAT_STD, FLAT_STD],
        ),
        (
            0,
            [-15.5, 8.0, -15.5],
            [FLAT_STD, FLAT_STD, FLAT_STD, 0x33dd3bf02f06ad16, FLAT_STD],
        ),
        (
            -987654321012345,
            [-31.5, 7.0, 32.5],
            [FLAT_STD, 0x8fb02bff1f940363, FLAT_STD, FLAT_STD, FLAT_STD],
        ),
        (
            42,
            [16.5, 8.0, 16.5],
            [
                0x9c17584089e39da3,
                0x12b28f3f66bdaf85,
                FLAT_STD,
                FLAT_STD,
                FLAT_STD,
            ],
        ),
    ];
    for (seed, sp, fnvs) in cases {
        check_fnvs(seed, WorldType::Flat, sp, fnvs);
    }
    let g = generator(&reg, 12345, WorldType::Flat);
    assert_eq!(
        counts(&reg, &chunk(&g, 0, 0)),
        "air=29696 grass=256 sand=768 bedrock=256 lava=1792"
    );
    let (f, hist) = light_00(&reg, &g);
    assert_eq!(hist[0], 1280);
    assert_eq!(hist[15], 31488);
    assert_eq!(f, 0xeb539c0b961c1625);
}

/// Écart volontaire : FLAT n'est plus noyé sous la lave (décision de Gabriel).
#[test]
fn flat_corrected_has_no_lava() {
    let reg = BlockRegistry::builtin();
    let ids = reg.ids();
    let g = WorldGenerator::new(12345, Arc::new(classic(WorldType::Flat)), reg.ids());
    for (cx, cz) in POSITIONS {
        let c = chunk(&g, cx, cz);
        assert!(
            c.voxels().iter().all(|&b| b != ids.lava),
            "lave dans ({cx},{cz})"
        );
    }
    // Plaine standard : bedrock, 3 couches de sable, herbe en y = 4 (désormais à l'air libre,
    // donc la végétation y pousse), air au-dessus.
    let c = chunk(&g, 0, 0);
    assert_eq!(
        counts(&reg, &c),
        "air=31427 grass=256 sand=768 bedrock=256 tall_grass=61"
    );
    for x in 0..16 {
        for z in 0..16 {
            assert_eq!(c.block(x, 0, z), ids.bedrock);
            assert_eq!(c.block(x, 4, z), ids.grass);
            assert!([ids.air, ids.tall_grass].contains(&c.block(x, 5, z)));
            assert_eq!(c.block(x, 6, z), ids.air);
        }
    }
    // Le spawn ne dépend pas de la passe de lave.
    assert_eq!(spawn(&g), [32.5, 8.0, -31.5]);
    // Les autres préréglages (grottes actives) restent identiques au Java.
    let normal = WorldGenerator::new(12345, Arc::new(classic(WorldType::Normal)), reg.ids());
    assert_eq!(chunk(&normal, 0, 0).fingerprint(), 0x3300759565bbadfe);
}

#[test]
fn surface_heights_seed_12345() {
    let reg = BlockRegistry::builtin();
    let g = generator(&reg, 12345, WorldType::Normal);
    for (wx, wz, h) in [
        (0, 0, 26),
        (100, -50, 24),
        (-300, 700, 12),
        (1000, 1000, 26),
        (-2048, 513, 49),
        (4000, -4000, 26),
    ] {
        assert_eq!(g.compute_surface_height(wx, wz), h, "({wx},{wz})");
    }
}

#[test]
fn generation_is_deterministic_and_seed_dependent() {
    let reg = BlockRegistry::builtin();
    let a = generator(&reg, 777, WorldType::Normal);
    let b = generator(&reg, 777, WorldType::Normal);
    // Deux graines égales modulo 2^48 donnent le même monde (java.util.Random).
    let c = generator(&reg, 777 + (1i64 << 48), WorldType::Normal);
    let d = generator(&reg, 778, WorldType::Normal);
    let mut differs = false;
    for (cx, cz) in [(0, 0), (3, -7), (-12, 4)] {
        let fa = chunk(&a, cx, cz);
        assert_eq!(fa, chunk(&b, cx, cz));
        assert_eq!(fa.voxels(), chunk(&c, cx, cz).voxels());
        differs |= fa.voxels() != chunk(&d, cx, cz).voxels();
    }
    assert!(differs);
    // Coordonnées extrêmes : arithmétique enveloppante, pas de panique.
    let _ = chunk(&a, i32::MAX / 16, i32::MIN / 16);
}
