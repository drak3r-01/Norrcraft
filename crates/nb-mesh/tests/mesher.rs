//! Tests de comportement du mesher : fusion greedy, AO, voisins de chunk, fluides, spéciaux,
//! lumière.

use nb_data::BlockRegistry;
use nb_mesh::{
    ao_scale, calc_ao, light_scale, mesh_chunk, BlockTable, ChunkPos, ChunkVertex, ChunkVoxels,
    FnBiomes, LayerCode, LightVolume, MeshOutput, Neighborhood, UniformBiome,
};

const STONE: u8 = 3;
const WATER: u8 = 4;
const LEAVES: u8 = 10;
const TALL_GRASS: u8 = 19;
const LILY_PAD: u8 = 26;
const LAVA: u8 = 33;

fn table() -> BlockTable {
    BlockTable::from_registry(&BlockRegistry::builtin(), |_| 7)
}

fn floor(id: u8, y: usize) -> ChunkVoxels {
    let mut v = ChunkVoxels::filled(0);
    for x in 0..16 {
        for z in 0..16 {
            v.set(x, y, z, id);
        }
    }
    v
}

/// Maille `center` entouré de 8 copies de `around` (ou d'aucun voisin).
fn mesh(center: &ChunkVoxels, around: Option<&ChunkVoxels>) -> MeshOutput {
    let mut chunks: [Option<&ChunkVoxels>; 9] = [around; 9];
    chunks[4] = Some(center);
    mesh_chunk(
        ChunkPos::new(0, 0),
        &Neighborhood::from_voxels(chunks),
        &table(),
        &UniformBiome(0),
    )
}

fn quads(m: &MeshOutput) -> Vec<&[ChunkVertex]> {
    m.vertices.chunks(6).collect()
}

/// Étendue (max − min) d'un quad sur chaque axe.
fn extent(q: &[ChunkVertex]) -> [f32; 3] {
    std::array::from_fn(|a| {
        let lo = q.iter().map(|v| v.pos[a]).fold(f32::MAX, f32::min);
        let hi = q.iter().map(|v| v.pos[a]).fold(f32::MIN, f32::max);
        hi - lo
    })
}

#[test]
fn missing_center_gives_empty_mesh() {
    let n = Neighborhood::from_voxels([None; 9]);
    assert!(mesh_chunk(ChunkPos::new(0, 0), &n, &table(), &UniformBiome(0)).is_empty());
}

#[test]
fn single_block_emits_six_faces_in_java_order() {
    let mut c = ChunkVoxels::filled(0);
    c.set(5, 10, 5, STONE);
    let m = mesh(&c, None);
    assert_eq!(m.vertex_count(), 36);
    let q = quads(&m);
    // Ordre : dessus, dessous (tranche y), puis +Z, −Z, −X, +X.
    assert!(q[0].iter().all(|v| v.pos[1] == 11.0));
    assert!(q[1].iter().all(|v| v.pos[1] == 10.0));
    assert!(q[2].iter().all(|v| v.pos[2] == 6.0));
    assert!(q[3].iter().all(|v| v.pos[2] == 5.0));
    assert!(q[4].iter().all(|v| v.pos[0] == 5.0));
    assert!(q[5].iter().all(|v| v.pos[0] == 6.0));
    // Dessus en plein ciel sans voisin : AO 3, lumière 15, dirLight 1 → blanc.
    assert!(q[0].iter().all(|v| v.color == [1.0; 3]));
    // Triangulation 0-1-2 / 2-3-0.
    assert_eq!(q[0][2].pos, q[0][3].pos);
    assert_eq!(q[0][0].pos, q[0][5].pos);
    assert!(m.vertices.iter().all(|v| v.layer == 7.0));
}

#[test]
fn flat_floor_merges_into_single_quads() {
    // Plancher de pierre continu sur les 9 chunks : un seul quad 16×16 dessus, un dessous
    // (y = −1 vaut AIR), aucune face latérale (voisins pleins).
    let c = floor(STONE, 0);
    let m = mesh(&c, Some(&c));
    let q = quads(&m);
    assert_eq!(q.len(), 2);
    assert_eq!(extent(q[0]), [16.0, 0.0, 16.0]);
    assert_eq!(extent(q[1]), [16.0, 0.0, 16.0]);
    // UV répétées en unités bloc.
    let umax = q[0].iter().map(|v| v.uv[0]).fold(0.0, f32::max);
    let vmax = q[0].iter().map(|v| v.uv[1]).fold(0.0, f32::max);
    assert_eq!((umax, vmax), (16.0, 16.0));
}

#[test]
fn absent_neighbours_expose_border_faces() {
    // Mêmes blocs sans voisins : 4 faces latérales de bordure, chacune fusionnée en 16×1.
    let c = floor(STONE, 0);
    let m = mesh(&c, None);
    let q = quads(&m);
    assert_eq!(q.len(), 6);
    for side in &q[2..] {
        let e = extent(side);
        assert_eq!(e[1], 1.0);
        assert_eq!(e[0] + e[2], 16.0);
    }
}

#[test]
fn neighbour_chunk_block_hides_shared_face() {
    let mut c = ChunkVoxels::filled(0);
    c.set(15, 5, 5, STONE);
    let alone = mesh(&c, None);
    assert_eq!(quads(&alone).len(), 6);
    // Bloc collé dans le chunk est (cx = 2, cz = 1).
    let mut east = ChunkVoxels::filled(0);
    east.set(0, 5, 5, STONE);
    let mut chunks: [Option<&ChunkVoxels>; 9] = [None; 9];
    chunks[4] = Some(&c);
    chunks[5] = Some(&east);
    let m = mesh_chunk(
        ChunkPos::new(0, 0),
        &Neighborhood::from_voxels(chunks),
        &table(),
        &UniformBiome(0),
    );
    let q = quads(&m);
    assert_eq!(q.len(), 5);
    assert!(
        !q.iter().any(|q| q.iter().all(|v| v.pos[0] == 16.0)),
        "face +X masquée"
    );
}

#[test]
fn biome_border_splits_merging() {
    let c = floor(STONE, 0);
    let mut chunks: [Option<&ChunkVoxels>; 9] = [Some(&c); 9];
    chunks[4] = Some(&c);
    let biomes = FnBiomes(|wx: i32, _wz: i32| u8::from(wx >= 8));
    let m = mesh_chunk(
        ChunkPos::new(0, 0),
        &Neighborhood::from_voxels(chunks),
        &table(),
        &biomes,
    );
    // Dessus et dessous coupés en deux (8×16) par la frontière de biome.
    let q = quads(&m);
    assert_eq!(q.len(), 4);
    for quad in q {
        assert_eq!(extent(quad), [8.0, 0.0, 16.0]);
    }
}

#[test]
fn ambient_occlusion_darkens_corner() {
    assert_eq!(calc_ao(true, true, false), 0);
    assert_eq!(calc_ao(true, false, false), 2);
    assert_eq!(calc_ao(false, false, true), 2);
    assert_eq!(calc_ao(true, false, true), 1);
    assert_eq!(calc_ao(false, false, false), 3);
    assert_eq!(
        [ao_scale(3), ao_scale(2), ao_scale(1), ao_scale(0)],
        [1.0, 0.75, 0.5, 0.3]
    );

    // Plancher + un bloc posé en (5,1,5) : le dessus de (6,0,5) a ses coins x=6 occultés.
    let mut c = floor(STONE, 0);
    c.set(5, 1, 5, STONE);
    let m = mesh(&c, Some(&floor(STONE, 0)));
    let top = quads(&m)
        .into_iter()
        .find(|q| {
            q.iter().all(|v| v.pos[1] == 1.0)
                && extent(q) == [1.0, 0.0, 1.0]
                && q.iter().any(|v| v.pos == [7.0, 1.0, 6.0])
                && q.iter().any(|v| v.pos == [6.0, 1.0, 5.0])
        })
        .expect("quad 1×1 de la cellule (6,0,5)");
    let color = |p: [f32; 3]| top.iter().find(|v| v.pos == p).unwrap().color[0];
    // Coin 0 (6,1,6) : côté −x solide → AO 2 ; lumière (15 + 0 + 15 + 15) / 4 = 11.
    assert_eq!(
        color([6.0, 1.0, 6.0]),
        1.0f32 * 1.0 * 0.75 * (11.0f32 / 15.0)
    );
    // Coin 1 (7,1,6) : dégagé.
    assert_eq!(color([7.0, 1.0, 6.0]), 1.0);
}

#[test]
fn leaves_keep_internal_faces() {
    let mut c = ChunkVoxels::filled(0);
    c.set(4, 20, 4, LEAVES);
    c.set(5, 20, 4, LEAVES);
    let m = mesh(&c, None);
    // Dessus, dessous, +Z, −Z fusionnés en 2×1 ; −X, +X extérieurs ; et les deux faces
    // internes (x = 5) conservées (bizarrerie Java).
    let q = quads(&m);
    assert_eq!(q.len(), 8);
    let internal = q
        .iter()
        .filter(|q| q.iter().all(|v| v.pos[0] == 5.0))
        .count();
    assert_eq!(internal, 2);
    assert!(m
        .vertices
        .iter()
        .all(|v| LayerCode::decode(v.layer) == LayerCode::Leaves(7)));
    // Teinte feuilles PLAINS × 2.2 (double puis f32), dessus en plein ciel.
    assert_eq!(m.vertices[0].color[0], (0.36f64 * 2.2) as f32);
}

#[test]
fn fluid_tops_are_never_merged_and_lowered() {
    let mut c = floor(STONE, 4);
    for x in 0..16 {
        for z in 0..16 {
            c.set(x, 5, z, WATER);
        }
    }
    let m = mesh(&c, Some(&c));
    let q = quads(&m);
    // Dessus d'eau : 256 quads 1×1 abaissés de 0.15 ; dessous de la pierre (y=4 → eau ?
    // non : sous la pierre il y a de l'air) fusionné.
    let water_tops: Vec<_> = q
        .iter()
        .filter(|q| q.iter().all(|v| v.layer < 0.0))
        .collect();
    assert_eq!(water_tops.len(), 256);
    for wq in &water_tops {
        assert!(wq.iter().all(|v| v.pos[1] == 6.0f32 + -0.15f32));
        assert_eq!(LayerCode::decode(wq[0].layer), LayerCode::Fluid(7));
        assert_eq!(wq[0].color[2], 0.8f32); // waterColor PLAINS, sans × 2.2
    }
    let (solid, fluid) = m.split_fluid();
    assert_eq!(fluid.len(), 256 * 6);
    assert_eq!(solid.len() + fluid.len(), m.vertex_count());
}

#[test]
fn fluid_side_surface_row_has_lowered_top_corners() {
    let mut c = ChunkVoxels::filled(0);
    c.set(3, 10, 3, WATER);
    c.set(3, 11, 3, WATER);
    let m = mesh(&c, None);
    let q = quads(&m);
    // Face +Z : rangée y=10 (non surface, coins à 10 et 11) puis y=11 surface (coins hauts
    // à 12 − 0.15).
    let pz: Vec<_> = q
        .iter()
        .filter(|q| q.iter().all(|v| v.pos[2] == 4.0))
        .collect();
    assert_eq!(pz.len(), 2);
    let max_y = |q: &&&[ChunkVertex]| q.iter().map(|v| v.pos[1]).fold(f32::MIN, f32::max);
    assert_eq!(max_y(&pz[0]), 11.0);
    assert_eq!(max_y(&pz[1]), 12.0f32 + -0.15f32);
}

#[test]
fn specials_come_last_with_their_encodings() {
    let mut c = floor(STONE, 0);
    c.set(3, 1, 3, TALL_GRASS);
    let mut w = floor(STONE, 0);
    for x in 0..16 {
        for z in 0..16 {
            w.set(x, 1, z, WATER);
        }
    }
    w.set(8, 2, 8, LILY_PAD);
    let m = mesh(&c, None);
    let tail = &m.vertices[m.vertex_count() - 24..];
    assert!(tail
        .iter()
        .all(|v| LayerCode::decode(v.layer) == LayerCode::Cross(7)));
    // Croix teintée herbe × 2.2, lumière 15, facteur 0.85.
    assert_eq!(
        tail[0].color[1],
        (0.74f64 * 2.2) as f32 * light_scale(15) * 0.85
    );

    let m = mesh(&w, None);
    let tail = &m.vertices[m.vertex_count() - 12..];
    assert!(tail
        .iter()
        .all(|v| v.layer == 7.0 && v.pos[1] == 2.0f32 + 0.015f32));
}

#[test]
fn light_volume_lava_and_darkness() {
    let t = table();
    // Toit de pierre sur tout le voisinage à y = 10 : dessous dans le noir.
    let roof = floor(STONE, 10);
    let n = Neighborhood::from_voxels([Some(&roof); 9]);
    let l = LightVolume::compute(&n, &t);
    assert_eq!(l.at(5, 5, 5), 0);
    assert_eq!(l.at(5, 11, 5), 15);
    assert_eq!(l.get_clamped(5, 200, 5), 15);
    // Lave sous le toit : 15 sur sa colonne vers le bas (règle « ciel »), 14 à côté.
    let mut lit = roof.clone();
    lit.set(5, 5, 5, LAVA);
    let mut chunks = [Some(&roof); 9];
    chunks[4] = Some(&lit);
    let l = LightVolume::compute(&Neighborhood::from_voxels(chunks), &t);
    assert_eq!(l.at(5, 5, 5), 15);
    assert_eq!(l.at(5, 4, 5), 15);
    assert_eq!(l.at(5, 0, 5), 15);
    assert_eq!(l.at(6, 5, 5), 14);
    assert_eq!(light_scale(0), 0.08);
    assert_eq!(light_scale(15), 1.0);
}

/// Normale (non normalisée) du triangle `a, b, c`.
fn tri_normal(a: &ChunkVertex, b: &ChunkVertex, c: &ChunkVertex) -> [f32; 3] {
    let u: [f32; 3] = std::array::from_fn(|i| b.pos[i] - a.pos[i]);
    let v: [f32; 3] = std::array::from_fn(|i| c.pos[i] - a.pos[i]);
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

#[test]
fn quad_diagonal_flip_keeps_winding() {
    // Terrain accidenté : piliers isolés sur un sol → AO asymétrique, diagonales retournées.
    let mut c = floor(STONE, 10);
    for (x, z) in [(3, 3), (4, 4), (8, 8), (9, 7), (12, 2)] {
        c.set(x, 11, z, STONE);
    }
    let m = mesh(&c, None);
    let mut asymmetric = 0;
    for q in quads(&m) {
        let n1 = tri_normal(&q[0], &q[1], &q[2]);
        let n2 = tri_normal(&q[3], &q[4], &q[5]);
        let dot: f32 = (0..3).map(|i| n1[i] * n2[i]).sum();
        assert!(
            dot > 0.0,
            "les deux triangles d'un quad doivent être coplanaires et de même sens"
        );
        // Diagonale commune = q[0]/q[2] ; coins hors diagonale = q[1]/q[4].
        let shared = q[0].color[0] + q[2].color[0];
        let other = q[1].color[0] + q[4].color[0];
        assert!(
            shared >= other - 1e-6,
            "la diagonale doit joindre les coins les plus clairs"
        );
        if (shared - other).abs() > 1e-6 {
            asymmetric += 1;
        }
    }
    assert!(
        asymmetric > 0,
        "scène sans AO asymétrique : test sans objet"
    );
}

/// Source de biomes de test : biome 1 à partir de x = 8, teintes 0 et 1, moyenne sur 5 × 5.
struct BlendedHalves(i32);

impl nb_mesh::BiomeSource for BlendedHalves {
    fn biome_at(&self, wx: i32, _wz: i32) -> nb_mesh::BiomeId {
        u8::from(wx >= 8)
    }

    fn tints(&self, biome: nb_mesh::BiomeId) -> nb_mesh::BiomeTints {
        let v = f64::from(biome) * 0.4;
        nb_mesh::BiomeTints {
            grass: [v; 3],
            leaves: [v; 3],
            water: [v; 3],
        }
    }

    fn tint_blend_radius(&self) -> i32 {
        self.0
    }
}

#[test]
fn tint_blend_averages_five_by_five_columns() {
    const GRASS: u8 = 2;
    let c = floor(GRASS, 0);
    let chunks: [Option<&ChunkVoxels>; 9] = [Some(&c); 9];
    let top_shades = |radius: i32| {
        let m = mesh_chunk(
            ChunkPos::new(0, 0),
            &Neighborhood::from_voxels(chunks),
            &table(),
            &BlendedHalves(radius),
        );
        let mut v: Vec<u32> = m
            .vertices
            .iter()
            .filter(|v| v.pos[1] == 1.0)
            .map(|v| v.color[0].to_bits())
            .collect();
        v.sort_unstable();
        v.dedup();
        v.len()
    };
    // Sans moyenne : deux teintes (comportement vanilla) ; rayon 2 : la frontière passe par
    // 0, 1/5, 2/5, 3/5, 4/5 et 1 sur cinq colonnes.
    assert_eq!(top_shades(0), 2);
    assert_eq!(top_shades(2), 6);
}
