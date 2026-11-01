//! Cibles de rendu (équivalents des FBO Java, spec §4) : HDR (2 couleurs + profondeur),
//! ping-pong du bloom en demi-résolution et shadow map.

use ash::vk;

use super::resources::{Image, ImageDesc};
use super::{GpuContext, Result};

pub const HDR_COLOR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

/// Cibles dépendant de la taille de la fenêtre.
pub struct HdrTargets {
    /// `colorBuffer`.
    pub color: Image,
    /// `brightBuffer` (source du bloom).
    pub bright: Image,
    /// `depthBuffer`, échantillonné par les fluides et le composite.
    pub depth: Image,
    /// `pingpong[0..2]`, `(W/2) × (H/2)`.
    pub ping: [Image; 2],
    pub extent: vk::Extent2D,
}

impl HdrTargets {
    pub fn new(ctx: &GpuContext, extent: vk::Extent2D) -> Result<Self> {
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let (w, h) = (extent.width, extent.height);
        let (hw, hh) = ((w / 2).max(1), (h / 2).max(1));
        Ok(Self {
            color: Image::new(
                ctx,
                ImageDesc::color_2d(w, h, HDR_COLOR_FORMAT, usage),
                "hdr-color",
            )?,
            bright: Image::new(
                ctx,
                ImageDesc::color_2d(w, h, HDR_COLOR_FORMAT, usage),
                "hdr-bright",
            )?,
            depth: Image::new(
                ctx,
                ImageDesc::depth_2d(
                    w,
                    h,
                    DEPTH_FORMAT,
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                ),
                "hdr-depth",
            )?,
            ping: [
                Image::new(
                    ctx,
                    ImageDesc::color_2d(hw, hh, HDR_COLOR_FORMAT, usage),
                    "ping0",
                )?,
                Image::new(
                    ctx,
                    ImageDesc::color_2d(hw, hh, HDR_COLOR_FORMAT, usage),
                    "ping1",
                )?,
            ],
            extent,
        })
    }

    pub fn destroy(&mut self, ctx: &GpuContext) {
        self.color.destroy(ctx);
        self.bright.destroy(ctx);
        self.depth.destroy(ctx);
        for p in &mut self.ping {
            p.destroy(ctx);
        }
    }
}

/// Shadow map carrée `N × N` (profondeur seule).
pub fn shadow_map(ctx: &GpuContext, size: u32) -> Result<Image> {
    Image::new(
        ctx,
        ImageDesc::depth_2d(
            size,
            size,
            DEPTH_FORMAT,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        ),
        "shadow-map",
    )
}
