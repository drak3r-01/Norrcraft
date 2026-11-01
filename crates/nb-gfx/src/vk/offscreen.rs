//! Rendu hors écran (sans fenêtre ni swapchain) avec relecture de l'image : tests headless,
//! captures de référence.

use std::sync::Arc;

use ash::vk;
use gpu_allocator::MemoryLocation;
use image::RgbaImage;

use super::resources::{Buffer, Image, ImageDesc, Transition};
use super::scene::{OutputTarget, OverlayPass, SceneAssets, SceneRenderer, OUTPUT_FORMAT};
use super::{GpuContext, Result};
use crate::frame::{FrameInput, GfxSettings};

/// [`SceneRenderer`] + image de sortie `B8G8R8A8_UNORM` relue en RGBA8.
pub struct OffscreenRenderer {
    pub scene: SceneRenderer,
    output: Image,
    readback: Buffer,
}

impl OffscreenRenderer {
    pub fn new(
        ctx: Arc<GpuContext>,
        assets: SceneAssets,
        width: u32,
        height: u32,
        settings: &GfxSettings,
    ) -> Result<Self> {
        let scene = SceneRenderer::new(ctx.clone(), assets, OUTPUT_FORMAT, settings)?;
        let (output, readback) = Self::create_output(&ctx, width, height)?;
        Ok(Self {
            scene,
            output,
            readback,
        })
    }

    fn create_output(ctx: &GpuContext, width: u32, height: u32) -> Result<(Image, Buffer)> {
        let output = Image::new(
            ctx,
            ImageDesc::color_2d(
                width,
                height,
                OUTPUT_FORMAT,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
            ),
            "offscreen-output",
        )?;
        let readback = Buffer::new(
            ctx,
            u64::from(width) * u64::from(height) * 4,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
            "readback",
        )?;
        Ok((output, readback))
    }

    pub fn extent(&self) -> (u32, u32) {
        (self.output.desc.width, self.output.desc.height)
    }

    /// Change la taille de sortie (les cibles HDR suivent à la frame suivante).
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if self.extent() == (width, height) {
            return Ok(());
        }
        let ctx = self.scene.context().clone();
        ctx.wait_idle();
        self.output.destroy(&ctx);
        self.readback.destroy(&ctx);
        let (o, r) = Self::create_output(&ctx, width, height)?;
        self.output = o;
        self.readback = r;
        Ok(())
    }

    /// Rend une frame et la relit (RGBA8, ligne 0 = haut de l'image).
    pub fn render(&mut self, input: &FrameInput<'_>) -> Result<RgbaImage> {
        self.render_with_overlay(input, &mut |_| {})
    }

    pub fn render_with_overlay(
        &mut self,
        input: &FrameInput<'_>,
        overlay: &mut dyn FnMut(&mut OverlayPass<'_>),
    ) -> Result<RgbaImage> {
        let ctx = self.scene.context().clone();
        let (w, h) = self.extent();
        let token = self.scene.begin_frame()?;
        let out = OutputTarget {
            image: self.output.raw,
            view: self.output.view,
            extent: self.output.extent(),
            final_state: Transition::TRANSFER_SRC,
        };
        self.scene.record_frame(token, input, &out, overlay)?;
        let region = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_extent(vk::Extent3D {
                width: w,
                height: h,
                depth: 1,
            });
        let host = [vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.readback.raw)
            .offset(0)
            .size(vk::WHOLE_SIZE)];
        // SAFETY : tampon de commandes ouvert ; image en TRANSFER_SRC après record_frame.
        unsafe {
            ctx.device.cmd_copy_image_to_buffer(
                token.cmd,
                self.output.raw,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.readback.raw,
                &[region],
            );
            ctx.device.cmd_pipeline_barrier2(
                token.cmd,
                &vk::DependencyInfo::default().buffer_memory_barriers(&host),
            );
        }
        self.scene.submit_frame(token, &[], &[])?;
        self.scene.wait_slot(token.slot)?;
        let bgra = self.readback.read()?;
        let mut rgba = Vec::with_capacity(bgra.len());
        for p in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
        Ok(RgbaImage::from_raw(w, h, rgba).expect("taille de relecture"))
    }
}

impl Drop for OffscreenRenderer {
    fn drop(&mut self) {
        let ctx = self.scene.context().clone();
        ctx.wait_idle();
        self.output.destroy(&ctx);
        self.readback.destroy(&ctx);
    }
}
