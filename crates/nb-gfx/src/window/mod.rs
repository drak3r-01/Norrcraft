//! Partie fenêtrée (feature `window`) : surface + swapchain autour du [`SceneRenderer`], et
//! aides winit pour la fenêtre Java (§3.1). Toute la logique de rendu est dans [`crate::vk`],
//! testable hors écran.

mod swapchain;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub use swapchain::{choose_format, choose_present_mode, surface_extent, Swapchain};

use crate::atlas::TextureArrayCpu;
use crate::frame::{FrameInput, GfxSettings};
use crate::mob_model::{MobModelDesc, MobRoots};
use crate::packs::{self, ResourcePackMeta};
use crate::vk::{
    GfxError, GpuContext, OutputTarget, OverlayPass, Result, SceneAssets, SceneRenderer,
    FRAMES_IN_FLIGHT,
};
use crate::ChunkPos;
use crate::ChunkVertex;

/// Renderer fenêtré (contrat §19).
pub struct Renderer {
    scene: SceneRenderer,
    swapchain: Option<Swapchain>,
    surface: vk::SurfaceKHR,
    /// Un sémaphore d'acquisition par frame en vol.
    acquire: Vec<vk::Semaphore>,
    /// Un sémaphore de fin de rendu par image de la swapchain.
    present: Vec<vk::Semaphore>,
    size: (u32, u32),
    vsync: bool,
    needs_recreate: bool,
    packs_root: PathBuf,
    pack_meta: ResourcePackMeta,
    ctx: Arc<GpuContext>,
}

fn semaphore(ctx: &GpuContext) -> Result<vk::Semaphore> {
    // SAFETY : device valide.
    Ok(unsafe {
        ctx.device
            .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?
    })
}

impl Renderer {
    /// Crée le contexte Vulkan pour `window`, la swapchain et charge le pack « Default »
    /// (atlas de blocs et modèles des `mobs`, §5.2 init).
    pub fn new(
        window: &(impl HasWindowHandle + HasDisplayHandle),
        size: (u32, u32),
        packs_root: &Path,
        mobs: &[MobModelDesc],
        settings: &GfxSettings,
    ) -> Result<Self> {
        let display = window
            .display_handle()
            .map_err(|e| GfxError::Other(e.to_string()))?
            .as_raw();
        let handle = window
            .window_handle()
            .map_err(|e| GfxError::Other(e.to_string()))?
            .as_raw();
        let exts = ash_window::enumerate_required_extensions(display)?;
        let (ctx, surface) = GpuContext::with_surface(exts, &|entry, instance| {
            // SAFETY : handles valides tant que la fenêtre vit (elle survit au renderer).
            Ok(unsafe { ash_window::create_surface(entry, instance, display, handle, None)? })
        })?;
        let ctx = Arc::new(ctx);
        let (assets, pack_meta) = SceneAssets::load(packs_root, packs::DEFAULT_PACK, mobs);
        let mut scene =
            SceneRenderer::new(ctx.clone(), assets, crate::vk::OUTPUT_FORMAT, settings)?;
        // L'occlusion culling temporel laissait voir sous le relief : désactivé par défaut
        // (`NB_OCCLUSION=1` pour le réactiver).
        scene.set_occlusion_culling(std::env::var_os("NB_OCCLUSION").is_some_and(|v| v != "0"));
        let swapchain = Swapchain::new(&ctx, surface, size, settings.vsync, None)?;
        if let Some(sc) = &swapchain {
            if sc.format != scene.output_format() {
                return Err(GfxError::Other(format!(
                    "format de swapchain {:?} non pris en charge",
                    sc.format
                )));
            }
        }
        let acquire = (0..FRAMES_IN_FLIGHT)
            .map(|_| semaphore(&ctx))
            .collect::<Result<_>>()?;
        let mut r = Self {
            scene,
            swapchain,
            surface,
            acquire,
            present: Vec::new(),
            size,
            vsync: settings.vsync,
            needs_recreate: false,
            packs_root: packs_root.to_path_buf(),
            pack_meta,
            ctx,
        };
        r.sync_present_semaphores()?;
        Ok(r)
    }

    fn sync_present_semaphores(&mut self) -> Result<()> {
        let n = self.swapchain.as_ref().map_or(0, |s| s.images.len());
        while self.present.len() < n {
            self.present.push(semaphore(&self.ctx)?);
        }
        Ok(())
    }

    fn recreate_swapchain(&mut self) -> Result<()> {
        self.ctx.wait_idle();
        let new = Swapchain::new(
            &self.ctx,
            self.surface,
            self.size,
            self.vsync,
            self.swapchain.as_ref(),
        )?;
        if let Some(mut old) = self.swapchain.take() {
            old.destroy(&self.ctx);
        }
        self.swapchain = new;
        self.needs_recreate = false;
        self.sync_present_semaphores()
    }

    pub fn context(&self) -> &Arc<GpuContext> {
        &self.ctx
    }

    pub fn scene(&self) -> &SceneRenderer {
        &self.scene
    }

    pub fn scene_mut(&mut self) -> &mut SceneRenderer {
        &mut self.scene
    }

    /// Taille du framebuffer (pixels). `0` suspend le rendu (§3.3, déviation obligatoire).
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.size != (width, height) {
            self.size = (width, height);
            self.needs_recreate = true;
        }
    }

    pub fn apply_settings(&mut self, s: &GfxSettings) -> Result<()> {
        if s.vsync != self.vsync {
            self.vsync = s.vsync;
            self.needs_recreate = true;
        }
        self.scene.apply_settings(s)
    }

    pub fn block_atlas(&self) -> &TextureArrayCpu {
        self.scene.block_atlas()
    }

    /// Atlas des particules (couches de [`crate::ParticleInstance::layer`]).
    pub fn particle_atlas(&self) -> &TextureArrayCpu {
        self.scene.particle_atlas()
    }

    /// Méta du pack actif.
    pub fn resource_pack(&self) -> &ResourcePackMeta {
        &self.pack_meta
    }

    /// `reloadBlockTextures(nom)` (§5.2) : recharge l'atlas, vide tous les maillages (l'appelant
    /// doit remailler et régénérer les icônes) puis recharge tous les modèles de mobs avec
    /// les racines `(packs_root/nom, packs_root/Default)`.
    pub fn reload_resource_pack(
        &mut self,
        name: &str,
        mobs: &[MobModelDesc],
    ) -> Result<ResourcePackMeta> {
        let (dir, meta) = packs::resolve_block_textures(&self.packs_root, name);
        self.scene
            .reload_block_atlas(TextureArrayCpu::from_dir(&dir))?;
        self.scene
            .reload_mob_models(MobRoots::for_pack(&self.packs_root, name), mobs)?;
        self.pack_meta = meta.clone();
        Ok(meta)
    }

    /// `reloadMobModel(id)` : recharge un seul modèle avec les racines actives (l'ancien est
    /// gardé si rien ne peut être construit). Retourne `true` si remplacé.
    pub fn reload_mob_model(&mut self, desc: &MobModelDesc) -> Result<bool> {
        self.scene.reload_mob_model(desc)
    }

    pub fn upload_chunk_mesh(&mut self, pos: ChunkPos, vertices: &[ChunkVertex]) -> Result<()> {
        self.scene.upload_chunk_mesh(pos, vertices)
    }

    pub fn remove_chunk_mesh(&mut self, pos: ChunkPos) {
        self.scene.remove_chunk_mesh(pos);
    }

    pub fn clear_chunk_meshes(&mut self) {
        self.scene.clear_chunk_meshes();
    }

    /// Rend et présente une frame. Ne fait rien si la fenêtre est de taille nulle ; recrée la
    /// swapchain (et les cibles) dès que l'extent change.
    pub fn render_frame(
        &mut self,
        input: &FrameInput<'_>,
        overlay: &mut dyn FnMut(&mut OverlayPass<'_>),
    ) -> Result<()> {
        self.present_frame(&mut |scene, token, out| {
            scene.record_frame(token, input, out, &mut *overlay)
        })
    }

    /// Frame d'interface seule (pas de scène 3D : [`SceneRenderer::record_ui_frame`]), image
    /// effacée à `clear` avant l'overlay.
    pub fn render_ui_frame(
        &mut self,
        clear: [f32; 4],
        overlay: &mut dyn FnMut(&mut OverlayPass<'_>),
    ) -> Result<()> {
        self.present_frame(&mut |scene, token, out| {
            scene.record_ui_frame(token, out, clear, &mut *overlay)
        })
    }

    fn present_frame(
        &mut self,
        record: &mut dyn FnMut(
            &mut SceneRenderer,
            crate::vk::FrameToken,
            &OutputTarget,
        ) -> Result<()>,
    ) -> Result<()> {
        if self.size.0 == 0 || self.size.1 == 0 {
            return Ok(());
        }
        if self.needs_recreate || self.swapchain.is_none() {
            self.recreate_swapchain()?;
        }
        let Some(sc) = &self.swapchain else {
            return Ok(());
        };
        let perf = std::env::var_os("NB_PERF").is_some_and(|v| v != "0");
        let t_start = std::time::Instant::now();
        let slot = self.scene.next_slot();
        self.scene.wait_slot(slot)?;
        let t_wait = t_start.elapsed();
        // SAFETY : le sémaphore de ce créneau n'est plus en attente (fence attendu).
        let acquired = unsafe {
            sc.loader
                .acquire_next_image(sc.raw, u64::MAX, self.acquire[slot], vk::Fence::null())
        };
        let index = match acquired {
            Ok((i, suboptimal)) => {
                self.needs_recreate |= suboptimal;
                i as usize
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.needs_recreate = true;
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        let t_acquire = t_start.elapsed();
        let out = OutputTarget {
            image: sc.images[index],
            view: sc.views[index],
            extent: sc.extent,
            final_state: crate::vk::Transition::PRESENT,
        };
        let token = self.scene.begin_frame()?;
        let t_begin = t_start.elapsed();
        record(&mut self.scene, token, &out)?;
        let t_record = t_start.elapsed();
        let waits = [vk::SemaphoreSubmitInfo::default()
            .semaphore(self.acquire[slot])
            .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
        let signals = [vk::SemaphoreSubmitInfo::default()
            .semaphore(self.present[index])
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
        self.scene.submit_frame(token, &waits, &signals)?;
        let t_submit = t_start.elapsed();
        let sc = self.swapchain.as_ref().expect("swapchain");
        let wait = [self.present[index]];
        let swapchains = [sc.raw];
        let indices = [index as u32];
        let info = vk::PresentInfoKHR::default()
            .wait_semaphores(&wait)
            .swapchains(&swapchains)
            .image_indices(&indices);
        match self.ctx.present(&sc.loader, &info) {
            Ok(suboptimal) => self.needs_recreate |= suboptimal,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.needs_recreate = true,
            Err(e) => return Err(e.into()),
        }
        if perf && t_start.elapsed().as_millis() > 30 {
            let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
            eprintln!(
                "[perf-gfx] attente créneau {:.1} | acquire {:.1} | begin_frame {:.1} | enregistrement {:.1} | submit {:.1} | present {:.1} ms",
                ms(t_wait),
                ms(t_acquire - t_wait),
                ms(t_begin - t_acquire),
                ms(t_record - t_begin),
                ms(t_submit - t_record),
                ms(t_start.elapsed() - t_submit)
            );
        }
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.ctx.wait_idle();
        if let Some(mut sc) = self.swapchain.take() {
            sc.destroy(&self.ctx);
        }
        // SAFETY : GPU inactif ; la surface est détruite avant l'instance (contexte).
        unsafe {
            for s in self.acquire.drain(..).chain(self.present.drain(..)) {
                self.ctx.device.destroy_semaphore(s, None);
            }
            if let Some(l) = self.ctx.surface_loader() {
                l.destroy_surface(self.surface, None);
            }
        }
    }
}

// ------------------------------------------------------------------------ winit

/// Taille initiale de la fenêtre Java.
pub const DEFAULT_SIZE: (u32, u32) = (1280, 720);
/// Titre de la fenêtre.
pub const TITLE: &str = "Norrblock";

/// Attributs de la fenêtre (§3.1) : « Norrblock », redimensionnable, maximisée par défaut
/// (1280×720 une fois restaurée), créée invisible (à centrer avec [`center_on_monitor`] puis afficher).
pub fn window_attributes() -> winit::window::WindowAttributes {
    winit::window::Window::default_attributes()
        .with_title(TITLE)
        .with_inner_size(winit::dpi::LogicalSize::new(DEFAULT_SIZE.0, DEFAULT_SIZE.1))
        .with_resizable(true)
        .with_maximized(true)
        .with_visible(false)
}

/// Centre la fenêtre sur le moniteur principal (ou courant).
pub fn center_on_monitor(window: &winit::window::Window) {
    if window.is_maximized() {
        return;
    }
    let Some(m) = window
        .primary_monitor()
        .or_else(|| window.current_monitor())
    else {
        return;
    };
    let (ms, mp, ws) = (m.size(), m.position(), window.outer_size());
    let x = mp.x + (ms.width as i32 - ws.width as i32) / 2;
    let y = mp.y + (ms.height as i32 - ws.height as i32) / 2;
    window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
}

/// Icône RGBA8 depuis un PNG, **sans** retournement vertical. `None` si illisible.
pub fn load_icon(png: &[u8]) -> Option<winit::window::Icon> {
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}

/// Plein écran exclusif sur le moniteur principal avec le mode vidéo courant ; repli
/// `Borderless` (déviation). Idempotent. La position/taille fenêtrée est restaurée par winit.
pub fn set_fullscreen(window: &winit::window::Window, on: bool) {
    use winit::window::Fullscreen;
    if window.fullscreen().is_some() == on {
        return;
    }
    if !on {
        window.set_fullscreen(None);
        return;
    }
    let monitor = window
        .primary_monitor()
        .or_else(|| window.current_monitor());
    let mode = monitor.as_ref().and_then(|m| {
        let size = m.size();
        let hz = m.refresh_rate_millihertz();
        m.video_modes()
            .filter(|v| v.size() == size)
            .max_by_key(|v| (Some(v.refresh_rate_millihertz()) == hz, v.bit_depth()))
    });
    window.set_fullscreen(Some(match mode {
        Some(v) => Fullscreen::Exclusive(v),
        None => Fullscreen::Borderless(monitor),
    }));
}
