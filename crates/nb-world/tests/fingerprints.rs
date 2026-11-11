//! Empreintes de référence des chunks vanilla (plan Norrcraft, session 0.1).
//!
//! Gèle la sortie des deux générateurs vanilla (Classic = Java, Norrblock) pour 3 graines × 16
//! chunks (carré 4×4, de -2 à 1). Toute migration du code vanilla doit laisser ces valeurs
//! identiques. Pour les régénérer volontairement : (vider les constantes ou `NB_PRINT_FP=1`) `cargo test -p nb-world
//! --test fingerprints -- --nocapture`.

use std::sync::Arc;

use nb_core::ChunkPos;
use nb_data::BlockRegistry;
use nb_world::{Chunk, GeneratorKind, WorldConfig, WorldGenerator, WorldType};

const SEEDS: [i64; 3] = [12345, -987_654_321, 20_261_107];

fn table(kind: GeneratorKind) -> Vec<u64> {
    let reg = BlockRegistry::builtin();
    let mut out = Vec::new();
    for seed in SEEDS {
        let cfg = match kind {
            GeneratorKind::Classic => WorldConfig::preset_java(WorldType::Normal),
            _ => WorldConfig::preset(WorldType::Normal),
        };
        assert_eq!(cfg.generator, kind);
        let g = WorldGenerator::new(seed, Arc::new(cfg), reg.ids());
        for cz in -2..2 {
            for cx in -2..2 {
                let mut c = Chunk::new(ChunkPos::new(cx, cz));
                g.generate_chunk(&mut c);
                out.push(c.fingerprint());
            }
        }
    }
    out
}

fn check(name: &str, got: Vec<u64>, want: &[u64]) {
    if want.is_empty() || std::env::var_os("NB_PRINT_FP").is_some() {
        println!("{name}:");
        for row in got.chunks(4) {
            let s: Vec<String> = row.iter().map(|v| format!("{v:#018x}")).collect();
            println!("    {},", s.join(", "));
        }
    }
    assert_eq!(got.len(), want.len(), "{name}: table de référence incomplète");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(g, w, "{name}: graine {} chunk n°{}", SEEDS[i / 16], i % 16);
    }
}

#[test]
fn classic_fingerprints_are_frozen() {
    check("CLASSIC", table(GeneratorKind::Classic), &CLASSIC);
}

#[test]
fn norrblock_fingerprints_are_frozen() {
    check("NORRBLOCK", table(GeneratorKind::Norrblock), &NORRBLOCK);
}

const CLASSIC: [u64; 48] = [
    0x8e50b62c55490146, 0x4e7d50c9cbba7a5a, 0x43ae3e8ba419c852, 0xdbc3835451428985,
    0xf0979564f9c216b6, 0x8f03463ad9b0620f, 0x0d1f72a70860397b, 0x104bb5bc6eafcc00,
    0x593f368feafaede1, 0x1364cb8f30605128, 0x3300759565bbadfe, 0x09e56eb1e18a545e,
    0xe9e29c51b6c442c8, 0x789d82bb529567df, 0x7b1fb771faf01c3d, 0xaf13e8bf7d454e27,
    0x20ef69fb05b3724c, 0xb46872f91b725767, 0x7d28d415e169c04a, 0xcbf018ebf6b973b9,
    0x998e3a38e7abf68f, 0x5edb3c9d831022a0, 0xa0079d2665ebebf6, 0xd02642945ddcbc6b,
    0xf5f375afaa8e3be7, 0x4b5f0080093e19a8, 0x7de56f86d46bcda9, 0xd229e778473f2b4b,
    0x9cc487c21290f203, 0xc2ebcc460761284a, 0x85511fb6743b5d35, 0x1fc58a29271b4d88,
    0xff845950645e77c7, 0x1215eb28a9033f86, 0xdc4c4151c7f26a51, 0x3199fcdecbf6d22d,
    0xd5d2bab7aec24dc9, 0x5a944f6393c37549, 0x6ffe1871e062098e, 0x14011f4678b0f745,
    0x1d5a40c5ab94eaf3, 0xfe22e20fd1aea0c9, 0xfd440698949ab426, 0x3f10d3c513d769b0,
    0x5668ba8748cd3038, 0xd919d7f081d58e3d, 0x72cff0177e7226cf, 0xd34bd611d0d667f6,
];

const NORRBLOCK: [u64; 48] = [
    0xca34ba51c0702b3f, 0x22095d1d16a827b0, 0x45e9fe6373c3ad23, 0x4c2e6ced45a7347d,
    0xf70616b1b8eb5c41, 0x28da2933ace38c4d, 0xfb2ae0d9cbe339d6, 0xe4cfce8c045e9301,
    0x22e03bac1b163e63, 0x0d0c2af20e81e24f, 0x312e3b428ae28da7, 0x7fd860c8a1ab22ca,
    0xb76be23a17396b88, 0xf2fe89bc74bbb545, 0x834df076a96dba68, 0x4a73abaa289b0a4a,
    0xfc782604d7162a6f, 0xfce433ddef543908, 0xe89d74057ed10557, 0x03bc42396bcf6ae6,
    0x91178ab63295e456, 0xa26e748464653c83, 0x803b1b58809b349c, 0x056e4a3a6220f8b8,
    0xb7b11df9218057a6, 0xa958dc915ab5cc16, 0x3ffc54ac3efcf9f3, 0xef75aa276d9e6acd,
    0xbbcb2a4066ad49d4, 0x671ece2663589d15, 0x364b4a4019a0e406, 0xe44e297a8bca9ea7,
    0x7efb6c1a6dbf686e, 0xb9b40e4a3ff92cf3, 0xbcf624e37241c1bd, 0x88b894bd6fb6ffaf,
    0xac07d697eb840160, 0x171ebe297db0cfed, 0x875b70b0fb81cf7c, 0x0a5ce3221e763410,
    0xfd7be875a0957779, 0xdb8b9e26264910ff, 0xa0162f2a7a551336, 0x46b3e8c9f643bfd3,
    0xd7c3e3bfc7bf0567, 0xb6cdc64c18600b98, 0xf4f814fcd0680ea0, 0x6f304512fc6b7266,
];
