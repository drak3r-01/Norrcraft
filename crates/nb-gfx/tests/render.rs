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
    sky_params, Camera, ChunkPos, ChunkVertex, ColorBox, EyeFluid, FrameInput, GfxSettings,
    GpuContext, MobInstance, MobModelDesc, OffscreenRenderer, ParticleInstance, SceneAssets,
    ShadowQuality, SkyClock, TextureArrayCpu,
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
    SceneAssets::load(&crate_dir().join("../../resource_packs"), "Default", &[]).0
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
        mobs: &[],
        particles: &[],
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

// ------------------------------------------------------------------ mobs et particules

fn no_bloom_no_shadow() -> GfxSettings {
    GfxSettings {
        shadow_quality: ShadowQuality::Off,
        bloom: false,
        ..GfxSettings::default()
    }
}

/// Caméra à (0, 70, 0) regardant vers −Z (horizontale).
fn mob_camera() -> Camera {
    Camera {
        position: Vec3::new(0.0, 70.0, 0.0),
        pitch_deg: 0.0,
        yaw_deg: 0.0,
        ..Camera::default()
    }
}

fn mob_at(id: &str, position: Vec3) -> MobInstance<'_> {
    MobInstance {
        model_id: id,
        position,
        yaw_rad: 0.0,
        pitch_rad: 0.0,
        missing_scale: Vec3::new(2.0, 2.0, 2.0),
    }
}

fn color_box(min: [f32; 3], size: [f32; 3], rgba: [f32; 4]) -> ColorBox {
    ColorBox {
        min_px: min,
        size_px: size,
        rgba,
    }
}

/// Couleur attendue au centre de l'écran pour une surface HDR `hdr` à `dist` (regard
/// horizontal à midi : ciel = mix(horizon, zenith, 0.05^0.6), pas de halo ni de nuages).
fn expected_center(hdr: Vec3, dist: f32) -> Vec3 {
    let sky = sky_params(900.0);
    let g = 0.05f32.powf(0.6);
    let fog_col = sky.horizon.lerp(sky.zenith, g);
    let fog = (-(dist * 0.002f32).powf(1.8)).exp();
    aces_gamma(fog_col.lerp(hdr, fog) * Vec3::new(1.15, 1.05, 0.95))
}

fn assert_pixel(img: &RgbaImage, x: u32, y: u32, expect: Vec3, what: &str) {
    let p = img.get_pixel(x, y);
    for k in 0..3 {
        assert!(
            (f32::from(p[k]) - expect[k]).abs() <= 2.0,
            "{what} canal {k} : {:?} vs {expect}",
            p.0
        );
    }
}

/// Mob en mode couleur : `texture(blanc) · teinte · luminosité`, sans éclairage ni gamma
/// (§9.6), plage opaque avec écriture de profondeur puis plage transparente mélangée (§14.1).
#[test]
fn color_mobs_match_cpu_reference() {
    let Some((ctx, _g)) = gpu() else { return };
    let mut r = OffscreenRenderer::new(ctx, assets(), W, H, &no_bloom_no_shadow()).unwrap();
    let solid = nb_gfx::mob_model::build_geometry(&[color_box(
        [-16.0, 0.0, -16.0],
        [32.0, 32.0, 32.0],
        [0.6, 0.3, 0.1, 1.0],
    )])
    .unwrap();
    r.scene.install_mob_model("solid", &solid).unwrap();
    // Boîte opaque verte au fond, boîte rouge à 50 % devant (plage transparente en fin de
    // tampon).
    let glass = nb_gfx::mob_model::build_geometry(&[
        color_box([-16.0, 0.0, -8.0], [32.0, 32.0, 16.0], [1.0, 0.0, 0.0, 0.5]),
        color_box(
            [-16.0, 0.0, -32.0],
            [32.0, 32.0, 16.0],
            [0.0, 0.8, 0.0, 1.0],
        ),
    ])
    .unwrap();
    assert_eq!((glass.opaque_count, glass.transparent_count), (36, 36));
    r.scene.install_mob_model("glass", &glass).unwrap();
    assert_eq!(r.scene.mob_model_counts("glass"), Some((36, 36)));

    let cam = mob_camera();
    let clock = SkyClock::new();
    let mobs = [mob_at("solid", Vec3::new(0.0, 69.0, -5.0))];
    let mut input = frame(&cam, &clock);
    input.mobs = &mobs;
    let img = r.render(&input).unwrap();
    save("mob_solid", &img);
    // Face +Z (luminosité 0.85) à 4 blocs.
    let expect = expected_center(Vec3::new(0.6, 0.3, 0.1) * 0.85, 4.0);
    assert_pixel(&img, W / 2, H / 2, expect, "mob opaque");

    let mobs = [mob_at("glass", Vec3::new(0.0, 69.0, -5.0))];
    input.mobs = &mobs;
    let img = r.render(&input).unwrap();
    save("mob_glass", &img);
    // Rouge à 50 % sur vert ; profondeur = boîte opaque (la transparente n'écrit pas).
    let hdr = Vec3::new(1.0, 0.0, 0.0) * 0.85 * 0.5 + Vec3::new(0.0, 0.8, 0.0) * 0.85 * 0.5;
    assert_pixel(
        &img,
        W / 2,
        H / 2,
        expected_center(hdr, 6.0),
        "mob transparent",
    );

    // Rotation : yaw = π (la face −Z, même luminosité, fait face à la caméra) ; un second
    // mob derrière la caméra ne change rien.
    let mut turned = mob_at("solid", Vec3::new(0.0, 69.0, -5.0));
    turned.yaw_rad = std::f32::consts::PI;
    let away = mob_at("solid", Vec3::new(0.0, 69.0, 50.0));
    let mobs = [turned, away];
    input.mobs = &mobs;
    let img = r.render(&input).unwrap();
    assert_pixel(&img, W / 2, H / 2, expect, "mob tourné");
    assert_no_validation_errors();
}

/// Écrit un PNG RGBA uni.
fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: [u8; 4]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    RgbaImage::from_pixel(w, h, image::Rgba(rgba))
        .save(path)
        .unwrap();
}

/// Cube manquant (damier magenta/noir mis à l'échelle de la config), forme vanilla texturée
/// depuis un pack, puis `reload_mob_model` (JEM ajouté au pack) et changement de pack.
#[test]
fn missing_vanilla_and_reloaded_mobs() {
    let Some((ctx, _g)) = gpu() else { return };
    let tmp = std::env::temp_dir().join(format!("nb-gfx-mobs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let entity = tmp.join("Default/assets/minecraft/textures/entity");
    write_png(&entity.join("pig/pig.png"), 64, 32, [20, 40, 230, 255]);
    let pig = MobModelDesc {
        id: "pig".into(),
        built_in: false,
        geometry: None,
    };
    let (a, _) = SceneAssets::load(&tmp, "Default", std::slice::from_ref(&pig));
    assert_eq!(a.mob_models["pig"].vertices.len(), 216);
    let mut r = OffscreenRenderer::new(ctx, a, W, H, &no_bloom_no_shadow()).unwrap();
    assert_eq!(r.scene.mob_model_counts("pig"), Some((216, 0)));
    assert_eq!(r.scene.mob_model_counts("inconnu"), None);
    let cam = mob_camera();
    let clock = SkyClock::new();

    // Cube manquant 2×2×2 à 3 blocs : damier magenta / noir.
    let mobs = [mob_at("inconnu", Vec3::new(0.0, 69.0, -4.0))];
    let mut input = frame(&cam, &clock);
    input.mobs = &mobs;
    let img = r.render(&input).unwrap();
    save("mob_missing", &img);
    let count = |img: &RgbaImage, f: &dyn Fn(&image::Rgba<u8>) -> bool| {
        img.pixels().filter(|p| f(p)).count()
    };
    let magenta = count(&img, &|p| p[0] > 150 && p[2] > 150 && p[1] < 80);
    let black = count(&img, &|p| p[0] < 40 && p[1] < 40 && p[2] < 40);
    assert!(
        magenta > 1000 && black > 1000,
        "damier : {magenta} / {black}"
    );

    // Cochon vanilla, texture bleue du pack (corps entre 7 et 15 px de haut, vu de face).
    let mobs = [mob_at("pig", Vec3::new(0.0, 69.4, -3.0))];
    input.mobs = &mobs;
    let img = r.render(&input).unwrap();
    save("mob_pig", &img);
    let c = img.get_pixel(W / 2, H / 2);
    assert!(c[2] > 150 && c[0] < 100, "cochon bleu : {c:?}");

    // JEM rouge ajouté au pack puis rechargement d'un seul modèle.
    let cem = tmp.join("Default/assets/minecraft/optifine/cem");
    std::fs::create_dir_all(&cem).unwrap();
    std::fs::write(
        cem.join("pig.jem"),
        r#"{"textureSize": [64, 32], "models": [{"boxes": [{"coordinates": [-16, 0, -16, 32, 32, 32], "color": [0.9, 0.1, 0.1, 1]}]}]}"#,
    )
    .unwrap();
    assert!(r.scene.reload_mob_model(&pig).unwrap());
    assert_eq!(r.scene.mob_model_counts("pig"), Some((36, 0)));
    let img = r.render(&input).unwrap();
    save("mob_pig_jem", &img);
    let c = img.get_pixel(W / 2, H / 2);
    assert!(c[0] > 150 && c[2] < 100, "JEM rouge : {c:?}");
    // Rien ne peut être construit (intégré sans géométrie) : rien n'est remplacé.
    let orca = MobModelDesc {
        id: "orca".into(),
        built_in: true,
        geometry: None,
    };
    assert!(!r.scene.reload_mob_model(&orca).unwrap());
    assert_eq!(r.scene.mob_model_counts("orca"), None);
    // Changement de pack : tous les modèles rechargés (plus de JEM ni de texture → damier).
    std::fs::remove_file(cem.join("pig.jem")).unwrap();
    std::fs::remove_dir_all(&entity).unwrap();
    r.scene
        .reload_mob_models(nb_gfx::MobRoots::for_pack(&tmp, "Autre"), &[pig])
        .unwrap();
    assert_eq!(r.scene.mob_model_counts("pig"), Some((216, 0)));
    assert_eq!(r.scene.mob_roots().pack_root, tmp.join("Autre"));
    let img = r.render(&input).unwrap();
    let c = img.get_pixel(W / 2, H / 2);
    assert!(c[1] < 40, "cochon en damier : {c:?}");
    drop(r);
    let _ = std::fs::remove_dir_all(&tmp);
    assert_no_validation_errors();
}

/// PNG 16×16 : moitié haute rouge, moitié basse verte.
fn red_over_green_png() -> Vec<u8> {
    let mut img = RgbaImage::from_pixel(16, 16, image::Rgba([0, 255, 0, 255]));
    for y in 0..8 {
        for x in 0..16 {
            img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Billboard face à la caméra, image **tête en bas** (UV inversées, §15), sans écriture de
/// profondeur : sur le ciel, le brouillard du composite l'efface (comportement Java).
#[test]
fn particles_billboard_upside_down() {
    let Some((ctx, _g)) = gpu() else { return };
    let png = red_over_green_png();
    let mut a = assets();
    a.particle_atlas = TextureArrayCpu::from_named(&[("test", Some(&png))]);
    let mut r = OffscreenRenderer::new(ctx, a, W, H, &no_bloom_no_shadow()).unwrap();
    assert_eq!(r.scene.particle_atlas().layer("test"), 0);
    let wall = nb_gfx::mob_model::build_geometry(&[color_box(
        [-160.0, -160.0, -16.0],
        [320.0, 320.0, 16.0],
        [0.2, 0.2, 0.2, 1.0],
    )])
    .unwrap();
    r.scene.install_mob_model("wall", &wall).unwrap();
    let cam = mob_camera();
    let clock = SkyClock::new();
    let particles = [ParticleInstance {
        position: Vec3::new(0.0, 70.0, -3.0),
        scale: 1.0,
        layer: 0,
        color: Vec3::ONE,
    }];
    let mut input = frame(&cam, &clock);
    input.particles = &particles;
    let sky_only = r.render(&frame(&cam, &clock)).unwrap();
    let over_sky = r.render(&input).unwrap();
    assert_eq!(
        sky_only, over_sky,
        "particule sur le ciel effacée par le brouillard"
    );

    let mobs = [mob_at("wall", Vec3::new(0.0, 70.0, -8.0))];
    input.mobs = &mobs;
    let img = r.render(&input).unwrap();
    save("particle_upside_down", &img);
    // Particule de 1 bloc à 3 blocs : ±15 px. Bas de l'écran = haut du PNG (rouge).
    let low = img.get_pixel(W / 2, H / 2 + 8);
    let high = img.get_pixel(W / 2, H / 2 - 8);
    assert!(low[0] > 150 && low[1] < 80, "bas : {low:?}");
    assert!(high[1] > 150 && high[0] < 80, "haut : {high:?}");
    // Teinte bleue pure : les deux moitiés deviennent noires.
    let tinted = [ParticleInstance {
        color: Vec3::new(0.0, 0.0, 1.0),
        ..particles[0]
    }];
    input.particles = &tinted;
    let img = r.render(&input).unwrap();
    let p = img.get_pixel(W / 2, H / 2 + 8);
    assert!(p[0] < 40 && p[1] < 40, "teinte : {p:?}");
    // Couche manquante (damier) : rendu sans erreur.
    let missing = [ParticleInstance {
        layer: r.scene.particle_atlas().missing_layer,
        ..particles[0]
    }];
    input.particles = &missing;
    r.render(&input).unwrap();
    assert_no_validation_errors();
}

/// Mobs et particules intégrés dans le monde de démonstration (inspection visuelle :
/// `renders/demo_mobs.png`), avec ombres et bloom, sans erreur de validation.
#[test]
fn demo_world_with_builtin_mobs_and_particles() {
    let Some((ctx, _g)) = gpu() else { return };
    let reg = nb_data::MobRegistry::builtin();
    let descs = MobModelDesc::from_registry(&reg);
    let (a, _) = SceneAssets::load(&crate_dir().join("../../resource_packs"), "Default", &descs);
    let mut r = OffscreenRenderer::new(ctx, a, W, H, &GfxSettings::default()).unwrap();
    demo_world(&mut r);
    for id in ["reindeer", "fish", "orca"] {
        assert!(r.scene.mob_model_counts(id).is_some(), "{id}");
    }
    let cam = Camera {
        position: Vec3::new(4.0, 63.5, 12.0),
        pitch_deg: 10.0,
        yaw_deg: 10.0,
        ..Camera::default()
    };
    let clock = SkyClock::new();
    let at = |id, x: f32, y: f32, z: f32, yaw: f32| {
        let c = reg.get(id).unwrap();
        MobInstance {
            model_id: id,
            position: Vec3::new(x, y, z),
            yaw_rad: yaw,
            pitch_rad: 0.0,
            missing_scale: Vec3::new(c.width_x, c.height, c.width_z),
        }
    };
    let mobs = [
        at("reindeer", 4.0, 61.0, 4.0, 0.8),
        at("fish", 5.0, 60.3, -8.0, 0.0),
        at("orca", 3.0, 60.0, -10.0, 1.2),
        MobInstance {
            model_id: "inconnu",
            ..at("reindeer", 7.5, 61.0, 6.0, 0.0)
        },
    ];
    let preg = nb_data::ParticleRegistry::builtin();
    let patlas = r.scene.particle_atlas().clone();
    assert_eq!(patlas.layer("leaf"), 0);
    assert_eq!(patlas.layer("lava_drop"), 1);
    let particles: Vec<_> = (0..30)
        .map(|i| {
            let ty = if i % 3 == 0 {
                preg.get("lava").unwrap()
            } else {
                preg.get("leaf").unwrap()
            };
            ParticleInstance {
                position: Vec3::new(1.0 + (i % 6) as f32 * 0.4, 62.0 + (i / 6) as f32 * 0.4, 7.0),
                scale: ty.scale,
                layer: patlas.layer(&ty.texture),
                color: Vec3::ONE,
            }
        })
        .collect();
    let mut input = frame(&cam, &clock);
    input.mobs = &mobs;
    input.particles = &particles;
    let base = render_settled(&mut r, &frame(&cam, &clock));
    let img = render_settled(&mut r, &input);
    save("demo_mobs", &img);
    // Bright buffer inchangé par la passe B2 (bloom actif) : de grosses particules devant le
    // ciel sont effacées par le brouillard et ne laissent aucun halo.
    let sky_particles: Vec<_> = particles
        .iter()
        .map(|p| ParticleInstance {
            position: p.position + Vec3::new(0.0, 4.0, 0.0),
            scale: 1.0,
            ..*p
        })
        .collect();
    let mut in_sky = frame(&cam, &clock);
    in_sky.particles = &sky_particles;
    assert_eq!(render_settled(&mut r, &in_sky), base, "halo parasite");
    let changed = base
        .pixels()
        .zip(img.pixels())
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 1000, "mobs invisibles ({changed} pixels changés)");

    // Renne vu de profil (caméra vers −X : la tête, côté +Z du modèle, est à gauche).
    let side = Camera {
        position: Vec3::new(8.0, 62.5, 4.0),
        pitch_deg: 5.0,
        yaw_deg: -90.0,
        ..Camera::default()
    };
    let mobs = [at("reindeer", 4.0, 61.0, 4.0, 0.0)];
    let mut input = frame(&side, &clock);
    input.mobs = &mobs;
    let img = render_settled(&mut r, &input);
    save("mob_reindeer_side", &img);
    let base = render_settled(&mut r, &frame(&side, &clock));
    let diff_x: Vec<u32> = (0..W)
        .filter(|&x| (0..H).any(|y| img.get_pixel(x, y) != base.get_pixel(x, y)))
        .collect();
    let (min_x, max_x) = (diff_x[0], *diff_x.last().unwrap());
    // Bois (y ≥ 31 px) au-dessus de la tête : pixels modifiés en haut de la moitié gauche.
    let top_left = (0..H / 3)
        .any(|y| (min_x..(min_x + max_x) / 2).any(|x| img.get_pixel(x, y) != base.get_pixel(x, y)));
    assert!(top_left, "tête / bois du renne attendus à gauche");
    assert_no_validation_errors();
}

/// Ciel nocturne : aurores (dominante verte/cyan au-dessus de l'horizon), étoiles, lune ; rien
/// de tout cela de jour. Les images 960×540 (`renders/night_*.png`) servent à l'inspection.
#[test]
fn night_sky_aurora_stars_and_moon() {
    let Some((ctx, _g)) = gpu() else { return };
    let (w, h) = (960, 540);
    let mut r = OffscreenRenderer::new(ctx, assets(), w, h, &GfxSettings::default()).unwrap();
    let views = [
        ("night_n", 0.0, 35.0),
        ("night_e", 90.0, 35.0),
        ("night_s", 180.0, 25.0),
        ("night_w", 270.0, 45.0),
        ("night_zenith", 0.0, 80.0),
        ("night_low", 45.0, 10.0),
    ];
    let mut best = 0.0f32;
    for (name, yaw, up) in views {
        let cam = Camera {
            position: Vec3::new(0.0, 70.0, 0.0),
            pitch_deg: -up,
            yaw_deg: yaw,
            ..Camera::default()
        };
        let mut clock = SkyClock::new();
        clock.set_time(300.0);
        clock.aurora_phase = 3.0;
        let mut input = frame(&cam, &clock);
        input.time_seconds = 40.0;
        let img = render_settled(&mut r, &input);
        save(name, &img);
        // Part de pixels « aurore » : vert nettement au-dessus du rouge et du bleu.
        let green = img
            .pixels()
            .filter(|p| p[1] > 60 && i32::from(p[1]) > i32::from(p[0]) + 25 && p[1] > p[2])
            .count() as f32
            / (w * h) as f32;
        best = best.max(green);
        let stars = img
            .pixels()
            .filter(|p| p[0] > 110 && p[1] > 110 && p[2] > 110)
            .count();
        eprintln!("{name} : aurore {green:.3}, pixels clairs {stars}");
    }
    assert!(best > 0.02, "aucune aurore visible (max {best})");

    // Le jour : ni aurore ni étoile, ciel inchangé sauf soleil.
    let cam = Camera {
        position: Vec3::new(0.0, 70.0, 0.0),
        pitch_deg: -35.0,
        yaw_deg: 0.0,
        ..Camera::default()
    };
    let clock = SkyClock::new();
    let day = render_settled(&mut r, &frame(&cam, &clock));
    save("day_sky_up", &day);
    let greens = day
        .pixels()
        .filter(|p| i32::from(p[1]) > i32::from(p[0]) + 25 && p[1] > p[2])
        .count();
    assert_eq!(greens, 0, "aurore de jour");
    assert_no_validation_errors();
}

/// Mesure indicative (lavapipe, CPU) du coût du post-traitement la nuit :
/// `cargo test -p nb-gfx --test render bench_night_sky -- --ignored --nocapture`.
#[test]
#[ignore]
fn bench_night_sky() {
    let Some((ctx, _g)) = gpu() else { return };
    let mut r = OffscreenRenderer::new(ctx, assets(), 960, 540, &GfxSettings::default()).unwrap();
    let cam = Camera {
        position: Vec3::new(0.0, 70.0, 0.0),
        pitch_deg: -35.0,
        yaw_deg: 0.0,
        ..Camera::default()
    };
    let mut clock = SkyClock::new();
    clock.set_time(300.0);
    let input = frame(&cam, &clock);
    for _ in 0..3 {
        r.render(&input).unwrap();
    }
    let t = std::time::Instant::now();
    for _ in 0..20 {
        r.render(&input).unwrap();
    }
    eprintln!("nuit : {:.1} ms / frame", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
    clock.set_time(900.0);
    let input = frame(&cam, &clock);
    let t = std::time::Instant::now();
    for _ in 0..20 {
        r.render(&input).unwrap();
    }
    eprintln!("jour : {:.1} ms / frame", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
}

/// Soleil bas sur l'horizon (disque + halo) : image d'inspection `renders/sunset_sun.png`.
#[test]
fn sunset_sun_disc() {
    let Some((ctx, _g)) = gpu() else { return };
    let mut r = OffscreenRenderer::new(ctx, assets(), 960, 540, &GfxSettings::default()).unwrap();
    let mut clock = SkyClock::new();
    clock.set_time(1165.0);
    let sky = clock.params();
    // Regarde vers le soleil (direction opposée à la lumière), un peu au-dessus de l'horizon.
    let sun = -sky.shadow_light_dir;
    let yaw = sun.x.atan2(-sun.z).to_degrees();
    let cam = Camera {
        position: Vec3::new(0.0, 70.0, 0.0),
        pitch_deg: -sun.y.asin().to_degrees(),
        yaw_deg: yaw,
        ..Camera::default()
    };
    let img = render_settled(&mut r, &frame(&cam, &clock));
    save("sunset_sun", &img);
    let bright = img.pixels().filter(|p| p[0] > 245 && p[1] > 225).count();
    assert!(bright > 50, "disque solaire absent ({bright})");
    assert_no_validation_errors();
}
