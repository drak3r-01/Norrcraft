//! Carte Norrcraft : relief ombré (session 1.2), champs macroscopiques (session 1.1), zones
//! climatiques, provinces, sens des glaces, formes de relief (session 1.3), hydrographie (session
//! 1.4), biomes (2.1), sous-sol (2.2 : `geologie`, `coupe`). Sortie PNG et statistiques de répartition.

use nb_world::norrcraft::{
    ClimateZone, Coast, Forms, Hydrography, MacroSample, NorrConfig, NorrPreset, Province,
    WaterKind, COAST_C,
};

use crate::image_out::write_image;

pub const MODES: &str =
    "relief | hydro | w | h | fjord | formes | biomes | geologie | coupe | overview | c | o | t0 | hm | u | e | lk | gt | vr | zones | pv | theta";

type Rgb = [f64; 3];

/// Couleur d'une forme de relief de la session 1.3 (mode `formes`), ou `None`.
fn form_color(f: &Forms, sea_floor: bool) -> Option<Rgb> {
    if f.lava > 0.5 {
        return Some([200.0, 50.0, 30.0]);
    }
    if f.ice > 0.5 {
        return Some([235.0, 248.0, 255.0]);
    }
    if f.moraine > 1.0 {
        return Some([150.0, 120.0, 90.0]);
    }
    if f.palsa > 1.0 {
        return Some([150.0, 60.0, 200.0]);
    }
    if f.mire > 0.5 {
        return Some([150.0, 110.0, 40.0]);
    }
    if f.esker > 1.0 {
        return Some([250.0, 220.0, 60.0]);
    }
    if f.kettle > 0.5 {
        return Some([40.0, 200.0, 220.0]);
    }
    if f.drumlin > 1.0 {
        return Some([120.0, 200.0, 90.0]);
    }
    match f.coast {
        Coast::Skargard if !sea_floor => Some([240.0, 120.0, 90.0]),
        Coast::Klint => Some([250.0, 245.0, 200.0]),
        Coast::Raised => Some([200.0, 70.0, 60.0]),
        Coast::Dunes => Some([240.0, 210.0, 140.0]),
        Coast::SaltMeadow => Some([90.0, 220.0, 150.0]),
        _ => None,
    }
}

/// Noms des formes comptées (statistiques).
const FORM_STATS: [&str; 13] = [
    "îlots (K > 0,3, terre)",
    "klint",
    "côte soulevée",
    "dunes",
    "pré salé",
    "eskers",
    "marmites",
    "drumlins",
    "tourbières",
    "palses",
    "glace",
    "moraines",
    "lave",
];

fn form_flags(f: &Forms, land: bool) -> [bool; 13] {
    [
        f.archipelago > 0.3 && land,
        f.coast == Coast::Klint,
        f.coast == Coast::Raised,
        f.coast == Coast::Dunes,
        f.coast == Coast::SaltMeadow,
        f.esker > 1.0,
        f.kettle > 0.5,
        f.drumlin > 1.0,
        f.mire > 0.5,
        f.palsa > 1.0,
        f.ice > 0.5,
        f.moraine > 1.0,
        f.lava > 0.5,
    ]
}

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Rampe continue à plusieurs nœuds.
fn ramp_of(stops: &[Rgb], t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
    let i = (t.floor() as usize).min(stops.len() - 2);
    mix(stops[i], stops[i + 1], t - i as f64)
}

/// Rampe des champs (bleu nuit → turquoise → vert → jaune → blanc).
fn ramp(t: f64) -> Rgb {
    ramp_of(
        &[
            [15.0, 20.0, 80.0],
            [30.0, 110.0, 170.0],
            [60.0, 160.0, 90.0],
            [220.0, 210.0, 70.0],
            [200.0, 100.0, 50.0],
            [250.0, 245.0, 240.0],
        ],
        t,
    )
}

/// Teintes hypsométriques des terres, de S à S + 84.
fn land_tint(above_sea: f64) -> Rgb {
    ramp_of(
        &[
            [96.0, 140.0, 80.0],
            [70.0, 112.0, 62.0],
            [120.0, 125.0, 80.0],
            [140.0, 120.0, 95.0],
            [150.0, 145.0, 140.0],
            [235.0, 238.0, 242.0],
        ],
        above_sea / 84.0,
    )
}

/// Bleu de la mer selon la profondeur (0 à 40 blocs).
fn water_tint(depth: f64) -> Rgb {
    mix(
        [95.0, 160.0, 205.0],
        [8.0, 22.0, 70.0],
        (depth / 32.0).sqrt(),
    )
}

/// Couleur d'un pixel des modes d'eau (session 1.4). `hydro` : relief ombré, mer par profondeur,
/// lacs en turquoise, rivières en bleu vif (élargies aux grands pas pour rester visibles).
/// `w` : terres teintées par marche du niveau en escalier
/// W (une teinte toutes les 6 marches, alternance claire / foncée d'une marche à l'autre).
fn water_color(mode: &str, p: &Px, shade: f64, step: i32, sea: f64) -> Rgb {
    let wet = p.level > p.top;
    let s = |c: Rgb, k: f64| {
        let f = 1.0 + (shade - 1.0) * k;
        [c[0] * f, c[1] * f, c[2] * f]
    };
    if p.kind == WaterKind::River
        || (!wet && p.channel < p.river_width.max(f64::from(step) * 0.5) && step > 2)
    {
        return [30.0, 90.0, 235.0];
    }
    if wet {
        return match p.kind {
            WaterKind::Lake => s([60.0, 170.0, 200.0], 0.2),
            _ => s(water_tint(p.level - p.top), 0.35),
        };
    }
    if mode == "w" {
        let k = (p.stair - sea as i32) / 6;
        let base = ramp(0.35 + 0.65 * f64::from(k) / 14.0);
        let base = if k % 2 == 0 {
            base
        } else {
            mix(base, [0.0; 3], 0.25)
        };
        return s(base, 0.6);
    }
    s(land_tint(p.top - sea), 1.0)
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

fn print_quantiles(name: &str, v: Vec<f64>) {
    let q = quantiles(v);
    println!(
        "{name:>3} {}",
        q.iter()
            .map(|x| format!("{x:6.2}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
}

/// Ce que la seconde passe (couleurs du relief) doit savoir d'un pixel.
#[derive(Clone, Copy)]
struct Px {
    h: f64,
    /// Fond après l'hydrographie (bloc), niveau de l'eau, nature, niveau en escalier W.
    top: f64,
    level: f64,
    kind: WaterKind,
    stair: i32,
    channel: f64,
    river_width: f64,
    fjord: bool,
    valley: f64,
    plateau: f64,
    /// Formes de relief de la 1.3 (épaisseur de glace comprise).
    forms: Forms,
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
    let mode = a.get(8).map(String::as_str).unwrap_or("relief");
    if !MODES.split(" | ").any(|m| m == mode) {
        eprintln!("mode inconnu « {mode} » ; modes : {MODES}");
        std::process::exit(2);
    }
    if mode == "biomes" {
        crate::biomemap::run(seed, preset, cx, cz, size, step, &out);
        return;
    }
    if mode == "geologie" {
        crate::geomap::geology(seed, preset, cx, cz, size, step, &out);
        return;
    }
    if mode == "coupe" {
        crate::geomap::section(seed, preset, cx, cz, size, step, &out);
        return;
    }

    let cfg = NorrConfig::with_preset(preset);
    let hy = Hydrography::new(seed, cfg);
    let hf = hy.heights();
    let f = hf.macro_fields();
    let with_water = matches!(mode, "hydro" | "w");
    let sea = f64::from(cfg.sea_level);
    let n = (size * size) as usize;
    let x0 = cx - size * step / 2;
    let z0 = cz - size * step / 2;
    // Lacs de toute la carte (calculés une fois), seulement pour les modes d'eau.
    let lakes = if with_water {
        Some(hy.lakes(x0, z0, x0 + size * step, z0 + size * step))
    } else {
        None
    };
    let mut water_count = [0u64; 4];

    let mut px = vec![0u8; n * 3];
    let mut cells = Vec::with_capacity(n);
    let mut field_values: Vec<Vec<f64>> = (0..9).map(|_| Vec::with_capacity(n)).collect();
    let mut heights = Vec::with_capacity(n);
    let mut zone_all = [0u64; 5];
    let mut zone_land = [0u64; 5];
    let mut prov = [0u64; 8];
    let (mut land, mut land_c, mut lava, mut fjord, mut plateau, mut high) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut forms = [0u64; FORM_STATS.len()];
    for j in 0..size {
        for i in 0..size {
            let (wx, wz) = (f64::from(x0 + i * step), f64::from(z0 + j * step));
            let s = f.sample(wx, wz);
            let r = hf.relief(wx, wz, &s);
            let wc = lakes
                .as_ref()
                .map(|l| hy.hydrology().column(wx, wz, &r, &s, l));
            if let Some(c) = wc {
                let k = match c.kind {
                    WaterKind::None => 0,
                    WaterKind::Sea => 1,
                    WaterKind::Lake => 2,
                    WaterKind::River => 3,
                };
                water_count[k] += 1;
            }
            let p = f.province(wx, wz);
            let zone = ClimateZone::from_t0(s.t0);
            let zi = ClimateZone::ALL.iter().position(|&z| z == zone).unwrap();
            zone_all[zi] += 1;
            prov[p as usize] += 1;
            let is_land = r.h.floor() >= sea;
            if is_land {
                land += 1;
                zone_land[zi] += 1;
            }
            land_c += u64::from(s.is_land());
            lava += u64::from(s.gt > 0.5);
            fjord += u64::from(r.fjord && !is_land);
            plateau += u64::from(r.plateau > 0.5);
            high += u64::from(r.h >= sea + 44.0);
            heights.push(r.h);
            for (k, v) in [s.c, s.o, s.t0, s.hm, s.u, s.e, s.lk, s.gt, s.vr]
                .into_iter()
                .enumerate()
            {
                field_values[k].push(v);
            }
            for (k, on) in form_flags(&r.forms, is_land).into_iter().enumerate() {
                forms[k] += u64::from(on);
            }
            cells.push(Px {
                h: r.h,
                top: wc.map_or(r.h, |c| f64::from(c.top) + 0.5),
                level: wc.map_or(sea, |c| f64::from(c.level)),
                kind: wc.map_or(WaterKind::None, |c| c.kind),
                stair: wc.map_or(cfg.sea_level, |c| c.stair),
                channel: wc.map_or(f64::INFINITY, |c| c.channel_distance),
                river_width: wc.map_or(0.0, |c| c.river_width),
                fjord: r.fjord,
                valley: r.valley,
                plateau: r.plateau,
                forms: r.forms,
            });

            let mut c: Rgb = match mode {
                "relief" | "hydro" | "w" | "h" | "fjord" | "formes" => [0.0; 3],
                "overview" => overview_color(&s),
                "zones" => {
                    let base = zone_color(zone);
                    if is_land {
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
                    // Trait de côte réel (h = S) en blanc sur les cartes de champs.
                    if (r.h - sea).abs() < 0.6 * (f64::from(step) / 8.0).max(1.0) {
                        c = [255.0, 255.0, 255.0];
                    }
                    c
                }
            };
            if mode == "overview" && (r.h - sea).abs() < 0.4 {
                c = mix(c, [255.0, 255.0, 255.0], 0.5);
            }
            let o = ((j * size + i) * 3) as usize;
            for k in 0..3 {
                px[o + k] = c[k].clamp(0.0, 255.0) as u8;
            }
        }
    }

    // Seconde passe : relief ombré (lumière du nord-ouest), à partir des hauteurs voisines.
    if matches!(mode, "relief" | "hydro" | "w" | "h" | "fjord" | "formes") {
        let at =
            |i: i32, j: i32| cells[(j.clamp(0, size - 1) * size + i.clamp(0, size - 1)) as usize];
        for j in 0..size {
            for i in 0..size {
                let p = at(i, j);
                let hh = |q: Px| {
                    if with_water {
                        q.top.max(q.level)
                    } else {
                        q.h + q.forms.ice
                    }
                };
                let (dx, dz) = (
                    (hh(at(i + 1, j)) - hh(at(i - 1, j))) / f64::from(2 * step),
                    (hh(at(i, j + 1)) - hh(at(i, j - 1))) / f64::from(2 * step),
                );
                // Éclairage de type Lambert sans trigonométrie : normale (−dx, 1, −dz) normalisée.
                let k = 3.0;
                let (nx, nz) = (-dx * k, -dz * k);
                let norm = (nx * nx + 1.0 + nz * nz).sqrt();
                let light = (-0.6 * nx - 0.6 * nz + 0.55) / norm;
                let shade = (0.55 + 0.75 * light).clamp(0.25, 1.35);
                let c: Rgb = match mode {
                    "hydro" | "w" => water_color(mode, &p, shade, step, sea),
                    "h" => {
                        let v = ramp((p.h - 8.0) / 116.0);
                        if p.h < sea {
                            mix(v, [0.0, 0.0, 40.0], 0.3)
                        } else {
                            v
                        }
                    }
                    _ if (p.h + p.forms.ice).floor() < sea => {
                        let mut c = water_tint(sea - p.h);
                        if mode == "fjord" && p.fjord {
                            c = mix(c, [200.0, 40.0, 160.0], 0.6);
                        }
                        if mode == "formes" {
                            if let Some(f) = form_color(&p.forms, true) {
                                c = mix(c, f, 0.45);
                            }
                        }
                        // Le relief sous-marin reste lisible, atténué.
                        let s = 1.0 + (shade - 1.0) * 0.35;
                        [c[0] * s, c[1] * s, c[2] * s]
                    }
                    _ => {
                        let mut c = land_tint(p.h - sea);
                        // Relief : glace, basalte et tourbe se voient aussi sans le mode `formes`.
                        let f = &p.forms;
                        if f.ice > 0.5 {
                            c = [228.0, 240.0, 250.0];
                        } else if f.lava > 0.5 {
                            c = mix(c, [55.0, 50.0, 52.0], 0.85);
                        } else if f.mire > 0.5 {
                            c = mix(c, [120.0, 100.0, 50.0], 0.5);
                        }
                        if mode == "fjord" {
                            if p.valley > 0.5 {
                                c = mix(c, [230.0, 170.0, 40.0], 0.5);
                            } else if p.plateau > 0.5 {
                                c = mix(c, [60.0, 170.0, 200.0], 0.4);
                            }
                        }
                        if mode == "formes" {
                            if let Some(fc) = form_color(f, false) {
                                c = mix(c, fc, 0.75);
                            }
                        }
                        [c[0] * shade, c[1] * shade, c[2] * shade]
                    }
                };
                let o = ((j * size + i) * 3) as usize;
                for k in 0..3 {
                    px[o + k] = c[k].clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    let total = n as f64;
    let pct = |v: u64| v as f64 * 100.0 / total;
    println!(
        "-- Norrcraft « {} », graine {seed}, {size} px × {step} blocs ({} km), centre ({cx}, {cz}), mode {mode}",
        preset.label(),
        f64::from(size * step) / 1000.0
    );
    println!(
        "terre (h ≥ S) : {:.1} %   (C ≥ {COAST_C} : {:.1} %)   fjords : {:.2} %   plateaux : {:.1} %   au-dessus de S+44 : {:.1} %   lave (Gt > 0,5) : {:.2} %",
        pct(land),
        pct(land_c),
        pct(fjord),
        pct(plateau),
        pct(high),
        pct(lava)
    );
    if with_water {
        let lake_n = lakes.as_ref().map_or(0, |l| l.lakes().count());
        println!(
            "eau : mer {:.1} %   lacs {:.2} %   rivières {:.2} %   ({lake_n} lacs du Worley sur la carte)",
            pct(water_count[1]),
            pct(water_count[2]),
            pct(water_count[3]),
        );
    }
    println!(
        "-- formes de relief (% de la carte) : {}",
        FORM_STATS
            .iter()
            .zip(forms)
            .map(|(name, v)| format!("{name} {:.2}", pct(v)))
            .collect::<Vec<_>>()
            .join(" · ")
    );
    println!("-- quantiles (min 5% 25% 50% 75% 95% max)");
    print_quantiles("h", heights);
    for (name, v) in ["c", "o", "t0", "hm", "u", "e", "lk", "gt", "vr"]
        .iter()
        .zip(field_values)
    {
        print_quantiles(name, v);
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
        println!("{:>12} {:5.1}", p.name(), pct(prov[p as usize]));
    }

    write_image(&out, size as u32, size as u32, &px).expect("écriture de l'image");
    println!("écrit : {out}");
}
