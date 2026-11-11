//! Carte des champs macroscopiques Norrcraft (session 1.1) : vue d'ensemble, un champ à la fois,
//! zones climatiques, provinces, sens des glaces. Sortie PNG et statistiques de répartition.

use nb_world::norrcraft::{
    ClimateZone, MacroFields, MacroSample, NorrConfig, NorrPreset, Province, COAST_C,
};

use crate::image_out::write_image;

pub const MODES: &str = "overview | c | o | t0 | hm | u | e | lk | gt | vr | zones | pv | theta";

type Rgb = [f64; 3];

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Rampe continue à plusieurs nœuds (bleu nuit → turquoise → vert → jaune → blanc).
fn ramp(t: f64) -> Rgb {
    const STOPS: [Rgb; 6] = [
        [15.0, 20.0, 80.0],
        [30.0, 110.0, 170.0],
        [60.0, 160.0, 90.0],
        [220.0, 210.0, 70.0],
        [200.0, 100.0, 50.0],
        [250.0, 245.0, 240.0],
    ];
    let t = t.clamp(0.0, 1.0) * (STOPS.len() - 1) as f64;
    let i = (t.floor() as usize).min(STOPS.len() - 2);
    mix(STOPS[i], STOPS[i + 1], t - i as f64)
}

fn zone_color(z: ClimateZone) -> Rgb {
    match z {
        ClimateZone::Nemoral => [130.0, 180.0, 70.0],
        ClimateZone::Hemiboreal => [80.0, 145.0, 70.0],
        ClimateZone::Boreal => [38.0, 98.0, 60.0],
        ClimateZone::Subarctic => [150.0, 135.0, 100.0],
        ClimateZone::Arctic => [225.0, 232.0, 240.0],
    }
}

fn province_color(p: Province) -> Rgb {
    match p {
        Province::GraniteGrey => [150.0, 150.0, 150.0],
        Province::Gneiss => [90.0, 90.0, 110.0],
        Province::GraniteRed => [190.0, 90.0, 80.0],
        Province::Limestone => [230.0, 220.0, 170.0],
        Province::Sandstone => [210.0, 150.0, 80.0],
        Province::AlumShale => [60.0, 55.0, 60.0],
        Province::Greenstone => [60.0, 150.0, 110.0],
        Province::Porphyry => [170.0, 60.0, 160.0],
    }
}

fn sea_color(s: &MacroSample) -> Rgb {
    let depth = (s.c / COAST_C).clamp(0.0, 1.0);
    mix([8.0, 22.0, 70.0], [80.0, 150.0, 200.0], depth * depth)
}

fn overview_color(s: &MacroSample) -> Rgb {
    if s.gt > 0.5 {
        return [190.0, 45.0, 25.0];
    }
    if !s.is_land() {
        return sea_color(s);
    }
    let zone = zone_color(ClimateZone::from_t0(s.t0));
    // Chaîne de montagnes : gris puis blanc sur les sommets.
    let rock = mix(zone, [135.0, 135.0, 140.0], s.u * 0.9);
    mix(
        rock,
        [245.0, 245.0, 250.0],
        ((s.u - 0.85) * 6.0).clamp(0.0, 1.0),
    )
}

fn scalar(mode: &str, s: &MacroSample) -> Option<f64> {
    Some(match mode {
        "c" => s.c,
        "o" => s.o,
        "t0" => s.t0,
        "hm" => s.hm,
        "u" => s.u,
        "e" => s.e,
        "lk" => s.lk,
        "gt" => s.gt,
        "vr" => s.vr,
        _ => return None,
    })
}

fn quantiles(mut v: Vec<f64>) -> [f64; 7] {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |f: f64| v[((v.len() - 1) as f64 * f) as usize];
    [q(0.0), q(0.05), q(0.25), q(0.5), q(0.75), q(0.95), q(1.0)]
}

pub fn run(a: &[String]) {
    let seed: i64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(12345);
    let preset = a
        .get(2)
        .and_then(|s| NorrPreset::parse(s))
        .unwrap_or_default();
    let cx: i32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let cz: i32 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let size: i32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(512);
    let step: i32 = a.get(6).and_then(|s| s.parse().ok()).unwrap_or(16);
    let out = a.get(7).cloned().unwrap_or_else(|| "norrmap.png".into());
    let mode = a.get(8).map(String::as_str).unwrap_or("overview");
    if !MODES.split(" | ").any(|m| m == mode) {
        eprintln!("mode inconnu « {mode} » ; modes : {MODES}");
        std::process::exit(2);
    }

    let cfg = NorrConfig::with_preset(preset);
    let f = MacroFields::new(seed, cfg);
    let n = (size * size) as usize;
    let x0 = cx - size * step / 2;
    let z0 = cz - size * step / 2;

    let mut px = vec![0u8; n * 3];
    let mut field_values: Vec<Vec<f64>> = (0..9).map(|_| Vec::with_capacity(n)).collect();
    let mut zone_all = [0u64; 5];
    let mut zone_land = [0u64; 5];
    let mut prov = [0u64; 8];
    let mut land = 0u64;
    let mut lava = 0u64;
    for j in 0..size {
        for i in 0..size {
            let (wx, wz) = (f64::from(x0 + i * step), f64::from(z0 + j * step));
            let s = f.sample(wx, wz);
            let p = f.province(wx, wz);
            let zone = ClimateZone::from_t0(s.t0);
            let zi = ClimateZone::ALL.iter().position(|&z| z == zone).unwrap();
            zone_all[zi] += 1;
            prov[p as usize] += 1;
            if s.is_land() {
                land += 1;
                zone_land[zi] += 1;
            }
            lava += u64::from(s.gt > 0.5);
            for (k, v) in [s.c, s.o, s.t0, s.hm, s.u, s.e, s.lk, s.gt, s.vr]
                .into_iter()
                .enumerate()
            {
                field_values[k].push(v);
            }

            let mut c: Rgb = match mode {
                "overview" => overview_color(&s),
                "zones" => {
                    let base = zone_color(zone);
                    if s.is_land() {
                        base
                    } else {
                        mix(base, [10.0, 20.0, 60.0], 0.65)
                    }
                }
                "pv" => province_color(p),
                "theta" => [
                    255.0 * (s.theta[0] * 0.5 + 0.5),
                    255.0 * (s.theta[1] * 0.5 + 0.5),
                    150.0,
                ],
                m => {
                    let v = scalar(m, &s).unwrap_or(0.0);
                    let mut c = ramp(v);
                    // Trait de côte en blanc sur les cartes de champs.
                    if (s.c - COAST_C).abs() < 0.003 * (f64::from(step) / 8.0).max(1.0) && m != "c"
                    {
                        c = [255.0, 255.0, 255.0];
                    }
                    c
                }
            };
            // Trait de côte discret aussi sur la vue d'ensemble.
            if mode == "overview" && (s.c - COAST_C).abs() < 0.002 {
                c = mix(c, [255.0, 255.0, 255.0], 0.5);
            }
            let o = ((j * size + i) * 3) as usize;
            for k in 0..3 {
                px[o + k] = c[k].clamp(0.0, 255.0) as u8;
            }
        }
    }

    let total = n as f64;
    println!(
        "-- Norrcraft « {} », graine {seed}, {size} px × {step} blocs ({} km), centre ({cx}, {cz})",
        preset.label(),
        f64::from(size * step) / 1000.0
    );
    println!(
        "terre (C ≥ {COAST_C}) : {:.1} %   province de lave (Gt > 0,5) : {:.2} %",
        land as f64 * 100.0 / total,
        lava as f64 * 100.0 / total
    );
    println!("-- quantiles (min 5% 25% 50% 75% 95% max)");
    for (name, v) in ["c", "o", "t0", "hm", "u", "e", "lk", "gt", "vr"]
        .iter()
        .zip(field_values)
    {
        let q = quantiles(v);
        println!(
            "{name:>3} {}",
            q.iter()
                .map(|x| format!("{x:5.2}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    println!("-- zones climatiques (% de la carte / % de la terre)");
    for (k, z) in ClimateZone::ALL.iter().enumerate() {
        println!(
            "{:>12} {:5.1} {:5.1}",
            z.name(),
            zone_all[k] as f64 * 100.0 / total,
            zone_land[k] as f64 * 100.0 / land.max(1) as f64
        );
    }
    println!("-- provinces rocheuses (% de la carte)");
    for p in Province::ALL {
        println!(
            "{:>12} {:5.1}",
            p.name(),
            prov[p as usize] as f64 * 100.0 / total
        );
    }

    write_image(&out, size as u32, size as u32, &px).expect("écriture de l'image");
    println!("écrit : {out}");
}
