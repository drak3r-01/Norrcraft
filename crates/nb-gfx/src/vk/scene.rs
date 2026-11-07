//! Rendu d'une frame complète (spec §6, §10–§13, §16, §18.4) dans une image couleur
//! quelconque. Aucune dépendance à une fenêtre : la swapchain et l'image hors écran ne sont que
//! des [`OutputTarget`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use ash::vk;
use glam::{IVec3, Mat4, Vec3};
use gpu_allocator::MemoryLocation;
use image::RgbaImage;
use nb_core::ChunkPos;
use nb_mesh::{highlight_lines, ChunkQuadtree, ChunkVertex, Frustum};

use super::mobs::MobResources;
use super::pipelines::Pipelines;
use super::resources::{barrier, Buffer, Image, ImageDesc, Transition};
use super::targets::{shadow_map, HdrTargets, DEPTH_FORMAT};
use super::{GpuContext, Result};
use crate::atlas::{self, TextureArrayCpu};
use crate::frame::{shadow_resolution, FrameDerived, FrameInput, FrameUniforms, GfxSettings};
use crate::mob_model::{load_all_mob_models, load_mob_model, MobModelCpu, MobModelDesc, MobRoots};
use crate::packs::{self, ResourcePackMeta};

/// Nombre de frames en vol (ressources par frame : UBO, commandes, requêtes d'occlusion).
pub const FRAMES_IN_FLIGHT: usize = 2;
/// Format de sortie : UNORM, jamais sRGB (le composite fait lui-même le gamma, §4).
pub const OUTPUT_FORMAT: vk::Format = vk::Format::B8G8R8A8_UNORM;
/// Format de la profondeur HDR (attachée en lecture seule dans la passe overlay).
pub const HDR_DEPTH_FORMAT: vk::Format = DEPTH_FORMAT;

/// `clouds.png` (ressource « classpath » `/textures/environment/clouds.png`).
const CLOUDS_PNG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/textures/environment/clouds.png"
));

/// Textures de particules « classpath » (`/textures/particle`, §5.4).
const LEAF_PNG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/textures/particle/leaf.png"
));
const LAVA_DROP_PNG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/textures/particle/lava_drop.png"
));

/// Assets CPU nécessaires au renderer.
#[derive(Clone, Debug)]
pub struct SceneAssets {
    pub block_atlas: TextureArrayCpu,
    /// Texture de nuages retournée (512×512 attendue).
    pub clouds: Option<RgbaImage>,
    /// Atlas des particules (`leaf` → 0, `lava_drop` → 1, manquante → 2).
    pub particle_atlas: TextureArrayCpu,
    /// Racines des modèles de mobs (pour `reload_mob_model`).
    pub mob_roots: MobRoots,
    /// Modèles de mobs chargés (`MobModelLoader.loadAll`) ; id absent → cube manquant.
    pub mob_models: HashMap<String, MobModelCpu>,
}

impl SceneAssets {
    /// Atlas du pack `pack` (repli Default, §5.2), nuages et particules intégrés, modèles de
    /// `mobs` avec les racines `(packs_root/pack, packs_root/Default)`.
    pub fn load(packs_root: &Path, pack: &str, mobs: &[MobModelDesc]) -> (Self, ResourcePackMeta) {
        let (dir, meta) = packs::resolve_block_textures(packs_root, pack);
        let mob_roots = MobRoots::for_pack(packs_root, pack);
        (
            Self {
                block_atlas: TextureArrayCpu::from_dir(&dir),
                clouds: Self::builtin_clouds(),
                particle_atlas: Self::builtin_particle_atlas(),
                mob_models: load_all_mob_models(mobs, &mob_roots),
                mob_roots,
            },
            meta,
        )
    }

    pub fn builtin_clouds() -> Option<RgbaImage> {
        atlas::cloud_texture(CLOUDS_PNG)
    }

    /// `loadFromClasspath("/textures/particle", "leaf", "lava_drop")`.
    pub fn builtin_particle_atlas() -> TextureArrayCpu {
        TextureArrayCpu::from_named(&[("leaf", Some(LEAF_PNG)), ("lava_drop", Some(LAVA_DROP_PNG))])
    }
}

/// Image de sortie d'une frame.
#[derive(Clone, Copy, Debug)]
pub struct OutputTarget {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub extent: vk::Extent2D,
    /// État final de l'image (présentation ou copie).
    pub final_state: Transition,
}

/// Passe overlay (après le composite et la surbrillance) : l'image de sortie et la profondeur
/// HDR (lecture seule) sont attachées ; nb-ui y enregistre ses commandes (pipelines créées avec
/// `color_format` / `depth_format`, test de profondeur désactivé).
pub struct OverlayPass<'a> {
    pub device: &'a ash::Device,
    pub cmd: vk::CommandBuffer,
    pub extent: vk::Extent2D,
    pub color_format: vk::Format,
    pub depth_format: vk::Format,
    /// Mapping nom → couche (icônes de blocs).
    pub block_atlas: &'a TextureArrayCpu,
}

/// Frame en cours d'enregistrement (rendue par [`SceneRenderer::begin_frame`]).
#[derive(Clone, Copy, Debug)]
pub struct FrameToken {
    pub slot: usize,
    pub cmd: vk::CommandBuffer,
}

struct ChunkMesh {
    buffer: Buffer,
    vertex_count: u32,
}

struct Slot {
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    ubo: Buffer,
    frame_set: vk::DescriptorSet,
    pp_set: vk::DescriptorSet,
    queries: vk::QueryPool,
    query_capacity: u32,
    query_chunks: Vec<ChunkPos>,
    frame_number: Option<u64>,
}

struct Samplers {
    atlas: vk::Sampler,
    nearest_repeat: vk::Sampler,
    linear_repeat: vk::Sampler,
    linear_clamp: vk::Sampler,
    nearest_clamp: vk::Sampler,
    shadow: vk::Sampler,
}

impl Samplers {
    fn new(ctx: &GpuContext) -> Result<Self> {
        let mk = |filter: vk::Filter,
                  mip: vk::SamplerMipmapMode,
                  address: vk::SamplerAddressMode,
                  max_lod: f32,
                  aniso: Option<f32>|
         -> Result<vk::Sampler> {
            let ci = vk::SamplerCreateInfo::default()
                .mag_filter(filter)
                .min_filter(filter)
                .mipmap_mode(mip)
                .address_mode_u(address)
                .address_mode_v(address)
                .address_mode_w(address)
                .anisotropy_enable(aniso.is_some())
                .max_anisotropy(aniso.unwrap_or(1.0))
                .min_lod(0.0)
                .max_lod(max_lod)
                .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE);
            // SAFETY : device valide.
            Ok(unsafe { ctx.device.create_sampler(&ci, None)? })
        };
        use vk::Filter as F;
        use vk::SamplerAddressMode as A;
        use vk::SamplerMipmapMode as M;
        Ok(Self {
            // NEAREST_MIPMAP_LINEAR / NEAREST, REPEAT, aniso ≤ 8 (§5.1, §18.3).
            atlas: mk(
                F::NEAREST,
                M::LINEAR,
                A::REPEAT,
                vk::LOD_CLAMP_NONE,
                ctx.max_anisotropy,
            )?,
            nearest_repeat: mk(F::NEAREST, M::NEAREST, A::REPEAT, 0.0, None)?,
            linear_repeat: mk(F::LINEAR, M::NEAREST, A::REPEAT, 0.0, None)?,
            linear_clamp: mk(F::LINEAR, M::NEAREST, A::CLAMP_TO_EDGE, 0.0, None)?,
            nearest_clamp: mk(F::NEAREST, M::NEAREST, A::CLAMP_TO_EDGE, 0.0, None)?,
            shadow: mk(F::NEAREST, M::NEAREST, A::CLAMP_TO_BORDER, 0.0, None)?,
        })
    }

    fn destroy(&self, ctx: &GpuContext) {
        for s in [
            self.atlas,
            self.nearest_repeat,
            self.linear_repeat,
            self.linear_clamp,
            self.nearest_clamp,
            self.shadow,
        ] {
            // SAFETY : plus utilisé par le GPU.
            unsafe { ctx.device.destroy_sampler(s, None) };
        }
    }
}

/// Boîte `[0,16]×[0,128]×[0,16]` des requêtes d'occlusion (36 sommets).
fn chunk_aabb() -> Vec<[f32; 3]> {
    let (x1, y1, z1) = (16.0, 128.0, 16.0);
    let c = |i: u32| {
        [
            if i & 1 != 0 { x1 } else { 0.0 },
            if i & 2 != 0 { y1 } else { 0.0 },
            if i & 4 != 0 { z1 } else { 0.0 },
        ]
    };
    // 6 faces × 2 triangles (orientation indifférente : cull désactivé).
    const FACES: [[u32; 4]; 6] = [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ];
    FACES
        .iter()
        .flat_map(|f| [f[0], f[1], f[2], f[2], f[3], f[0]])
        .map(c)
        .collect()
}

fn image_info(
    view: vk::ImageView,
    sampler: vk::Sampler,
    layout: vk::ImageLayout,
) -> [vk::DescriptorImageInfo; 1] {
    [vk::DescriptorImageInfo {
        sampler,
        image_view: view,
        image_layout: layout,
    }]
}

/// Renderer de scène : assets GPU, cibles, pipelines, maillages et état d'occlusion.
pub struct SceneRenderer {
    ctx: Arc<GpuContext>,
    settings: GfxSettings,
    pipelines: Pipelines,
    samplers: Samplers,
    atlas_cpu: TextureArrayCpu,
    atlas: Image,
    clouds: Image,
    noise: Image,
    shadow: Image,
    shadow_size: u32,
    targets: Option<HdrTargets>,
    descriptor_pool: vk::DescriptorPool,
    command_pool: vk::CommandPool,
    slots: Vec<Slot>,
    /// Set 1 de la passe opaque (texture factice : `opaqueDepth` n'y est pas lu).
    dummy_set: vk::DescriptorSet,
    /// Set 1 des fluides : profondeur HDR.
    depth_set: vk::DescriptorSet,
    /// Sources du flou : bright, ping0, ping1.
    blur_sets: [vk::DescriptorSet; 3],
    aabb: Buffer,
    lines: Buffer,
    /// Modèles de mobs, atlas et quad des particules (passe B2).
    mobs: MobResources,
    meshes: HashMap<ChunkPos, ChunkMesh>,
    retired: Vec<(Option<u64>, Buffer)>,
    quadtree: ChunkQuadtree,
    visible: Vec<ChunkPos>,
    shadow_visible: Vec<ChunkPos>,
    /// Chunks visibles à la frame précédente (réinitialisation de l'occlusion).
    prev_visible: std::collections::HashSet<ChunkPos>,
    /// Chunks de la passe fluide, triés de l'arrière vers l'avant.
    fluid_order: Vec<ChunkPos>,
    /// `lastQueryResults` (défaut : visible).
    occlusion: HashMap<ChunkPos, bool>,
    frame_counter: u64,
    completed: Option<u64>,
}

impl SceneRenderer {
    pub fn new(
        ctx: Arc<GpuContext>,
        assets: SceneAssets,
        output_format: vk::Format,
        settings: &GfxSettings,
    ) -> Result<Self> {
        let c = &*ctx;
        let pipelines = Pipelines::new(c, output_format)?;
        let samplers = Samplers::new(c)?;
        let atlas = upload_atlas(c, &assets.block_atlas)?;
        let clouds_img = assets
            .clouds
            .unwrap_or_else(|| RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0])));
        let clouds = Image::upload(
            c,
            ImageDesc::color_2d(
                clouds_img.width(),
                clouds_img.height(),
                vk::Format::R8G8B8A8_UNORM,
                vk::ImageUsageFlags::SAMPLED,
            ),
            &[clouds_img.into_raw()],
            "clouds",
        )?;
        let noise = Image::upload(
            c,
            ImageDesc::color_2d(256, 256, vk::Format::R8_UNORM, vk::ImageUsageFlags::SAMPLED),
            &[atlas::noise_texture(atlas::NOISE_SEED)],
            "noise",
        )?;
        let shadow_size = shadow_resolution(settings.shadow_quality).unwrap_or(1024);
        let shadow = shadow_map(c, shadow_size)?;
        clear_shadow(c, &shadow)?;

        let n = FRAMES_IN_FLIGHT as u32;
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 2 * n,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 7 * n + 5,
            },
        ];
        let pool_ci = vk::DescriptorPoolCreateInfo::default()
            .max_sets(2 * n + 5)
            .pool_sizes(&sizes);
        // SAFETY : device valide.
        let descriptor_pool = unsafe { c.device.create_descriptor_pool(&pool_ci, None)? };
        let alloc_sets =
            |layout: vk::DescriptorSetLayout, count: usize| -> Result<Vec<vk::DescriptorSet>> {
                let layouts = vec![layout; count];
                let ai = vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&layouts);
                // SAFETY : pool dimensionné pour ces sets.
                Ok(unsafe { c.device.allocate_descriptor_sets(&ai)? })
            };
        let frame_sets = alloc_sets(pipelines.frame_set_layout, FRAMES_IN_FLIGHT)?;
        let pp_sets = alloc_sets(pipelines.pp_set_layout, FRAMES_IN_FLIGHT)?;
        let tex_sets = alloc_sets(pipelines.texture_set_layout, 5)?;

        let cp_ci = vk::CommandPoolCreateInfo::default()
            .queue_family_index(c.queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        // SAFETY : device valide.
        let command_pool = unsafe { c.device.create_command_pool(&cp_ci, None)? };
        let ai = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(n);
        // SAFETY : pool valide.
        let cmds = unsafe { c.device.allocate_command_buffers(&ai)? };
        let mut slots = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for i in 0..FRAMES_IN_FLIGHT {
            let fci = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            // SAFETY : device valide.
            let fence = unsafe { c.device.create_fence(&fci, None)? };
            let ubo = Buffer::new(
                c,
                std::mem::size_of::<FrameUniforms>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
                "frame-ubo",
            )?;
            let (queries, query_capacity) = create_query_pool(c, 64)?;
            slots.push(Slot {
                cmd: cmds[i],
                fence,
                ubo,
                frame_set: frame_sets[i],
                pp_set: pp_sets[i],
                queries,
                query_capacity,
                query_chunks: Vec::new(),
                frame_number: None,
            });
        }
        let aabb = Buffer::with_data(
            c,
            &chunk_aabb(),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            "aabb",
        )?;
        let lines = Buffer::with_data(
            c,
            &highlight_lines(),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            "highlight",
        )?;
        let mobs = MobResources::new(
            c,
            pipelines.texture_set_layout,
            samplers.nearest_repeat,
            samplers.atlas,
            assets.particle_atlas,
            assets.mob_roots,
            assets.mob_models,
        )?;

        let renderer = Self {
            ctx: ctx.clone(),
            settings: *settings,
            pipelines,
            samplers,
            atlas_cpu: assets.block_atlas,
            atlas,
            clouds,
            noise,
            shadow,
            shadow_size,
            targets: None,
            descriptor_pool,
            command_pool,
            slots,
            dummy_set: tex_sets[0],
            depth_set: tex_sets[1],
            blur_sets: [tex_sets[2], tex_sets[3], tex_sets[4]],
            aabb,
            lines,
            mobs,
            meshes: HashMap::new(),
            retired: Vec::new(),
            quadtree: ChunkQuadtree::new(),
            visible: Vec::new(),
            shadow_visible: Vec::new(),
            prev_visible: std::collections::HashSet::new(),
            fluid_order: Vec::new(),
            occlusion: HashMap::new(),
            frame_counter: 0,
            completed: None,
        };
        renderer.write_static_sets();
        Ok(renderer)
    }

    pub fn context(&self) -> &Arc<GpuContext> {
        &self.ctx
    }

    pub fn settings(&self) -> &GfxSettings {
        &self.settings
    }

    /// Mapping nom → couche de l'atlas de blocs courant.
    pub fn block_atlas(&self) -> &TextureArrayCpu {
        &self.atlas_cpu
    }

    pub fn output_format(&self) -> vk::Format {
        self.pipelines.output_format
    }

    /// Nombre de maillages de chunks chargés.
    pub fn chunk_mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Chunks retenus par le frustum caméra à la dernière frame (ordre de dessin).
    pub fn visible_chunks(&self) -> &[ChunkPos] {
        &self.visible
    }

    /// Chunks retenus par le frustum de la lumière à la dernière frame.
    pub fn shadow_visible_chunks(&self) -> &[ChunkPos] {
        &self.shadow_visible
    }

    /// Dernier résultat d'occlusion connu (`true` si jamais mesuré).
    pub fn occlusion_visible(&self, pos: ChunkPos) -> bool {
        self.occlusion.get(&pos).copied().unwrap_or(true)
    }

    // ------------------------------------------------------------------ ressources

    /// Remplace le maillage d'un chunk ; vide = suppression. Destruction différée de l'ancien.
    pub fn upload_chunk_mesh(&mut self, pos: ChunkPos, vertices: &[ChunkVertex]) -> Result<()> {
        if vertices.is_empty() {
            self.remove_chunk_mesh(pos);
            return Ok(());
        }
        let buffer = Buffer::with_data(
            &self.ctx,
            vertices,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            "chunk-mesh",
        )?;
        let mesh = ChunkMesh {
            buffer,
            vertex_count: vertices.len() as u32,
        };
        if let Some(old) = self.meshes.insert(pos, mesh) {
            self.retire(old.buffer);
        }
        Ok(())
    }

    pub fn remove_chunk_mesh(&mut self, pos: ChunkPos) {
        if let Some(old) = self.meshes.remove(&pos) {
            self.retire(old.buffer);
        }
    }

    pub fn clear_chunk_meshes(&mut self) {
        let all: Vec<_> = self.meshes.drain().map(|(_, m)| m.buffer).collect();
        for b in all {
            self.retire(b);
        }
    }

    fn retire(&mut self, buffer: Buffer) {
        // Dernière frame soumise susceptible d'utiliser le tampon.
        let last = self.frame_counter.checked_sub(1);
        self.retired.push((last, buffer));
        self.collect_retired();
    }

    fn collect_retired(&mut self) {
        let completed = self.completed;
        let ctx = self.ctx.clone();
        self.retired.retain_mut(|(last, b)| {
            let free = match (*last, completed) {
                (None, _) => true,
                (Some(l), Some(c)) => l <= c,
                (Some(_), None) => false,
            };
            if free {
                b.destroy(&ctx);
            }
            !free
        });
    }

    /// Remplace l'atlas de blocs (changement de pack, §5.2) et invalide tous les maillages.
    pub fn reload_block_atlas(&mut self, atlas_cpu: TextureArrayCpu) -> Result<()> {
        let new_atlas = upload_atlas(&self.ctx, &atlas_cpu)?;
        self.ctx.wait_idle();
        self.atlas.destroy(&self.ctx);
        self.atlas = new_atlas;
        self.atlas_cpu = atlas_cpu;
        self.clear_chunk_meshes();
        self.write_static_sets();
        Ok(())
    }

    /// Atlas des particules (mapping texture → couche pour [`crate::ParticleInstance::layer`]).
    pub fn particle_atlas(&self) -> &TextureArrayCpu {
        self.mobs.particle_atlas()
    }

    /// Racines actives des modèles de mobs.
    pub fn mob_roots(&self) -> &MobRoots {
        self.mobs.roots()
    }

    /// `(sommets opaques, sommets transparents)` du modèle chargé pour `id` (`None` → cube
    /// manquant au rendu).
    pub fn mob_model_counts(&self, id: &str) -> Option<(u32, u32)> {
        self.mobs.model_counts(id)
    }

    /// `reloadMobModel(id)` (`loadMob`, §5.2, §14.2) avec les racines actives : l'ancien modèle
    /// n'est remplacé que si un nouveau est construit. Retourne `true` si remplacé.
    pub fn reload_mob_model(&mut self, desc: &MobModelDesc) -> Result<bool> {
        let Some(m) = load_mob_model(desc, self.mobs.roots()) else {
            return Ok(false);
        };
        self.install_mob_model(&desc.id, &m)?;
        Ok(true)
    }

    /// Installe un modèle déjà construit (remplace l'ancien).
    pub fn install_mob_model(&mut self, id: &str, model: &MobModelCpu) -> Result<()> {
        // Les frames en vol peuvent encore lire l'ancien modèle (rechargement rare).
        self.ctx.wait_idle();
        self.mobs.replace(&self.ctx, id, model)
    }

    /// Changement de pack : détruit tous les modèles puis `loadAll(roots)` (§5.2 étape 4).
    pub fn reload_mob_models(&mut self, roots: MobRoots, mobs: &[MobModelDesc]) -> Result<()> {
        let models = load_all_mob_models(mobs, &roots);
        self.ctx.wait_idle();
        self.mobs.replace_all(&self.ctx, roots, models)
    }

    /// Applique les réglages (qualité d'ombre, bloom, correctif des seuils d'ombre).
    pub fn apply_settings(&mut self, s: &GfxSettings) -> Result<()> {
        let old = self.settings;
        self.settings = *s;
        match shadow_resolution(s.shadow_quality) {
            Some(size) if size != self.shadow_size => {
                let new_map = shadow_map(&self.ctx, size)?;
                clear_shadow(&self.ctx, &new_map)?;
                self.ctx.wait_idle();
                self.shadow.destroy(&self.ctx);
                self.shadow = new_map;
                self.shadow_size = size;
                self.write_static_sets();
            }
            None if old.shadow_quality != s.shadow_quality => {
                // [DÉVIATION] appliquée (décision 2026-11-01) : carte effacée à 1.0 au passage
                // à OFF (Java : ombres figées de la dernière frame).
                self.ctx.wait_idle();
                clear_shadow(&self.ctx, &self.shadow)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Côté de la shadow map courante.
    pub fn shadow_map_size(&self) -> u32 {
        self.shadow_size
    }

    /// Réécrit les descripteurs qui référencent des images (après création / changement).
    fn write_static_sets(&self) {
        let d = &self.ctx.device;
        let ro = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        let depth_ro = vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;
        let atlas = image_info(self.atlas.view, self.samplers.atlas, ro);
        let shadow = image_info(self.shadow.view, self.samplers.shadow, depth_ro);
        let clouds = image_info(self.clouds.view, self.samplers.nearest_repeat, ro);
        let noise = image_info(self.noise.view, self.samplers.linear_repeat, ro);
        let dummy = image_info(self.noise.view, self.samplers.nearest_clamp, ro);
        let mut writes = Vec::new();
        let mut ubos = Vec::new();
        for s in &self.slots {
            ubos.push([vk::DescriptorBufferInfo {
                buffer: s.ubo.raw,
                offset: 0,
                range: vk::WHOLE_SIZE,
            }]);
        }
        fn tex<'a>(
            set: vk::DescriptorSet,
            binding: u32,
            info: &'a [vk::DescriptorImageInfo; 1],
        ) -> vk::WriteDescriptorSet<'a> {
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(binding)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(info)
        }
        for (s, ubo) in self.slots.iter().zip(&ubos) {
            for set in [s.frame_set, s.pp_set] {
                writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(ubo),
                );
            }
            writes.push(tex(s.frame_set, 1, &atlas));
            writes.push(tex(s.frame_set, 2, &shadow));
            writes.push(tex(s.pp_set, 4, &clouds));
            writes.push(tex(s.pp_set, 5, &noise));
        }
        writes.push(tex(self.dummy_set, 0, &dummy));
        let target_infos = self.targets.as_ref().map(|t| {
            (
                image_info(t.color.view, self.samplers.linear_clamp, ro),
                image_info(t.ping[0].view, self.samplers.linear_clamp, ro),
                image_info(t.depth.view, self.samplers.nearest_clamp, depth_ro),
                image_info(t.bright.view, self.samplers.linear_clamp, ro),
                image_info(t.ping[1].view, self.samplers.linear_clamp, ro),
            )
        });
        if let Some((color, ping0, depth, bright, ping1)) = &target_infos {
            for s in &self.slots {
                writes.push(tex(s.pp_set, 1, color));
                writes.push(tex(s.pp_set, 2, ping0));
                writes.push(tex(s.pp_set, 3, depth));
            }
            writes.push(tex(self.depth_set, 0, depth));
            writes.push(tex(self.blur_sets[0], 0, bright));
            writes.push(tex(self.blur_sets[1], 0, ping0));
            writes.push(tex(self.blur_sets[2], 0, ping1));
        }
        // SAFETY : aucun de ces sets n'est utilisé par une frame en cours (appelant idle).
        unsafe { d.update_descriptor_sets(&writes, &[]) };
    }

    fn ensure_targets(&mut self, extent: vk::Extent2D) -> Result<()> {
        if self.targets.as_ref().map(|t| t.extent) == Some(extent) {
            return Ok(());
        }
        self.ctx.wait_idle();
        if let Some(mut t) = self.targets.take() {
            t.destroy(&self.ctx);
        }
        self.targets = Some(HdrTargets::new(&self.ctx, extent)?);
        self.write_static_sets();
        Ok(())
    }

    // ------------------------------------------------------------------ frame

    /// Attend la fin de la frame qui utilisait ce créneau, récupère ses requêtes d'occlusion
    /// et ouvre son tampon de commandes.
    pub fn begin_frame(&mut self) -> Result<FrameToken> {
        let slot_index = (self.frame_counter % FRAMES_IN_FLIGHT as u64) as usize;
        let d = &self.ctx.device;
        let slot = &mut self.slots[slot_index];
        // SAFETY : fence de ce créneau.
        unsafe { d.wait_for_fences(&[slot.fence], true, u64::MAX)? };
        if let Some(n) = slot.frame_number {
            self.completed = Some(self.completed.map_or(n, |c| c.max(n)));
        }
        if !slot.query_chunks.is_empty() {
            let mut results = vec![[0u64; 2]; slot.query_chunks.len()];
            // SAFETY : requêtes 0..n écrites par la frame terminée de ce créneau.
            let r = unsafe {
                d.get_query_pool_results(
                    slot.queries,
                    0,
                    &mut results,
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WITH_AVAILABILITY,
                )
            };
            match r {
                Ok(()) | Err(vk::Result::NOT_READY) => {
                    for (pos, [samples, available]) in slot.query_chunks.iter().zip(&results) {
                        if *available != 0 {
                            self.occlusion.insert(*pos, *samples > 0);
                        }
                    }
                }
                Err(e) => return Err(e.into()),
            }
            slot.query_chunks.clear();
        }
        // SAFETY : la frame précédente de ce créneau est terminée.
        unsafe {
            d.reset_fences(&[slot.fence])?;
            d.reset_command_buffer(slot.cmd, vk::CommandBufferResetFlags::empty())?;
            d.begin_command_buffer(
                slot.cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }
        let cmd = slot.cmd;
        self.collect_retired();
        Ok(FrameToken {
            slot: slot_index,
            cmd,
        })
    }

    /// Termine et soumet la frame. `waits` / `signals` : sémaphores de la swapchain.
    pub fn submit_frame(
        &mut self,
        token: FrameToken,
        waits: &[vk::SemaphoreSubmitInfo<'_>],
        signals: &[vk::SemaphoreSubmitInfo<'_>],
    ) -> Result<()> {
        let slot = &mut self.slots[token.slot];
        // SAFETY : tampon ouvert par `begin_frame`.
        unsafe { self.ctx.device.end_command_buffer(token.cmd)? };
        let cbi = [vk::CommandBufferSubmitInfo::default().command_buffer(token.cmd)];
        let submit = [vk::SubmitInfo2::default()
            .wait_semaphore_infos(waits)
            .command_buffer_infos(&cbi)
            .signal_semaphore_infos(signals)];
        self.ctx.submit(&submit, slot.fence)?;
        slot.frame_number = Some(self.frame_counter);
        self.frame_counter += 1;
        Ok(())
    }

    /// Créneau qu'utilisera le prochain [`Self::begin_frame`].
    pub fn next_slot(&self) -> usize {
        (self.frame_counter % FRAMES_IN_FLIGHT as u64) as usize
    }

    /// Attend la fin de la frame soumise dans ce créneau.
    pub fn wait_slot(&self, slot: usize) -> Result<()> {
        // SAFETY : fence valide.
        unsafe {
            self.ctx
                .device
                .wait_for_fences(&[self.slots[slot].fence], true, u64::MAX)?
        };
        Ok(())
    }

    /// Enregistre toute la frame (§6) dans `token.cmd`, jusqu'à la transition finale de
    /// `out`. `overlay` est appelé dans la dernière passe (après la surbrillance).
    pub fn record_frame(
        &mut self,
        token: FrameToken,
        input: &FrameInput<'_>,
        out: &OutputTarget,
        overlay: &mut dyn FnMut(&mut OverlayPass<'_>),
    ) -> Result<()> {
        self.ensure_targets(out.extent)?;
        let extent = out.extent;
        let derived = FrameDerived::new(input, (extent.width, extent.height), &self.settings);

        // 1–2, 6. Culling caméra et lumière (quadtree reconstruit chaque frame).
        self.quadtree.build(self.meshes.keys().copied());
        self.visible.clear();
        self.quadtree.query(
            &Frustum::from_gl_matrix(&derived.camera.frustum_matrix_gl()),
            &mut self.visible,
        );
        self.shadow_visible.clear();
        self.quadtree.query(
            &Frustum::from_gl_matrix(&derived.light_space_gl.to_cols_array()),
            &mut self.shadow_visible,
        );

        let slot = &mut self.slots[token.slot];
        slot.ubo.write(0, bytemuck::bytes_of(&derived.uniforms))?;
        let needed = self.visible.len() as u32;
        if needed > slot.query_capacity {
            // SAFETY : la frame précédente de ce créneau est terminée (fence attendu).
            unsafe { self.ctx.device.destroy_query_pool(slot.queries, None) };
            let (q, cap) = create_query_pool(&self.ctx, needed.next_power_of_two())?;
            slot.queries = q;
            slot.query_capacity = cap;
        }
        slot.query_chunks.clear();
        slot.query_chunks.extend_from_slice(&self.visible);

        // Occlusion : un chunk qui (ré)entre dans le frustum repart de « visible » (le dernier
        // résultat date d'avant sa sortie : sinon il resterait masqué quelques frames), et les
        // chunks entourant la caméra ne sont jamais masqués. La boîte d'un chunk qui contient
        // la caméra est vue de l'intérieur : en grotte, toutes ses faces sont derrière les
        // parois, la requête renvoyait 0, le chunk disparaissait, puis réapparaissait à la
        // frame suivante (scintillement).
        {
            let cam = derived.camera.position;
            let (ccx, ccz) = ((cam.x / 16.0).floor() as i32, (cam.z / 16.0).floor() as i32);
            let meshes = &self.meshes;
            self.occlusion.retain(|p, _| meshes.contains_key(p));
            for pos in &self.visible {
                let near = (pos.x - ccx).abs() <= 1 && (pos.z - ccz).abs() <= 1;
                if near || !self.prev_visible.contains(pos) {
                    self.occlusion.insert(*pos, true);
                }
            }
            self.prev_visible.clear();
            self.prev_visible.extend(self.visible.iter().copied());
            // Fluides : de l'arrière vers l'avant (le mélange alpha n'est pas commutatif).
            self.fluid_order.clear();
            let occlusion = &self.occlusion;
            self.fluid_order.extend(
                self.visible
                    .iter()
                    .copied()
                    .filter(|p| occlusion.get(p).copied().unwrap_or(true)),
            );
            let dist2 = |p: &ChunkPos| {
                let dx = (p.x * 16 + 8) as f32 - cam.x;
                let dz = (p.z * 16 + 8) as f32 - cam.z;
                dx * dx + dz * dz
            };
            self.fluid_order
                .sort_by(|a, b| dist2(b).total_cmp(&dist2(a)));
        }

        let cmd = token.cmd;
        let slot = &self.slots[token.slot];
        let (frame_set, pp_set, queries, query_capacity) = (
            slot.frame_set,
            slot.pp_set,
            slot.queries,
            slot.query_capacity,
        );
        let ctx = &*self.ctx;
        let d = &ctx.device;
        let t = self.targets.as_ref().expect("cibles HDR");

        // SAFETY : enregistrement dans un tampon ouvert ; toutes les ressources référencées
        // vivent au moins jusqu'à la fin de la frame (destruction différée).
        unsafe {
            d.cmd_reset_query_pool(cmd, queries, 0, query_capacity);

            // 7. Passe d'ombre (§10).
            if self.settings.shadow_quality != nb_data::ShadowQuality::Off {
                let n = self.shadow_size;
                barrier(
                    ctx,
                    cmd,
                    self.shadow.raw,
                    self.shadow.range(),
                    Transition::DEPTH_READ.discard(),
                    Transition::DEPTH_ATTACHMENT,
                );
                let depth_att =
                    depth_attachment(self.shadow.view, vk::AttachmentLoadOp::CLEAR, false);
                begin_rendering(d, cmd, square(n), &[], Some(&depth_att));
                set_viewport(d, cmd, square(n));
                d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipelines.shadow);
                d.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipelines.terrain_layout,
                    0,
                    &[frame_set],
                    &[],
                );
                for pos in &self.shadow_visible {
                    self.draw_chunk(cmd, *pos);
                }
                d.cmd_end_rendering(cmd);
                barrier(
                    ctx,
                    cmd,
                    self.shadow.raw,
                    self.shadow.range(),
                    Transition::DEPTH_ATTACHMENT,
                    Transition::DEPTH_READ,
                );
            }

            // 8. Scène A : terrain opaque + requêtes d'occlusion (§6.1).
            for img in [&t.color, &t.bright] {
                barrier(
                    ctx,
                    cmd,
                    img.raw,
                    img.range(),
                    Transition::SHADER_READ.discard(),
                    Transition::COLOR_ATTACHMENT,
                );
            }
            barrier(
                ctx,
                cmd,
                t.depth.raw,
                t.depth.range(),
                Transition::DEPTH_READ.discard(),
                Transition::DEPTH_ATTACHMENT,
            );
            let cc = derived.clear_color;
            let clear = [cc.x, cc.y, cc.z, 1.0];
            let colors = [
                color_attachment(t.color.view, vk::AttachmentLoadOp::CLEAR, clear),
                color_attachment(t.bright.view, vk::AttachmentLoadOp::CLEAR, clear),
            ];
            let depth_att = depth_attachment(t.depth.view, vk::AttachmentLoadOp::CLEAR, false);
            begin_rendering(d, cmd, extent, &colors, Some(&depth_att));
            set_viewport(d, cmd, extent);
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipelines.opaque);
            d.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.terrain_layout,
                0,
                &[frame_set, self.dummy_set],
                &[],
            );
            for pos in &self.visible {
                if self.occlusion_visible(*pos) {
                    self.draw_chunk(cmd, *pos);
                }
            }
            d.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.occlusion,
            );
            d.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.hl_layout,
                0,
                &[frame_set],
                &[],
            );
            d.cmd_bind_vertex_buffers(cmd, 0, &[self.aabb.raw], &[0]);
            for (i, pos) in self.visible.iter().enumerate() {
                let model = Mat4::from_translation(Vec3::new(
                    (pos.x * 16) as f32,
                    0.0,
                    (pos.z * 16) as f32,
                ));
                push_model(d, cmd, self.pipelines.hl_layout, &model);
                d.cmd_begin_query(cmd, queries, i as u32, vk::QueryControlFlags::empty());
                d.cmd_draw(cmd, 36, 1, 0, 0);
                d.cmd_end_query(cmd, queries, i as u32);
            }
            d.cmd_end_rendering(cmd);
            barrier(
                ctx,
                cmd,
                t.depth.raw,
                t.depth.range(),
                Transition::DEPTH_ATTACHMENT,
                Transition::DEPTH_READ,
            );

            // Scène B1 : fluides, profondeur en lecture seule + échantillonnée (§18.4.4).
            let colors = [
                color_attachment(t.color.view, vk::AttachmentLoadOp::LOAD, clear),
                color_attachment(t.bright.view, vk::AttachmentLoadOp::LOAD, clear),
            ];
            let depth_att = depth_attachment(t.depth.view, vk::AttachmentLoadOp::LOAD, true);
            begin_rendering(d, cmd, extent, &colors, Some(&depth_att));
            set_viewport(d, cmd, extent);
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipelines.fluid);
            d.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.terrain_layout,
                0,
                &[frame_set, self.depth_set],
                &[],
            );
            for pos in &self.fluid_order {
                self.draw_chunk(cmd, *pos);
            }
            d.cmd_end_rendering(cmd);

            // 9–10. Scène B2 : mobs (plage opaque avec écriture de profondeur) puis particules
            // (§14.1, §15, §18.4.4). Profondeur repassée en attachement inscriptible, plus
            // échantillonnée. Seul `colorBuffer` est attaché : le bright buffer reste inchangé
            // (§14.1, voir `Pipelines`). Passe sautée si rien à dessiner (résultat identique).
            if !input.mobs.is_empty() || !input.particles.is_empty() {
                barrier(
                    ctx,
                    cmd,
                    t.color.raw,
                    t.color.range(),
                    Transition::COLOR_ATTACHMENT,
                    Transition::COLOR_ATTACHMENT,
                );
                barrier(
                    ctx,
                    cmd,
                    t.depth.raw,
                    t.depth.range(),
                    Transition::DEPTH_READ,
                    Transition::DEPTH_ATTACHMENT,
                );
                let colors = [color_attachment(
                    t.color.view,
                    vk::AttachmentLoadOp::LOAD,
                    clear,
                )];
                let depth_att = depth_attachment(t.depth.view, vk::AttachmentLoadOp::LOAD, false);
                begin_rendering(d, cmd, extent, &colors, Some(&depth_att));
                set_viewport(d, cmd, extent);
                self.mobs
                    .record(ctx, cmd, &self.pipelines, frame_set, &derived.view, input);
                d.cmd_end_rendering(cmd);
                barrier(
                    ctx,
                    cmd,
                    t.depth.raw,
                    t.depth.range(),
                    Transition::DEPTH_ATTACHMENT,
                    Transition::DEPTH_READ,
                );
            }
            for img in [&t.color, &t.bright] {
                barrier(
                    ctx,
                    cmd,
                    img.raw,
                    img.range(),
                    Transition::COLOR_ATTACHMENT,
                    Transition::SHADER_READ,
                );
            }

            // 11. Bloom (§12).
            let half = t.ping[0].extent();
            if self.settings.bloom {
                let mut horizontal = true;
                for i in 0..10 {
                    let target = &t.ping[if horizontal { 1 } else { 0 }];
                    let source = if i == 0 {
                        self.blur_sets[0]
                    } else {
                        self.blur_sets[if horizontal { 1 } else { 2 }]
                    };
                    barrier(
                        ctx,
                        cmd,
                        target.raw,
                        target.range(),
                        Transition::SHADER_READ.discard(),
                        Transition::COLOR_ATTACHMENT,
                    );
                    let att = [color_attachment(
                        target.view,
                        vk::AttachmentLoadOp::DONT_CARE,
                        [0.0; 4],
                    )];
                    begin_rendering(d, cmd, half, &att, None);
                    set_viewport(d, cmd, half);
                    d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipelines.blur);
                    d.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.pipelines.blur_layout,
                        0,
                        &[source],
                        &[],
                    );
                    d.cmd_push_constants(
                        cmd,
                        self.pipelines.blur_layout,
                        vk::ShaderStageFlags::FRAGMENT,
                        0,
                        &i32::from(horizontal).to_ne_bytes(),
                    );
                    d.cmd_draw(cmd, 4, 1, 0, 0);
                    d.cmd_end_rendering(cmd);
                    barrier(
                        ctx,
                        cmd,
                        target.raw,
                        target.range(),
                        Transition::COLOR_ATTACHMENT,
                        Transition::SHADER_READ,
                    );
                    horizontal = !horizontal;
                }
            } else {
                let p0 = &t.ping[0];
                barrier(
                    ctx,
                    cmd,
                    p0.raw,
                    p0.range(),
                    Transition::SHADER_READ.discard(),
                    Transition::COLOR_ATTACHMENT,
                );
                let att = [color_attachment(
                    p0.view,
                    vk::AttachmentLoadOp::CLEAR,
                    [0.0; 4],
                )];
                begin_rendering(d, cmd, half, &att, None);
                d.cmd_end_rendering(cmd);
                barrier(
                    ctx,
                    cmd,
                    p0.raw,
                    p0.range(),
                    Transition::COLOR_ATTACHMENT,
                    Transition::SHADER_READ,
                );
            }

            // 12–14. Composite → sortie, surbrillance puis overlay (§13, §16, §18.4.7).
            let initial = Transition {
                layout: vk::ImageLayout::UNDEFINED,
                stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags2::TRANSFER,
                access: vk::AccessFlags2::NONE,
            };
            let out_range = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };
            barrier(
                ctx,
                cmd,
                out.image,
                out_range,
                initial,
                Transition::COLOR_ATTACHMENT,
            );
            let att = [color_attachment(
                out.view,
                vk::AttachmentLoadOp::CLEAR,
                clear,
            )];
            let depth_att = depth_attachment(t.depth.view, vk::AttachmentLoadOp::LOAD, true);
            begin_rendering(d, cmd, extent, &att, Some(&depth_att));
            set_viewport(d, cmd, extent);
            d.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.composite,
            );
            d.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.pp_layout,
                0,
                &[pp_set],
                &[],
            );
            d.cmd_draw(cmd, 4, 1, 0, 0);
            if let Some(b) = input.highlight_block {
                self.record_highlight(cmd, frame_set, b);
            }
            let mut pass = OverlayPass {
                device: d,
                cmd,
                extent,
                color_format: self.pipelines.output_format,
                depth_format: DEPTH_FORMAT,
                block_atlas: &self.atlas_cpu,
            };
            overlay(&mut pass);
            d.cmd_end_rendering(cmd);
            barrier(
                ctx,
                cmd,
                out.image,
                out_range,
                Transition::COLOR_ATTACHMENT,
                out.final_state,
            );
        }
        Ok(())
    }

    /// Frame **sans scène 3D** (états dont l'écran Java n'appelle pas `MasterRenderer.render` :
    /// choix du monde, réglages, packs, mobs) : la sortie est effacée à `clear` puis seule la
    /// passe overlay est enregistrée (mêmes formats que [`Self::record_frame`] ; la profondeur
    /// HDR est attachée en lecture seule, contenu abandonné, l'UI ne la teste pas).
    pub fn record_ui_frame(
        &mut self,
        token: FrameToken,
        out: &OutputTarget,
        clear: [f32; 4],
        overlay: &mut dyn FnMut(&mut OverlayPass<'_>),
    ) -> Result<()> {
        self.ensure_targets(out.extent)?;
        let extent = out.extent;
        let cmd = token.cmd;
        let ctx = &*self.ctx;
        let d = &ctx.device;
        let t = self.targets.as_ref().expect("cibles HDR");
        let out_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let initial = Transition {
            layout: vk::ImageLayout::UNDEFINED,
            stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags2::TRANSFER,
            access: vk::AccessFlags2::NONE,
        };
        // SAFETY : enregistrement dans un tampon ouvert ; ressources vivantes jusqu'à la fin de
        // la frame.
        unsafe {
            barrier(
                ctx,
                cmd,
                t.depth.raw,
                t.depth.range(),
                Transition::DEPTH_READ.discard(),
                Transition::DEPTH_READ,
            );
            barrier(
                ctx,
                cmd,
                out.image,
                out_range,
                initial,
                Transition::COLOR_ATTACHMENT,
            );
            let att = [color_attachment(
                out.view,
                vk::AttachmentLoadOp::CLEAR,
                clear,
            )];
            let depth_att = depth_attachment(t.depth.view, vk::AttachmentLoadOp::LOAD, true);
            begin_rendering(d, cmd, extent, &att, Some(&depth_att));
            set_viewport(d, cmd, extent);
            let mut pass = OverlayPass {
                device: d,
                cmd,
                extent,
                color_format: self.pipelines.output_format,
                depth_format: DEPTH_FORMAT,
                block_atlas: &self.atlas_cpu,
            };
            overlay(&mut pass);
            d.cmd_end_rendering(cmd);
            barrier(
                ctx,
                cmd,
                out.image,
                out_range,
                Transition::COLOR_ATTACHMENT,
                out.final_state,
            );
        }
        Ok(())
    }

    fn record_highlight(&self, cmd: vk::CommandBuffer, frame_set: vk::DescriptorSet, b: IVec3) {
        let d = &self.ctx.device;
        // SAFETY : appelé pendant l'enregistrement de la passe composite.
        unsafe {
            d.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.highlight,
            );
            d.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.hl_layout,
                0,
                &[frame_set],
                &[],
            );
            push_model(
                d,
                cmd,
                self.pipelines.hl_layout,
                &Mat4::from_translation(b.as_vec3()),
            );
            d.cmd_bind_vertex_buffers(cmd, 0, &[self.lines.raw], &[0]);
            d.cmd_draw(cmd, 24, 1, 0, 0);
        }
    }

    fn draw_chunk(&self, cmd: vk::CommandBuffer, pos: ChunkPos) {
        if let Some(m) = self.meshes.get(&pos) {
            // SAFETY : tampon vivant (destruction différée), pipeline liée par l'appelant.
            unsafe {
                self.ctx
                    .device
                    .cmd_bind_vertex_buffers(cmd, 0, &[m.buffer.raw], &[0]);
                self.ctx.device.cmd_draw(cmd, m.vertex_count, 1, 0, 0);
            }
        }
    }
}

impl Drop for SceneRenderer {
    fn drop(&mut self) {
        let ctx = self.ctx.clone();
        ctx.wait_idle();
        for (_, mut m) in self.meshes.drain() {
            m.buffer.destroy(&ctx);
        }
        for (_, mut b) in self.retired.drain(..) {
            b.destroy(&ctx);
        }
        if let Some(mut t) = self.targets.take() {
            t.destroy(&ctx);
        }
        for s in &mut self.slots {
            s.ubo.destroy(&ctx);
            // SAFETY : GPU inactif.
            unsafe {
                ctx.device.destroy_fence(s.fence, None);
                ctx.device.destroy_query_pool(s.queries, None);
            }
        }
        self.aabb.destroy(&ctx);
        self.lines.destroy(&ctx);
        self.mobs.destroy(&ctx);
        self.atlas.destroy(&ctx);
        self.clouds.destroy(&ctx);
        self.noise.destroy(&ctx);
        self.shadow.destroy(&ctx);
        self.samplers.destroy(&ctx);
        self.pipelines.destroy(&ctx);
        // SAFETY : GPU inactif ; les sets et tampons appartiennent à ces pools.
        unsafe {
            ctx.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            ctx.device.destroy_command_pool(self.command_pool, None);
        }
    }
}

// ---------------------------------------------------------------------- helpers

fn square(n: u32) -> vk::Extent2D {
    vk::Extent2D {
        width: n,
        height: n,
    }
}

fn create_query_pool(ctx: &GpuContext, capacity: u32) -> Result<(vk::QueryPool, u32)> {
    let capacity = capacity.max(64);
    let ci = vk::QueryPoolCreateInfo::default()
        .query_type(vk::QueryType::OCCLUSION)
        .query_count(capacity);
    // SAFETY : device valide.
    let pool = unsafe { ctx.device.create_query_pool(&ci, None)? };
    Ok((pool, capacity))
}

/// Upload de l'atlas : chaîne de mips CPU (boîte 2×2) de chaque couche.
pub(super) fn upload_atlas(ctx: &GpuContext, a: &TextureArrayCpu) -> Result<Image> {
    let levels = a.mip_levels();
    let chains: Vec<_> = a
        .layers
        .iter()
        .map(|l| atlas::mip_chain(l, a.tile, a.tile))
        .collect();
    let data: Vec<Vec<u8>> = (0..levels as usize)
        .map(|lvl| {
            chains
                .iter()
                .flat_map(|c| c[lvl].2.iter().copied())
                .collect()
        })
        .collect();
    Image::upload(
        ctx,
        ImageDesc {
            width: a.tile,
            height: a.tile,
            layers: a.layer_count(),
            mip_levels: levels,
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::SAMPLED,
            aspect: vk::ImageAspectFlags::COLOR,
            array_view: true,
        },
        &data,
        "block-atlas",
    )
}

/// Efface la shadow map à 1.0 et la laisse en lecture (création, passage à OFF).
fn clear_shadow(ctx: &GpuContext, img: &Image) -> Result<()> {
    ctx.run_once(|cmd| {
        barrier(
            ctx,
            cmd,
            img.raw,
            img.range(),
            Transition::DEPTH_READ.discard(),
            Transition::DEPTH_ATTACHMENT,
        );
        let att = depth_attachment(img.view, vk::AttachmentLoadOp::CLEAR, false);
        // SAFETY : tampon en cours d'enregistrement.
        unsafe {
            begin_rendering(&ctx.device, cmd, img.extent(), &[], Some(&att));
            ctx.device.cmd_end_rendering(cmd);
        }
        barrier(
            ctx,
            cmd,
            img.raw,
            img.range(),
            Transition::DEPTH_ATTACHMENT,
            Transition::DEPTH_READ,
        );
    })
}

fn color_attachment(
    view: vk::ImageView,
    load: vk::AttachmentLoadOp,
    clear: [f32; 4],
) -> vk::RenderingAttachmentInfo<'static> {
    vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(load)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue { float32: clear },
        })
}

fn depth_attachment(
    view: vk::ImageView,
    load: vk::AttachmentLoadOp,
    read_only: bool,
) -> vk::RenderingAttachmentInfo<'static> {
    vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(if read_only {
            vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
        } else {
            vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL
        })
        .load_op(load)
        .store_op(if read_only {
            vk::AttachmentStoreOp::NONE
        } else {
            vk::AttachmentStoreOp::STORE
        })
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        })
}

/// # Safety
/// `cmd` doit être en cours d'enregistrement.
unsafe fn begin_rendering(
    d: &ash::Device,
    cmd: vk::CommandBuffer,
    extent: vk::Extent2D,
    colors: &[vk::RenderingAttachmentInfo<'_>],
    depth: Option<&vk::RenderingAttachmentInfo<'_>>,
) {
    let mut info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .color_attachments(colors);
    if let Some(dep) = depth {
        info = info.depth_attachment(dep);
    }
    // SAFETY : garanti par l'appelant.
    unsafe { d.cmd_begin_rendering(cmd, &info) };
}

/// # Safety
/// `cmd` doit être en cours d'enregistrement.
unsafe fn set_viewport(d: &ash::Device, cmd: vk::CommandBuffer, extent: vk::Extent2D) {
    let vp = vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let sc = vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent,
    };
    // SAFETY : garanti par l'appelant.
    unsafe {
        d.cmd_set_viewport(cmd, 0, &[vp]);
        d.cmd_set_scissor(cmd, 0, &[sc]);
    }
}

/// # Safety
/// `cmd` doit être en cours d'enregistrement avec un layout à push constant mat4.
unsafe fn push_model(
    d: &ash::Device,
    cmd: vk::CommandBuffer,
    layout: vk::PipelineLayout,
    m: &Mat4,
) {
    let cols = m.to_cols_array();
    // SAFETY : garanti par l'appelant.
    unsafe {
        d.cmd_push_constants(
            cmd,
            layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            bytemuck::cast_slice(&cols),
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_covers_chunk() {
        let v = chunk_aabb();
        assert_eq!(v.len(), 36);
        for axis in 0..3 {
            let max = v.iter().map(|p| p[axis]).fold(0.0f32, f32::max);
            let min = v.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
            assert_eq!(min, 0.0);
            assert_eq!(max, [16.0, 128.0, 16.0][axis]);
        }
        // Chaque face est un vrai quad (2 triangles non dégénérés).
        for tri in v.chunks(3) {
            let a = Vec3::from(tri[0]);
            let n = (Vec3::from(tri[1]) - a).cross(Vec3::from(tri[2]) - a);
            assert!(n.length() > 0.0);
        }
    }

    #[test]
    fn builtin_clouds_decode() {
        let c = SceneAssets::builtin_clouds().unwrap();
        assert_eq!(c.dimensions(), (512, 512));
    }
}
