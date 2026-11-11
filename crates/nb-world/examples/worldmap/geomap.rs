//! Cartes du sous-sol Norrcraft (session 2.2) : mode `geologie` (provinces rocheuses, filons de
//! diabase, corps de minerai et grottes) et mode `coupe` (coupe verticale le long de x, blocs
//! réellement générés : socle, Malmberg, cavités noyées).

use nb_core::ChunkPos;
use nb_data::{BlockRegistry, WorldKind};
use nb_world::norrcraft::{
    NorrBlocks, NorrConfig, NorrPreset, NorrcraftTerrain, Province, Region, WaterKind,
};
use nb_world::{Chunk, TerrainGenerator};

use crate::image_out::write_image;

type Rgb = [u8; 3];

fn province_color(p: Province) -> Rgb {
    match p {
        Province::GraniteGrey => [150, 150, 156],
        Province::Gneiss => [112, 120, 150],
        Province::GraniteRed => [196, 110, 96],
        Province::Limestone => [228, 222, 186],
        Province::Sandstone => [210, 160, 100],
        Province::AlumShale => [60, 62, 70],
        Province::Greenstone => [70, 140, 96],
        Province::Porphyry => [160, 70, 110],
    }
}

fn deposit_color(kind: &str) -> Rgb {
    match kind {
        "malmberg" => [230, 40, 30],
        "cuivre" => [240, 140, 40],
        "cobalt" => [40, 80, 230],
        "tungstène" => [20, 20, 20],
        "stéatite" => [80, 220, 200],
        "grotte marine" => [250, 250, 80],
        _ => [255, 0, 255],
    }
}

/// Couleur d'un bloc pour la coupe.
fn block_color(name: &str) -> Rgb {
    match name {
        "air" => [170, 205, 245],
        "water" => [40, 80, 190],
        "bedrock" => [10, 10, 10],
        "djupberg" => [58, 60, 70],
        "granit" => [150, 150, 156],
        "gnejs" => [112, 120, 150],
        "rodgranit" => [196, 110, 96],
        "kalksten" => [228, 222, 186],
        "krita" => [250, 250, 240],
        "flinta" => [30, 30, 40],
        "sandsten" => [210, 160, 100],
        "alunskiffer" => [60, 62, 70],
        "gronsten" => [70, 140, 96],
        "porfyr" => [160, 70, 110],
        "diabas" => [0, 0, 0],
        "basaltsten" => [70, 70, 80],
        "jarnmalm" => [150, 60, 40],
        "rik_jarnmalm" => [210, 50, 30],
        "kopparmalm" => [230, 130, 50],
        "silvermalm" => [240, 240, 255],
        "koboltmalm" => [40, 80, 230],
        "volframmalm" => [200, 120, 40],
        "guldkvarts" | "guldgrus" => [250, 210, 40],
        "kvarts" => [235, 235, 245],
        "taljsten" => [100, 200, 180],
        "myrmalm" | "sjomalm" => [190, 120, 50],
        "svavel" => [230, 220, 60],
        "rodmull" => [150, 70, 40],
        "torv" | "vitmossa" => [90, 70, 50],
        "blais" | "havsis" | "sno" => [225, 238, 250],
        "gras" | "mossa" | "barrmatta" | "lavhed" | "lovforna" => [70, 130, 60],
        _ => [150, 140, 120],
    }
}

fn terrain(seed: i64, preset: NorrPreset) -> NorrcraftTerrain {
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    NorrcraftTerrain::new(
        seed,
        NorrConfig::with_preset(preset),
        NorrBlocks::from_registry(&reg),
    )
}

/// Mode `geologie` : provinces ombrées par le relief, mer, filons de diabase (pas ≤ 4), corps de
/// minerai en carrés colorés.
pub fn geology(seed: i64, preset: NorrPreset, cx: i32, cz: i32, size: i32, step: i32, out: &str) {
    let t = terrain(seed, preset);
    let x0 = cx - size * step / 2;
    let z0 = cz - size * step / 2;
    let mut px = vec![0u8; (size * size * 3) as usize];
    let mut count = [0u64; 8];
    let mut land = 0u64;
    let mut dikes = 0u64;
    for j in 0..size {
        for i in 0..size {
            let (wx, wz) = (x0 + i * step, z0 + j * step);
            let m = t.heights().macro_fields().sample_lattice(wx, wz);
            let w = t.hydrography().column(wx, wz);
            let h = w.top;
            let hx = t.hydrography().column(wx + step, wz).top;
            let hz = t.hydrography().column(wx, wz + step).top;
            let shade = 1.0 + f64::from(h - hx + h - hz) * 0.04 / f64::from(step.max(1)) * 8.0;
            let shade = shade.clamp(0.6, 1.4);
            let mut c = if w.is_wet() && w.kind == WaterKind::Sea {
                let d = f64::from((w.level - w.top).clamp(0, 40)) / 40.0;
                [
                    (40.0 - 30.0 * d) as u8,
                    (90.0 - 50.0 * d) as u8,
                    (190.0 - 60.0 * d) as u8,
                ]
            } else if w.is_wet() {
                [60, 120, 220]
            } else {
                let (p, _) = t.subsoil().province(wx, wz, &m);
                count[p as usize] += 1;
                land += 1;
                let b = province_color(p);
                [
                    (f64::from(b[0]) * shade).min(255.0) as u8,
                    (f64::from(b[1]) * shade).min(255.0) as u8,
                    (f64::from(b[2]) * shade).min(255.0) as u8,
                ]
            };
            if step <= 4 && !w.is_wet() && t.subsoil().dike_at(wx, wz) {
                c = [0, 0, 0];
                dikes += 1;
            }
            let o = ((j * size + i) * 3) as usize;
            px[o..o + 3].copy_from_slice(&c);
        }
    }
    let region = Region {
        x0,
        z0,
        x1: x0 + size * step,
        z1: z0 + size * step,
    };
    let mut kinds: std::collections::BTreeMap<&str, u32> = Default::default();
    let mut nearest_malm = f64::INFINITY;
    for d in t.deposits(region) {
        *kinds.entry(d.kind).or_insert(0) += 1;
        if d.kind == "malmberg" {
            nearest_malm = nearest_malm.min(f64::from(d.center[0]).hypot(f64::from(d.center[2])));
        }
        let col = deposit_color(d.kind);
        let r = ((d.radius / f64::from(step)) as i32).clamp(1, 12);
        let r = if d.kind == "malmberg" { r } else { r.min(3) };
        let (pi, pj) = ((d.center[0] - x0) / step, (d.center[2] - z0) / step);
        for dj in -r..=r {
            for di in -r..=r {
                let (a, b) = (pi + di, pj + dj);
                let edge = di.abs() == r || dj.abs() == r;
                if a >= 0 && b >= 0 && a < size && b < size && (edge || r <= 2) {
                    let o = ((b * size + a) * 3) as usize;
                    px[o..o + 3].copy_from_slice(&col);
                }
            }
        }
    }
    println!(
        "-- sous-sol Norrcraft « {} », graine {seed}, {size} px × {step} blocs ({} km), centre ({cx}, {cz})",
        preset.label(),
        f64::from(size * step) / 1000.0
    );
    println!("{:>14} {:>7} {:>7}", "province", "part %", "cible");
    for p in Province::ALL {
        println!(
            "{:>14} {:7.1} {:7.1}",
            p.name(),
            100.0 * count[p as usize] as f64 / land.max(1) as f64,
            p.share()
        );
    }
    println!("corps (carrés : Malmberg rouge, cuivre orange, cobalt bleu, tungstène noir, stéatite cyan, grotte marine jaune) : {kinds:?}");
    if nearest_malm.is_finite() {
        println!(
            "Malmberg le plus proche de l'origine : {:.1} km",
            nearest_malm / 1000.0
        );
    }
    if step <= 4 {
        println!("colonnes de filon de diabase : {dikes}");
    }
    write_image(out, size as u32, size as u32, &px).expect("écriture de l'image");
    println!("écrit : {out}");
}

/// Mode `coupe` : coupe verticale le long de x à `z = cz`, `size` colonnes de `step` blocs, sur
/// toute la hauteur (128 blocs = 128 × `scale` px, `scale` = 3).
pub fn section(seed: i64, preset: NorrPreset, cx: i32, cz: i32, size: i32, step: i32, out: &str) {
    const SCALE: i32 = 3;
    let t = terrain(seed, preset);
    let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
    let names: Vec<String> = (0..=255u8).map(|id| reg.from_id(id).name.clone()).collect();
    let x0 = cx - size * step / 2;
    let (w, hpx) = (size * SCALE, 128 * SCALE);
    let mut px = vec![0u8; (w * hpx * 3) as usize];
    let mut chunks: std::collections::HashMap<i32, Chunk> = Default::default();
    for i in 0..size {
        let wx = x0 + i * step;
        let ccx = wx.div_euclid(16);
        let chunk = chunks.entry(ccx).or_insert_with(|| {
            let mut c = Chunk::new(ChunkPos::new(ccx, cz.div_euclid(16)));
            t.generate_chunk(&mut c);
            c
        });
        for y in 0..128 {
            let b = chunk.block(wx.rem_euclid(16), y, cz.rem_euclid(16));
            let col = block_color(&names[b as usize]);
            for dx in 0..SCALE {
                for dy in 0..SCALE {
                    let (a, r) = (i * SCALE + dx, (127 - y) * SCALE + dy);
                    let o = ((r * w + a) * 3) as usize;
                    px[o..o + 3].copy_from_slice(&col);
                }
            }
        }
    }
    println!(
        "-- coupe Norrcraft graine {seed}, x de {x0} à {} (pas {step}), z = {cz}",
        x0 + size * step
    );
    write_image(out, w as u32, hpx as u32, &px).expect("écriture de l'image");
    println!("écrit : {out}");
}
