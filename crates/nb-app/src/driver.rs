//! Raccord jeu ↔ GPU commun à la fenêtre et au mode hors écran : création du [`Game`] et de
//! l'interface, demandes de la plateforme (réglages, packs, modèles de mobs), maillages, rendu
//! d'une frame (scène 3D + UI, ou UI seule).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::RgbaImage;
use nb_game::{Game, GameOptions, MeshAction, NbUi, PlatformRequest, Registries, UiFrame};
use nb_gfx::ui_pass::{UiImage, UiRenderer};
use nb_gfx::vk::{OverlayPass, HDR_DEPTH_FORMAT};
use nb_gfx::window::Renderer;
use nb_gfx::{
    packs, FrameInput, GfxError, GfxSettings, GpuContext, MobModelDesc, MobRoots,
    OffscreenRenderer, SceneRenderer, TextureArrayCpu,
};
use nb_mesh::BlockTable;

type Result<T> = std::result::Result<T, GfxError>;

/// Couleur d'effacement des frames d'interface seule (couverte par le fond de terre).
pub const UI_CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Ce dont le pilote a besoin d'un renderer (fenêtré ou hors écran).
pub trait Backend {
    fn scene(&self) -> &SceneRenderer;
    fn scene_mut(&mut self) -> &mut SceneRenderer;
    fn context(&self) -> Arc<GpuContext> {
        self.scene().context().clone()
    }
    fn apply_settings(&mut self, s: &GfxSettings) -> Result<()>;
    /// `reloadBlockTextures(nom)` + modèles de mobs avec les racines du pack.
    fn reload_resource_pack(&mut self, name: &str, mobs: &[MobModelDesc]) -> Result<()>;
    fn render_world(
        &mut self,
        input: &FrameInput<'_>,
        overlay: &mut dyn FnMut(&mut OverlayPass<'_>),
    ) -> Result<()>;
    fn render_ui(&mut self, overlay: &mut dyn FnMut(&mut OverlayPass<'_>)) -> Result<()>;
}

impl Backend for Renderer {
    fn scene(&self) -> &SceneRenderer {
        Renderer::scene(self)
    }
    fn scene_mut(&mut self) -> &mut SceneRenderer {
        Renderer::scene_mut(self)
    }
    fn apply_settings(&mut self, s: &GfxSettings) -> Result<()> {
        Renderer::apply_settings(self, s)
    }
    fn reload_resource_pack(&mut self, name: &str, mobs: &[MobModelDesc]) -> Result<()> {
        Renderer::reload_resource_pack(self, name, mobs).map(|_| ())
    }
    fn render_world(
        &mut self,
        input: &FrameInput<'_>,
        overlay: &mut dyn FnMut(&mut OverlayPass<'_>),
    ) -> Result<()> {
        self.render_frame(input, overlay)
    }
    fn render_ui(&mut self, overlay: &mut dyn FnMut(&mut OverlayPass<'_>)) -> Result<()> {
        self.render_ui_frame(UI_CLEAR, overlay)
    }
}

/// Renderer hors écran (mode `--headless`) : la dernière image rendue est gardée pour les
/// captures.
pub struct Headless {
    pub off: OffscreenRenderer,
    packs_root: PathBuf,
    pub last: Option<RgbaImage>,
}

impl Headless {
    pub fn new(packs_root: &Path, size: (u32, u32), mobs: &[MobModelDesc]) -> Result<Self> {
        let ctx = Arc::new(GpuContext::new_headless()?);
        let (assets, _) = nb_gfx::SceneAssets::load(packs_root, packs::DEFAULT_PACK, mobs);
        let off = OffscreenRenderer::new(ctx, assets, size.0, size.1, &GfxSettings::default())?;
        Ok(Self {
            off,
            packs_root: packs_root.to_path_buf(),
            last: None,
        })
    }
}

impl Backend for Headless {
    fn scene(&self) -> &SceneRenderer {
        &self.off.scene
    }
    fn scene_mut(&mut self) -> &mut SceneRenderer {
        &mut self.off.scene
    }
    fn apply_settings(&mut self, s: &GfxSettings) -> Result<()> {
        self.off.scene.apply_settings(s)
    }
    fn reload_resource_pack(&mut self, name: &str, mobs: &[MobModelDesc]) -> Result<()> {
        let (dir, _) = packs::resolve_block_textures(&self.packs_root, name);
        let scene = &mut self.off.scene;
        scene.reload_block_atlas(TextureArrayCpu::from_dir(&dir))?;
        scene.reload_mob_models(MobRoots::for_pack(&self.packs_root, name), mobs)
    }
    fn render_world(
        &mut self,
        input: &FrameInput<'_>,
        overlay: &mut dyn FnMut(&mut OverlayPass<'_>),
    ) -> Result<()> {
        self.last = Some(self.off.render_with_overlay(input, overlay)?);
        Ok(())
    }
    fn render_ui(&mut self, overlay: &mut dyn FnMut(&mut OverlayPass<'_>)) -> Result<()> {
        self.last = Some(self.off.render_ui_only(UI_CLEAR, overlay)?);
        Ok(())
    }
}

/// Table de maillage d'après les couches de l'atlas courant.
pub fn block_table(scene: &SceneRenderer, regs: &Registries) -> Arc<BlockTable> {
    let atlas = scene.block_atlas();
    Arc::new(BlockTable::from_registry(&regs.blocks, |n| atlas.layer(n)))
}

/// Couches d'atlas des icônes d'objets purs.
pub fn item_icons(scene: &SceneRenderer, regs: &Registries) -> Arc<nb_game::ItemIcons> {
    let atlas = scene.block_atlas();
    Arc::new(nb_game::ItemIcons::build(&regs.items, |n| atlas.layer(n)))
}

/// Jeu + rendu de l'interface.
pub struct Driver {
    pub game: Game,
    pub ui: UiRenderer,
    /// Dernière liste de dessin 2D (tests, captures).
    pub last_ui: UiFrame,
}

impl Driver {
    /// `Main.initEngine` côté plateforme : jeu (registres, réglages, monde du menu), modèles
    /// des mobs de `mobs.json`, interface nb-ui, textures de l'UI.
    pub fn new(b: &mut impl Backend, mut opts: GameOptions) -> Result<Self> {
        let regs = Registries::builtin();
        let table = block_table(b.scene(), &regs);
        if opts.window_size == (0, 0) {
            opts.window_size = (1280, 720);
        }
        let icons = item_icons(b.scene(), &regs);
        let mut game = Game::new(opts, regs, table);
        game.set_item_icons(icons);
        game.set_particle_layers(b.scene().particle_atlas());
        // Le renderer a été créé avec le registre intégré ; `Game` a pu lire `mobs.json`.
        if game.regs.mobs.to_json() != nb_data::MobRegistry::builtin().to_json() {
            let roots = b.scene().mob_roots().clone();
            let descs = MobModelDesc::from_registry(&game.regs.mobs);
            b.scene_mut().reload_mob_models(roots, &descs)?;
        }
        game.set_ui(Box::new(NbUi::new()));
        let images = nb_ui::textures::load_all().map_err(|e| GfxError::Other(e.to_string()))?;
        let ui_images: Vec<UiImage<'_>> = images
            .iter()
            .map(|i| UiImage {
                width: i.width(),
                height: i.height(),
                rgba: i.as_raw(),
            })
            .collect();
        let scene = b.scene();
        let ui = UiRenderer::new(
            b.context(),
            scene.output_format(),
            HDR_DEPTH_FORMAT,
            &ui_images,
            scene.block_atlas(),
        )?;
        Ok(Self {
            game,
            ui,
            last_ui: UiFrame::default(),
        })
    }

    /// Applique les demandes GPU du jeu ; renvoie celles qui concernent la fenêtre (curseur,
    /// plein écran, taille, fermeture).
    pub fn apply_requests(&mut self, b: &mut impl Backend) -> Vec<PlatformRequest> {
        let mut window = Vec::new();
        for r in self.game.take_requests() {
            match r {
                PlatformRequest::ApplyGraphics(s) => {
                    if let Err(e) = b.apply_settings(&s) {
                        eprintln!("réglages graphiques : {e}");
                    }
                }
                PlatformRequest::ReloadResourcePack(name) => {
                    let mobs = MobModelDesc::from_registry(&self.game.regs.mobs);
                    if let Err(e) = b.reload_resource_pack(&name, &mobs) {
                        eprintln!("pack {name} : {e}");
                    }
                    // « Re-génération des icônes » : nouvel atlas pour l'UI.
                    if let Err(e) = self.ui.set_block_atlas(b.scene().block_atlas()) {
                        eprintln!("icônes du pack {name} : {e}");
                    }
                    let table = block_table(b.scene(), &self.game.regs);
                    self.game.set_block_table(table);
                    let icons = item_icons(b.scene(), &self.game.regs);
                    self.game.set_item_icons(icons);
                }
                PlatformRequest::ReloadMobModels => {
                    // `applyAndLeave` : modèle rechargé pour chaque mob qui n'en a pas.
                    for cfg in self.game.regs.mobs.all() {
                        if b.scene().mob_model_counts(&cfg.id).is_none() {
                            let desc = MobModelDesc::from_config(cfg);
                            if let Err(e) = b.scene_mut().reload_mob_model(&desc) {
                                eprintln!("modèle du mob {} : {e}", cfg.id);
                            }
                        }
                    }
                }
                PlatformRequest::ReloadMobModel(id) => {
                    if let Some(cfg) = self.game.regs.mobs.get(&id) {
                        let desc = MobModelDesc::from_config(cfg);
                        if let Err(e) = b.scene_mut().reload_mob_model(&desc) {
                            eprintln!("modèle du mob {id} : {e}");
                        }
                    }
                }
                other => window.push(other),
            }
        }
        window
    }

    pub fn apply_meshes(&mut self, b: &mut impl Backend) {
        for a in self.game.take_mesh_actions() {
            match a {
                MeshAction::Upload(pos, mesh) => {
                    if let Err(e) = b.scene_mut().upload_chunk_mesh(pos, &mesh.vertices) {
                        eprintln!("upload du chunk {pos:?} : {e}");
                    }
                }
                MeshAction::Remove(pos) => b.scene_mut().remove_chunk_mesh(pos),
            }
        }
    }

    /// `stateManager.render()` + `chatController.render` : scène 3D si l'état la montre, puis
    /// l'interface ; sinon interface seule.
    pub fn render(&mut self, b: &mut impl Backend, time_seconds: f32) -> Result<()> {
        let frame = self.game.draw_ui();
        let screen = (frame.width as f32, frame.height as f32);
        let ui = &mut self.ui;
        let mut err = None;
        let mut overlay = |pass: &mut OverlayPass<'_>| {
            if let Err(e) = ui.record(pass, screen, &frame.vertices, &frame.draws) {
                err = Some(e);
            }
        };
        let g = &self.game;
        if g.state().renders_world() {
            let view = g.render_view();
            b.render_world(&view.frame_input(time_seconds), &mut overlay)?;
        } else {
            b.render_ui(&mut overlay)?;
        }
        self.last_ui = frame;
        match err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_game::{GameMode, GameState, NewWorld, UiCommand, TIME_STEP};

    fn backend(size: (u32, u32)) -> Option<Headless> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource_packs");
        let mobs = MobModelDesc::from_registry(&nb_data::MobRegistry::builtin());
        match Headless::new(&root, size, &mobs) {
            Ok(b) => Some(b),
            Err(e) => {
                if std::env::var("NB_GFX_REQUIRE_GPU").as_deref() == Ok("1") {
                    panic!("Vulkan indisponible : {e}");
                }
                eprintln!("SKIP : Vulkan indisponible ({e})");
                None
            }
        }
    }

    /// Menu (scène + UI), choix du monde (UI seule, fond de terre), jeu (scène + HUD + mob) :
    /// rendu réel sous lavapipe, sans erreur de validation.
    #[test]
    fn renders_menu_ui_only_and_world_frames() {
        let size = (640u32, 360u32);
        let Some(mut b) = backend(size) else { return };
        let dir = std::env::temp_dir().join(format!(
            "nb-app-driver-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{ "renderDistance": 2 }"#).unwrap();
        // `mobs.json` avec un mob personnalisé : son modèle doit être chargé au démarrage
        // (le renderer a été créé avec le registre intégré).
        let custom = |id: &str| {
            let mut c = nb_data::MobRegistry::builtin()
                .get("reindeer")
                .unwrap()
                .clone();
            c.id = id.into();
            c
        };
        let mut reg = nb_data::MobRegistry::builtin();
        reg.all_mut().push(custom("elan"));
        std::fs::write(dir.join("mobs.json"), reg.to_json()).unwrap();
        assert!(b.scene().mob_model_counts("elan").is_none());
        let mut opts = GameOptions::new(&dir);
        opts.window_size = size;
        opts.menu_seed = Some(7);
        opts.rng_seed = Some(7);
        opts.packs_root = Some(dir.clone());
        let mut d = Driver::new(&mut b, opts).unwrap();
        assert!(d.game.regs.mobs.get("elan").is_some(), "mobs.json lu");
        assert!(
            b.scene().mob_model_counts("elan").is_some(),
            "modèle chargé"
        );
        // Sortie de l'écran des mobs : modèle chargé pour chaque mob qui n'en a pas.
        let mut reg = d.game.regs.mobs.clone();
        reg.all_mut().push(custom("cerf"));
        d.game.command(UiCommand::OpenSettings);
        d.game
            .command(UiCommand::SwitchState(GameState::MobSettings));
        d.game.command(UiCommand::SetMobRegistry(Box::new(reg)));
        d.game.command(UiCommand::LeaveMobSettings);
        d.apply_requests(&mut b);
        assert!(
            b.scene().mob_model_counts("cerf").is_some(),
            "ReloadMobModels"
        );
        d.game.command(UiCommand::SettingsBack);
        let frame = |d: &mut Driver, b: &mut Headless| {
            d.game.update(TIME_STEP);
            d.apply_requests(b);
            d.apply_meshes(b);
            d.render(b, 0.0).unwrap();
            d.game.input.end_frame();
            b.last.clone().unwrap()
        };
        let menu = frame(&mut d, &mut b);
        assert!(!d.last_ui.is_empty(), "menu dessiné");
        assert_eq!(menu.dimensions(), size);

        d.game
            .command(UiCommand::SwitchState(GameState::WorldSelect));
        let ws = frame(&mut d, &mut b);
        // Bande centrale : terre teintée 0,32 sous un voile noir 0,55 → très sombre.
        let p = ws.get_pixel(320, 200).0;
        assert!(
            p[0] < 60 && p[1] < 60 && p[2] < 60,
            "fond de terre assombri : {p:?}"
        );

        d.game.command(UiCommand::CreateWorld(Box::new(NewWorld {
            name: "Pilote".into(),
            seed_text: "5".into(),
            game_mode: GameMode::Survival,
            allow_cheats: false,
            config: Default::default(),
        })));
        assert_eq!(d.game.state(), GameState::Playing);
        let cam = d.game.camera();
        let cfg = d.game.regs.mobs.get("reindeer").unwrap().clone();
        let mut rng = nb_core::JavaRandom::new(1);
        nb_ecs::spawn::spawn_mob(
            &mut d.game.session_mut().sim.entities,
            &cfg,
            f64::from(cam.position.x),
            f64::from(cam.position.y) - 1.0,
            f64::from(cam.position.z) - 4.0,
            &mut rng,
        );
        for _ in 0..20 {
            frame(&mut d, &mut b);
        }
        assert!(!d.game.render_view().gfx_mobs.is_empty());
        let Driver { game, ui, .. } = d;
        game.shutdown();
        drop(ui);
        drop(b);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(nb_gfx::vk::validation_error_count(), 0, "validation Vulkan");
    }
}
