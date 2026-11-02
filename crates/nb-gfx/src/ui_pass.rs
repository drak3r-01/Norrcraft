//! Rendu GPU des listes de dessin 2D de nb-ui dans l'[`OverlayPass`] (après le composite et
//! la surbrillance de bloc).
//!
//! nb-gfx ne dépend pas de nb-ui : l'appelant (nb-game) convertit la `DrawList` en
//! [`UiVertex`] (même disposition mémoire que `nb_ui::Vertex`, `bytemuck::cast_slice`) et en
//! [`UiDraw`]. Une seule pipeline : blend `SRC_ALPHA / ONE_MINUS_SRC_ALPHA`, pas de test de
//! profondeur, pas de culling, échantillonnage NEAREST + REPEAT (textures Java : MIN/MAG
//! NEAREST, mips jamais utilisés). Les icônes de blocs lisent une copie du tableau de textures
//! de blocs (niveau 0, remis à l'endroit) gérée ici.
//!
//! Shaders : `shaders/ui/ui.{vert,frag}` (SPIR-V commités, `shaders/ui/compile.sh`).

use std::collections::HashMap;
use std::sync::Arc;

use ash::vk;
use bytemuck::{Pod, Zeroable};
use gpu_allocator::MemoryLocation;

use crate::atlas::TextureArrayCpu;
use crate::shaders;
use crate::vk::{Buffer, GfxError, GpuContext, Image, ImageDesc, OverlayPass};

type Result<T> = std::result::Result<T, GfxError>;

const UI_VERT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/shaders/ui/ui.vert.spv"
));
const UI_FRAG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/shaders/ui/ui.frag.spv"
));

const TEX_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

/// Sommet UI (32 octets) : pixels fenêtre, UV en espace image, couleur RGBA.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// Texture d'un lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiTextureSlot {
    /// Index dans les images passées à [`UiRenderer::new`].
    Image(u32),
    /// Couche du tableau de textures de blocs.
    BlockLayer(u32),
}

/// Lot : `count` sommets (triangles) à partir de `first`, découpe optionnelle
/// `[x, y, w, h]` (pixels, origine en haut à gauche, bornée à l'écran).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiDraw {
    pub texture: UiTextureSlot,
    pub scissor: Option<[i32; 4]>,
    pub first: u32,
    pub count: u32,
}

/// Image RGBA8 à l'endroit (ligne 0 = haut).
#[derive(Clone, Copy, Debug)]
pub struct UiImage<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Push {
    screen: [f32; 2],
    mode: i32,
    layer: f32,
}

/// Rectangle de découpe borné à `extent` (`None` si vide).
pub fn clamp_scissor(s: Option<[i32; 4]>, extent: vk::Extent2D) -> Option<vk::Rect2D> {
    let (ew, eh) = (extent.width as i64, extent.height as i64);
    let [x, y, w, h] = s.map_or([0, 0, ew, eh], |s| s.map(i64::from));
    let x0 = x.clamp(0, ew);
    let y0 = y.clamp(0, eh);
    let x1 = (x + w).clamp(0, ew);
    let y1 = (y + h).clamp(0, eh);
    (x1 > x0 && y1 > y0).then(|| vk::Rect2D {
        offset: vk::Offset2D {
            x: x0 as i32,
            y: y0 as i32,
        },
        extent: vk::Extent2D {
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
        },
    })
}

/// Couches du tableau de blocs remises à l'endroit (le CPU les garde retournées, §5.1).
pub fn unflipped_layers(atlas: &TextureArrayCpu) -> Vec<u8> {
    let row = (atlas.tile * 4) as usize;
    let mut out = Vec::with_capacity(atlas.layers.len() * row * atlas.tile as usize);
    for layer in &atlas.layers {
        for r in layer.chunks_exact(row).rev() {
            out.extend_from_slice(r);
        }
    }
    out
}

/// Pipeline, textures et tampons de sommets de l'UI.
pub struct UiRenderer {
    ctx: Arc<GpuContext>,
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    sampler: vk::Sampler,
    pool: vk::DescriptorPool,
    images: Vec<Image>,
    sets: Vec<vk::DescriptorSet>,
    atlas: Image,
    /// Un tampon par tampon de commandes : réutilisé seulement quand la frame précédente qui
    /// l'utilisait est terminée (barrière de la frame en vol du renderer de scène).
    vertex_buffers: HashMap<vk::CommandBuffer, Buffer>,
}

impl UiRenderer {
    /// `color_format` / `depth_format` : ceux de l'[`OverlayPass`]
    /// (`SceneRenderer::output_format()`, `HDR_DEPTH_FORMAT`).
    pub fn new(
        ctx: Arc<GpuContext>,
        color_format: vk::Format,
        depth_format: vk::Format,
        images: &[UiImage<'_>],
        atlas: &TextureArrayCpu,
    ) -> Result<Self> {
        let d = &ctx.device;
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        // SAFETY : device valide ; toutes les structures vivent pendant les appels.
        let set_layout = unsafe {
            d.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?
        };
        let push = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            offset: 0,
            size: std::mem::size_of::<Push>() as u32,
        }];
        let set_layouts = [set_layout];
        // SAFETY : idem.
        let layout = unsafe {
            d.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(&push),
                None,
            )?
        };
        let pipeline = create_pipeline(&ctx, layout, color_format, depth_format)?;
        let sampler_ci = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .max_lod(0.0);
        // SAFETY : idem.
        let sampler = unsafe { d.create_sampler(&sampler_ci, None)? };
        let count = images.len().max(1) as u32;
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: count * 2,
        }];
        // SAFETY : idem.
        let pool = unsafe {
            d.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(count)
                    .pool_sizes(&sizes),
                None,
            )?
        };
        let mut gpu_images = Vec::with_capacity(images.len());
        for (i, img) in images.iter().enumerate() {
            let desc = ImageDesc::color_2d(
                img.width,
                img.height,
                TEX_FORMAT,
                vk::ImageUsageFlags::SAMPLED,
            );
            gpu_images.push(Image::upload(
                &ctx,
                desc,
                &[img.rgba.to_vec()],
                &format!("ui-texture-{i}"),
            )?);
        }
        let atlas_img = upload_atlas(&ctx, atlas)?;
        let layouts = vec![set_layout; images.len()];
        let sets = if layouts.is_empty() {
            Vec::new()
        } else {
            // SAFETY : pool dimensionné pour `images.len()` ensembles.
            unsafe {
                d.allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(pool)
                        .set_layouts(&layouts),
                )?
            }
        };
        let mut r = Self {
            ctx: ctx.clone(),
            set_layout,
            layout,
            pipeline,
            sampler,
            pool,
            images: gpu_images,
            sets,
            atlas: atlas_img,
            vertex_buffers: HashMap::new(),
        };
        r.write_sets(true);
        Ok(r)
    }

    fn write_sets(&mut self, with_images: bool) {
        for (i, &set) in self.sets.iter().enumerate() {
            let img = [vk::DescriptorImageInfo {
                sampler: self.sampler,
                image_view: self.images[i].view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let arr = [vk::DescriptorImageInfo {
                sampler: self.sampler,
                image_view: self.atlas.view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let mut writes = vec![vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&arr)];
            if with_images {
                writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&img),
                );
            }
            // SAFETY : ensembles non utilisés par une frame en vol (création, ou après
            // `wait_idle` dans `set_block_atlas`).
            unsafe { self.ctx.device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// Nouveau tableau de blocs (changement de pack : « re-génération des icônes »).
    /// Attend que le GPU soit inactif.
    pub fn set_block_atlas(&mut self, atlas: &TextureArrayCpu) -> Result<()> {
        self.ctx.wait_idle();
        let new = upload_atlas(&self.ctx, atlas)?;
        let mut old = std::mem::replace(&mut self.atlas, new);
        old.destroy(&self.ctx);
        self.write_sets(false);
        Ok(())
    }

    /// Enregistre les lots dans la passe overlay. `screen` = taille logique de la liste de
    /// dessin (taille de la fenêtre), en général l'extent de la passe.
    pub fn record(
        &mut self,
        pass: &OverlayPass<'_>,
        screen: (f32, f32),
        vertices: &[UiVertex],
        draws: &[UiDraw],
    ) -> Result<()> {
        if vertices.is_empty() || draws.is_empty() || self.sets.is_empty() {
            return Ok(());
        }
        let bytes: &[u8] = bytemuck::cast_slice(vertices);
        let needed = bytes.len() as u64;
        let buf = match self.vertex_buffers.get_mut(&pass.cmd) {
            Some(b) if b.size >= needed => b,
            _ => {
                if let Some(mut old) = self.vertex_buffers.remove(&pass.cmd) {
                    // Utilisé au plus par la soumission précédente de ce tampon de commandes,
                    // terminée puisqu'il est de nouveau en enregistrement.
                    old.destroy(&self.ctx);
                }
                let b = Buffer::new(
                    &self.ctx,
                    needed.next_power_of_two().max(64 * 1024),
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    MemoryLocation::CpuToGpu,
                    "ui-vertices",
                )?;
                self.vertex_buffers.entry(pass.cmd).or_insert(b)
            }
        };
        buf.write(0, bytes)?;
        let raw = buf.raw;
        let d = pass.device;
        let cmd = pass.cmd;
        let vp = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: pass.extent.width as f32,
            height: pass.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        // SAFETY : passe overlay ouverte (dynamic rendering) sur `cmd`, formats identiques à
        // ceux de la pipeline ; tampon et ensembles vivants jusqu'à la fin de la frame.
        unsafe {
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            d.cmd_set_viewport(cmd, 0, &[vp]);
            d.cmd_bind_vertex_buffers(cmd, 0, &[raw], &[0]);
        }
        let mut bound: Option<vk::DescriptorSet> = None;
        for draw in draws {
            let Some(rect) = clamp_scissor(draw.scissor, pass.extent) else {
                continue;
            };
            let (set, push) = match draw.texture {
                UiTextureSlot::Image(i) => {
                    let Some(&set) = self.sets.get(i as usize) else {
                        continue;
                    };
                    (
                        set,
                        Push {
                            screen: [screen.0, screen.1],
                            mode: 0,
                            layer: 0.0,
                        },
                    )
                }
                UiTextureSlot::BlockLayer(l) => (
                    self.sets[0],
                    Push {
                        screen: [screen.0, screen.1],
                        mode: 1,
                        layer: l as f32,
                    },
                ),
            };
            // SAFETY : idem.
            unsafe {
                if bound != Some(set) {
                    d.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.layout,
                        0,
                        &[set],
                        &[],
                    );
                    bound = Some(set);
                }
                d.cmd_push_constants(
                    cmd,
                    self.layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push),
                );
                d.cmd_set_scissor(cmd, 0, &[rect]);
                d.cmd_draw(cmd, draw.count, 1, draw.first, 0);
            }
        }
        // La passe overlay suppose ensuite un scissor plein écran.
        let full = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: pass.extent,
        };
        // SAFETY : idem.
        unsafe { d.cmd_set_scissor(cmd, 0, &[full]) };
        Ok(())
    }
}

impl Drop for UiRenderer {
    fn drop(&mut self) {
        let ctx = self.ctx.clone();
        ctx.wait_idle();
        for (_, mut b) in self.vertex_buffers.drain() {
            b.destroy(&ctx);
        }
        for img in &mut self.images {
            img.destroy(&ctx);
        }
        self.atlas.destroy(&ctx);
        // SAFETY : GPU inactif, objets créés par ce renderer.
        unsafe {
            ctx.device.destroy_descriptor_pool(self.pool, None);
            ctx.device.destroy_sampler(self.sampler, None);
            ctx.device.destroy_pipeline(self.pipeline, None);
            ctx.device.destroy_pipeline_layout(self.layout, None);
            ctx.device
                .destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}

fn upload_atlas(ctx: &GpuContext, atlas: &TextureArrayCpu) -> Result<Image> {
    let desc = ImageDesc {
        layers: atlas.layer_count(),
        array_view: true,
        ..ImageDesc::color_2d(
            atlas.tile,
            atlas.tile,
            TEX_FORMAT,
            vk::ImageUsageFlags::SAMPLED,
        )
    };
    Image::upload(ctx, desc, &[unflipped_layers(atlas)], "ui-block-atlas")
}

fn create_pipeline(
    ctx: &GpuContext,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    depth_format: vk::Format,
) -> Result<vk::Pipeline> {
    let d = &ctx.device;
    let module = |spv: &[u8]| -> Result<vk::ShaderModule> {
        let words = shaders::words(spv);
        // SAFETY : SPIR-V validé par spirv-val à la génération.
        Ok(unsafe {
            d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)?
        })
    };
    let vs = module(UI_VERT)?;
    let fs = match module(UI_FRAG) {
        Ok(m) => m,
        Err(e) => {
            // SAFETY : module non utilisé.
            unsafe { d.destroy_shader_module(vs, None) };
            return Err(e);
        }
    };
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vs)
            .name(c"main"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fs)
            .name(c"main"),
    ];
    let binding = [vk::VertexInputBindingDescription {
        binding: 0,
        stride: std::mem::size_of::<UiVertex>() as u32,
        input_rate: vk::VertexInputRate::VERTEX,
    }];
    let attr = |location, format, offset| vk::VertexInputAttributeDescription {
        location,
        binding: 0,
        format,
        offset,
    };
    let attrs = [
        attr(0, vk::Format::R32G32_SFLOAT, 0),
        attr(1, vk::Format::R32G32_SFLOAT, 8),
        attr(2, vk::Format::R32G32B32A32_SFLOAT, 16),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding)
        .vertex_attribute_descriptions(&attrs);
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false);
    let blend_att = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_att);
    let dyn_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dyn_states);
    let formats = [color_format];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&formats)
        .depth_attachment_format(depth_format);
    let ci = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // SAFETY : structures vivantes pendant l'appel.
    let r = unsafe { d.create_graphics_pipelines(vk::PipelineCache::null(), &[ci], None) };
    // SAFETY : modules inutiles après la création.
    unsafe {
        d.destroy_shader_module(vs, None);
        d.destroy_shader_module(fs, None);
    }
    r.map(|p| p[0]).map_err(|(_, e)| e.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vk::{validation_error_count, HDR_DEPTH_FORMAT, OUTPUT_FORMAT};
    use crate::{
        Camera, EyeFluid, FrameInput, GfxSettings, OffscreenRenderer, SceneAssets, SkyClock,
    };
    use sha2::{Digest, Sha256};

    #[test]
    fn ui_spirv_is_up_to_date() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders/ui");
        let manifest = std::fs::read_to_string(dir.join("SOURCES.sha256")).unwrap();
        let mut n = 0;
        for line in manifest.lines().filter(|l| !l.trim().is_empty()) {
            let (hash, name) = line.split_once("  ").unwrap();
            let src = std::fs::read(dir.join(name.trim())).unwrap();
            let got: String = Sha256::digest(&src)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(got, hash, "{name} modifié : relancer shaders/ui/compile.sh");
            n += 1;
        }
        assert_eq!(n, 2);
    }

    #[test]
    fn vertex_layout() {
        assert_eq!(std::mem::size_of::<UiVertex>(), 32);
        assert_eq!(std::mem::size_of::<Push>(), 16);
    }

    #[test]
    fn scissor_clamping() {
        let e = vk::Extent2D {
            width: 100,
            height: 50,
        };
        let full = clamp_scissor(None, e).unwrap();
        assert_eq!((full.extent.width, full.extent.height), (100, 50));
        let r = clamp_scissor(Some([-10, 40, 30, 30]), e).unwrap();
        assert_eq!((r.offset.x, r.offset.y), (0, 40));
        assert_eq!((r.extent.width, r.extent.height), (20, 10));
        assert!(clamp_scissor(Some([0, 60, 10, 10]), e).is_none());
        assert!(clamp_scissor(Some([5, 5, 0, 10]), e).is_none());
    }

    #[test]
    fn atlas_layers_are_unflipped() {
        let atlas = TextureArrayCpu {
            tile: 2,
            layers: vec![vec![
                1, 1, 1, 1, 2, 2, 2, 2, // ligne 0 du tampon retourné = bas de l'image
                3, 3, 3, 3, 4, 4, 4, 4,
            ]],
            names: Default::default(),
            missing_layer: 0,
            mipmapped: false,
        };
        assert_eq!(&unflipped_layers(&atlas)[..8], &[3, 3, 3, 3, 4, 4, 4, 4]);
    }

    /// Rendu réel (lavapipe sous Docker) : quad uni, quad texturé, découpe et icône de bloc.
    #[test]
    fn renders_quads_icons_and_scissor() {
        let ctx = match GpuContext::new_headless() {
            Ok(c) => Arc::new(c),
            Err(e) => {
                if std::env::var("NB_GFX_REQUIRE_GPU").as_deref() == Ok("1") {
                    panic!("Vulkan indisponible : {e}");
                }
                eprintln!("SKIP : Vulkan indisponible ({e})");
                return;
            }
        };
        // Atlas 2×2 à une couche : haut vert, bas bleu (stocké retourné).
        let green = [0u8, 255, 0, 255];
        let blue = [0u8, 0, 255, 255];
        let flipped: Vec<u8> = [blue, blue, green, green].concat();
        let atlas = TextureArrayCpu {
            tile: 2,
            layers: vec![flipped],
            names: Default::default(),
            missing_layer: 0,
            mipmapped: false,
        };
        let assets = SceneAssets {
            block_atlas: atlas.clone(),
            clouds: None,
        };
        let (w, h) = (64u32, 32u32);
        let mut off =
            OffscreenRenderer::new(ctx.clone(), assets, w, h, &GfxSettings::default()).unwrap();
        let white = [255u8; 4];
        // Texture 1 : 2×1, gauche rouge, droite blanche.
        let tex1: Vec<u8> = [[255, 0, 0, 255], white].concat();
        let images = [
            UiImage {
                width: 1,
                height: 1,
                rgba: &white,
            },
            UiImage {
                width: 2,
                height: 1,
                rgba: &tex1,
            },
        ];
        let mut ui = UiRenderer::new(
            ctx.clone(),
            OUTPUT_FORMAT,
            HDR_DEPTH_FORMAT,
            &images,
            &atlas,
        )
        .unwrap();
        let quad = |x: f32, y: f32, s: f32, uv: [f32; 4], c: [f32; 4]| {
            let v = |px, py, u, vv| UiVertex {
                pos: [px, py],
                uv: [u, vv],
                color: c,
            };
            vec![
                v(x, y, uv[0], uv[1]),
                v(x, y + s, uv[0], uv[3]),
                v(x + s, y + s, uv[2], uv[3]),
                v(x, y, uv[0], uv[1]),
                v(x + s, y + s, uv[2], uv[3]),
                v(x + s, y, uv[2], uv[1]),
            ]
        };
        let full = [0.0, 0.0, 1.0, 1.0];
        let mut verts = quad(0.0, 0.0, 16.0, full, [1.0, 1.0, 0.0, 1.0]); // jaune uni
        verts.extend(quad(16.0, 0.0, 16.0, full, [1.0, 1.0, 1.0, 1.0])); // tex1
        verts.extend(quad(32.0, 0.0, 16.0, full, [1.0, 1.0, 1.0, 1.0])); // icône
        verts.extend(quad(48.0, 0.0, 16.0, full, [1.0, 0.0, 1.0, 1.0])); // découpé
        let draws = [
            UiDraw {
                texture: UiTextureSlot::Image(0),
                scissor: None,
                first: 0,
                count: 6,
            },
            UiDraw {
                texture: UiTextureSlot::Image(1),
                scissor: None,
                first: 6,
                count: 6,
            },
            UiDraw {
                texture: UiTextureSlot::BlockLayer(0),
                scissor: None,
                first: 12,
                count: 6,
            },
            UiDraw {
                texture: UiTextureSlot::Image(0),
                scissor: Some([48, 0, 16, 8]),
                first: 18,
                count: 6,
            },
        ];
        let cam = Camera::default();
        let clock = SkyClock::new();
        let input = FrameInput {
            camera: &cam,
            time_seconds: 0.0,
            sky_clock: &clock,
            eye_fluid: EyeFluid::None,
            heat_intensity: 0.0,
            highlight_block: None,
        };
        let mut err = None;
        let img = off
            .render_with_overlay(&input, &mut |pass| {
                if let Err(e) = ui.record(pass, (w as f32, h as f32), &verts, &draws) {
                    err = Some(e);
                }
            })
            .unwrap();
        assert!(err.is_none());
        let px = |x, y| img.get_pixel(x, y).0;
        assert_eq!(px(8, 8), [255, 255, 0, 255], "quad uni");
        assert_eq!(px(18, 8), [255, 0, 0, 255], "texel gauche");
        assert_eq!(px(29, 8), [255, 255, 255, 255], "texel droit");
        assert_eq!(
            px(40, 3),
            [0, 255, 0, 255],
            "icône : haut de l'image en haut"
        );
        assert_eq!(px(40, 12), [0, 0, 255, 255], "icône : bas");
        assert_eq!(px(56, 4), [255, 0, 255, 255], "dans la découpe");
        assert_ne!(px(56, 12), [255, 0, 255, 255], "hors découpe");
        // Deuxième frame (autre tampon de commandes) puis changement d'atlas.
        ui.set_block_atlas(&TextureArrayCpu::fallback()).unwrap();
        let img = off
            .render_with_overlay(&input, &mut |pass| {
                ui.record(pass, (w as f32, h as f32), &verts, &draws)
                    .unwrap();
            })
            .unwrap();
        assert_eq!(
            img.get_pixel(40, 3).0,
            [255, 0, 255, 255],
            "atlas de repli magenta"
        );
        drop(ui);
        drop(off);
        assert_eq!(validation_error_count(), 0, "erreurs de validation Vulkan");
    }
}
