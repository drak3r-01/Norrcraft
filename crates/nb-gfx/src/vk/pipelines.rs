//! Layouts de descripteurs et pipelines graphiques (dynamic rendering), spec §6, §9, §18.

use ash::vk;

use super::mobs::PARTICLE_PUSH_SIZE;
use super::targets::{DEPTH_FORMAT, HDR_COLOR_FORMAT};
use super::{GpuContext, Result};
use crate::shaders;

/// Format des sommets en entrée.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Vertex {
    /// `ChunkVertex` (36 o) ; `color = false` : locations 0..2 seulement (ombre).
    Chunk { color: bool },
    /// `vec3` seul (12 o) : AABB d'occlusion, contour de bloc.
    Position,
    /// `MobVertex` (40 o) : vec3, vec2, vec4, float.
    Mob,
    /// Quad de particule (20 o) : vec3, vec2.
    Particle,
    /// Aucun (quad plein écran généré dans le vertex shader).
    None,
}

#[derive(Clone, Copy)]
struct ColorTarget {
    format: vk::Format,
    blend: bool,
    write: bool,
}

struct Desc<'a> {
    vs: &'a [u8],
    fs: &'a [u8],
    render_pass_constant: Option<i32>,
    vertex: Vertex,
    topology: vk::PrimitiveTopology,
    cull: vk::CullModeFlags,
    /// `(test, write)` ; `None` = pas de test de profondeur.
    depth: Option<(bool, bool)>,
    depth_format: vk::Format,
    colors: &'a [ColorTarget],
    layout: vk::PipelineLayout,
}

fn module(ctx: &GpuContext, spv: &[u8]) -> Result<vk::ShaderModule> {
    let words = shaders::words(spv);
    let ci = vk::ShaderModuleCreateInfo::default().code(&words);
    // SAFETY : SPIR-V validé par spirv-val à la génération.
    Ok(unsafe { ctx.device.create_shader_module(&ci, None)? })
}

fn build(ctx: &GpuContext, d: &Desc<'_>) -> Result<vk::Pipeline> {
    let vs = module(ctx, d.vs)?;
    let fs = match module(ctx, d.fs) {
        Ok(m) => m,
        Err(e) => {
            // SAFETY : module non utilisé.
            unsafe { ctx.device.destroy_shader_module(vs, None) };
            return Err(e);
        }
    };
    let spec_value = d.render_pass_constant.unwrap_or(0).to_ne_bytes();
    let spec_entries = [vk::SpecializationMapEntry {
        constant_id: 0,
        offset: 0,
        size: 4,
    }];
    let spec = vk::SpecializationInfo::default()
        .map_entries(&spec_entries)
        .data(&spec_value);
    let mut fs_stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(fs)
        .name(c"main");
    if d.render_pass_constant.is_some() {
        fs_stage = fs_stage.specialization_info(&spec);
    }
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vs)
            .name(c"main"),
        fs_stage,
    ];

    let f = |location, format, offset| vk::VertexInputAttributeDescription {
        location,
        binding: 0,
        format,
        offset,
    };
    let (bindings, attrs): (Vec<_>, Vec<_>) = match d.vertex {
        Vertex::Chunk { color } => {
            let mut a = vec![
                f(0, vk::Format::R32G32B32_SFLOAT, 0),
                f(1, vk::Format::R32G32_SFLOAT, 12),
                f(2, vk::Format::R32_SFLOAT, 20),
            ];
            if color {
                a.push(f(3, vk::Format::R32G32B32_SFLOAT, 24));
            }
            (
                vec![vk::VertexInputBindingDescription {
                    binding: 0,
                    stride: 36,
                    input_rate: vk::VertexInputRate::VERTEX,
                }],
                a,
            )
        }
        Vertex::Position => (
            vec![vk::VertexInputBindingDescription {
                binding: 0,
                stride: 12,
                input_rate: vk::VertexInputRate::VERTEX,
            }],
            vec![f(0, vk::Format::R32G32B32_SFLOAT, 0)],
        ),
        Vertex::Mob => (
            vec![vk::VertexInputBindingDescription {
                binding: 0,
                stride: 40,
                input_rate: vk::VertexInputRate::VERTEX,
            }],
            vec![
                f(0, vk::Format::R32G32B32_SFLOAT, 0),
                f(1, vk::Format::R32G32_SFLOAT, 12),
                f(2, vk::Format::R32G32B32A32_SFLOAT, 20),
                f(3, vk::Format::R32_SFLOAT, 36),
            ],
        ),
        Vertex::Particle => (
            vec![vk::VertexInputBindingDescription {
                binding: 0,
                stride: 20,
                input_rate: vk::VertexInputRate::VERTEX,
            }],
            vec![
                f(0, vk::Format::R32G32B32_SFLOAT, 0),
                f(1, vk::Format::R32G32_SFLOAT, 12),
            ],
        ),
        Vertex::None => (Vec::new(), Vec::new()),
    };
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attrs);
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(d.topology);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(d.cull)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let (test, write) = d.depth.unwrap_or((false, false));
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(test)
        .depth_write_enable(write)
        // [DÉVIATION] recommandée §6.2 : LEQUAL partout (GL : LESS puis LEQUAL).
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
    let blends: Vec<_> = d
        .colors
        .iter()
        .map(|c| {
            vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(c.blend)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(if c.write {
                    vk::ColorComponentFlags::RGBA
                } else {
                    vk::ColorComponentFlags::empty()
                })
        })
        .collect();
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blends);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let formats: Vec<vk::Format> = d.colors.iter().map(|c| c.format).collect();
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&formats)
        .depth_attachment_format(d.depth_format);

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
        .layout(d.layout)
        .push_next(&mut rendering);
    // SAFETY : toutes les structures référencées vivent jusqu'à la fin de l'appel.
    let r = unsafe {
        ctx.device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[ci], None)
    };
    // SAFETY : les modules ne sont plus nécessaires après la création.
    unsafe {
        ctx.device.destroy_shader_module(vs, None);
        ctx.device.destroy_shader_module(fs, None);
    }
    r.map(|p| p[0]).map_err(|(_, e)| e.into())
}

fn set_layout(
    ctx: &GpuContext,
    bindings: &[(vk::DescriptorType, vk::ShaderStageFlags)],
) -> Result<vk::DescriptorSetLayout> {
    let b: Vec<_> = bindings
        .iter()
        .enumerate()
        .map(|(i, &(ty, stages))| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(i as u32)
                .descriptor_type(ty)
                .descriptor_count(1)
                .stage_flags(stages)
        })
        .collect();
    let ci = vk::DescriptorSetLayoutCreateInfo::default().bindings(&b);
    // SAFETY : device valide.
    Ok(unsafe { ctx.device.create_descriptor_set_layout(&ci, None)? })
}

fn pipeline_layout(
    ctx: &GpuContext,
    sets: &[vk::DescriptorSetLayout],
    push: Option<(vk::ShaderStageFlags, u32)>,
) -> Result<vk::PipelineLayout> {
    let ranges: Vec<_> = push
        .map(|(stage_flags, size)| vk::PushConstantRange {
            stage_flags,
            offset: 0,
            size,
        })
        .into_iter()
        .collect();
    let ci = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(sets)
        .push_constant_ranges(&ranges);
    // SAFETY : device valide.
    Ok(unsafe { ctx.device.create_pipeline_layout(&ci, None)? })
}

/// Toutes les pipelines du rendu.
pub struct Pipelines {
    /// Set 0 des passes 3D : UBO, atlas blocs, shadow map.
    pub frame_set_layout: vk::DescriptorSetLayout,
    /// Une seule texture (source du flou, texture factice de la passe opaque).
    pub texture_set_layout: vk::DescriptorSetLayout,
    /// Set 1 des fluides : `opaqueDepth` (profondeur du terrain opaque) + `refractTex` (copie de la
    /// couleur HDR du terrain opaque, pour la réfraction).
    pub fluid_set_layout: vk::DescriptorSetLayout,
    /// Composite : UBO + scene, bloomBlur, depthMap, cloudTexture, noiseTexture.
    pub pp_set_layout: vk::DescriptorSetLayout,
    pub terrain_layout: vk::PipelineLayout,
    /// Fluides : set 0 (UBO, atlas, ombres) + set 1 (profondeur + copie de couleur).
    pub fluid_layout: vk::PipelineLayout,
    pub hl_layout: vk::PipelineLayout,
    pub blur_layout: vk::PipelineLayout,
    pub pp_layout: vk::PipelineLayout,
    /// Mobs : set 0 (UBO) + set 1 (texture du modèle), push `model` (64 o, vertex).
    pub mob_layout: vk::PipelineLayout,
    /// Particules : set 0 (UBO) + set 1 (atlas particules), push `model`, `colorTint`,
    /// `layer` (80 o, vertex + fragment).
    pub particle_layout: vk::PipelineLayout,
    pub shadow: vk::Pipeline,
    pub opaque: vk::Pipeline,
    pub occlusion: vk::Pipeline,
    pub fluid: vk::Pipeline,
    pub blur: vk::Pipeline,
    pub composite: vk::Pipeline,
    pub highlight: vk::Pipeline,
    /// Plage opaque des mobs : blend off, écriture de profondeur.
    pub mob_opaque: vk::Pipeline,
    /// Plage transparente des mobs : blend, sans écriture de profondeur.
    pub mob_transparent: vk::Pipeline,
    pub particle: vk::Pipeline,
    pub output_format: vk::Format,
}

impl Pipelines {
    pub fn new(ctx: &GpuContext, output_format: vk::Format) -> Result<Self> {
        use vk::DescriptorType as T;
        use vk::ShaderStageFlags as S;
        let ubo = (T::UNIFORM_BUFFER, S::VERTEX | S::FRAGMENT);
        let tex = (T::COMBINED_IMAGE_SAMPLER, S::FRAGMENT);
        let frame_set_layout = set_layout(ctx, &[ubo, tex, tex])?;
        let texture_set_layout = set_layout(ctx, &[tex])?;
        let fluid_set_layout = set_layout(ctx, &[tex, tex])?;
        let pp_set_layout = set_layout(ctx, &[ubo, tex, tex, tex, tex, tex])?;
        let terrain_layout = pipeline_layout(ctx, &[frame_set_layout, texture_set_layout], None)?;
        let fluid_layout = pipeline_layout(ctx, &[frame_set_layout, fluid_set_layout], None)?;
        let hl_layout = pipeline_layout(ctx, &[frame_set_layout], Some((S::VERTEX, 64)))?;
        let blur_layout = pipeline_layout(ctx, &[texture_set_layout], Some((S::FRAGMENT, 4)))?;
        let pp_layout = pipeline_layout(ctx, &[pp_set_layout], None)?;
        let mob_layout = pipeline_layout(
            ctx,
            &[frame_set_layout, texture_set_layout],
            Some((S::VERTEX | S::FRAGMENT, 80)),
        )?;
        let particle_layout = pipeline_layout(
            ctx,
            &[frame_set_layout, texture_set_layout],
            Some((S::VERTEX | S::FRAGMENT, PARTICLE_PUSH_SIZE)),
        )?;

        let hdr = |blend, write| ColorTarget {
            format: HDR_COLOR_FORMAT,
            blend,
            write,
        };
        let shadow = build(
            ctx,
            &Desc {
                vs: shaders::SHADOW_VERT,
                fs: shaders::SHADOW_FRAG,
                render_pass_constant: None,
                vertex: Vertex::Chunk { color: false },
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                cull: vk::CullModeFlags::FRONT,
                depth: Some((true, true)),
                depth_format: DEPTH_FORMAT,
                colors: &[],
                layout: terrain_layout,
            },
        )?;
        let terrain = |fs: &'static [u8], layout: vk::PipelineLayout, depth_write: bool| Desc {
            vs: shaders::MAIN_VERT,
            fs,
            render_pass_constant: None,
            vertex: Vertex::Chunk { color: true },
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            cull: vk::CullModeFlags::BACK,
            depth: Some((true, depth_write)),
            depth_format: DEPTH_FORMAT,
            colors: &[],
            layout,
        };
        let opaque_colors = [hdr(false, true), hdr(false, true)];
        let opaque = build(
            ctx,
            &Desc {
                colors: &opaque_colors,
                ..terrain(shaders::MAIN_FRAG, terrain_layout, true)
            },
        )?;
        let fluid_colors = [hdr(true, true), hdr(true, true)];
        let fluid = build(
            ctx,
            &Desc {
                colors: &fluid_colors,
                ..terrain(shaders::FLUID_FRAG, fluid_layout, false)
            },
        )?;
        let masked = [hdr(false, false), hdr(false, false)];
        let occlusion = build(
            ctx,
            &Desc {
                vs: shaders::HL_VERT,
                fs: shaders::HL_FRAG,
                render_pass_constant: None,
                vertex: Vertex::Position,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                cull: vk::CullModeFlags::NONE,
                depth: Some((true, false)),
                depth_format: DEPTH_FORMAT,
                colors: &masked,
                layout: hl_layout,
            },
        )?;
        let blur_colors = [hdr(false, true)];
        let blur = build(
            ctx,
            &Desc {
                vs: shaders::QUAD_VERT,
                fs: shaders::BLUR_FRAG,
                render_pass_constant: None,
                vertex: Vertex::None,
                topology: vk::PrimitiveTopology::TRIANGLE_STRIP,
                cull: vk::CullModeFlags::NONE,
                depth: None,
                depth_format: vk::Format::UNDEFINED,
                colors: &blur_colors,
                layout: blur_layout,
            },
        )?;
        let out = [ColorTarget {
            format: output_format,
            blend: false,
            write: true,
        }];
        let composite = build(
            ctx,
            &Desc {
                vs: shaders::QUAD_VERT,
                fs: shaders::PP_FRAG,
                render_pass_constant: None,
                vertex: Vertex::None,
                topology: vk::PrimitiveTopology::TRIANGLE_STRIP,
                cull: vk::CullModeFlags::NONE,
                depth: None,
                depth_format: DEPTH_FORMAT,
                colors: &out,
                layout: pp_layout,
            },
        )?;
        // Surbrillance : la profondeur HDR est attachée en lecture seule dans la passe
        // composite/overlay (§18.4.7) → test LEQUAL sans écriture (invisible, rien ne la relit).
        let highlight = build(
            ctx,
            &Desc {
                vs: shaders::HL_VERT,
                fs: shaders::HL_FRAG,
                render_pass_constant: None,
                vertex: Vertex::Position,
                topology: vk::PrimitiveTopology::LINE_LIST,
                cull: vk::CullModeFlags::NONE,
                depth: Some((true, false)),
                depth_format: DEPTH_FORMAT,
                colors: &out,
                layout: hl_layout,
            },
        )?;
        // Passe B2 (§14.1, §15, §18.4.4) : mobs et particules écrivent la couleur ET le bright buffer.
        // [DÉVIATION] Le Java laissait le bright buffer intact (« indéfini » en GL), si bien que le
        // bloom du terrain situé derrière un mob (lave, feuillage lumineux) se superposait au mob. Les
        // shaders écrivent maintenant (0, 0, 0, alpha) en location 1 : avec le même mélange que la
        // couleur, le bloom est masqué par les mobs et particules opaques, atténué par les
        // translucides. Aucune valeur indéfinie, pas de colorWriteMask nul.
        let mob_desc = |depth_write: bool, colors| Desc {
            vs: shaders::MOB_VERT,
            fs: shaders::MOB_FRAG,
            render_pass_constant: None,
            vertex: Vertex::Mob,
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            cull: vk::CullModeFlags::BACK,
            depth: Some((true, depth_write)),
            depth_format: DEPTH_FORMAT,
            colors,
            layout: mob_layout,
        };
        let opaque_b2 = [hdr(false, true), hdr(false, true)];
        let blended_b2 = [hdr(true, true), hdr(true, true)];
        let mob_opaque = build(ctx, &mob_desc(true, &opaque_b2))?;
        let mob_transparent = build(ctx, &mob_desc(false, &blended_b2))?;
        let particle = build(
            ctx,
            &Desc {
                vs: shaders::PARTICLE_VERT,
                fs: shaders::PARTICLE_FRAG,
                render_pass_constant: None,
                vertex: Vertex::Particle,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                cull: vk::CullModeFlags::BACK,
                depth: Some((true, false)),
                depth_format: DEPTH_FORMAT,
                colors: &blended_b2,
                layout: particle_layout,
            },
        )?;
        Ok(Self {
            frame_set_layout,
            texture_set_layout,
            fluid_set_layout,
            pp_set_layout,
            terrain_layout,
            fluid_layout,
            hl_layout,
            blur_layout,
            pp_layout,
            mob_layout,
            particle_layout,
            shadow,
            opaque,
            occlusion,
            fluid,
            blur,
            composite,
            highlight,
            mob_opaque,
            mob_transparent,
            particle,
            output_format,
        })
    }

    pub fn destroy(&mut self, ctx: &GpuContext) {
        // SAFETY : l'appelant garantit que le GPU a fini d'utiliser ces objets.
        unsafe {
            for p in [
                self.shadow,
                self.opaque,
                self.occlusion,
                self.fluid,
                self.blur,
                self.composite,
                self.highlight,
                self.mob_opaque,
                self.mob_transparent,
                self.particle,
            ] {
                ctx.device.destroy_pipeline(p, None);
            }
            for l in [
                self.terrain_layout,
                self.fluid_layout,
                self.hl_layout,
                self.blur_layout,
                self.pp_layout,
                self.mob_layout,
                self.particle_layout,
            ] {
                ctx.device.destroy_pipeline_layout(l, None);
            }
            for l in [
                self.frame_set_layout,
                self.texture_set_layout,
                self.fluid_set_layout,
                self.pp_set_layout,
            ] {
                ctx.device.destroy_descriptor_set_layout(l, None);
            }
        }
    }
}
