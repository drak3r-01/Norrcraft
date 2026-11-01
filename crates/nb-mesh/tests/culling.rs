//! Culling : matrices caméra, `testAab` et quadtree comparés à JOML 1.10.5 / `ChunkQuadtree`
//! (Java), plus des tests de comportement.

use nb_mesh::camera::{
    camera_frustum_matrix, default_fov_y, perspective_gl, view_matrix, DEFAULT_FAR, DEFAULT_NEAR,
};
use nb_mesh::{clamp_pitch, ChunkPos, ChunkQuadtree, Frustum};

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % u64::from(n)) as u32
    }
    fn nf(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next(1 << 20) as f32 / (1 << 20) as f32)
    }
}

fn mix(mut h: u64, v: i32) -> u64 {
    for k in 0..4 {
        h ^= ((v as u32) >> (8 * k)) as u64 & 0xFF;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn mix_mat(mut h: u64, m: &[f32; 16]) -> u64 {
    for f in m {
        h = mix(h, f.to_bits() as i32);
    }
    h
}

/// Transcription du harnais Java (`Golden2`) : 40 caméras aléatoires ; pour chacune les
/// matrices view/proj/proj·view, 200 `testAab` et une requête de quadtree.
#[test]
fn camera_frustum_and_quadtree_match_java() {
    let mut r = Lcg(99);
    let mut h: u64 = 0xcbf29ce484222325;
    let mut hb: u64 = 0xcbf29ce484222325;
    let mut qt = ChunkQuadtree::new();
    let mut first = 0;
    for cam in 0..40 {
        let px = r.nf(-200.0, 200.0);
        let py = r.nf(0.0, 128.0);
        let pz = r.nf(-200.0, 200.0);
        let pitch = clamp_pitch(r.nf(-120.0, 120.0));
        let yaw = r.nf(-720.0, 720.0);
        let aspect = r.nf(0.5, 2.5);
        let view = view_matrix([px, py, pz], pitch, yaw);
        let proj = perspective_gl(default_fov_y(), aspect, DEFAULT_NEAR, DEFAULT_FAR);
        let m = camera_frustum_matrix(
            [px, py, pz],
            pitch,
            yaw,
            default_fov_y(),
            aspect,
            DEFAULT_NEAR,
            DEFAULT_FAR,
        );
        h = mix_mat(h, &view);
        h = mix_mat(h, &proj);
        h = mix_mat(h, &m);
        let f = Frustum::from_gl_matrix(&m);
        for _ in 0..200 {
            let x0 = r.nf(-400.0, 400.0);
            let y0 = r.nf(-50.0, 150.0);
            let z0 = r.nf(-400.0, 400.0);
            let x1 = x0 + r.nf(0.0, 64.0);
            let y1 = y0 + r.nf(0.0, 64.0);
            let z1 = z0 + r.nf(0.0, 64.0);
            hb = mix(hb, i32::from(f.test_aab([x0, y0, z0], [x1, y1, z1])));
        }
        // LinkedHashMap : un doublon garde sa première position.
        let count = 1 + r.next(300);
        let mut chunks: Vec<ChunkPos> = Vec::new();
        for _ in 0..count {
            let x = r.next(41) as i32 - 20 + (px / 16.0) as i32;
            let z = r.next(41) as i32 - 20 + (pz / 16.0) as i32;
            let p = ChunkPos::new(x, z);
            if !chunks.contains(&p) {
                chunks.push(p);
            }
        }
        qt.build_ordered(&chunks);
        let mut res = Vec::new();
        qt.query(&f, &mut res);
        for p in &res {
            hb = mix(hb, p.x);
            hb = mix(hb, p.z);
        }
        hb = mix(hb, res.len() as i32);
        if cam == 0 {
            first = res.len();
        }
    }
    assert_eq!(first, 46);
    assert_eq!(h, 0x81d0c5b27f032d55, "matrices");
    assert_eq!(hb, 0x2225414ecc6e9797, "testAab + quadtree");
}

/// Matrice « boîte » : envoie `[lo, hi]` sur `[-1, 1]` sur chaque axe (convention GL).
fn box_matrix(lo: [f32; 3], hi: [f32; 3]) -> [f32; 16] {
    let mut m = [0.0f32; 16];
    for a in 0..3 {
        m[a * 4 + a] = 2.0 / (hi[a] - lo[a]);
        m[12 + a] = -(hi[a] + lo[a]) / (hi[a] - lo[a]);
    }
    m[15] = 1.0;
    m
}

#[test]
fn test_aab_inside_outside_and_touching() {
    let f = Frustum::from_gl_matrix(&box_matrix([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]));
    assert!(f.test_aab([1.0, 1.0, 1.0], [2.0, 2.0, 2.0]), "contenue");
    assert!(
        f.test_aab([-5.0, -5.0, -5.0], [20.0, 20.0, 20.0]),
        "englobante"
    );
    assert!(
        f.test_aab([9.0, 9.0, 9.0], [12.0, 12.0, 12.0]),
        "chevauchante"
    );
    assert!(
        f.test_aab([10.0, 0.0, 0.0], [11.0, 1.0, 1.0]),
        "contact (>=)"
    );
    assert!(!f.test_aab([10.5, 0.0, 0.0], [11.0, 1.0, 1.0]), "à droite");
    assert!(!f.test_aab([0.0, -3.0, 0.0], [1.0, -1.0, 1.0]), "dessous");
    assert!(!f.test_aab([0.0, 0.0, 11.0], [1.0, 1.0, 12.0]), "au-delà");
}

fn brute_force(chunks: &[ChunkPos], f: &Frustum) -> Vec<ChunkPos> {
    let mut v: Vec<ChunkPos> = chunks
        .iter()
        .copied()
        .filter(|p| {
            let (x, z) = ((p.x * 16) as f32, (p.z * 16) as f32);
            f.test_aab([x, 0.0, z], [x + 16.0, 128.0, z + 16.0])
        })
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn quadtree_query_equals_brute_force() {
    let mut chunks = Vec::new();
    for x in -12..12 {
        for z in -9..15 {
            chunks.push(ChunkPos::new(x, z));
        }
    }
    let mut qt = ChunkQuadtree::new();
    qt.build(chunks.iter().copied());
    for (lo, hi) in [
        ([-50.0, 0.0, -50.0], [50.0, 128.0, 50.0]),
        ([-1000.0, -10.0, -1000.0], [1000.0, 200.0, 1000.0]),
        ([33.0, 10.0, -140.0], [34.0, 20.0, -20.0]),
        ([500.0, 0.0, 500.0], [600.0, 128.0, 600.0]),
    ] {
        let f = Frustum::from_gl_matrix(&box_matrix(lo, hi));
        let mut out = Vec::new();
        qt.query(&f, &mut out);
        let mut sorted = out.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), out.len(), "pas de doublon");
        assert_eq!(sorted, brute_force(&chunks, &f));
    }
}

#[test]
fn quadtree_build_is_order_independent() {
    // Correction appliquée : `build` trie par (x, z) → même ordre de dessin quel que soit
    // l'ordre d'itération de la map d'origine.
    let mut chunks = Vec::new();
    for x in -6..7 {
        for z in -6..7 {
            chunks.push(ChunkPos::new(x, z));
        }
    }
    let mut shuffled = chunks.clone();
    let mut r = Lcg(5);
    for i in (1..shuffled.len()).rev() {
        let j = r.next(i as u32 + 1) as usize;
        shuffled.swap(i, j);
    }
    shuffled.extend_from_slice(&chunks[..10]); // doublons ignorés
    let f = Frustum::from_gl_matrix(&box_matrix([-40.0, 0.0, -70.0], [60.0, 128.0, 30.0]));
    let (mut a, mut b) = (ChunkQuadtree::new(), ChunkQuadtree::new());
    a.build(chunks.iter().copied());
    b.build(shuffled);
    let (mut oa, mut ob) = (Vec::new(), Vec::new());
    a.query(&f, &mut oa);
    b.query(&f, &mut ob);
    assert!(!oa.is_empty());
    assert_eq!(oa, ob);
}

#[test]
fn quadtree_empty_and_rebuild() {
    let mut qt = ChunkQuadtree::new();
    let f = Frustum::from_gl_matrix(&box_matrix([-1e4; 3], [1e4; 3]));
    let mut out = Vec::new();
    qt.query(&f, &mut out);
    assert!(out.is_empty());
    qt.build((0..100).map(|i| ChunkPos::new(i % 10, i / 10)));
    qt.query(&f, &mut out);
    assert_eq!(out.len(), 100);
    qt.build(std::iter::empty());
    out.clear();
    qt.query(&f, &mut out);
    assert!(out.is_empty());
    qt.build([ChunkPos::new(3, 4)]);
    qt.query(&f, &mut out);
    assert_eq!(out, vec![ChunkPos::new(3, 4)]);
}

#[test]
fn camera_sees_ahead_not_behind() {
    // Pitch 0, yaw 0 : la caméra regarde vers −Z.
    let m = camera_frustum_matrix(
        [8.0, 64.0, 8.0],
        0.0,
        0.0,
        default_fov_y(),
        16.0 / 9.0,
        DEFAULT_NEAR,
        DEFAULT_FAR,
    );
    let f = Frustum::from_gl_matrix(&m);
    let chunk = |x: i32, z: i32| {
        let (cx, cz) = ((x * 16) as f32, (z * 16) as f32);
        f.test_aab([cx, 0.0, cz], [cx + 16.0, 128.0, cz + 16.0])
    };
    assert!(chunk(0, -3), "devant");
    assert!(chunk(0, 0), "contient la caméra");
    assert!(!chunk(0, 3), "derrière");
    assert!(!chunk(0, -80), "au-delà du plan lointain");
}
