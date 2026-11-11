//! Couches de neige (blocs partiels) et teintes de saison du mailleur (session 3.2).

use nb_data::{BlockRegistry, WorldKind};
use nb_mesh::{
    mesh_chunk, BiomeSource, BiomeTints, BlockRole, BlockTable, ChunkPos, ChunkVertex, ChunkVoxels,
    MeshOutput, Neighborhood, SeasonTint,
};

fn registry() -> BlockRegistry {
    BlockRegistry::for_kind(WorldKind::Norrcraft)
}

fn table(reg: &BlockRegistry) -> BlockTable {
    BlockTable::from_registry(reg, |_| 7)
}

/// Biome unique avec teintes fixes et saison réglable.
struct Season {
    leaf: Option<SeasonTint>,
    grass: Option<SeasonTint>,
}

impl BiomeSource for Season {
    fn biome_at(&self, _: i32, _: i32) -> u8 {
        0
    }
    fn tints(&self, _: u8) -> BiomeTints {
        BiomeTints {
            grass: [0.4, 0.6, 0.2],
            leaves: [0.3, 0.6, 0.2],
            water: [0.2, 0.4, 0.8],
        }
    }
    fn leaf_season(&self) -> Option<SeasonTint> {
        self.leaf
    }
    fn grass_season(&self) -> Option<SeasonTint> {
        self.grass
    }
}

fn mesh(v: &ChunkVoxels, reg: &BlockRegistry, biomes: &dyn BiomeSource) -> MeshOutput {
    let mut chunks: [Option<&ChunkVoxels>; 9] = [None; 9];
    chunks[4] = Some(v);
    mesh_chunk(
        ChunkPos::new(0, 0),
        &Neighborhood::from_voxels(chunks),
        &table(reg),
        biomes,
    )
}

fn plain() -> Season {
    Season {
        leaf: None,
        grass: None,
    }
}

fn quads(m: &MeshOutput) -> Vec<&[ChunkVertex]> {
    m.vertices.chunks(6).collect()
}

fn max_y(q: &[ChunkVertex]) -> f32 {
    q.iter().map(|v| v.pos[1]).fold(f32::MIN, f32::max)
}

#[test]
fn layer_blocks_are_partial_and_transparent() {
    let reg = registry();
    let t = table(&reg);
    for i in 1..=7u8 {
        let id = reg.get(&format!("snotacke_{i}")).id;
        let info = t.get(id);
        assert_eq!(info.role, BlockRole::Layer);
        assert_eq!(info.layer_eighths, i);
        assert!(!info.is_light_blocking() && !info.is_ao_solid());
    }
    assert_eq!(t.get(reg.get("sno").id).role, BlockRole::Other);
    assert!(t.get(reg.get("bjork_lov").id).seasonal);
    assert!(!t.get(reg.get("gran_barr").id).seasonal);
}

#[test]
fn a_layer_is_a_slab_of_its_height() {
    let reg = registry();
    let (stone, snow3) = (reg.get("granit").id, reg.get("snotacke_3").id);
    let mut v = ChunkVoxels::filled(0);
    v.set(5, 10, 5, stone);
    v.set(5, 11, 5, snow3);
    let m = mesh(&v, &reg, &plain());
    // Bloc de pierre : 5 faces visibles (le dessus est contre la dalle, transparente : visible).
    // Dalle : dessus + 4 côtés.
    let slab: Vec<&[ChunkVertex]> = quads(&m)
        .into_iter()
        .filter(|q| {
            q.iter().all(|p| p.pos[1] >= 11.0) && max_y(q) > 11.0 && max_y(q) <= 11.375 + 1e-4
        })
        .filter(|q| q.iter().all(|p| p.pos[0] >= 5.0 && p.pos[0] <= 6.0))
        .collect();
    assert_eq!(slab.len(), 5, "dessus + 4 côtés");
    let top: Vec<_> = slab
        .iter()
        .filter(|q| q.iter().all(|p| p.pos[1] == 11.375))
        .collect();
    assert_eq!(top.len(), 1);
    let sides = slab
        .iter()
        .filter(|q| q.iter().any(|p| p.pos[1] == 11.0) && max_y(q) == 11.375);
    assert_eq!(sides.count(), 4);
}

#[test]
fn a_side_against_a_taller_layer_is_hidden() {
    let reg = registry();
    let (stone, low, high) = (
        reg.get("granit").id,
        reg.get("snotacke_2").id,
        reg.get("snotacke_6").id,
    );
    let mut v = ChunkVoxels::filled(0);
    for x in 5..7 {
        v.set(x, 10, 5, stone);
    }
    v.set(5, 11, 5, low);
    v.set(6, 11, 5, high);
    let m = mesh(&v, &reg, &plain());
    // La face +X de la basse dalle (x = 6) touche la haute : cachée ; la haute garde sa face −X.
    let faces_x6: Vec<_> = quads(&m)
        .into_iter()
        .filter(|q| q.iter().all(|p| p.pos[0] == 6.0) && q.iter().any(|p| p.pos[1] >= 11.0))
        .filter(|q| max_y(q) <= 11.75)
        .collect();
    assert!(
        faces_x6.iter().all(|q| max_y(q) != 11.25),
        "côté de la basse dalle contre la haute non masqué"
    );
}

#[test]
fn seasonal_leaves_follow_the_season_tint_but_conifers_do_not() {
    let reg = registry();
    let (birch, spruce) = (reg.get("bjork_lov").id, reg.get("gran_barr").id);
    let autumn = SeasonTint {
        target: [0.8, 0.4, 0.1],
        mix: 1.0,
    };
    let color_of = |id: u8, biomes: &Season| {
        let mut v = ChunkVoxels::filled(0);
        v.set(5, 10, 5, id);
        let m = mesh(&v, &reg, biomes);
        let top = quads(&m)
            .into_iter()
            .find(|q| q.iter().all(|p| p.pos[1] == 11.0))
            .unwrap();
        top[0].color
    };
    let (b0, s0) = (color_of(birch, &plain()), color_of(spruce, &plain()));
    let shifted = Season {
        leaf: Some(autumn),
        grass: None,
    };
    let (b1, s1) = (color_of(birch, &shifted), color_of(spruce, &shifted));
    assert_ne!(b0, b1, "le bouleau doit changer de teinte");
    assert_eq!(s0, s1, "l'épicéa garde sa teinte");
    // La teinte cible est un rapport de 0,8 / 0,4 / 0,1 (× 2,2 × éclairage).
    assert!(b1[0] > b1[1] && b1[1] > b1[2]);
}

#[test]
fn grass_season_tints_the_grass_top() {
    let reg = registry();
    let grass = reg.get("gras").id;
    let mut v = ChunkVoxels::filled(0);
    v.set(5, 10, 5, grass);
    let top_color = |biomes: &Season| {
        let m = mesh(&v, &reg, biomes);
        quads(&m)
            .into_iter()
            .find(|q| q.iter().all(|p| p.pos[1] == 11.0))
            .unwrap()[0]
            .color
    };
    let dull = Season {
        leaf: None,
        grass: Some(SeasonTint {
            target: [0.5, 0.45, 0.3],
            mix: 1.0,
        }),
    };
    assert_ne!(top_color(&plain()), top_color(&dull));
}
