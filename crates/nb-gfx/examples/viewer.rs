//! Visionneuse de démonstration (fenêtre winit + Vulkan) : terrain procédural maillé par
//! nb-mesh, cycle jour/nuit, ombres, brouillard, nuages, surbrillance.
//!
//! `cargo run -p nb-gfx --example viewer --release`
//!
//! Commandes : ZQSD/WASD + Espace/Maj pour voler, flèches ou glisser-souris (clic gauche)
//! pour tourner, T : +100 ticks, N : midi, B : bloom, O : ombres OFF/LOW/HIGH, V : vsync,
//! F11 : plein écran, Échap : quitter.

use std::collections::HashSet;
use std::time::Instant;

use glam::{IVec3, Vec3};
use nb_data::BlockRegistry;
use nb_gfx::window::{center_on_monitor, load_icon, set_fullscreen, window_attributes, Renderer};
use nb_gfx::{
    find_packs_root, Camera, ChunkPos, EyeFluid, FrameInput, GfxSettings, ShadowQuality, SkyClock,
};
use nb_mesh::{mesh_chunk, BlockTable, ChunkVoxels, Neighborhood, UniformBiome};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const RADIUS: i32 = 4;

fn height_at(x: i32, z: i32) -> i32 {
    let (x, z) = (x as f32, z as f32);
    (62.0 + 6.0 * (x * 0.07).sin() * (z * 0.05).cos() + 3.0 * ((x + z) * 0.13).sin()) as i32
}

fn build_world(r: &mut Renderer) {
    let reg = BlockRegistry::builtin();
    let ids = *reg.ids();
    let atlas = r.block_atlas().clone();
    let table = BlockTable::from_registry(&reg, |n| atlas.layer(n));
    let block = |x: i32, y: i32, z: i32| -> u8 {
        let h = height_at(x, z);
        if y < h - 3 {
            ids.stone
        } else if y < h {
            ids.dirt
        } else if y == h {
            if h <= 60 {
                ids.sand
            } else {
                ids.grass
            }
        } else if y <= 60 {
            ids.water
        } else if (x.rem_euclid(23) == 5 && z.rem_euclid(19) == 7) && y <= h + 4 && h > 61 {
            ids.wood
        } else {
            let tx = x - (x - 5).rem_euclid(23);
            let tz = z - (z - 7).rem_euclid(19);
            let th = height_at(tx + 5, tz + 7);
            let (dx, dz) = (x - tx - 5, z - tz - 7);
            if th > 61 && (th + 3..th + 7).contains(&y) && dx.abs() <= 2 && dz.abs() <= 2 {
                ids.leaves
            } else {
                0
            }
        }
    };
    let mut voxels = std::collections::HashMap::new();
    for cz in -RADIUS - 1..=RADIUS + 1 {
        for cx in -RADIUS - 1..=RADIUS + 1 {
            let mut v = ChunkVoxels::filled(0);
            for lx in 0..16usize {
                for lz in 0..16usize {
                    for y in 0..100usize {
                        let id = block(cx * 16 + lx as i32, y as i32, cz * 16 + lz as i32);
                        if id != 0 {
                            v.set(lx, y, lz, id);
                        }
                    }
                }
            }
            voxels.insert((cx, cz), v);
        }
    }
    for cz in -RADIUS..=RADIUS {
        for cx in -RADIUS..=RADIUS {
            let n: [Option<&ChunkVoxels>; 9] = std::array::from_fn(|i| {
                voxels.get(&(cx + (i % 3) as i32 - 1, cz + (i / 3) as i32 - 1))
            });
            let mesh = mesh_chunk(
                ChunkPos::new(cx, cz),
                &Neighborhood::from_voxels(n),
                &table,
                &UniformBiome(0),
            );
            r.upload_chunk_mesh(ChunkPos::new(cx, cz), &mesh.vertices)
                .expect("upload");
        }
    }
}

struct App {
    renderer: Option<Renderer>,
    window: Option<Window>,
    settings: GfxSettings,
    camera: Camera,
    clock: SkyClock,
    start: Instant,
    last: Instant,
    keys: HashSet<KeyCode>,
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    fullscreen: bool,
    /// `NB_VIEWER_FRAMES=n` : quitte après n frames (test de fumée, ex. Xvfb + lavapipe).
    frame_limit: Option<u64>,
    frames: u64,
}

impl App {
    fn update(&mut self, dt: f32) {
        self.clock.update(f64::from(dt));
        let k = |c| self.keys.contains(&c);
        let mut d_pitch = 0.0;
        let mut d_yaw = 0.0;
        if k(KeyCode::ArrowUp) {
            d_pitch -= 90.0 * dt;
        }
        if k(KeyCode::ArrowDown) {
            d_pitch += 90.0 * dt;
        }
        if k(KeyCode::ArrowLeft) {
            d_yaw -= 90.0 * dt;
        }
        if k(KeyCode::ArrowRight) {
            d_yaw += 90.0 * dt;
        }
        self.camera.rotate(d_pitch, d_yaw);
        let yaw = self.camera.yaw_deg.to_radians();
        let forward = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
        let right = Vec3::new(yaw.cos(), 0.0, yaw.sin());
        let mut m = Vec3::ZERO;
        if k(KeyCode::KeyW) {
            m += forward;
        }
        if k(KeyCode::KeyS) {
            m -= forward;
        }
        if k(KeyCode::KeyA) {
            m -= right;
        }
        if k(KeyCode::KeyD) {
            m += right;
        }
        if k(KeyCode::Space) {
            m += Vec3::Y;
        }
        if k(KeyCode::ShiftLeft) {
            m -= Vec3::Y;
        }
        self.camera.position += m * 12.0 * dt;
    }

    fn key(&mut self, code: KeyCode, el: &ActiveEventLoop) {
        let Some(r) = self.renderer.as_mut() else {
            return;
        };
        match code {
            KeyCode::Escape => el.exit(),
            KeyCode::KeyT => self.clock.set_time(self.clock.time_of_day + 100.0),
            KeyCode::KeyN => self.clock.set_time(900.0),
            KeyCode::KeyB => self.settings.bloom = !self.settings.bloom,
            KeyCode::KeyV => self.settings.vsync = !self.settings.vsync,
            KeyCode::KeyO => {
                self.settings.shadow_quality = match self.settings.shadow_quality {
                    ShadowQuality::Off => ShadowQuality::Low,
                    ShadowQuality::Low => ShadowQuality::High,
                    ShadowQuality::High => ShadowQuality::Off,
                }
            }
            KeyCode::F11 => {
                self.fullscreen = !self.fullscreen;
                if let Some(w) = &self.window {
                    set_fullscreen(w, self.fullscreen);
                }
            }
            _ => return,
        }
        r.apply_settings(&self.settings).expect("réglages");
        println!(
            "heure {:.0}, bloom {}, ombres {:?}, vsync {}",
            self.clock.time_of_day,
            self.settings.bloom,
            self.settings.shadow_quality,
            self.settings.vsync
        );
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let icon = std::fs::read("assets/textures/ui/icon.png")
            .ok()
            .and_then(|b| load_icon(&b));
        let window = el
            .create_window(window_attributes().with_window_icon(icon))
            .expect("fenêtre");
        center_on_monitor(&window);
        window.set_visible(true);
        let size = window.inner_size();
        let mut r = Renderer::new(
            &window,
            (size.width, size.height),
            &find_packs_root(),
            &self.settings,
        )
        .expect("renderer");
        println!("GPU : {}", r.context().device_name);
        build_world(&mut r);
        self.renderer = Some(r);
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                if let Some(r) = self.renderer.as_mut() {
                    r.resize(s.width, s.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        if !event.repeat {
                            self.key(code, el);
                        }
                        self.keys.insert(code);
                    } else {
                        self.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.dragging = state == ElementState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = (position.x, position.y);
                if let (true, Some(l)) = (self.dragging, self.last_cursor) {
                    self.camera
                        .rotate(((p.1 - l.1) * 0.2) as f32, ((p.0 - l.0) * 0.2) as f32);
                }
                self.last_cursor = Some(p);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32();
                self.last = now;
                self.update(dt);
                let target = self.camera.position.floor().as_ivec3() - IVec3::new(0, 2, 0);
                let input = FrameInput {
                    camera: &self.camera,
                    time_seconds: self.start.elapsed().as_secs_f32(),
                    sky_clock: &self.clock,
                    eye_fluid: if self.camera.position.y < 61.0 {
                        EyeFluid::Water {
                            color: Vec3::new(0.25, 0.45, 0.85),
                        }
                    } else {
                        EyeFluid::None
                    },
                    heat_intensity: 0.0,
                    highlight_block: Some(target),
                };
                if let Some(r) = self.renderer.as_mut() {
                    r.render_frame(&input, &mut |_| {}).expect("rendu");
                }
                self.frames += 1;
                if let Some(limit) = self.frame_limit {
                    if self.frames == limit / 2 {
                        // Redimensionnement + vsync : recréation de la swapchain.
                        if let Some(w) = &self.window {
                            let _ = w.request_inner_size(winit::dpi::PhysicalSize::new(640, 360));
                        }
                        self.key(KeyCode::KeyV, el);
                        self.key(KeyCode::KeyO, el);
                    }
                    if self.frames >= limit {
                        let errors = nb_gfx::vk::validation_error_count();
                        println!(
                            "{} frames rendues, erreurs de validation : {errors}",
                            self.frames
                        );
                        if errors > 0 {
                            std::process::exit(1);
                        }
                        el.exit();
                    }
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("boucle d'événements");
    let mut app = App {
        renderer: None,
        window: None,
        settings: GfxSettings::default(),
        camera: Camera {
            position: Vec3::new(0.0, 75.0, 20.0),
            pitch_deg: 15.0,
            ..Camera::default()
        },
        clock: SkyClock::new(),
        start: Instant::now(),
        last: Instant::now(),
        keys: HashSet::new(),
        dragging: false,
        last_cursor: None,
        fullscreen: false,
        frame_limit: std::env::var("NB_VIEWER_FRAMES")
            .ok()
            .and_then(|v| v.parse().ok()),
        frames: 0,
    };
    event_loop.run_app(&mut app).expect("exécution");
}
