//! Culling des chunks (spec §15) : test frustum AABB (réplique de `Matrix4f.testAab` de JOML
//! 1.10.5) et `ChunkQuadtree`.

use nb_core::math::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};
use nb_core::ChunkPos;

/// Les 6 plans non normalisés `(a, b, c, d)` dans l'ordre `nx, px, ny, py, nz, pz`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    /// Depuis une matrice `[f32; 16]` column-major en convention GL (z_clip ∈ [-w, w]).
    pub fn from_gl_matrix(m: &[f32; 16]) -> Self {
        // m[c*4 + r] = JOML mCR.
        let e = |c: usize, r: usize| m[c * 4 + r];
        let plane = |row: usize, sign: f32| -> [f32; 4] {
            let mut p = [0.0f32; 4];
            for (c, slot) in p.iter_mut().enumerate() {
                *slot = if sign > 0.0 {
                    e(c, 3) + e(c, row)
                } else {
                    e(c, 3) - e(c, row)
                };
            }
            p
        };
        Self {
            planes: [
                plane(0, 1.0),
                plane(0, -1.0),
                plane(1, 1.0),
                plane(1, -1.0),
                plane(2, 1.0),
                plane(2, -1.0),
            ],
        }
    }

    /// Plans (pour diagnostic).
    pub fn planes(&self) -> &[[f32; 4]; 6] {
        &self.planes
    }

    /// `testAab` : vrai si la boîte intersecte ou est contenue (test « p-vertex »).
    pub fn test_aab(&self, min: [f32; 3], max: [f32; 3]) -> bool {
        self.planes.iter().all(|p| {
            let x = if p[0] < 0.0 { min[0] } else { max[0] };
            let y = if p[1] < 0.0 { min[1] } else { max[1] };
            let z = if p[2] < 0.0 { min[2] } else { max[2] };
            p[0] * x + p[1] * y + p[2] * z >= -p[3]
        })
    }
}

/// Capacité d'une feuille avant division.
pub const QUADTREE_LEAF_CAPACITY: usize = 16;

#[derive(Clone, Debug)]
struct Node {
    min_x: f32,
    min_z: f32,
    max_x: f32,
    max_z: f32,
    elements: Vec<ChunkPos>,
    children: Option<[usize; 4]>,
}

/// Quadtree XZ des chunks maillés, reconstruit à chaque frame.
///
/// Écart voulu (décision de Gabriel) : [`ChunkQuadtree::build`] insère les chunks triés par
/// `(x, z)` au lieu de l'ordre d'itération d'un `HashMap` Java, pour un ordre de dessin
/// déterministe.
#[derive(Clone, Debug, Default)]
pub struct ChunkQuadtree {
    pool: Vec<Node>,
    used: usize,
    root: Option<usize>,
    scratch: Vec<ChunkPos>,
}

#[inline]
fn chunk_min(p: ChunkPos) -> (f32, f32) {
    (
        (p.x * CHUNK_SIZE_X as i32) as f32,
        (p.z * CHUNK_SIZE_Z as i32) as f32,
    )
}

#[inline]
fn chunk_center(p: ChunkPos) -> (f32, f32) {
    (
        (p.x * CHUNK_SIZE_X as i32) as f32 + CHUNK_SIZE_X as f32 / 2.0,
        (p.z * CHUNK_SIZE_Z as i32) as f32 + CHUNK_SIZE_Z as f32 / 2.0,
    )
}

impl ChunkQuadtree {
    pub fn new() -> Self {
        Self::default()
    }

    fn allocate(&mut self, min_x: f32, min_z: f32, max_x: f32, max_z: f32) -> usize {
        let node = Node {
            min_x,
            min_z,
            max_x,
            max_z,
            elements: Vec::new(),
            children: None,
        };
        if self.used < self.pool.len() {
            let slot = &mut self.pool[self.used];
            let mut elements = std::mem::take(&mut slot.elements);
            elements.clear();
            *slot = Node { elements, ..node };
        } else {
            self.pool.push(node);
        }
        self.used += 1;
        self.used - 1
    }

    /// Reconstruit l'arbre. Les positions sont triées par `(x, z)` (doublons ignorés) avant
    /// insertion.
    pub fn build(&mut self, chunks: impl IntoIterator<Item = ChunkPos>) {
        let mut list = std::mem::take(&mut self.scratch);
        list.clear();
        list.extend(chunks);
        list.sort_unstable();
        list.dedup();
        self.build_ordered(&list);
        self.scratch = list;
    }

    /// Reconstruit l'arbre en insérant dans l'ordre donné (ordre d'itération Java).
    pub fn build_ordered(&mut self, chunks: &[ChunkPos]) {
        self.used = 0;
        if chunks.is_empty() {
            self.root = None;
            return;
        }
        let (mut min_x, mut min_z) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_z) = (-f32::MAX, -f32::MAX);
        for &p in chunks {
            let (cx, cz) = chunk_min(p);
            if cx < min_x {
                min_x = cx;
            }
            if cz < min_z {
                min_z = cz;
            }
            if cx + CHUNK_SIZE_X as f32 > max_x {
                max_x = cx + CHUNK_SIZE_X as f32;
            }
            if cz + CHUNK_SIZE_Z as f32 > max_z {
                max_z = cz + CHUNK_SIZE_Z as f32;
            }
        }
        let root = self.allocate(min_x, min_z, max_x, max_z);
        self.root = Some(root);
        for &p in chunks {
            self.insert(root, p);
        }
    }

    fn quadrant(&self, node: usize, x: f32, z: f32) -> usize {
        let n = &self.pool[node];
        let mid_x = (n.min_x + n.max_x) / 2.0;
        let mid_z = (n.min_z + n.max_z) / 2.0;
        match (x < mid_x, z < mid_z) {
            (true, true) => 0,
            (true, false) => 2,
            (false, true) => 1,
            (false, false) => 3,
        }
    }

    fn insert(&mut self, mut node: usize, p: ChunkPos) {
        loop {
            let n = &self.pool[node];
            if n.children.is_none() && n.elements.len() < QUADTREE_LEAF_CAPACITY {
                self.pool[node].elements.push(p);
                return;
            }
            if n.children.is_none() {
                self.split(node);
            }
            let (cx, cz) = chunk_center(p);
            let q = self.quadrant(node, cx, cz);
            node = self.pool[node].children.expect("enfants")[q];
        }
    }

    fn split(&mut self, node: usize) {
        let n = &self.pool[node];
        let (min_x, min_z, max_x, max_z) = (n.min_x, n.min_z, n.max_x, n.max_z);
        let mid_x = (min_x + max_x) / 2.0;
        let mid_z = (min_z + max_z) / 2.0;
        let children = [
            self.allocate(min_x, min_z, mid_x, mid_z),
            self.allocate(mid_x, min_z, max_x, mid_z),
            self.allocate(min_x, mid_z, mid_x, max_z),
            self.allocate(mid_x, mid_z, max_x, max_z),
        ];
        self.pool[node].children = Some(children);
        let elements = std::mem::take(&mut self.pool[node].elements);
        for &e in &elements {
            let (cx, cz) = chunk_center(e);
            let q = self.quadrant(node, cx, cz);
            self.pool[children[q]].elements.push(e);
        }
        let mut elements = elements;
        elements.clear();
        self.pool[node].elements = elements;
    }

    /// Chunks visibles, dans l'ordre de parcours en profondeur (0, 1, 2, 3) — ordre de dessin.
    /// Les résultats sont ajoutés à `out` (non vidé).
    pub fn query(&self, f: &Frustum, out: &mut Vec<ChunkPos>) {
        if let Some(root) = self.root {
            self.query_node(root, f, out);
        }
    }

    fn query_node(&self, node: usize, f: &Frustum, out: &mut Vec<ChunkPos>) {
        let n = &self.pool[node];
        let h = CHUNK_SIZE_Y as f32;
        if !f.test_aab([n.min_x, 0.0, n.min_z], [n.max_x, h, n.max_z]) {
            return;
        }
        if let Some(children) = n.children {
            for c in children {
                self.query_node(c, f, out);
            }
        } else {
            for &e in &n.elements {
                let (cx, cz) = chunk_min(e);
                if f.test_aab(
                    [cx, 0.0, cz],
                    [cx + CHUNK_SIZE_X as f32, h, cz + CHUNK_SIZE_Z as f32],
                ) {
                    out.push(e);
                }
            }
        }
    }
}
