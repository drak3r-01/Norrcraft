//! Liste les structures d'une région et leur répartition.
//! Usage : cargo run --release -p nb-world --example structmap -- <graine> <type> <x0> <z0> <taille>
use std::collections::BTreeMap;
use std::sync::Arc;

use nb_data::BlockRegistry;
use nb_world::{WorldConfig, WorldGenerator, WorldType};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let seed: i64 = a.get(1).map_or(12345, |s| s.parse().unwrap());
    let ty = match a.get(2).map_or("normal", String::as_str) {
        "flat" => WorldType::Flat,
        "amplified" => WorldType::Amplified,
        "island" => WorldType::Island,
        "archipelago" => WorldType::Archipelago,
        "highlands" => WorldType::Highlands,
        _ => WorldType::Normal,
    };
    let x0: i32 = a.get(3).map_or(-2000, |s| s.parse().unwrap());
    let z0: i32 = a.get(4).map_or(-2000, |s| s.parse().unwrap());
    let size: i32 = a.get(5).map_or(4000, |s| s.parse().unwrap());
    let reg = BlockRegistry::builtin();
    let cfg = Arc::new(WorldConfig::preset(ty));
    let g = WorldGenerator::new(seed, cfg, reg.ids());
    let sites = g.structure_sites(x0, z0, x0 + size, z0 + size);
    let mut by: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &sites {
        *by.entry(s.kind.name()).or_default() += 1;
    }
    let area = f64::from(size) * f64::from(size) / 1.0e6;
    println!(
        "{} structures sur {area:.1} km² ({:.1}/km²)",
        sites.len(),
        sites.len() as f64 / area
    );
    for (k, n) in &by {
        println!("  {k:28} {n}");
    }
    for s in sites.iter().take(12) {
        println!("{:24} x={} z={} y={}", s.kind.name(), s.x, s.z, s.base_y);
    }
}
