//! Tampons et images alloués avec `gpu-allocator`, barrières et uploads.

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};
use gpu_allocator::MemoryLocation;

use super::{GfxError, GpuContext, Result};

/// Tampon Vulkan + allocation.
pub struct Buffer {
    pub raw: vk::Buffer,
    pub size: u64,
    alloc: Option<Allocation>,
}

impl Buffer {
    pub fn new(
        ctx: &GpuContext,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
        name: &str,
    ) -> Result<Self> {
        let ci = vk::BufferCreateInfo::default()
            .size(size.max(4))
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        // SAFETY : device valide, structure valide.
        let raw = unsafe { ctx.device.create_buffer(&ci, None)? };
        // SAFETY : tampon valide.
        let requirements = unsafe { ctx.device.get_buffer_memory_requirements(raw) };
        let alloc = ctx
            .allocator
            .lock()
            .expect("allocateur")
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            });
        let alloc = match alloc {
            Ok(a) => a,
            Err(e) => {
                // SAFETY : tampon jamais utilisé.
                unsafe { ctx.device.destroy_buffer(raw, None) };
                return Err(e.into());
            }
        };
        // SAFETY : mémoire compatible, offset fourni par l'allocateur.
        unsafe {
            ctx.device
                .bind_buffer_memory(raw, alloc.memory(), alloc.offset())?
        };
        Ok(Self {
            raw,
            size,
            alloc: Some(alloc),
        })
    }

    /// Tampon visible de l'hôte rempli avec `data`.
    pub fn with_data<T: bytemuck::Pod>(
        ctx: &GpuContext,
        data: &[T],
        usage: vk::BufferUsageFlags,
        name: &str,
    ) -> Result<Self> {
        let bytes: &[u8] = bytemuck::cast_slice(data);
        let mut b = Self::new(
            ctx,
            bytes.len() as u64,
            usage,
            MemoryLocation::CpuToGpu,
            name,
        )?;
        b.write(0, bytes)?;
        Ok(b)
    }

    /// Écrit dans un tampon mappé.
    pub fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<()> {
        let slice = self
            .alloc
            .as_mut()
            .and_then(|a| a.mapped_slice_mut())
            .ok_or_else(|| GfxError::Other("tampon non mappé".into()))?;
        slice[offset..offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// Lit un tampon mappé.
    pub fn read(&self) -> Result<&[u8]> {
        self.alloc
            .as_ref()
            .and_then(|a| a.mapped_slice())
            .map(|s| &s[..self.size as usize])
            .ok_or_else(|| GfxError::Other("tampon non mappé".into()))
    }

    pub fn destroy(&mut self, ctx: &GpuContext) {
        if let Some(a) = self.alloc.take() {
            let _ = ctx.allocator.lock().expect("allocateur").free(a);
            // SAFETY : l'appelant garantit que le GPU n'utilise plus le tampon.
            unsafe { ctx.device.destroy_buffer(self.raw, None) };
        }
    }
}

/// Description d'une image 2D (ou tableau 2D).
#[derive(Clone, Copy, Debug)]
pub struct ImageDesc {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub mip_levels: u32,
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub aspect: vk::ImageAspectFlags,
    /// Vue `2D_ARRAY` (sinon `2D`).
    pub array_view: bool,
}

impl ImageDesc {
    pub fn color_2d(
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Self {
        Self {
            width,
            height,
            layers: 1,
            mip_levels: 1,
            format,
            usage,
            aspect: vk::ImageAspectFlags::COLOR,
            array_view: false,
        }
    }

    pub fn depth_2d(
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Self {
        Self {
            aspect: vk::ImageAspectFlags::DEPTH,
            ..Self::color_2d(width, height, format, usage)
        }
    }
}

/// Image + vue + allocation.
pub struct Image {
    pub raw: vk::Image,
    pub view: vk::ImageView,
    pub desc: ImageDesc,
    alloc: Option<Allocation>,
}

impl Image {
    pub fn new(ctx: &GpuContext, desc: ImageDesc, name: &str) -> Result<Self> {
        let ci = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(desc.format)
            .extent(vk::Extent3D {
                width: desc.width,
                height: desc.height,
                depth: 1,
            })
            .mip_levels(desc.mip_levels)
            .array_layers(desc.layers)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(desc.usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        // SAFETY : device valide, structure valide.
        let raw = unsafe { ctx.device.create_image(&ci, None)? };
        // SAFETY : image valide.
        let requirements = unsafe { ctx.device.get_image_memory_requirements(raw) };
        let alloc = ctx
            .allocator
            .lock()
            .expect("allocateur")
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            });
        let alloc = match alloc {
            Ok(a) => a,
            Err(e) => {
                // SAFETY : image jamais utilisée.
                unsafe { ctx.device.destroy_image(raw, None) };
                return Err(e.into());
            }
        };
        // SAFETY : mémoire compatible.
        unsafe {
            ctx.device
                .bind_image_memory(raw, alloc.memory(), alloc.offset())?
        };
        let view_ci = vk::ImageViewCreateInfo::default()
            .image(raw)
            .view_type(if desc.array_view {
                vk::ImageViewType::TYPE_2D_ARRAY
            } else {
                vk::ImageViewType::TYPE_2D
            })
            .format(desc.format)
            .subresource_range(full_range(&desc));
        // SAFETY : image liée à sa mémoire.
        let view = unsafe { ctx.device.create_image_view(&view_ci, None)? };
        Ok(Self {
            raw,
            view,
            desc,
            alloc: Some(alloc),
        })
    }

    pub fn extent(&self) -> vk::Extent2D {
        vk::Extent2D {
            width: self.desc.width,
            height: self.desc.height,
        }
    }

    pub fn range(&self) -> vk::ImageSubresourceRange {
        full_range(&self.desc)
    }

    pub fn destroy(&mut self, ctx: &GpuContext) {
        if let Some(a) = self.alloc.take() {
            // SAFETY : l'appelant garantit que le GPU n'utilise plus l'image.
            unsafe {
                ctx.device.destroy_image_view(self.view, None);
                ctx.device.destroy_image(self.raw, None);
            }
            let _ = ctx.allocator.lock().expect("allocateur").free(a);
        }
    }

    /// Crée une image échantillonnée et y copie `levels[mip]` (toutes les couches à la suite),
    /// puis la laisse en `SHADER_READ_ONLY_OPTIMAL`.
    pub fn upload(
        ctx: &GpuContext,
        desc: ImageDesc,
        levels: &[Vec<u8>],
        name: &str,
    ) -> Result<Self> {
        let desc = ImageDesc {
            usage: desc.usage | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            ..desc
        };
        let total: usize = levels.iter().map(Vec::len).sum();
        let mut staging = Buffer::new(
            ctx,
            total as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "staging",
        )?;
        let mut regions = Vec::with_capacity(levels.len());
        let mut offset = 0usize;
        for (mip, data) in levels.iter().enumerate() {
            staging.write(offset, data)?;
            regions.push(
                vk::BufferImageCopy::default()
                    .buffer_offset(offset as u64)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: desc.aspect,
                        mip_level: mip as u32,
                        base_array_layer: 0,
                        layer_count: desc.layers,
                    })
                    .image_extent(vk::Extent3D {
                        width: (desc.width >> mip).max(1),
                        height: (desc.height >> mip).max(1),
                        depth: 1,
                    }),
            );
            offset += data.len();
        }
        let image = Self::new(ctx, desc, name)?;
        let range = image.range();
        let r = ctx.run_once(|cmd| {
            barrier(
                ctx,
                cmd,
                image.raw,
                range,
                Transition::UNDEFINED,
                Transition::TRANSFER_DST,
            );
            // SAFETY : régions dans les bornes du tampon et de l'image.
            unsafe {
                ctx.device.cmd_copy_buffer_to_image(
                    cmd,
                    staging.raw,
                    image.raw,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &regions,
                )
            };
            barrier(
                ctx,
                cmd,
                image.raw,
                range,
                Transition::TRANSFER_DST,
                Transition::SHADER_READ,
            );
        });
        staging.destroy(ctx);
        r.map(|_| image)
    }
}

fn full_range(desc: &ImageDesc) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: desc.aspect,
        base_mip_level: 0,
        level_count: desc.mip_levels,
        base_array_layer: 0,
        layer_count: desc.layers,
    }
}

/// État d'une image pour une barrière (layout + étapes + accès).
#[derive(Clone, Copy, Debug)]
pub struct Transition {
    pub layout: vk::ImageLayout,
    pub stage: vk::PipelineStageFlags2,
    pub access: vk::AccessFlags2,
}

impl Transition {
    pub const UNDEFINED: Self = Self {
        layout: vk::ImageLayout::UNDEFINED,
        stage: vk::PipelineStageFlags2::TOP_OF_PIPE,
        access: vk::AccessFlags2::NONE,
    };
    pub const TRANSFER_DST: Self = Self {
        layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        stage: vk::PipelineStageFlags2::TRANSFER,
        access: vk::AccessFlags2::TRANSFER_WRITE,
    };
    pub const TRANSFER_SRC: Self = Self {
        layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        stage: vk::PipelineStageFlags2::TRANSFER,
        access: vk::AccessFlags2::TRANSFER_READ,
    };
    /// Échantillonnée dans un fragment shader.
    pub const SHADER_READ: Self = Self {
        layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        stage: vk::PipelineStageFlags2::FRAGMENT_SHADER,
        access: vk::AccessFlags2::SHADER_SAMPLED_READ,
    };
    pub const COLOR_ATTACHMENT: Self = Self {
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        access: vk::AccessFlags2::from_raw(
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE.as_raw()
                | vk::AccessFlags2::COLOR_ATTACHMENT_READ.as_raw(),
        ),
    };
    pub const DEPTH_ATTACHMENT: Self = Self {
        layout: vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
        stage: vk::PipelineStageFlags2::from_raw(
            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS.as_raw()
                | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS.as_raw(),
        ),
        access: vk::AccessFlags2::from_raw(
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE.as_raw()
                | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ.as_raw(),
        ),
    };
    /// Profondeur en lecture seule : attachement (tests) et/ou échantillonnée.
    pub const DEPTH_READ: Self = Self {
        layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
        stage: vk::PipelineStageFlags2::from_raw(
            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS.as_raw()
                | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS.as_raw()
                | vk::PipelineStageFlags2::FRAGMENT_SHADER.as_raw(),
        ),
        access: vk::AccessFlags2::from_raw(
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ.as_raw()
                | vk::AccessFlags2::SHADER_SAMPLED_READ.as_raw(),
        ),
    };
    pub const PRESENT: Self = Self {
        layout: vk::ImageLayout::PRESENT_SRC_KHR,
        stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        access: vk::AccessFlags2::NONE,
    };

    /// Même état, ancien contenu abandonné (layout `UNDEFINED`) mais en gardant les étapes
    /// pour la dépendance d'exécution (écriture après lecture).
    pub fn discard(self) -> Self {
        Self {
            layout: vk::ImageLayout::UNDEFINED,
            ..self
        }
    }
}

/// Barrière d'image (synchronization2).
pub fn barrier(
    ctx: &GpuContext,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    range: vk::ImageSubresourceRange,
    from: Transition,
    to: Transition,
) {
    let b = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(from.stage)
        .src_access_mask(from.access)
        .old_layout(from.layout)
        .dst_stage_mask(to.stage)
        .dst_access_mask(to.access)
        .new_layout(to.layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(range)];
    let dep = vk::DependencyInfo::default().image_memory_barriers(&b);
    // SAFETY : tampon de commandes en cours d'enregistrement.
    unsafe { ctx.device.cmd_pipeline_barrier2(cmd, &dep) };
}
