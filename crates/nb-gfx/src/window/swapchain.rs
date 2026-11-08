//! Swapchain `VK_KHR_swapchain` (format UNORM, mode de présentation selon la vsync, §3.1, §18.3).

use ash::vk;

use crate::vk::{GfxError, GpuContext, Result};

pub struct Swapchain {
    pub loader: ash::khr::swapchain::Device,
    pub raw: vk::SwapchainKHR,
    pub images: Vec<vk::Image>,
    pub views: Vec<vk::ImageView>,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub present_mode: vk::PresentModeKHR,
}

/// Choix du mode : vsync → FIFO ; sinon IMMEDIATE, à défaut MAILBOX (déviation), repli FIFO.
pub fn choose_present_mode(available: &[vk::PresentModeKHR], vsync: bool) -> vk::PresentModeKHR {
    // Diagnostic : `NB_PRESENT=fifo|mailbox|immediate` force le mode (s'il est disponible).
    if let Ok(v) = std::env::var("NB_PRESENT") {
        let forced = match v.to_ascii_lowercase().as_str() {
            "fifo" => Some(vk::PresentModeKHR::FIFO),
            "mailbox" => Some(vk::PresentModeKHR::MAILBOX),
            "immediate" => Some(vk::PresentModeKHR::IMMEDIATE),
            _ => None,
        };
        if let Some(m) = forced.filter(|m| available.contains(m)) {
            return m;
        }
    }
    if vsync {
        return vk::PresentModeKHR::FIFO;
    }
    [vk::PresentModeKHR::IMMEDIATE, vk::PresentModeKHR::MAILBOX]
        .into_iter()
        .find(|m| available.contains(m))
        .unwrap_or(vk::PresentModeKHR::FIFO)
}

/// Format UNORM (jamais sRGB : le composite applique lui-même le gamma).
pub fn choose_format(available: &[vk::SurfaceFormatKHR]) -> Option<vk::SurfaceFormatKHR> {
    [vk::Format::B8G8R8A8_UNORM, vk::Format::R8G8B8A8_UNORM]
        .into_iter()
        .find_map(|f| {
            available
                .iter()
                .find(|s| s.format == f && s.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
                .copied()
        })
}

/// Taille de la surface à utiliser (`None` si nulle : fenêtre minimisée).
pub fn surface_extent(
    caps: &vk::SurfaceCapabilitiesKHR,
    wanted: (u32, u32),
) -> Option<vk::Extent2D> {
    let e = if caps.current_extent.width != u32::MAX {
        caps.current_extent
    } else {
        vk::Extent2D {
            width: wanted
                .0
                .clamp(caps.min_image_extent.width, caps.max_image_extent.width),
            height: wanted
                .1
                .clamp(caps.min_image_extent.height, caps.max_image_extent.height),
        }
    };
    (e.width > 0 && e.height > 0).then_some(e)
}

impl Swapchain {
    /// Crée (ou recrée, en passant l'ancienne) la swapchain. `None` si la surface est de taille
    /// nulle.
    pub fn new(
        ctx: &GpuContext,
        surface: vk::SurfaceKHR,
        wanted: (u32, u32),
        vsync: bool,
        old: Option<&Swapchain>,
    ) -> Result<Option<Self>> {
        let sl = ctx
            .surface_loader()
            .ok_or_else(|| GfxError::Other("contexte sans surface".into()))?;
        // SAFETY : GPU et surface valides.
        let (caps, formats, modes) = unsafe {
            (
                sl.get_physical_device_surface_capabilities(ctx.physical, surface)?,
                sl.get_physical_device_surface_formats(ctx.physical, surface)?,
                sl.get_physical_device_surface_present_modes(ctx.physical, surface)?,
            )
        };
        let Some(extent) = surface_extent(&caps, wanted) else {
            return Ok(None);
        };
        let format = choose_format(&formats)
            .ok_or_else(|| GfxError::Other("aucun format de swapchain UNORM".into()))?;
        let present_mode = choose_present_mode(&modes, vsync);
        let mut count = caps.min_image_count + 1;
        if caps.max_image_count > 0 {
            count = count.min(caps.max_image_count);
        }
        let alpha = [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::INHERIT,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        ]
        .into_iter()
        .find(|a| caps.supported_composite_alpha.contains(*a))
        .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE);
        let loader = ash::khr::swapchain::Device::new(&ctx.instance, &ctx.device);
        let ci = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old.map_or(vk::SwapchainKHR::null(), |o| o.raw));
        // SAFETY : structures valides ; l'ancienne swapchain est détruite par l'appelant ensuite.
        let raw = unsafe { loader.create_swapchain(&ci, None)? };
        // SAFETY : swapchain fraîchement créée.
        let images = unsafe { loader.get_swapchain_images(raw)? };
        let mut views = Vec::with_capacity(images.len());
        for &image in &images {
            let vci = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format.format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            // SAFETY : image de la swapchain.
            views.push(unsafe { ctx.device.create_image_view(&vci, None)? });
        }
        Ok(Some(Self {
            loader,
            raw,
            images,
            views,
            format: format.format,
            extent,
            present_mode,
        }))
    }

    pub fn destroy(&mut self, ctx: &GpuContext) {
        // SAFETY : l'appelant a attendu la fin du travail GPU.
        unsafe {
            for v in self.views.drain(..) {
                ctx.device.destroy_image_view(v, None);
            }
            self.loader.destroy_swapchain(self.raw, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_mode_policy() {
        use vk::PresentModeKHR as P;
        assert_eq!(choose_present_mode(&[P::FIFO, P::IMMEDIATE], true), P::FIFO);
        assert_eq!(
            choose_present_mode(&[P::FIFO, P::IMMEDIATE], false),
            P::IMMEDIATE
        );
        assert_eq!(
            choose_present_mode(&[P::FIFO, P::MAILBOX], false),
            P::MAILBOX
        );
        assert_eq!(choose_present_mode(&[P::FIFO], false), P::FIFO);
    }

    #[test]
    fn format_is_never_srgb() {
        let f = |format| vk::SurfaceFormatKHR {
            format,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        };
        assert_eq!(
            choose_format(&[f(vk::Format::B8G8R8A8_SRGB), f(vk::Format::B8G8R8A8_UNORM)])
                .map(|s| s.format),
            Some(vk::Format::B8G8R8A8_UNORM)
        );
        assert_eq!(choose_format(&[f(vk::Format::B8G8R8A8_SRGB)]), None);
    }

    #[test]
    fn zero_extent_is_skipped() {
        let caps = vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: 0,
                height: 0,
            },
            ..Default::default()
        };
        assert_eq!(surface_extent(&caps, (1280, 720)), None);
        let caps = vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: u32::MAX,
                height: u32::MAX,
            },
            min_image_extent: vk::Extent2D {
                width: 1,
                height: 1,
            },
            max_image_extent: vk::Extent2D {
                width: 1000,
                height: 1000,
            },
            ..Default::default()
        };
        assert_eq!(
            surface_extent(&caps, (1280, 720)),
            Some(vk::Extent2D {
                width: 1000,
                height: 720
            })
        );
    }
}
