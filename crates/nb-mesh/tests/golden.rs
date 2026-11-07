//! NOTE : depuis la passe de correction visuelle, le mesher s'écarte volontairement du Java sur
//! deux points (fusion des quads limitée aux axes où AO/lumière sont constantes, diagonale des
//! quads choisie selon la luminosité des coins ; seul le second change ces empreintes). Les empreintes ci-dessous sont celles du Rust actuel ;
//! les valeurs strictement Java sont dans l'historique git (commit 5e223dc).
//!
//! Tests dorés : empreintes bit à bit de `ChunkMesher.generateChunkMeshData` (Java) sur des
//! scènes pseudo-aléatoires.
//!
//! Les empreintes ont été produites par un harnais Java compilant les sources d'origine
//! (`ChunkMesher`, `ChunkSnapshot`, `BlockType`, `BlockRegistry`) avec des bouchons minimaux :
//! `TextureRegistry.getLayer(n) = floorMod(n.hashCode(), 61)` et un `BiomeSystem` dont
//! `getBiome` vaut soit un biome fixe, soit `floorMod(floorDiv(x,5) + floorDiv(z,7), 8)`, avec
//! les teintes de `BiomeSystem.setupConfigs`. Les générateurs de scènes ci-dessous sont la
//! transcription exacte de ceux du harnais (même LCG, même ordre de tirage).

use nb_core::math::{block_index, CHUNK_VOLUME};
use nb_data::BlockRegistry;
use nb_mesh::{
    java_tints, mesh_chunk, BiomeId, BiomeSource, BiomeTints, BlockTable, ChunkPos, ChunkVoxels,
    Neighborhood,
};

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % u64::from(n)) as u32
    }
}

fn idx(x: u32, y: u32, z: u32) -> usize {
    block_index(x as usize, y as usize, z as usize)
}

fn terrain(r: &mut Lcg) -> Vec<u8> {
    let mut v = vec![0u8; CHUNK_VOLUME];
    for x in 0..16 {
        for z in 0..16 {
            let h = 36 + r.next(9);
            for y in 0..=h {
                v[idx(x, y, z)] = if y == 0 {
                    6
                } else if y + 3 < h {
                    3
                } else if y < h {
                    1
                } else if h < 40 {
                    5
                } else {
                    2
                };
            }
            for y in h + 1..=40 {
                v[idx(x, y, z)] = 4;
            }
            if h >= 40 {
                let k = r.next(100);
                if k < 20 {
                    v[idx(x, h + 1, z)] = (19 + r.next(7)) as u8;
                } else if k < 23 {
                    for y in h + 1..h + 5 {
                        v[idx(x, y, z)] = 9;
                    }
                    v[idx(x, h + 5, z)] = 10;
                } else if k < 25 {
                    v[idx(x, h + 1, z)] = 27;
                    v[idx(x, h + 2, z)] = 27;
                }
            } else if r.next(4) == 0 {
                v[idx(x, 41, z)] = 26;
            }
            if r.next(30) == 0 {
                let y = 10 + r.next(10);
                v[idx(x, y, z)] = 33;
            }
            if r.next(25) == 0 {
                let y = 20 + r.next(10);
                v[idx(x, y, z)] = 0;
            }
        }
    }
    for _ in 0..40 {
        let x = r.next(16);
        let y = 42 + r.next(10);
        let z = r.next(16);
        v[idx(x, y, z)] = (10 + 2 * r.next(5)) as u8;
    }
    v
}

fn noise(r: &mut Lcg) -> Vec<u8> {
    (0..CHUNK_VOLUME)
        .map(|_| {
            let k = r.next(100);
            if k < 55 {
                0
            } else if k < 98 {
                r.next(35) as u8
            } else {
                (40 + r.next(200)) as u8
            }
        })
        .collect()
}

fn cave(r: &mut Lcg) -> Vec<u8> {
    let mut v = vec![3u8; CHUNK_VOLUME];
    for _ in 0..300 {
        let x = r.next(16);
        let y = 5 + r.next(60);
        let z = r.next(16);
        let s = 1 + r.next(3);
        let b = if r.next(10) == 0 {
            33
        } else if r.next(10) == 0 {
            4
        } else {
            0
        };
        for dx in 0..s {
            for dy in 0..s {
                for dz in 0..s {
                    if x + dx < 16 && y + dy < 128 && z + dz < 16 {
                        v[idx(x + dx, y + dy, z + dz)] = b;
                    }
                }
            }
        }
    }
    v
}

/// `BiomeSystem` du harnais : biome fixe (`mode >= 0`) ou damier `floorDiv(x,5)+floorDiv(z,7)`.
struct HarnessBiomes(i32);

impl BiomeSource for HarnessBiomes {
    fn biome_at(&self, wx: i32, wz: i32) -> BiomeId {
        if self.0 >= 0 {
            self.0 as BiomeId
        } else {
            (wx.div_euclid(5) + wz.div_euclid(7)).rem_euclid(8) as BiomeId
        }
    }
    fn tints(&self, biome: BiomeId) -> BiomeTints {
        java_tints(biome)
    }
}

/// `String.hashCode` de Java.
fn java_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
}

fn fnv(floats: &[f32]) -> u64 {
    nb_mesh::mesher::fingerprint_floats(floats)
}

#[allow(clippy::too_many_arguments)]
fn check(
    name: &str,
    px: i32,
    pz: i32,
    kind: u32,
    biome_mode: i32,
    missing: u32,
    seed: u64,
    len: usize,
    hash: u64,
) {
    let table = BlockTable::from_registry(&BlockRegistry::builtin(), |n| {
        java_hash(n).rem_euclid(61) as u32
    });
    let mut r = Lcg(seed);
    let voxels: Vec<ChunkVoxels> = (0..9)
        .map(|_| {
            ChunkVoxels::from_vec(match kind {
                0 => terrain(&mut r),
                1 => noise(&mut r),
                _ => cave(&mut r),
            })
        })
        .collect();
    let chunks: [Option<&ChunkVoxels>; 9] =
        std::array::from_fn(|i| (missing & (1 << i) == 0).then(|| &voxels[i]));
    let n = Neighborhood::from_voxels(chunks);
    let mesh = mesh_chunk(
        ChunkPos::new(px, pz),
        &n,
        &table,
        &HarnessBiomes(biome_mode),
    );
    let floats = mesh.as_floats();
    if std::env::var_os("PRINT_GOLDEN").is_some() {
        eprintln!("GOLDEN {name} {} {:#x}", floats.len(), fnv(floats));
        return;
    }
    assert_eq!(floats.len(), len, "{name} : nombre de floats");
    assert_eq!(fnv(floats), hash, "{name} : empreinte");
}

#[test]
fn terrain_matches_java() {
    check("terrain", 0, 0, 0, -1, 0x0, 1, 129762, 0x4580bfa37a7f9685);
}

#[test]
fn terrain_negative_chunk_matches_java() {
    check(
        "terrain_neg",
        -3,
        5,
        0,
        5,
        0x0,
        2,
        140400,
        0x72da8ac4114f6a68,
    );
}

#[test]
fn terrain_missing_neighbours_matches_java() {
    check(
        "terrain_missing",
        7,
        -2,
        0,
        -1,
        0x12a,
        3,
        135000,
        0xafd35b0e30e3af85,
    );
}

#[test]
fn noise_matches_java() {
    check("noise", 1, 1, 1, -1, 0x0, 4, 3297564, 0xcfbc182f98d67f7b);
}

#[test]
fn noise_center_only_matches_java() {
    check(
        "noise_center_only",
        -1,
        -1,
        1,
        2,
        0x1ef,
        5,
        3292272,
        0x12391dc006f093be,
    );
}

#[test]
fn cave_matches_java() {
    check("cave", 2, -5, 2, 0, 0x0, 6, 335988, 0xbc5d74611d24f373);
}

#[test]
fn cave_checker_biomes_matches_java() {
    check("cave_b", 0, 3, 2, -1, 0x0, 7, 328428, 0xecee42c1e7acb326);
}
