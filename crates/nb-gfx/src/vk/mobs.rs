//! Ressources GPU et enregistrement de la passe scène B2 (spec §14.1, §15, §18.4.4) : modèles
//! de mobs (tampon de sommets + texture), cube manquant, texture blanche, atlas et quad des
//! particules.

use std::collections::HashMap;

use ash::vk;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use nb_mesh::Frustum;

use super::pipelines::Pipelines;
use super::resources::{Buffer, Image, ImageDesc};
use super::scene::upload_atlas;
use super::{GpuContext, Result};
use crate::atlas::TextureArrayCpu;
use crate::frame::{FrameInput, MobInstance};
use crate::mob_model::{
    missing_cube, missing_texture, mob_model_matrix, particle_model_matrix, MobModelCpu, MobRoots,
    MobVertex, PARTICLE_QUAD,
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

/// Sommet envoyé au GPU : `MobVertex` + os porteur (44 o, binding 0).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    v: MobVertex,
    bone: u32,
}

const _: () = assert!(std::mem::size_of::<GpuVertex>() == 44);

/// Données par instance (80 o, binding 1) : matrice modèle (colonnes) et paramètres
/// `(lumière, indice de la 1re matrice de la palette, 0, 0)`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceData {
    model: [f32; 16],
    params: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<InstanceData>() == 80);

/// Palette identité placée en tête du tampon : os 0..`IDENTITY_BONES` des instances sans pose.
const IDENTITY_BONES: usize = nb_anim::MAX_BONES;
const IDENTITY: [f32; 12] = nb_anim::Mat34::IDENTITY.0;
/// Capacités initiales (matrices, instances) des tampons par frame ; ils grandissent à la demande.
const INITIAL_MATRICES: usize = 2048;
const INITIAL_INSTANCES: usize = 256;

/// Sphère englobante d'un modèle (espace modèle), élargie pour contenir les poses animées.
#[derive(Clone, Copy, Debug)]
struct Bounds {
    center: Vec3,
    radius: f32,
}

struct GpuModel {
    buffer: Buffer,
    opaque: u32,
    transparent: u32,
    /// `None` : mode couleur (texture blanche).
    texture: Option<(Image, TexSet)>,
    bounds: Bounds,
    /// Nombre d'os référencés par les sommets.
    bone_count: usize,
}

/// Tampons d'une frame en vol : palette des os, instances, set de descripteurs (set 2).
struct MobSlot {
    palette: Buffer,
    palette_cap: usize,
    instances: Buffer,
    instance_cap: usize,
    set: vk::DescriptorSet,
}

/// Un dessin instancié : `count` mobs du modèle `model`, à partir de l'instance `first`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Draw {
    model: usize,
    first: u32,
    count: u32,
}

/// Assets GPU des mobs et des particules.
pub(super) struct MobResources {
    sets: TextureSets,
    /// NEAREST / REPEAT (textures d'entité, blanc, damier) ; appartient au `SceneRenderer`.
    sampler: vk::Sampler,
    white: Image,
    white_set: TexSet,
    /// Modèles par indice ; l'indice 0 est le cube manquant.
    models: Vec<GpuModel>,
    index: HashMap<String, usize>,
    particle_atlas_cpu: TextureArrayCpu,
    particle_atlas: Image,
    particle_set: TexSet,
    particle_quad: Buffer,
    roots: MobRoots,
    // -- Palette, instances et dessins.
    slots: Vec<MobSlot>,
    slot_pool: vk::DescriptorPool,
    slot_layout: vk::DescriptorSetLayout,
    /// Dessins de la frame en cours (remplis par `prepare`).
    draws: Vec<Draw>,
    palette_cpu: Vec<[f32; 12]>,
    instances_cpu: Vec<(usize, InstanceData)>,
    instances_sorted: Vec<InstanceData>,
    /// Mobs réellement dessinés à la dernière frame (hors frustum exclus), pour les tests.
    last_drawn: usize,
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

fn bounds_of(m: &MobModelCpu) -> Bounds {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for v in &m.vertices {
        lo = lo.min(Vec3::from(v.pos));
        hi = hi.max(Vec3::from(v.pos));
    }
    let center = (lo + hi) * 0.5;
    // Marge d'animation : membres qui balancent, tête baissée, queue…
    Bounds {
        center,
        radius: (hi - lo).length() * 0.5 * 1.3 + 0.25,
    }
}

fn new_slot(
    ctx: &GpuContext,
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
    matrices: usize,
    instances: usize,
) -> Result<MobSlot> {
    let palette = Buffer::new(
        ctx,
        (matrices * 48) as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        gpu_allocator::MemoryLocation::CpuToGpu,
        "mob-palette",
    )?;
    let instances_buf = Buffer::new(
        ctx,
        (instances * std::mem::size_of::<InstanceData>()) as u64,
        vk::BufferUsageFlags::VERTEX_BUFFER,
        gpu_allocator::MemoryLocation::CpuToGpu,
        "mob-instances",
    )?;
    let layouts = [layout];
    let ai = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(pool)
        .set_layouts(&layouts);
    // SAFETY : pool dimensionné pour un set par créneau.
    let set = unsafe { ctx.device.allocate_descriptor_sets(&ai)? }[0];
    let slot = MobSlot {
        palette,
        palette_cap: matrices,
        instances: instances_buf,
        instance_cap: instances,
        set,
    };
    write_palette_set(ctx, &slot);
    Ok(slot)
}

fn write_palette_set(ctx: &GpuContext, slot: &MobSlot) {
    let info = [vk::DescriptorBufferInfo {
        buffer: slot.palette.raw,
        offset: 0,
        range: vk::WHOLE_SIZE,
    }];
    let write = vk::WriteDescriptorSet::default()
        .dst_set(slot.set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .buffer_info(&info);
    // SAFETY : le créneau n'est plus utilisé par le GPU (fence de la frame précédente attendu).
    unsafe { ctx.device.update_descriptor_sets(&[write], &[]) };
}

impl MobResources {
    /// `sampler` : NEAREST/REPEAT ; `atlas_sampler` : celui des atlas (§5.4) ; `slot_layout` :
    /// set 2 (palette) ; `slots` : nombre de frames en vol.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        ctx: &GpuContext,
        texture_set_layout: vk::DescriptorSetLayout,
        slot_layout: vk::DescriptorSetLayout,
        slots: usize,
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
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: slots as u32,
        }];
        let pool_ci = vk::DescriptorPoolCreateInfo::default()
            .max_sets(slots as u32)
            .pool_sizes(&sizes);
        // SAFETY : device valide.
        let slot_pool = unsafe { ctx.device.create_descriptor_pool(&pool_ci, None)? };
        let mut slot_bufs = Vec::with_capacity(slots);
        for _ in 0..slots {
            slot_bufs.push(new_slot(
                ctx,
                slot_pool,
                slot_layout,
                INITIAL_MATRICES,
                INITIAL_INSTANCES,
            )?);
        }
        let mut r = Self {
            models: vec![Self::upload_model(ctx, &mut sets, sampler, &missing_cpu)?],
            index: HashMap::new(),
            sets,
            sampler,
            white,
            white_set,
            particle_atlas_cpu,
            particle_atlas,
            particle_set,
            particle_quad,
            roots,
            slots: slot_bufs,
            slot_pool,
            slot_layout,
            draws: Vec::new(),
            palette_cpu: Vec::new(),
            instances_cpu: Vec::new(),
            instances_sorted: Vec::new(),
            last_drawn: 0,
        };
        for (id, m) in models {
            let g = Self::upload_model(ctx, &mut r.sets, sampler, &m)?;
            r.insert_model(&id, g, ctx);
        }
        Ok(r)
    }

    /// Place un modèle sous `id` (en remplaçant et détruisant l'ancien).
    fn insert_model(&mut self, id: &str, g: GpuModel, ctx: &GpuContext) {
        match self.index.get(id) {
            Some(&i) => {
                let old = std::mem::replace(&mut self.models[i], g);
                Self::destroy_model(&self.sets, ctx, old);
            }
            None => {
                self.index.insert(id.to_owned(), self.models.len());
                self.models.push(g);
            }
        }
    }

    fn upload_model(
        ctx: &GpuContext,
        sets: &mut TextureSets,
        sampler: vk::Sampler,
        m: &MobModelCpu,
    ) -> Result<GpuModel> {
        let max_bone = (IDENTITY_BONES - 1) as u32;
        let verts: Vec<GpuVertex> = m
            .vertices
            .iter()
            .enumerate()
            .map(|(i, &v)| GpuVertex {
                v,
                bone: m.bones.get(i).copied().unwrap_or(0).min(max_bone),
            })
            .collect();
        let mut buffer = Buffer::with_data(
            ctx,
            &verts,
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
            bounds: bounds_of(m),
            bone_count: m.bone_count().min(IDENTITY_BONES),
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
        self.index
            .get(id)
            .map(|&i| (self.models[i].opaque, self.models[i].transparent))
    }

    /// Mobs dessinés à la dernière frame (après le culling par frustum).
    pub(super) fn drawn_last_frame(&self) -> usize {
        self.last_drawn
    }

    /// `loadMob` : remplace (et détruit) l'ancien modèle. L'appelant garantit que le GPU est
    /// inactif.
    pub(super) fn replace(&mut self, ctx: &GpuContext, id: &str, m: &MobModelCpu) -> Result<()> {
        let g = Self::upload_model(ctx, &mut self.sets, self.sampler, m)?;
        self.insert_model(id, g, ctx);
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
        // L'indice 0 (cube manquant) reste ; les autres sont détruits.
        for old in self.models.drain(1..) {
            Self::destroy_model(&self.sets, ctx, old);
        }
        self.index.clear();
        self.roots = roots;
        for (id, m) in models {
            let g = Self::upload_model(ctx, &mut self.sets, self.sampler, &m)?;
            self.insert_model(&id, g, ctx);
        }
        Ok(())
    }

    /// Prépare le dessin des mobs de la frame : culling par frustum, regroupement par modèle,
    /// palette des os et instances écrites dans les tampons du créneau `slot` (le GPU a fini de
    /// lire ce créneau : fence de la frame précédente attendu).
    pub(super) fn prepare(
        &mut self,
        ctx: &GpuContext,
        slot: usize,
        mobs: &[MobInstance<'_>],
        frustum: &Frustum,
    ) -> Result<()> {
        self.draws.clear();
        self.palette_cpu.clear();
        self.palette_cpu
            .extend(std::iter::repeat_n(IDENTITY, IDENTITY_BONES));
        self.instances_cpu.clear();
        for m in mobs {
            // Modèle absent → cube manquant mis à l'échelle de la config (§14.1).
            let (mi, scale) = match self.index.get(m.model_id) {
                Some(&i) => (i, None),
                None => (0, Some(m.missing_scale)),
            };
            let model = &self.models[mi];
            let mat = mob_model_matrix(m.position, m.yaw_rad, m.pitch_rad, scale);
            // Culling : sphère englobante (agrandie par la plus grande échelle du modèle).
            let c = mat.transform_point3(model.bounds.center);
            let s = mat
                .x_axis
                .truncate()
                .length()
                .max(mat.y_axis.truncate().length())
                .max(mat.z_axis.truncate().length());
            let r = model.bounds.radius * s;
            if !frustum.test_aab((c - r).to_array(), (c + r).to_array()) {
                continue;
            }
            let base = if m.pose.is_empty() {
                0
            } else {
                let base = self.palette_cpu.len();
                self.palette_cpu.extend_from_slice(m.pose);
                if m.pose.len() < model.bone_count {
                    self.palette_cpu.extend(std::iter::repeat_n(
                        IDENTITY,
                        model.bone_count - m.pose.len(),
                    ));
                }
                base
            };
            self.instances_cpu.push((
                mi,
                InstanceData {
                    model: mat.to_cols_array(),
                    params: [m.light, base as f32, m.water_surface, 0.0],
                },
            ));
        }
        self.last_drawn = self.instances_cpu.len();
        // Regroupement par modèle (tri stable : l'ordre des mobs est conservé dans un groupe).
        self.instances_cpu.sort_by_key(|&(mi, _)| mi);
        self.instances_sorted.clear();
        for &(mi, d) in &self.instances_cpu {
            match self.draws.last_mut() {
                Some(dr) if dr.model == mi => dr.count += 1,
                _ => self.draws.push(Draw {
                    model: mi,
                    first: self.instances_sorted.len() as u32,
                    count: 1,
                }),
            }
            self.instances_sorted.push(d);
        }
        // Tampons du créneau (agrandis si besoin ; set de descripteurs mis à jour).
        let s = &mut self.slots[slot];
        if self.palette_cpu.len() > s.palette_cap {
            let cap = self.palette_cpu.len().next_power_of_two();
            s.palette.destroy(ctx);
            s.palette = Buffer::new(
                ctx,
                (cap * 48) as u64,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                gpu_allocator::MemoryLocation::CpuToGpu,
                "mob-palette",
            )?;
            s.palette_cap = cap;
            write_palette_set(ctx, s);
        }
        if self.instances_sorted.len() > s.instance_cap {
            let cap = self.instances_sorted.len().next_power_of_two();
            s.instances.destroy(ctx);
            s.instances = Buffer::new(
                ctx,
                (cap * std::mem::size_of::<InstanceData>()) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                gpu_allocator::MemoryLocation::CpuToGpu,
                "mob-instances",
            )?;
            s.instance_cap = cap;
        }
        s.palette
            .write(0, bytemuck::cast_slice(&self.palette_cpu))?;
        if !self.instances_sorted.is_empty() {
            s.instances
                .write(0, bytemuck::cast_slice(&self.instances_sorted))?;
        }
        Ok(())
    }

    /// Mobs puis particules (§6 étapes 9–10), dans une passe dont la profondeur HDR est
    /// attachée en écriture. [`Self::prepare`] doit avoir été appelé pour cette frame.
    ///
    /// # Safety
    /// `cmd` en cours d'enregistrement dans une passe compatible avec les pipelines B2.
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn record(
        &self,
        ctx: &GpuContext,
        cmd: vk::CommandBuffer,
        p: &Pipelines,
        frame_set: vk::DescriptorSet,
        view: &Mat4,
        input: &FrameInput<'_>,
        slot: usize,
    ) {
        let particles = input.particles;
        let d = &ctx.device;
        let g = vk::PipelineBindPoint::GRAPHICS;
        let s = &self.slots[slot];
        // SAFETY : garanti par l'appelant ; ressources vivantes jusqu'à la fin de la frame
        // (remplacements après attente du GPU).
        unsafe {
            if !self.draws.is_empty() {
                d.cmd_bind_descriptor_sets(cmd, g, p.mob_layout, 0, &[frame_set], &[]);
                d.cmd_bind_descriptor_sets(cmd, g, p.mob_layout, 2, &[s.set], &[]);
                d.cmd_bind_vertex_buffers(cmd, 1, &[s.instances.raw], &[0]);
                // Toutes les plages opaques d'abord (écriture de profondeur), puis toutes les
                // translucides (sans tri global, comme avant).
                for transparent in [false, true] {
                    d.cmd_bind_pipeline(
                        cmd,
                        g,
                        if transparent {
                            p.mob_transparent
                        } else {
                            p.mob_opaque
                        },
                    );
                    for dr in &self.draws {
                        let model = &self.models[dr.model];
                        let (first_vertex, count) = if transparent {
                            (model.opaque, model.transparent)
                        } else {
                            (0, model.opaque)
                        };
                        if count == 0 {
                            continue;
                        }
                        let set = model
                            .texture
                            .as_ref()
                            .map_or(self.white_set.set, |(_, s)| s.set);
                        d.cmd_bind_descriptor_sets(cmd, g, p.mob_layout, 1, &[set], &[]);
                        d.cmd_bind_vertex_buffers(cmd, 0, &[model.buffer.raw], &[0]);
                        d.cmd_draw(cmd, count, dr.count, first_vertex, dr.first);
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
        for m in self.models.drain(..) {
            Self::destroy_model(&self.sets, ctx, m);
        }
        for s in &mut self.slots {
            s.palette.destroy(ctx);
            s.instances.destroy(ctx);
        }
        // SAFETY : GPU inactif ; les sets sont libérés avec leur pool.
        unsafe { ctx.device.destroy_descriptor_pool(self.slot_pool, None) };
        let _ = self.slot_layout;
        self.white.destroy(ctx);
        self.particle_atlas.destroy(ctx);
        self.particle_quad.destroy(ctx);
        self.sets.destroy(ctx);
    }
}
