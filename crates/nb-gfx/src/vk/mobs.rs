//! Ressources GPU et enregistrement de la passe scène B2 (spec §14.1, §15, §18.4.4) : modèles
//! de mobs (tampon de sommets + texture), cube manquant, texture blanche, atlas et quad des
//! particules.

use std::collections::HashMap;

use ash::vk;
use bytemuck::{Pod, Zeroable};
use glam::Mat4;

use super::pipelines::Pipelines;
use super::resources::{Buffer, Image, ImageDesc};
use super::scene::upload_atlas;
use super::{GpuContext, Result};
use crate::atlas::TextureArrayCpu;
use crate::frame::FrameInput;
use crate::mob_model::{
    missing_cube, missing_texture, mob_model_matrix, particle_model_matrix, MobModelCpu, MobRoots,
    PARTICLE_QUAD,
};

/// Push constants des particules (miroir du bloc `Push` de `particle.vert/.frag`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct ParticlePush {
    model: [f32; 16],
    color_tint: [f32; 3],
    layer: i32,
}

pub(super) const PARTICLE_PUSH_SIZE: u32 = std::mem::size_of::<ParticlePush>() as u32;
const _: () = assert!(PARTICLE_PUSH_SIZE == 80);

/// Sets « une texture » alloués à la demande (pools successifs, libération individuelle).
struct TextureSets {
    layout: vk::DescriptorSetLayout,
    pools: Vec<vk::DescriptorPool>,
}

#[derive(Clone, Copy)]
struct TexSet {
    pool: usize,
    set: vk::DescriptorSet,
}

const SETS_PER_POOL: u32 = 64;

impl TextureSets {
    fn new_pool(ctx: &GpuContext) -> Result<vk::DescriptorPool> {
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: SETS_PER_POOL,
        }];
        let ci = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
            .max_sets(SETS_PER_POOL)
            .pool_sizes(&sizes);
        // SAFETY : device valide.
        Ok(unsafe { ctx.device.create_descriptor_pool(&ci, None)? })
    }

    fn alloc(
        &mut self,
        ctx: &GpuContext,
        view: vk::ImageView,
        sampler: vk::Sampler,
    ) -> Result<TexSet> {
        let layouts = [self.layout];
        let try_alloc = |pool| {
            let ai = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool)
                .set_layouts(&layouts);
            // SAFETY : pool et layout valides.
            unsafe { ctx.device.allocate_descriptor_sets(&ai) }
        };
        let mut found = None;
        for (i, &pool) in self.pools.iter().enumerate().rev() {
            match try_alloc(pool) {
                Ok(s) => {
                    found = Some(TexSet { pool: i, set: s[0] });
                    break;
                }
                Err(vk::Result::ERROR_OUT_OF_POOL_MEMORY | vk::Result::ERROR_FRAGMENTED_POOL) => {}
                Err(e) => return Err(e.into()),
            }
        }
        let ts = match found {
            Some(ts) => ts,
            None => {
                let pool = Self::new_pool(ctx)?;
                self.pools.push(pool);
                TexSet {
                    pool: self.pools.len() - 1,
                    set: try_alloc(pool)?[0],
                }
            }
        };
        let info = [vk::DescriptorImageInfo {
            sampler,
            image_view: view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(ts.set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&info);
        // SAFETY : set fraîchement alloué, jamais utilisé.
        unsafe { ctx.device.update_descriptor_sets(&[write], &[]) };
        Ok(ts)
    }

    fn free(&self, ctx: &GpuContext, s: TexSet) {
        // SAFETY : pool créé avec FREE_DESCRIPTOR_SET ; set plus utilisé (appelant idle).
        let _ = unsafe {
            ctx.device
                .free_descriptor_sets(self.pools[s.pool], &[s.set])
        };
    }

    fn destroy(&mut self, ctx: &GpuContext) {
        for p in self.pools.drain(..) {
            // SAFETY : GPU inactif.
            unsafe { ctx.device.destroy_descriptor_pool(p, None) };
        }
    }
}

struct GpuModel {
    buffer: Buffer,
    opaque: u32,
    transparent: u32,
    /// `None` : mode couleur (texture blanche).
    texture: Option<(Image, TexSet)>,
}

/// Assets GPU des mobs et des particules.
pub(super) struct MobResources {
    sets: TextureSets,
    /// NEAREST / REPEAT (textures d'entité, blanc, damier) ; appartient au `SceneRenderer`.
    sampler: vk::Sampler,
    white: Image,
    white_set: TexSet,
    missing: GpuModel,
    models: HashMap<String, GpuModel>,
    particle_atlas_cpu: TextureArrayCpu,
    particle_atlas: Image,
    particle_set: TexSet,
    particle_quad: Buffer,
    roots: MobRoots,
}

fn upload_rgba(ctx: &GpuContext, img: &image::RgbaImage, name: &str) -> Result<Image> {
    Image::upload(
        ctx,
        ImageDesc::color_2d(
            img.width(),
            img.height(),
            vk::Format::R8G8B8A8_UNORM,
            vk::ImageUsageFlags::SAMPLED,
        ),
        &[img.as_raw().clone()],
        name,
    )
}

impl MobResources {
    /// `sampler` : NEAREST/REPEAT ; `atlas_sampler` : celui des atlas (§5.4).
    pub(super) fn new(
        ctx: &GpuContext,
        texture_set_layout: vk::DescriptorSetLayout,
        sampler: vk::Sampler,
        atlas_sampler: vk::Sampler,
        particle_atlas_cpu: TextureArrayCpu,
        roots: MobRoots,
        models: HashMap<String, MobModelCpu>,
    ) -> Result<Self> {
        let mut sets = TextureSets {
            layout: texture_set_layout,
            pools: vec![TextureSets::new_pool(ctx)?],
        };
        let white_img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]));
        let white = upload_rgba(ctx, &white_img, "mob-white")?;
        let white_set = sets.alloc(ctx, white.view, sampler)?;
        let particle_atlas = upload_atlas(ctx, &particle_atlas_cpu)?;
        let particle_set = sets.alloc(ctx, particle_atlas.view, atlas_sampler)?;
        let particle_quad = Buffer::with_data(
            ctx,
            &PARTICLE_QUAD,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            "particle-quad",
        )?;
        let missing_cpu =
            MobModelCpu::from_lists(missing_cube(), Vec::new(), Some(missing_texture()))
                .expect("cube manquant");
        let mut r = Self {
            missing: Self::upload_model(ctx, &mut sets, sampler, &missing_cpu)?,
            sets,
            sampler,
            white,
            white_set,
            models: HashMap::new(),
            particle_atlas_cpu,
            particle_atlas,
            particle_set,
            particle_quad,
            roots,
        };
        for (id, m) in models {
            let g = Self::upload_model(ctx, &mut r.sets, sampler, &m)?;
            r.models.insert(id, g);
        }
        Ok(r)
    }

    fn upload_model(
        ctx: &GpuContext,
        sets: &mut TextureSets,
        sampler: vk::Sampler,
        m: &MobModelCpu,
    ) -> Result<GpuModel> {
        let mut buffer = Buffer::with_data(
            ctx,
            &m.vertices,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            "mob-model",
        )?;
        let texture = match &m.texture {
            None => None,
            Some(img) => {
                let tex = upload_rgba(ctx, img, "mob-texture").and_then(|mut tex| {
                    match sets.alloc(ctx, tex.view, sampler) {
                        Ok(s) => Ok((tex, s)),
                        Err(e) => {
                            tex.destroy(ctx);
                            Err(e)
                        }
                    }
                });
                match tex {
                    Ok(t) => Some(t),
                    Err(e) => {
                        buffer.destroy(ctx);
                        return Err(e);
                    }
                }
            }
        };
        Ok(GpuModel {
            buffer,
            opaque: m.opaque_count,
            transparent: m.transparent_count,
            texture,
        })
    }

    fn destroy_model(sets: &TextureSets, ctx: &GpuContext, mut m: GpuModel) {
        m.buffer.destroy(ctx);
        if let Some((mut img, set)) = m.texture.take() {
            sets.free(ctx, set);
            img.destroy(ctx);
        }
    }

    pub(super) fn roots(&self) -> &MobRoots {
        &self.roots
    }

    pub(super) fn particle_atlas(&self) -> &TextureArrayCpu {
        &self.particle_atlas_cpu
    }

    /// `(opaques, transparents)` du modèle chargé pour `id`.
    pub(super) fn model_counts(&self, id: &str) -> Option<(u32, u32)> {
        self.models.get(id).map(|m| (m.opaque, m.transparent))
    }

    /// `loadMob` : remplace (et détruit) l'ancien modèle. L'appelant garantit que le GPU est
    /// inactif.
    pub(super) fn replace(&mut self, ctx: &GpuContext, id: &str, m: &MobModelCpu) -> Result<()> {
        let g = Self::upload_model(ctx, &mut self.sets, self.sampler, m)?;
        if let Some(old) = self.models.insert(id.to_owned(), g) {
            Self::destroy_model(&self.sets, ctx, old);
        }
        Ok(())
    }

    /// Changement de pack : tous les modèles sont détruits puis rechargés (`loadAll`).
    /// L'appelant garantit que le GPU est inactif.
    pub(super) fn replace_all(
        &mut self,
        ctx: &GpuContext,
        roots: MobRoots,
        models: HashMap<String, MobModelCpu>,
    ) -> Result<()> {
        for (_, old) in self.models.drain() {
            Self::destroy_model(&self.sets, ctx, old);
        }
        self.roots = roots;
        for (id, m) in models {
            let g = Self::upload_model(ctx, &mut self.sets, self.sampler, &m)?;
            self.models.insert(id, g);
        }
        Ok(())
    }

    /// Mobs puis particules (§6 étapes 9–10), dans une passe dont la profondeur HDR est
    /// attachée en écriture.
    ///
    /// # Safety
    /// `cmd` en cours d'enregistrement dans une passe compatible avec les pipelines B2.
    pub(super) unsafe fn record(
        &self,
        ctx: &GpuContext,
        cmd: vk::CommandBuffer,
        p: &Pipelines,
        frame_set: vk::DescriptorSet,
        view: &Mat4,
        input: &FrameInput<'_>,
    ) {
        let (mobs, particles) = (input.mobs, input.particles);
        let d = &ctx.device;
        let g = vk::PipelineBindPoint::GRAPHICS;
        // SAFETY : garanti par l'appelant ; ressources vivantes jusqu'à la fin de la frame
        // (remplacements après attente du GPU).
        unsafe {
            if !mobs.is_empty() {
                d.cmd_bind_descriptor_sets(cmd, g, p.mob_layout, 0, &[frame_set], &[]);
                let mut bound = vk::Pipeline::null();
                let mut bind = |pipeline: vk::Pipeline| {
                    if bound != pipeline {
                        d.cmd_bind_pipeline(cmd, g, pipeline);
                        bound = pipeline;
                    }
                };
                for m in mobs {
                    // Modèle absent → cube manquant mis à l'échelle de la config (§14.1).
                    let (model, scale) = match self.models.get(m.model_id) {
                        Some(model) => (model, None),
                        None => (&self.missing, Some(m.missing_scale)),
                    };
                    let mat = mob_model_matrix(m.position, m.yaw_rad, m.pitch_rad, scale);
                    let set = model
                        .texture
                        .as_ref()
                        .map_or(self.white_set.set, |(_, s)| s.set);
                    d.cmd_bind_descriptor_sets(cmd, g, p.mob_layout, 1, &[set], &[]);
                    d.cmd_push_constants(
                        cmd,
                        p.mob_layout,
                        vk::ShaderStageFlags::VERTEX,
                        0,
                        bytemuck::cast_slice(&mat.to_cols_array()),
                    );
                    d.cmd_bind_vertex_buffers(cmd, 0, &[model.buffer.raw], &[0]);
                    // Par mob : plage opaque puis plage transparente (pas de tri global).
                    if model.opaque > 0 {
                        bind(p.mob_opaque);
                        d.cmd_draw(cmd, model.opaque, 1, 0, 0);
                    }
                    if model.transparent > 0 {
                        bind(p.mob_transparent);
                        d.cmd_draw(cmd, model.transparent, 1, model.opaque, 0);
                    }
                }
            }
            if !particles.is_empty() {
                d.cmd_bind_pipeline(cmd, g, p.particle);
                d.cmd_bind_descriptor_sets(
                    cmd,
                    g,
                    p.particle_layout,
                    0,
                    &[frame_set, self.particle_set.set],
                    &[],
                );
                d.cmd_bind_vertex_buffers(cmd, 0, &[self.particle_quad.raw], &[0]);
                // Pas de tri (ordre du gestionnaire de particules).
                for q in particles {
                    let push = ParticlePush {
                        model: particle_model_matrix(q.position, view, q.scale).to_cols_array(),
                        color_tint: q.color.to_array(),
                        layer: q.layer as i32,
                    };
                    d.cmd_push_constants(
                        cmd,
                        p.particle_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytemuck::bytes_of(&push),
                    );
                    d.cmd_draw(cmd, PARTICLE_QUAD.len() as u32, 1, 0, 0);
                }
            }
        }
    }

    /// Le GPU doit être inactif.
    pub(super) fn destroy(&mut self, ctx: &GpuContext) {
        for (_, m) in self.models.drain() {
            Self::destroy_model(&self.sets, ctx, m);
        }
        self.missing.buffer.destroy(ctx);
        if let Some((mut img, _)) = self.missing.texture.take() {
            img.destroy(ctx);
        }
        self.white.destroy(ctx);
        self.particle_atlas.destroy(ctx);
        self.particle_quad.destroy(ctx);
        self.sets.destroy(ctx);
    }
}
