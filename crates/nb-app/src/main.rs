//! Norrblock — plateforme : fenêtre winit, renderer Vulkan (nb-gfx), boucle fixe 60 Hz.
//! Toute la logique est dans nb-game (écrans nb-ui compris) ; ce binaire relaie événements,
//! demandes et rendu ([`driver::Driver`]).
//!
//! Options :
//! - `--saves <dossier>` (défaut `saves`) ;
//! - `--world <nom>` : charge ou crée le monde et démarre directement en jeu ;
//! - `--frames <n>` : quitte après n frames (test de fumée) ;
//! - `--script <fichier>` : entrées simulées ([`script`]), un tick fixe par frame ;
//! - `--headless` : rendu hors écran sans fenêtre (Vulkan sans surface) ;
//! - `--capture-dir <dossier>` : captures PNG du script (hors écran seulement) ;
//! - `--size <L>x<H>` : taille du rendu hors écran (défaut 1280x720).
//!
//! En mode `--frames`, `--script` ou `--headless`, le code de sortie est non nul si le script a
//! échoué ou si les couches de validation Vulkan ont signalé une erreur.

mod driver;
mod keymap;
mod script;

use std::path::PathBuf;
use std::time::Instant;

use driver::{Driver, Headless};
use nb_game::{
    FixedStep, GameMode, GameOptions, GameState, NewWorld, PlatformRequest, UiCommand, TIME_STEP,
};
use nb_gfx::window::{center_on_monitor, load_icon, set_fullscreen, window_attributes, Renderer};
use nb_gfx::{find_packs_root, GfxSettings, MobModelDesc};
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
    script: Option<PathBuf>,
    headless: bool,
    capture_dir: Option<PathBuf>,
    size: Option<(u32, u32)>,
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--saves" => a.saves = it.next().map(PathBuf::from),
            "--world" => a.world = it.next(),
            "--frames" => a.frames = it.next().and_then(|s| s.parse().ok()),
            "--script" => a.script = it.next().map(PathBuf::from),
            "--headless" => a.headless = true,
            "--capture-dir" => a.capture_dir = it.next().map(PathBuf::from),
            "--size" => {
                a.size = it.next().and_then(|s| {
                    let (w, h) = s.split_once('x')?;
                    Some((w.parse().ok()?, h.parse().ok()?))
                })
            }
            "-h" | "--help" => {
                println!(
                    "norrblock [--saves <dossier>] [--world <nom>] [--frames <n>] \
                     [--script <fichier>] [--headless] [--capture-dir <dossier>] [--size LxH]"
                );
                std::process::exit(0);
            }
            other => eprintln!("argument ignoré : {other}"),
        }
    }
    a
}

impl Args {
    fn checked(&self) -> bool {
        self.frames.is_some() || self.script.is_some() || self.headless
    }

    fn game_options(&self, packs_root: PathBuf, size: (u32, u32)) -> GameOptions {
        let mut opts = GameOptions::new(self.saves.clone().unwrap_or_else(|| "saves".into()));
        opts.packs_root = Some(packs_root);
        opts.window_size = size;
        opts
    }

    fn load_script(&self) -> Result<Option<script::Runner>, String> {
        let Some(path) = &self.script else {
            return Ok(None);
        };
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Some(script::Runner::new(script::parse(&text)?)))
    }
}

/// `--world <nom>` : charge ou crée le monde (créé en survie, triche autorisée).
fn start_in_world(d: &mut Driver, name: &str) {
    let g = &mut d.game;
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    let existing = g.worlds().iter().find(|m| m.name == name).cloned();
    g.command(match existing {
        Some(m) => UiCommand::PlayWorld(Box::new(m)),
        None => UiCommand::CreateWorld(Box::new(NewWorld {
            name: name.to_owned(),
            seed_text: String::new(),
            game_mode: GameMode::Survival,
            allow_cheats: true,
            config: Default::default(),
        })),
    });
}

/// Bilan de fin : erreurs de validation et échecs de script.
fn report(frames: u64, failures: usize) -> bool {
    let validation = nb_gfx::vk::validation_error_count();
    println!("frames : {frames} ; erreurs de validation Vulkan : {validation} ; échecs de script : {failures}");
    validation == 0 && failures == 0
}

/// [`find_packs_root`], affiché au démarrage (diagnostic d'un paquet mal assemblé : sans
/// dossier de packs, les blocs prennent la texture de repli).
fn packs_root() -> PathBuf {
    let root = find_packs_root();
    let found = if root.is_dir() { "" } else { " (introuvable)" };
    println!("packs de ressources : {}{found}", root.display());
    root
}

// ───────────────────────────────────────────────────────────────── hors écran

fn run_headless(args: &Args) -> bool {
    let size = args.size.unwrap_or((1280, 720));
    let packs_root = packs_root();
    let mobs = MobModelDesc::from_registry(&nb_data::MobRegistry::builtin());
    let mut b = match Headless::new(&packs_root, size, &mobs) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Vulkan hors écran indisponible : {e}");
            return false;
        }
    };
    let ctx = b.off.scene.context();
    println!(
        "GPU : {} (validation : {})",
        ctx.device_name, ctx.validation_enabled
    );
    let mut d = match Driver::new(&mut b, args.game_options(packs_root, size)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("démarrage impossible : {e}");
            return false;
        }
    };
    if let Some(name) = &args.world {
        start_in_world(&mut d, name);
    }
    let mut runner = match args.load_script() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("script : {e}");
            return false;
        }
    };
    if let Some(dir) = &args.capture_dir {
        let _ = std::fs::create_dir_all(dir);
    }
    let limit = args
        .frames
        .unwrap_or(if runner.is_some() { u64::MAX } else { 60 });
    let mut frames = 0u64;
    let mut ok = true;
    while frames < limit {
        let meshes = nb_gfx::SceneRenderer::chunk_mesh_count(&b.off.scene);
        let out = match &mut runner {
            Some(r) => r.before_frame(&mut d.game, meshes),
            None => Default::default(),
        };
        d.game.update(TIME_STEP);
        let quit = d.apply_requests(&mut b).contains(&PlatformRequest::Quit);
        d.apply_meshes(&mut b);
        let _ = d.game.take_events();
        if let Some(e) = d.game.take_error() {
            eprintln!("{e}");
        }
        let t = frames as f32 * TIME_STEP as f32;
        if let Err(e) = d.render(&mut b, t) {
            eprintln!("rendu : {e}");
            ok = false;
            break;
        }
        if let (Some(name), Some(img)) = (&out.capture, &b.last) {
            match &args.capture_dir {
                Some(dir) => {
                    let path = dir.join(format!("{name}.png"));
                    match img.save(&path) {
                        Ok(()) => println!(
                            "capture {} (état {:?}, {} chunks, {} mobs, {} particules)",
                            path.display(),
                            d.game.state(),
                            meshes,
                            d.game.render_view().gfx_mobs.len(),
                            d.game.render_view().gfx_particles.len()
                        ),
                        Err(e) => eprintln!("capture {} : {e}", path.display()),
                    }
                }
                None => eprintln!("capture {name} ignorée : pas de --capture-dir"),
            }
        }
        d.game.input.end_frame();
        frames += 1;
        if out.quit || quit || d.game.quit_requested() {
            break;
        }
        if runner.as_ref().is_some_and(|r| r.is_done()) {
            break;
        }
    }
    let failures = runner.as_ref().map_or(0, |r| r.failures.len());
    let Driver { game, ui, .. } = d;
    game.shutdown();
    drop(ui);
    drop(b);
    report(frames, failures) && ok
}

// ───────────────────────────────────────────────────────────────── fenêtre

struct Running {
    // Ordre de destruction : le pilote (UI GPU) puis le renderer, avant la fenêtre.
    driver: Option<Driver>,
    renderer: Renderer,
    window: Window,
    loop_: FixedStep,
    last: Instant,
    start: Instant,
    grabbed: bool,
    cursor: (f64, f64),
    frames: u64,
    script: Option<script::Runner>,
}

struct App {
    args: Args,
    run: Option<Running>,
    ok: bool,
}

impl App {
    fn start(&mut self, el: &ActiveEventLoop) -> Result<Running, String> {
        // Ressource intégrée comme dans le jar Java (`/textures/ui/icon.png`) : l'icône ne
        // dépend pas du dossier courant.
        let icon = load_icon(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/textures/ui/icon.png"
        )));
        let window = el
            .create_window(window_attributes().with_window_icon(icon))
            .map_err(|e| e.to_string())?;
        center_on_monitor(&window);
        window.set_visible(true);
        let size = window.inner_size();
        let packs_root = packs_root();
        let mob_models = MobModelDesc::from_registry(&nb_data::MobRegistry::builtin());
        let mut renderer = Renderer::new(
            &window,
            (size.width, size.height),
            &packs_root,
            &mob_models,
            &GfxSettings::default(),
        )
        .map_err(|e| e.to_string())?;
        let ctx = renderer.context();
        println!(
            "GPU : {} (validation : {})",
            ctx.device_name, ctx.validation_enabled
        );
        let opts = self
            .args
            .game_options(packs_root, (size.width, size.height));
        let mut driver = Driver::new(&mut renderer, opts).map_err(|e| e.to_string())?;
        if let Some(name) = &self.args.world {
            start_in_world(&mut driver, name);
        }
        let script = self.args.load_script()?;
        let now = Instant::now();
        Ok(Running {
            driver: Some(driver),
            renderer,
            window,
            loop_: FixedStep::new(),
            last: now,
            start: now,
            grabbed: false,
            cursor: (0.0, 0.0),
            frames: 0,
            script,
        })
    }
}

impl Running {
    fn driver(&mut self) -> &mut Driver {
        self.driver.as_mut().expect("jeu actif")
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

    fn apply_window_requests(&mut self, el: &ActiveEventLoop) {
        let d = self.driver.as_mut().expect("jeu actif");
        for r in d.apply_requests(&mut self.renderer) {
            match r {
                PlatformRequest::SetCursorGrab(on) => self.set_grab(on),
                PlatformRequest::SetFullscreen(on) => set_fullscreen(&self.window, on),
                PlatformRequest::SetWindowSize(w, h) => {
                    if self.window.fullscreen().is_none() {
                        let _ = self
                            .window
                            .request_inner_size(winit::dpi::PhysicalSize::new(w, h));
                    }
                }
                PlatformRequest::Quit => el.exit(),
                // Traitées par le pilote.
                _ => {}
            }
        }
    }

    /// Une frame : ticks fixes (un seul par frame en mode scripté), demandes, maillages, rendu.
    /// Renvoie vrai si la boucle doit s'arrêter.
    fn frame(&mut self, el: &ActiveEventLoop) -> bool {
        let now = Instant::now();
        let frame = (now - self.last).as_secs_f64();
        self.last = now;
        let meshes = self.renderer.scene().chunk_mesh_count();
        let mut stop = false;
        let mut t = self.start.elapsed().as_secs_f32();
        if let Some(mut s) = self.script.take() {
            let out = s.before_frame(&mut self.driver().game, meshes);
            self.driver().game.update(TIME_STEP);
            if let Some(name) = &out.capture {
                eprintln!("capture {name} ignorée en mode fenêtré (utiliser --headless)");
            }
            stop = out.quit || s.is_done();
            t = self.frames as f32 * TIME_STEP as f32;
            self.script = Some(s);
        } else {
            let n = self.loop_.advance(frame);
            for _ in 0..n {
                self.driver().game.update(TIME_STEP);
            }
        }
        self.apply_window_requests(el);
        let d = self.driver.as_mut().expect("jeu actif");
        d.apply_meshes(&mut self.renderer);
        let _ = d.game.take_events();
        if let Some(e) = d.game.take_error() {
            eprintln!("{e}");
        }
        if let Err(e) = d.render(&mut self.renderer, t) {
            eprintln!("rendu : {e}");
        }
        d.game.input.end_frame();
        self.frames += 1;
        stop || d.game.quit_requested()
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
                self.ok = false;
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
                r.driver().game.resize(s.width, s.height);
            }
            WindowEvent::Focused(false) => r.driver().game.input.release_all(),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    if let Some(k) = keymap::glfw_key(code) {
                        r.driver().game.input.key_event(k, pressed, event.repeat);
                    }
                }
                if pressed {
                    if let Some(text) = &event.text {
                        for c in text.chars().filter(|c| !c.is_control()) {
                            r.driver().game.input.char_event(c);
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(b) = keymap::glfw_button(button) {
                    r.driver()
                        .game
                        .input
                        .mouse_button(b, state == ElementState::Pressed);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if !r.grabbed {
                    r.cursor = (position.x, position.y);
                    r.driver().game.input.cursor_moved(position.x, position.y);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    // Pavés tactiles : ~ une ligne pour 40 px.
                    MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                r.driver().game.input.scroll(y);
            }
            WindowEvent::RedrawRequested => {
                let stop = r.frame(el);
                let limit = self.args.frames.is_some_and(|n| r.frames >= n);
                if stop || limit {
                    el.exit();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        let Some(r) = self.run.as_mut() else {
            return;
        };
        // Curseur capturé : position virtuelle non bornée (`GLFW_CURSOR_DISABLED`). Ignoré en
        // mode scripté (le script pilote la souris).
        if let DeviceEvent::MouseMotion { delta } = event {
            if r.grabbed && r.script.is_none() {
                r.cursor.0 += delta.0;
                r.cursor.1 += delta.1;
                let (x, y) = r.cursor;
                r.driver().game.input.cursor_moved(x, y);
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
            let failures = r.script.as_ref().map_or(0, |s| s.failures.len());
            if let Some(Driver { game, ui, .. }) = r.driver.take() {
                game.shutdown();
                drop(ui);
            }
            let frames = r.frames;
            drop(r);
            if self.args.checked() {
                self.ok &= report(frames, failures);
            }
        }
    }
}

fn main() {
    let args = parse_args();
    if args.headless {
        let ok = run_headless(&args);
        std::process::exit(if ok { 0 } else { 1 });
    }
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("boucle d'événements : {e}");
            std::process::exit(1);
        }
    };
    let mut app = App {
        args,
        run: None,
        ok: true,
    };
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    if !app.ok {
        std::process::exit(1);
    }
}
