//! Norrblock — plateforme : fenêtre winit, renderer Vulkan (nb-gfx), boucle fixe 60 Hz.
//! Toute la logique est dans nb-game ; ce binaire ne fait que relayer événements et rendu.
//!
//! Options : `--saves <dossier>` (défaut `saves`), `--world <nom>` (charge ou crée le monde et
//! démarre directement en jeu), `--frames <n>` (quitte après n frames, test de fumée).
//!
//! L'interface (nb-ui) n'est pas encore branchée : [`nb_game::StubUi`] fournit des écrans au
//! clavier, sans rendu (voir sa doc).

mod keymap;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use nb_game::{
    FixedStep, Game, GameMode, GameOptions, GameState, MeshAction, NewWorld, PlatformRequest,
    Registries, StubUi, UiCommand, TIME_STEP,
};
use nb_gfx::window::{center_on_monitor, load_icon, set_fullscreen, window_attributes, Renderer};
use nb_gfx::{find_packs_root, GfxSettings};
use nb_mesh::BlockTable;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{CursorGrabMode, Window, WindowId};

#[derive(Debug, Default)]
struct Args {
    saves: Option<PathBuf>,
    world: Option<String>,
    frames: Option<u64>,
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--saves" => a.saves = it.next().map(PathBuf::from),
            "--world" => a.world = it.next(),
            "--frames" => a.frames = it.next().and_then(|s| s.parse().ok()),
            "-h" | "--help" => {
                println!("norrblock [--saves <dossier>] [--world <nom>] [--frames <n>]");
                std::process::exit(0);
            }
            other => eprintln!("argument ignoré : {other}"),
        }
    }
    a
}

/// Table de maillage d'après les layers de l'atlas courant du renderer.
fn block_table(r: &Renderer, regs: &Registries) -> Arc<BlockTable> {
    let atlas = r.block_atlas();
    Arc::new(BlockTable::from_registry(&regs.blocks, |n| atlas.layer(n)))
}

struct Running {
    // Ordre de destruction : le renderer avant la fenêtre.
    game: Option<Game>,
    renderer: Renderer,
    window: Window,
    loop_: FixedStep,
    last: Instant,
    start: Instant,
    grabbed: bool,
    cursor: (f64, f64),
    frames: u64,
}

struct App {
    args: Args,
    run: Option<Running>,
}

impl App {
    fn start(&mut self, el: &ActiveEventLoop) -> Result<Running, String> {
        let icon = std::fs::read("assets/textures/ui/icon.png")
            .ok()
            .and_then(|b| load_icon(&b));
        let window = el
            .create_window(window_attributes().with_window_icon(icon))
            .map_err(|e| e.to_string())?;
        center_on_monitor(&window);
        window.set_visible(true);
        let size = window.inner_size();
        let packs_root = find_packs_root();
        let regs = Registries::builtin();
        let mob_models = nb_gfx::MobModelDesc::from_registry(&regs.mobs);
        let renderer = Renderer::new(
            &window,
            (size.width, size.height),
            &packs_root,
            &mob_models,
            &GfxSettings::default(),
        )
        .map_err(|e| e.to_string())?;
        println!("GPU : {}", renderer.context().device_name);
        let table = block_table(&renderer, &regs);
        let mut opts = GameOptions::new(self.args.saves.clone().unwrap_or_else(|| "saves".into()));
        opts.packs_root = Some(packs_root);
        opts.window_size = (size.width, size.height);
        let mut game = Game::new(opts, regs, table);
        game.set_ui(Box::new(StubUi::new()));
        if let Some(name) = &self.args.world {
            game.command(UiCommand::SwitchState(GameState::WorldSelect));
            let existing = game.worlds().iter().find(|m| &m.name == name).cloned();
            game.command(match existing {
                Some(m) => UiCommand::PlayWorld(Box::new(m)),
                None => UiCommand::CreateWorld(Box::new(NewWorld {
                    name: name.clone(),
                    seed_text: String::new(),
                    game_mode: GameMode::Survival,
                    allow_cheats: true,
                    config: Default::default(),
                })),
            });
        }
        let now = Instant::now();
        Ok(Running {
            game: Some(game),
            renderer,
            window,
            loop_: FixedStep::new(),
            last: now,
            start: now,
            grabbed: false,
            cursor: (0.0, 0.0),
            frames: 0,
        })
    }
}

impl Running {
    fn game(&mut self) -> &mut Game {
        self.game.as_mut().expect("jeu actif")
    }

    fn set_grab(&mut self, on: bool) {
        if on {
            let ok = self
                .window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Confined));
            if let Err(e) = ok {
                eprintln!("capture du curseur impossible : {e}");
            }
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        }
        self.window.set_cursor_visible(!on);
        self.grabbed = on;
    }

    fn apply_requests(&mut self, el: &ActiveEventLoop) {
        let requests = self.game().take_requests();
        for r in requests {
            match r {
                PlatformRequest::SetCursorGrab(on) => self.set_grab(on),
                PlatformRequest::ApplyGraphics(s) => {
                    if let Err(e) = self.renderer.apply_settings(&s) {
                        eprintln!("réglages graphiques : {e}");
                    }
                }
                PlatformRequest::SetFullscreen(on) => set_fullscreen(&self.window, on),
                PlatformRequest::SetWindowSize(w, h) => {
                    if self.window.fullscreen().is_none() {
                        let _ = self
                            .window
                            .request_inner_size(winit::dpi::PhysicalSize::new(w, h));
                    }
                }
                PlatformRequest::ReloadResourcePack(name) => {
                    let mob_models = {
                        let g = self.game.as_ref().expect("jeu actif");
                        nb_gfx::MobModelDesc::from_registry(&g.regs.mobs)
                    };
                    if let Err(e) = self.renderer.reload_resource_pack(&name, &mob_models) {
                        eprintln!("pack {name} : {e}");
                    }
                    let table = {
                        let g = self.game.as_ref().expect("jeu actif");
                        block_table(&self.renderer, &g.regs)
                    };
                    self.game().set_block_table(table);
                }
                // Le rendu des mobs n'est pas encore porté dans nb-gfx.
                PlatformRequest::ReloadMobModels => {}
                PlatformRequest::Quit => el.exit(),
            }
        }
    }

    fn apply_meshes(&mut self) {
        for a in self.game().take_mesh_actions() {
            match a {
                MeshAction::Upload(pos, mesh) => {
                    if let Err(e) = self.renderer.upload_chunk_mesh(pos, &mesh.vertices) {
                        eprintln!("upload du chunk {pos:?} : {e}");
                    }
                }
                MeshAction::Remove(pos) => self.renderer.remove_chunk_mesh(pos),
            }
        }
    }

    fn frame(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let frame = (now - self.last).as_secs_f64();
        self.last = now;
        let n = self.loop_.advance(frame);
        for _ in 0..n {
            self.game().update(TIME_STEP);
        }
        self.apply_requests(el);
        self.apply_meshes();
        let _ = self.game().take_events();
        if let Some(e) = self.game().take_error() {
            eprintln!("{e}");
        }
        let t = self.start.elapsed().as_secs_f32();
        let g = self.game.as_ref().expect("jeu actif");
        let view = g.render_view();
        if let Err(e) = self
            .renderer
            .render_frame(&view.frame_input(t), &mut |_overlay| {})
        {
            eprintln!("rendu : {e}");
        }
        self.game().input.end_frame();
        self.frames += 1;
        if self.game().quit_requested() {
            el.exit();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.run.is_some() {
            return;
        }
        match self.start(el) {
            Ok(r) => {
                self.run = Some(r);
                el.set_control_flow(ControlFlow::Poll);
            }
            Err(e) => {
                eprintln!("démarrage impossible : {e}");
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = self.run.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                r.renderer.resize(s.width, s.height);
                r.game().resize(s.width, s.height);
            }
            WindowEvent::Focused(false) => r.game().input.release_all(),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    if let Some(k) = keymap::glfw_key(code) {
                        r.game().input.key_event(k, pressed, event.repeat);
                    }
                }
                if pressed {
                    if let Some(text) = &event.text {
                        for c in text.chars().filter(|c| !c.is_control()) {
                            r.game().input.char_event(c);
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(b) = keymap::glfw_button(button) {
                    r.game()
                        .input
                        .mouse_button(b, state == ElementState::Pressed);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if !r.grabbed {
                    r.cursor = (position.x, position.y);
                    r.game().input.cursor_moved(position.x, position.y);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    // Pavés tactiles : ~ une ligne pour 40 px.
                    MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                r.game().input.scroll(y);
            }
            WindowEvent::RedrawRequested => {
                r.frame(el);
                if let Some(limit) = self.args.frames {
                    if r.frames >= limit {
                        el.exit();
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        let Some(r) = self.run.as_mut() else {
            return;
        };
        // Curseur capturé : position virtuelle non bornée (`GLFW_CURSOR_DISABLED`).
        if let DeviceEvent::MouseMotion { delta } = event {
            if r.grabbed {
                r.cursor.0 += delta.0;
                r.cursor.1 += delta.1;
                let (x, y) = r.cursor;
                r.game().input.cursor_moved(x, y);
            }
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(r) = &self.run {
            r.window.request_redraw();
        }
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        // `Main.cleanup` : sauvegarde du monde nommé, arrêt des threads, puis GPU.
        if let Some(mut r) = self.run.take() {
            if let Some(g) = r.game.take() {
                g.shutdown();
            }
            drop(r);
        }
    }
}

fn main() {
    let args = parse_args();
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("boucle d'événements : {e}");
            std::process::exit(1);
        }
    };
    let mut app = App { args, run: None };
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
