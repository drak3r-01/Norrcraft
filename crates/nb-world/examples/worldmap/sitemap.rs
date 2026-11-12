//! Carte des structures Norrcraft (session 4.1, mode `sites`) : biomes en fond, sites par classe
//! (majeure rouge, moyenne orange, mineure jaune), pistes en blanc (cairns bleus), et un tableau
//! des tirages contre les sites retenus par type (rendement des prédicats de site).

use std::collections::BTreeMap;

use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::structures::{Mode, RULES};
use nb_world::norrcraft::{Class, NorrBlocks, NorrConfig, NorrPreset, NorrcraftTerrain};

use crate::biomemap::biome_color;
use crate::image_out::write_image;

#[allow(clippy::too_many_arguments)]
pub fn run(seed: i64, preset: NorrPreset, cx: i32, cz: i32, size: i32, step: i32, out: &str) {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let terrain = NorrcraftTerrain::new(
        seed,
        NorrConfig::with_preset(preset),
        NorrBlocks::from_registry(&reg),
    );
    let x0 = cx - size * step / 2;
    let z0 = cz - size * step / 2;
    let (x1, z1) = (x0 + size * step - 1, z0 + size * step - 1);
    let mut px = vec![0u8; (size * size * 3) as usize];
    for j in 0..size {
        for i in 0..size {
            let s = terrain.column_biome(x0 + i * step, z0 + j * step);
            let mut c = biome_color(s.biome);
            if s.snow {
                c = [235, 235, 240];
            }
            // Fond assombri : les marques ressortent.
            let o = ((j * size + i) * 3) as usize;
            px[o..o + 3].copy_from_slice(&[c[0] / 2, c[1] / 2, c[2] / 2]);
        }
    }
    let mut put = |wx: i32, wz: i32, half: i32, c: [u8; 3]| {
        let (u, v) = ((wx - x0) / step, (wz - z0) / step);
        for dv in -half..=half {
            for du in -half..=half {
                let (a, b) = (u + du, v + dv);
                if a >= 0 && b >= 0 && a < size && b < size {
                    let o = ((b * size + a) * 3) as usize;
                    px[o..o + 3].copy_from_slice(&c);
                }
            }
        }
    };

    let (sites, routes, drawn) = terrain.with_probe(|p| {
        let st = terrain.structures();
        let sites = st.sites_in(p, x0, z0, x1, z1);
        let routes = st.routes_in(p, x0, z0, x1, z1);
        let mut drawn: BTreeMap<&'static str, usize> = BTreeMap::new();
        for class in Class::ALL {
            let pitch = class.pitch();
            for j in (z0 - pitch).div_euclid(pitch)..=z1.div_euclid(pitch) {
                for i in (x0 - pitch).div_euclid(pitch)..=x1.div_euclid(pitch) {
                    if let Some((kind, x, z)) = st.drawn_kind(class, i, j, p) {
                        if x >= x0 && x <= x1 && z >= z0 && z <= z1 {
                            *drawn.entry(kind.name()).or_default() += 1;
                        }
                    }
                }
            }
        }
        (sites, routes, drawn)
    });
    for r in &routes {
        for c in &r.cells {
            let col = match c.mode {
                Mode::Bridge | Mode::Ford => [90, 200, 255],
                _ => [255, 255, 255],
            };
            put(c.x, c.z, 0, col);
        }
        for &(x, z, _) in &r.cairns {
            put(x, z, 0, [0, 120, 255]);
        }
    }
    for s in &sites {
        let (half, col) = match s.class() {
            Class::Major => (2, [255, 40, 40]),
            Class::Medium => (1, [255, 160, 0]),
            Class::Minor => (0, [255, 255, 0]),
        };
        put(s.x, s.z, half, col);
    }

    println!(
        "-- structures Norrcraft « {} », graine {seed}, {size} px × {step} blocs ({} km), centre ({cx}, {cz})",
        preset.label(),
        f64::from(size * step) / 1000.0
    );
    let mut placed: BTreeMap<&'static str, usize> = BTreeMap::new();
    for s in &sites {
        *placed.entry(s.kind.name()).or_default() += 1;
    }
    println!(
        "{:>16} {:>7} {:>7} {:>7}",
        "type", "tirages", "retenus", "reste %"
    );
    for r in RULES.iter() {
        let (d, p) = (
            drawn.get(r.name).copied().unwrap_or(0),
            placed.get(r.name).copied().unwrap_or(0),
        );
        if d > 0 {
            println!(
                "{:>16} {d:7} {p:7} {:7.0}  {}",
                r.name,
                100.0 * p as f64 / d as f64,
                r.class.name()
            );
        }
    }
    let cells: usize = routes.iter().map(|r| r.cells.len()).sum();
    let wet: usize = routes.iter().map(|r| r.wet_cells()).sum();
    let cairns: usize = routes.iter().map(|r| r.cairns.len()).sum();
    println!(
        "pistes : {}  cellules : {cells}  dont eau : {wet}  cairns de piste : {cairns}",
        routes.len()
    );
    write_image(out, size as u32, size as u32, &px).expect("écriture de l'image");
    println!("écrit : {out}");
}
