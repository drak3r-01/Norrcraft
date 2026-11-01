//! Tests headless du rendu Vulkan (rendu hors écran + relecture).
//!
//! Sans pilote Vulkan, les tests sont sautés (message `SKIP`), sauf si
//! `NB_GFX_REQUIRE_GPU=1` (alors ils échouent). Sous Docker :
//! `NB_GFX_REQUIRE_GPU=1 crates/nb-gfx/scripts/vk-docker.sh test -p nb-gfx` (lavapipe).
//!
//! Les images rendues sont écrites dans `crates/nb-gfx/renders/` (ignoré par git) pour
//! inspection ; `NB_GFX_UPDATE_GOLDEN=1` régénère les références de `tests/golden/`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use glam::{IVec3, Vec3, Vec4};
use image::RgbaImage;
use nb_data::BlockRegistry;
use nb_gfx::vk::validation_error_count;
use nb_gfx::{
    sky_params, Camera, ChunkPos, ChunkVertex, EyeFluid, FrameInput, GfxSettings, GpuContext,
    OffscreenRenderer, SceneAssets, ShadowQuality, SkyClock, TextureArrayCpu,
};
use nb_mesh::{mesh_chunk, BlockTable, ChunkVoxels, Neighborhood, UniformBiome};

const W: u32 = 320;
const H: u32 = 180;

fn gpu() -> Option<(Arc<GpuContext>, MutexGuard<'static, ()>)> {
    static CTX: OnceLock<Option<Arc<GpuContext>>> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = CTX.get_or_init(|| match GpuContext::new_headless() {
        Ok(c) => {
            eprintln!(
                "GPU : {} (validation : {})",
                c.device_name, c.validation_enabled
            );
            Some(Arc::new(c))
        }
        Err(e) => {
            if std::env::var("NB_GFX_REQUIRE_GPU").as_deref() == Ok("1") {
                panic!("Vulkan indisponible : {e}");
            }
            eprintln!("SKIP : Vulkan indisponible ({e})");
            None
        }
    });
    ctx.clone().map(|c| (c, guard))
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn assets() -> SceneAssets {
    SceneAssets::load(&crate_dir().join("../../resource_packs"), "Default").0
}

fn save(name: &str, img: &RgbaImage) {
    let dir = crate_dir().join("renders");
    std::fs::create_dir_all(&dir).unwrap();
    img.save(dir.join(format!("{name}.png"))).unwrap();
}

/// Compare à `tests/golden/<name>.png` (écart moyen < 1/255, < 0.5 % de pixels à > 8/255).
fn check_golden(name: &str, img: &RgbaImage) {
    let path = crate_dir().join("tests/golden").join(format!("{name}.png"));
    if std::env::var("NB_GFX_UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        img.save(&path).unwrap();
        return;
    }
    let Ok(golden) = image::open(&path) else {
        panic!(
            "référence absente : {} (NB_GFX_UPDATE_GOLDEN=1)",
            path.display()
        );
    };
    let golden = golden.to_rgba8();
    assert_eq!(golden.dimensions(), img.dimensions());
    let mut sum = 0u64;
    let mut big = 0usize;
    for (a, b) in golden.pixels().zip(img.pixels()) {
        let d = (0..3)
            .map(|c| (i32::from(a[c]) - i32::from(b[c])).unsigned_abs())
            .max()
            .unwrap();
        sum += u64::from(d);
        if d > 8 {
            big += 1;
        }
    }
    let n = (img.width() * img.height()) as f64;
    let mean = sum as f64 / n;
    assert!(mean < 1.0, "{name} : écart moyen {mean}");
    assert!(
        (big as f64) / n < 0.005,
        "{name} : {big} pixels très différents"
    );
}

fn mean_rgb(img: &RgbaImage) -> Vec3 {
    let mut s = Vec3::ZERO;
    for p in img.pixels() {
        s += Vec3::new(f32::from(p[0]), f32::from(p[1]), f32::from(p[2]));
    }
    s / (img.width() * img.height()) as f32
}

/// Monde de démonstration 3×3 chunks : sol d'herbe y = 60, mare d'eau, bassin de lave,
/// arbre, herbes hautes et dalle flottante (ombre portée).
fn demo_world(r: &mut OffscreenRenderer) {
    let reg = BlockRegistry::builtin();
    let ids = *reg.ids();
    let atlas = r.scene.block_atlas().clone();
    let table = BlockTable::from_registry(&reg, |n| atlas.layer(n));
    let block_at = |x: i32, y: i32, z: i32| -> u8 {
        let in_pool = (2..8).contains(&x) && (-12..-6).contains(&z);
        let in_lava = (-10..-6).contains(&x) && (-4..0).contains(&z);
        if y < 56 {
            return ids.stone;
        }
        if y < 60 {
            if (in_pool || in_lava) && y >= 58 {
                return if in_pool { ids.water } else { ids.lava };
            }
            return ids.dirt;
        }
        if y == 60 {
            if in_pool {
                return ids.water;
            }
            if in_lava {
                return ids.lava;
            }
            return ids.grass;
        }
        // Arbre en (12, -4).
        if x == 12 && z == -4 && (61..66).contains(&y) {
            return ids.wood;
        }
        if (10..15).contains(&x) && (-6..-1).contains(&z) && (64..68).contains(&y) {
            return ids.leaves;
        }
        // Dalle flottante.
        if (-6..0).contains(&x) && (-16..-10).contains(&z) && y == 66 {
            return ids.stone;
        }
        if y == 61 && (x + z).rem_euclid(5) == 0 && !(in_pool || in_lava) && x < 0 && z > -6 {
            return ids.tall_grass;
        }
        0
    };
    let mut chunks = Vec::new();
    for cz in -1..=1 {
        for cx in -1..=1 {
            let mut v = ChunkVoxels::filled(0);
            for lx in 0..16 {
                for lz in 0..16 {
                    for y in 0..128 {
                        let id = block_at(cx * 16 + lx as i32, y as i32, cz * 16 + lz as i32);
                        if id != 0 {
                            v.set(lx, y, lz, id);
                        }
                    }
                }
            }
            chunks.push(((cx, cz), v));
        }
    }
    let get = |cx: i32, cz: i32| {
        chunks
            .iter()
            .find(|((x, z), _)| *x == cx && *z == cz)
            .map(|(_, v)| v)
    };
    for &((cx, cz), _) in &chunks {
        let n: [Option<&ChunkVoxels>; 9] =
            std::array::from_fn(|i| get(cx + (i % 3) as i32 - 1, cz + (i / 3) as i32 - 1));
        let mesh = mesh_chunk(
            ChunkPos::new(cx, cz),
            &Neighborhood::from_voxels(n),
            &table,
            &UniformBiome(0),
        );
        r.scene
            .upload_chunk_mesh(ChunkPos::new(cx, cz), &mesh.vertices)
            .unwrap();
    }
}

fn demo_camera() -> Camera {
    Camera {
        position: Vec3::new(4.0, 68.0, 14.0),
        pitch_deg: 20.0,
        yaw_deg: 10.0,
        ..Camera::default()
    }
}

fn frame<'a>(camera: &'a Camera, clock: &'a SkyClock) -> FrameInput<'a> {
    FrameInput {
        camera,
        time_seconds: 1.0,
        sky_clock: clock,
        eye_fluid: EyeFluid::None,
        heat_intensity: 0.0,
        highlight_block: None,
    }
}

/// Plusieurs frames pour stabiliser l'occlusion (résultats lus avec ≥ 1 frame de retard).
fn render_settled(r: &mut OffscreenRenderer, input: &FrameInput<'_>) -> RgbaImage {
    for _ in 0..3 {
        r.render(input).unwrap();
    }
    r.render(input).unwrap()
}

fn assert_no_validation_errors() {
    assert_eq!(validation_error_count(), 0, "erreurs de validation Vulkan");
}

// ---------------------------------------------------------------------------- tests

/// Ciel de midi sans géométrie : comparaison pixel à pixel avec un calcul CPU de `pp.frag`
/// (lignes sous l'horizon, où ni nuages ni aurores n'interviennent).
#[test]
fn sky_matches_cpu_reference() {
    let Some((ctx, _g)) = gpu() else { return };
    let mut r = OffscreenRenderer::new(ctx, assets(), W, H, &GfxSettings::default()).unwrap();
    let cam = Camera {
        position: Vec3::new(0.0, 70.0, 0.0),
        pitch_deg: 0.0,
        yaw_deg: 30.0,
        ..Camera::default()
    };
    let clock = SkyClock::new();
    let img = r.render(&frame(&cam, &clock)).unwrap();
    save("sky_noon", &img);

    let sky = sky_params(clock.time_of_day);
    let mut c = cam;
    c.aspect = W as f32 / H as f32;
    let inv_proj = c.proj_gl().inverse();
    let inv_view = c.view().inverse();
    let aces = |x: Vec3| {
        ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(Vec3::ZERO, Vec3::ONE)
    };
    let mut checked = 0;
    for py in (H / 2 + 4..H).step_by(7) {
        for px in (0..W).step_by(13) {
            let uv = (
                (px as f32 + 0.5) / W as f32,
                1.0 - (py as f32 + 0.5) / H as f32,
            );
            let ndc = Vec4::new(uv.0 * 2.0 - 1.0, uv.1 * 2.0 - 1.0, 1.0, 1.0);
            let mut vp = inv_proj * ndc;
            vp /= vp.w;
            let wp = inv_view * vp;
            let dir = (wp.truncate() - c.position).normalize();
            assert!(dir.y < 0.0);
            let g = (dir.y + 0.05).max(0.0).powf(0.6).clamp(0.0, 1.0);
            let sky_col = sky.horizon.lerp(sky.zenith, g);
            let sun_dot = dir.dot(-sky.shadow_light_dir.normalize()).max(0.0);
            let col = sky_col + sun_dot.powf(64.0) * sky.sun_color;
            let mapped = aces(col * Vec3::new(1.15, 1.05, 0.95)).powf(1.0 / 2.2) * 255.0;
            let p = img.get_pixel(px, py);
            for k in 0..3 {
                let d = (f32::from(p[k]) - mapped[k]).abs();
                assert!(
                    d <= 2.0,
                    "pixel ({px},{py}) canal {k} : {} vs {}",
                    p[k],
                    mapped[k]
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 50);
    assert_no_validation_errors();
}

fn aces_gamma(x: Vec3) -> Vec3 {
    let a = ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(Vec3::ZERO, Vec3::ONE);
    a.powf(1.0 / 2.2) * 255.0
}

/// Terrain opaque éclairé (main.frag) puis brouillard + tonemapping (pp.frag), comparé à un
/// calcul CPU : texture unie (atlas de repli magenta), sans ombre ni bloom.
#[test]
fn lit_terrain_matches_cpu_reference() {
    let Some((ctx, _g)) = gpu() else { return };
    let settings = GfxSettings {
        shadow_quality: ShadowQuality::Off,
        bloom: false,
        ..GfxSettings::default()
    };
    let mut r = OffscreenRenderer::new(ctx, assets(), W, H, &settings).unwrap();
    r.scene
        .reload_block_atlas(TextureArrayCpu::fallback())
        .unwrap();
    let v = |x: f32, z: f32| ChunkVertex {
        pos: [x, 60.0, z],
        // UV variables (texture unie en REPEAT) : des UV constantes donnent des dérivées nulles,
        // que lavapipe + anisotropie transforme en NaN.
        uv: [x, z],
        layer: 0.0,
        color: [0.8, 0.9, 0.7],
    };
    let (a, b, c, d) = (
        v(-50.0, 50.0),
        v(50.0, 50.0),
        v(50.0, -50.0),
        v(-50.0, -50.0),
    );
    r.scene
        .upload_chunk_mesh(ChunkPos::new(0, 0), &[a, b, c, c, d, a])
        .unwrap();
    for (time, name) in [
        (900.0, "flat_noon"),
        (0.0, "flat_sunset"),
        (300.0, "flat_midnight"),
    ] {
        let cam = Camera {
            position: Vec3::new(8.0, 70.0, 8.0),
            pitch_deg: 89.0,
            yaw_deg: 0.0,
            ..Camera::default()
        };
        let mut clock = SkyClock::new();
        clock.set_time(time);
        let img = render_settled(&mut r, &frame(&cam, &clock));
        save(name, &img);
        let sky = sky_params(time);
        let dist = 10.0 / 89f32.to_radians().sin();
        let fog = (-(dist * 0.002f32).powf(1.8)).exp();
        // Regard quasi vertical vers le bas : ciel = horizon, pas de halo.
        let lit =
            Vec3::new(1.0, 0.0, 1.0) * Vec3::new(0.8, 0.9, 0.7) * (sky.ambient + sky.light_color);
        let hdr = sky.horizon.lerp(lit, fog) * Vec3::new(1.15, 1.05, 0.95);
        let expect = aces_gamma(hdr);
        let p = img.get_pixel(W / 2, H / 2);
        for k in 0..3 {
            assert!(
                (f32::from(p[k]) - expect[k]).abs() <= 2.0,
                "{name} canal {k} : {} vs {}",
                p[k],
                expect[k]
            );
        }
    }
    assert_no_validation_errors();
}

#[test]
fn demo_world_noon_golden() {
    let Some((ctx, _g)) = gpu() else { return };
    let mut r = OffscreenRenderer::new(ctx, assets(), W, H, &GfxSettings::default()).unwrap();
    demo_world(&mut r);
    assert_eq!(r.scene.chunk_mesh_count(), 9);
    let cam = demo_camera();
    let clock = SkyClock::new();
    let img = render_settled(&mut r, &frame(&cam, &clock));
    save("demo_noon", &img);
    // Haut de l'image : ciel bleu clair ; bas : terrain (plus sombre, dominante verte).
    let top = img.get_pixel(W / 2, 2);
    assert!(top[2] > top[0], "ciel non bleu : {top:?}");
    let bottom = img.get_pixel(W / 2, H - 3);
    assert!(bottom[1] >= bottom[2], "sol non vert : {bottom:?}");
    assert!(!r.scene.visible_chunks().is_empty());
    check_golden("demo_noon", &img);
    assert_no_validation_errors();
}

#[test]
fn day_night_cycle_and_fluids() {
    let Some((ctx, _g)) = gpu() else { return };
    let mut r = OffscreenRenderer::new(ctx, assets(), W, H, &GfxSettings::default()).unwrap();
    demo_world(&mut r);
    let cam = demo_camera();
    let mut clock = SkyClock::new();
    let noon = render_settled(&mut r, &frame(&cam, &clock));
    clock.set_time(0.0);
    let sunset = render_settled(&mut r, &frame(&cam, &clock));
    save("demo_sunset", &sunset);
    clock.set_time(300.0);
    let midnight = render_settled(&mut r, &frame(&cam, &clock));
    save("demo_midnight", &midnight);
    check_golden("demo_sunset", &sunset);
    let (ln, ls, lm) = (mean_rgb(&noon), mean_rgb(&sunset), mean_rgb(&midnight));
    assert!(
        ln.length() > ls.length() && ls.length() > lm.length(),
        "{ln} {ls} {lm}"
    );
    // Coucher de soleil : dominante chaude.
    assert!(ls.x > ls.z, "{ls}");

    clock.set_time(900.0);
    let mut input = frame(&cam, &clock);
    input.eye_fluid = EyeFluid::Water {
        color: Vec3::new(0.25, 0.45, 0.85),
    };
    let water = render_settled(&mut r, &input);
    save("eye_water", &water);
    let mw = mean_rgb(&water);
    assert!(mw.z > mw.x, "sous l'eau non bleuté : {mw}");
    input.eye_fluid = EyeFluid::Lava;
    let lava = render_settled(&mut r, &input);
    save("eye_lava", &lava);
    let ml = mean_rgb(&lava);
    assert!(ml.x > 2.0 * ml.z, "dans la lave non rouge : {ml}");
    input.eye_fluid = EyeFluid::None;
    input.heat_intensity = 1.0;
    let heat = render_settled(&mut r, &input);
    save("heat", &heat);
    assert!(mean_rgb(&heat).x > ln.x);
    assert_no_validation_errors();
}

#[test]
fn shadows_and_shadow_off_clear() {
    let Some((ctx, _g)) = gpu() else { return };
    let cam = Camera {
        position: Vec3::new(-3.0, 75.0, -2.0),
        pitch_deg: 70.0,
        yaw_deg: 0.0,
        ..Camera::default()
    };
    let clock = SkyClock::new();
    let low = GfxSettings::default();
    let off = GfxSettings {
        shadow_quality: ShadowQuality::Off,
        ..low
    };
    let mut r = OffscreenRenderer::new(ctx.clone(), assets(), W, H, &low).unwrap();
    demo_world(&mut r);
    let shadowed = render_settled(&mut r, &frame(&cam, &clock));
    save("shadow_low", &shadowed);
    assert!(!r.scene.shadow_visible_chunks().is_empty());
    r.scene.apply_settings(&off).unwrap();
    let switched_off = render_settled(&mut r, &frame(&cam, &clock));
    save("shadow_switched_off", &switched_off);
    r.scene
        .apply_settings(&GfxSettings {
            shadow_quality: ShadowQuality::High,
            ..low
        })
        .unwrap();
    assert_eq!(r.scene.shadow_map_size(), 4096);
    let high = render_settled(&mut r, &frame(&cam, &clock));
    save("shadow_high", &high);
    drop(r);

    let mut r2 = OffscreenRenderer::new(ctx, assets(), W, H, &off).unwrap();
    demo_world(&mut r2);
    let off_from_start = render_settled(&mut r2, &frame(&cam, &clock));
    // Carte effacée à 1.0 : sans ombre, plus lumineux qu'avec ombres.
    let (s, o) = (mean_rgb(&shadowed), mean_rgb(&switched_off));
    assert!(o.length() > s.length() + 1.0, "ombres : {s} / sans : {o}");
    // OFF dès le démarrage = OFF après coup (pas d'ombres figées).
    assert_eq!(off_from_start, switched_off);
    assert!((mean_rgb(&high) - s).length() < 10.0);
    assert_no_validation_errors();
}

#[test]
fn highlight_outline_is_black() {
    let Some((ctx, _g)) = gpu() else { return };
    let mut r = OffscreenRenderer::new(ctx, assets(), W, H, &GfxSettings::default()).unwrap();
    demo_world(&mut r);
    // Joueur debout (œil à y = 62.6), regard à 45° vers le bas : vise le bloc (4, 60, 5).
    let cam = Camera {
        position: Vec3::new(4.5, 62.6, 7.1),
        pitch_deg: 45.0,
        yaw_deg: 0.0,
        ..Camera::default()
    };
    let clock = SkyClock::new();
    let base = render_settled(&mut r, &frame(&cam, &clock));
    let mut input = frame(&cam, &clock);
    input.highlight_block = Some(IVec3::new(4, 60, 5));
    let hl = render_settled(&mut r, &input);
    save("highlight", &hl);
    let black = |img: &RgbaImage| {
        img.pixels()
            .filter(|p| p[0] == 0 && p[1] == 0 && p[2] == 0)
            .count()
    };
    let added = black(&hl) - black(&base);
    assert!(added > 100, "contour absent ({added} pixels noirs ajoutés)");
    // Pixel central : intérieur de la face du dessus, pas sur une arête.
    assert_ne!(hl.get_pixel(W / 2, H / 2).0, [0, 0, 0, 255]);
    // Un bloc caché sous le sol n'est pas visible (test de profondeur).
    input.highlight_block = Some(IVec3::new(4, 50, 5));
    let hidden = render_settled(&mut r, &input);
    assert_eq!(black(&hidden), black(&base));
    assert_no_validation_errors();
}

#[test]
fn water_and_lava_surfaces() {
    let Some((ctx, _g)) = gpu() else { return };
    let mut r = OffscreenRenderer::new(ctx, assets(), W, H, &GfxSettings::default()).unwrap();
    demo_world(&mut r);
    let clock = SkyClock::new();
    // Au-dessus du bord sud de la mare (x 2..8, z -12..-6), regard vers le bas.
    let cam = Camera {
        position: Vec3::new(5.0, 64.0, -3.0),
        pitch_deg: 40.0,
        yaw_deg: 0.0,
        ..Camera::default()
    };
    let img = render_settled(&mut r, &frame(&cam, &clock));
    save("water_closeup", &img);
    let c = img.get_pixel(W / 2, H / 2);
    assert!(c[2] > c[0], "eau non bleue : {c:?}");
    // Bassin de lave (x -10..-6, z -4..0).
    let cam = Camera {
        position: Vec3::new(-8.0, 64.0, 2.5),
        pitch_deg: 45.0,
        yaw_deg: 0.0,
        ..Camera::default()
    };
    let img = render_settled(&mut r, &frame(&cam, &clock));
    save("lava_closeup", &img);
    let c = img.get_pixel(W / 2, H / 2);
    assert!(c[0] > 200 && c[2] < 120, "lave : {c:?}");
    assert_no_validation_errors();
}

fn quad(z: f32, half: f32, layer: f32) -> Vec<ChunkVertex> {
    let v = |x: f32, y: f32| ChunkVertex {
        pos: [x, y, z],
        uv: [0.0, 0.0],
        layer,
        color: [1.0; 3],
    };
    // Face vers +Z (CCW vue de +Z).
    let (a, b, c, d) = (
        v(-half, -half + 64.0),
        v(half, -half + 64.0),
        v(half, half + 64.0),
        v(-half, half + 64.0),
    );
    vec![a, b, c, c, d, a]
}

#[test]
fn culling_and_occlusion_queries() {
    let Some((ctx, _g)) = gpu() else { return };
    let mut r = OffscreenRenderer::new(ctx, assets(), W, H, &GfxSettings::default()).unwrap();
    let stone = r.scene.block_atlas().layer("stone") as f32;
    // Mur immense dans le chunk (0,-1) ; petit quad caché derrière dans (0,-3) ; un chunk
    // derrière la caméra (0,3).
    r.scene
        .upload_chunk_mesh(ChunkPos::new(0, -1), &quad(-2.0, 400.0, stone))
        .unwrap();
    r.scene
        .upload_chunk_mesh(ChunkPos::new(0, -3), &quad(-40.0, 1.0, stone))
        .unwrap();
    r.scene
        .upload_chunk_mesh(ChunkPos::new(0, 3), &quad(50.0, 1.0, stone))
        .unwrap();
    let cam = Camera {
        position: Vec3::new(8.0, 64.0, 8.0),
        ..Camera::default()
    };
    let clock = SkyClock::new();
    let input = frame(&cam, &clock);
    render_settled(&mut r, &input);
    let vis = r.scene.visible_chunks().to_vec();
    assert!(vis.contains(&ChunkPos::new(0, -1)));
    assert!(vis.contains(&ChunkPos::new(0, -3)));
    assert!(
        !vis.contains(&ChunkPos::new(0, 3)),
        "chunk derrière la caméra : {vis:?}"
    );
    assert!(r.scene.occlusion_visible(ChunkPos::new(0, -1)));
    assert!(!r.scene.occlusion_visible(ChunkPos::new(0, -3)));
    // Mur retiré : le chunk redevient visible après une frame de latence ou plus.
    r.scene.remove_chunk_mesh(ChunkPos::new(0, -1));
    render_settled(&mut r, &input);
    assert!(r.scene.occlusion_visible(ChunkPos::new(0, -3)));
    r.scene.clear_chunk_meshes();
    assert_eq!(r.scene.chunk_mesh_count(), 0);
    render_settled(&mut r, &input);
    assert!(r.scene.visible_chunks().is_empty());
    assert_no_validation_errors();
}

#[test]
fn resize_and_atlas_reload() {
    let Some((ctx, _g)) = gpu() else { return };
    let settings = GfxSettings {
        bloom: false,
        ..GfxSettings::default()
    };
    let mut r = OffscreenRenderer::new(ctx, assets(), 64, 48, &settings).unwrap();
    demo_world(&mut r);
    let cam = demo_camera();
    let clock = SkyClock::new();
    let a = r.render(&frame(&cam, &clock)).unwrap();
    assert_eq!(a.dimensions(), (64, 48));
    r.resize(101, 37).unwrap();
    let b = r.render(&frame(&cam, &clock)).unwrap();
    assert_eq!(b.dimensions(), (101, 37));
    r.scene
        .reload_block_atlas(TextureArrayCpu::fallback())
        .unwrap();
    assert_eq!(r.scene.chunk_mesh_count(), 0);
    let c = r.render(&frame(&cam, &clock)).unwrap();
    assert_eq!(c.dimensions(), (101, 37));
    // Projection suivant l'extent : la matrice reste finie (pas de NaN) même en 1×1.
    r.resize(1, 1).unwrap();
    r.render(&frame(&cam, &clock)).unwrap();
    assert_no_validation_errors();
}
