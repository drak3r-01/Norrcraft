//! Instance, device, file graphique et allocateur.

use std::ffi::{c_char, c_void, CStr};
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use ash::vk;
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};

use super::{GfxError, Result};

static VALIDATION_ERRORS: AtomicUsize = AtomicUsize::new(0);

/// Nombre total de messages d'erreur reçus des couches de validation (tous contextes).
pub fn validation_error_count() -> usize {
    VALIDATION_ERRORS.load(Ordering::SeqCst)
}

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _ty: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user: *mut c_void,
) -> vk::Bool32 {
    // SAFETY : pointeurs fournis par la couche de validation, valides pendant l'appel.
    let msg = unsafe {
        if data.is_null() || (*data).p_message.is_null() {
            String::new()
        } else {
            CStr::from_ptr((*data).p_message)
                .to_string_lossy()
                .into_owned()
        }
    };
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        VALIDATION_ERRORS.fetch_add(1, Ordering::SeqCst);
        eprintln!("[vulkan:error] {msg}");
    } else {
        eprintln!("[vulkan:warn] {msg}");
    }
    vk::FALSE
}

/// Contexte Vulkan partagé (à mettre dans un `Arc`).
pub struct GpuContext {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub physical: vk::PhysicalDevice,
    pub device: ash::Device,
    pub queue_family: u32,
    queue: Mutex<vk::Queue>,
    pub allocator: ManuallyDrop<Mutex<Allocator>>,
    command_pool: Mutex<vk::CommandPool>,
    /// `min(8, maxSamplerAnisotropy)` si `samplerAnisotropy` est disponible.
    pub max_anisotropy: Option<f32>,
    pub device_name: String,
    /// Couche `VK_LAYER_KHRONOS_validation` active (debug, ou `NB_GFX_VALIDATION=1`).
    pub validation_enabled: bool,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    surface_loader: Option<ash::khr::surface::Instance>,
}

/// Fabrique de surface appelée entre la création de l'instance et le choix du GPU.
pub type SurfaceFactory<'a> = &'a dyn Fn(&ash::Entry, &ash::Instance) -> Result<vk::SurfaceKHR>;

impl GpuContext {
    /// Contexte sans surface (rendu hors écran).
    pub fn new_headless() -> Result<Self> {
        Ok(Self::build(&[], None)?.0)
    }

    /// Contexte avec surface : `instance_extensions` vient de
    /// `ash_window::enumerate_required_extensions`. Le GPU choisi peut présenter sur la
    /// surface créée par `surface`, qui est rendue à l'appelant (à détruire avant le contexte
    /// avec [`GpuContext::surface_loader`]).
    pub fn with_surface(
        instance_extensions: &[*const c_char],
        surface: SurfaceFactory<'_>,
    ) -> Result<(Self, vk::SurfaceKHR)> {
        let (ctx, s) = Self::build(instance_extensions, Some(surface))?;
        Ok((ctx, s.expect("surface")))
    }

    fn build(
        extra_exts: &[*const c_char],
        surface_factory: Option<SurfaceFactory<'_>>,
    ) -> Result<(Self, Option<vk::SurfaceKHR>)> {
        // SAFETY : chargement dynamique de la bibliothèque Vulkan système.
        let entry = unsafe { load_entry()? };
        // SAFETY : appels Vulkan sur un `Entry` valide.
        let version =
            unsafe { entry.try_enumerate_instance_version()? }.unwrap_or(vk::API_VERSION_1_0);
        if version < vk::API_VERSION_1_3 {
            return Err(GfxError::NoDevice);
        }
        let want_validation =
            std::env::var("NB_GFX_VALIDATION").map_or(cfg!(debug_assertions), |v| v != "0");
        // SAFETY : énumération sur un `Entry` valide.
        let layers = unsafe { entry.enumerate_instance_layer_properties()? };
        let has_validation = layers
            .iter()
            .any(|l| l.layer_name_as_c_str().ok() == Some(VALIDATION_LAYER));
        let validation = want_validation && has_validation;

        let mut exts: Vec<*const c_char> = extra_exts.to_vec();
        if validation {
            exts.push(ash::ext::debug_utils::NAME.as_ptr());
        }
        // macOS : MoltenVK n'est exposé que comme implémentation « portabilité ».
        #[cfg(target_os = "macos")]
        exts.push(ash::khr::portability_enumeration::NAME.as_ptr());
        let layer_ptrs: Vec<*const c_char> = if validation {
            vec![VALIDATION_LAYER.as_ptr()]
        } else {
            Vec::new()
        };
        let app = vk::ApplicationInfo::default()
            .application_name(c"Norrblock")
            .engine_name(c"nb-gfx")
            .api_version(vk::API_VERSION_1_3);
        #[cfg(target_os = "macos")]
        let ci_flags = vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
        #[cfg(not(target_os = "macos"))]
        let ci_flags = vk::InstanceCreateFlags::empty();
        let ci = vk::InstanceCreateInfo::default()
            .flags(ci_flags)
            .application_info(&app)
            .enabled_extension_names(&exts)
            .enabled_layer_names(&layer_ptrs);
        // SAFETY : structure de création valide, durées de vie respectées.
        let instance = unsafe { entry.create_instance(&ci, None)? };

        let debug = if validation {
            let loader = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::GENERAL,
                )
                .pfn_user_callback(Some(debug_callback));
            // SAFETY : instance valide créée avec l'extension debug_utils.
            let m = unsafe { loader.create_debug_utils_messenger(&info, None)? };
            Some((loader, m))
        } else {
            None
        };

        let surface_loader = surface_factory
            .is_some()
            .then(|| ash::khr::surface::Instance::new(&entry, &instance));
        let surface = match surface_factory {
            Some(f) => Some(f(&entry, &instance)?),
            None => None,
        };

        // Choix du GPU : Vulkan 1.3, file graphique (+ présentation), discret d'abord.
        // SAFETY : instance valide.
        let physicals = unsafe { instance.enumerate_physical_devices()? };
        let mut best: Option<(u32, vk::PhysicalDevice, u32)> = None;
        for &pd in &physicals {
            // SAFETY : `pd` vient de l'instance.
            let props = unsafe { instance.get_physical_device_properties(pd) };
            if props.api_version < vk::API_VERSION_1_3 {
                continue;
            }
            let mut f13 = vk::PhysicalDeviceVulkan13Features::default();
            let mut f2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut f13);
            // SAFETY : chaîne de features valide.
            unsafe { instance.get_physical_device_features2(pd, &mut f2) };
            if f13.dynamic_rendering == vk::FALSE || f13.synchronization2 == vk::FALSE {
                continue;
            }
            // SAFETY : `pd` valide.
            let families = unsafe { instance.get_physical_device_queue_family_properties(pd) };
            let family = families.iter().enumerate().find_map(|(i, f)| {
                let i = i as u32;
                let graphics = f.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                let present = match (surface, &surface_loader) {
                    // SAFETY : surface et GPU valides.
                    (Some(s), Some(l)) => unsafe {
                        l.get_physical_device_surface_support(pd, i, s)
                            .unwrap_or(false)
                    },
                    _ => true,
                };
                (graphics && present).then_some(i)
            });
            let Some(family) = family else { continue };
            let score = match props.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 4,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                _ => 1,
            };
            if best.is_none_or(|(s, _, _)| score > s) {
                best = Some((score, pd, family));
            }
        }
        let Some((_, physical, queue_family)) = best else {
            // SAFETY : destruction dans l'ordre inverse de la création.
            unsafe {
                if let (Some(s), Some(l)) = (surface, &surface_loader) {
                    l.destroy_surface(s, None);
                }
                if let Some((l, m)) = &debug {
                    l.destroy_debug_utils_messenger(*m, None);
                }
                instance.destroy_instance(None);
            }
            return Err(GfxError::NoDevice);
        };

        // SAFETY : GPU valide.
        let props = unsafe { instance.get_physical_device_properties(physical) };
        // SAFETY : GPU valide.
        let base = unsafe { instance.get_physical_device_features(physical) };
        let anisotropy = base.sampler_anisotropy == vk::TRUE;
        let device_name = props
            .device_name_as_c_str()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        let prio = [1.0f32];
        let qci = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&prio)];
        let core = vk::PhysicalDeviceFeatures::default().sampler_anisotropy(anisotropy);
        let mut f13 = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true);
        let mut f2 = vk::PhysicalDeviceFeatures2::default()
            .features(core)
            .push_next(&mut f13);
        let mut dev_exts: Vec<*const c_char> = Vec::new();
        if surface.is_some() {
            dev_exts.push(ash::khr::swapchain::NAME.as_ptr());
        }
        #[cfg(target_os = "macos")]
        dev_exts.push(c"VK_KHR_portability_subset".as_ptr());
        let dci = vk::DeviceCreateInfo::default()
            .queue_create_infos(&qci)
            .enabled_extension_names(&dev_exts)
            .push_next(&mut f2);
        // SAFETY : création du device avec des structures valides.
        let device = unsafe { instance.create_device(physical, &dci, None)? };
        // SAFETY : file 0 de la famille demandée.
        let queue = unsafe { device.get_device_queue(queue_family, 0) };

        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device: physical,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })?;
        let pool_ci = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        // SAFETY : device valide.
        let command_pool = unsafe { device.create_command_pool(&pool_ci, None)? };

        Ok((
            Self {
                entry,
                instance,
                physical,
                device,
                queue_family,
                queue: Mutex::new(queue),
                allocator: ManuallyDrop::new(Mutex::new(allocator)),
                command_pool: Mutex::new(command_pool),
                max_anisotropy: anisotropy.then(|| props.limits.max_sampler_anisotropy.min(8.0)),
                device_name,
                validation_enabled: debug.is_some(),
                debug,
                surface_loader,
            },
            surface,
        ))
    }

    /// Chargeur `VK_KHR_surface` (contexte créé avec [`GpuContext::with_surface`]).
    pub fn surface_loader(&self) -> Option<&ash::khr::surface::Instance> {
        self.surface_loader.as_ref()
    }

    /// Soumission sur la file graphique (accès synchronisé).
    pub fn submit(&self, submits: &[vk::SubmitInfo2<'_>], fence: vk::Fence) -> Result<()> {
        let q = self.queue.lock().expect("file graphique");
        // SAFETY : accès exclusif à la file via le mutex.
        unsafe { self.device.queue_submit2(*q, submits, fence)? };
        Ok(())
    }

    /// Présentation (accès synchronisé à la file). Renvoie `suboptimal`.
    pub fn present(
        &self,
        loader: &ash::khr::swapchain::Device,
        info: &vk::PresentInfoKHR<'_>,
    ) -> std::result::Result<bool, vk::Result> {
        let q = self.queue.lock().expect("file graphique");
        // SAFETY : accès exclusif à la file via le mutex.
        unsafe { loader.queue_present(*q, info) }
    }

    /// Attend la fin de tout le travail GPU.
    pub fn wait_idle(&self) {
        let _q = self.queue.lock().expect("file graphique");
        // SAFETY : device valide ; la file est verrouillée.
        unsafe {
            let _ = self.device.device_wait_idle();
        }
    }

    /// Enregistre et exécute des commandes de façon synchrone (uploads, transitions).
    pub fn run_once(&self, record: impl FnOnce(vk::CommandBuffer)) -> Result<()> {
        let pool = self.command_pool.lock().expect("pool de commandes");
        let ai = vk::CommandBufferAllocateInfo::default()
            .command_pool(*pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY : pool verrouillé, device valide.
        let cmd = unsafe { self.device.allocate_command_buffers(&ai)?[0] };
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY : enregistrement d'un tampon fraîchement alloué.
        let res = unsafe {
            self.device.begin_command_buffer(cmd, &begin).and_then(|_| {
                record(cmd);
                self.device.end_command_buffer(cmd)
            })
        };
        let finish = || -> Result<()> {
            res?;
            // SAFETY : device valide.
            let fence = unsafe {
                self.device
                    .create_fence(&vk::FenceCreateInfo::default(), None)?
            };
            let cbi = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
            let submit = [vk::SubmitInfo2::default().command_buffer_infos(&cbi)];
            let r = self.submit(&submit, fence).and_then(|_| {
                // SAFETY : attente du fence soumis ci-dessus.
                unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX)? };
                Ok(())
            });
            // SAFETY : le fence n'est plus utilisé.
            unsafe { self.device.destroy_fence(fence, None) };
            r
        };
        let r = finish();
        // SAFETY : le tampon a fini de s'exécuter (ou n'a pas été soumis).
        unsafe { self.device.free_command_buffers(*pool, &[cmd]) };
        r
    }
}

impl Drop for GpuContext {
    fn drop(&mut self) {
        self.wait_idle();
        // SAFETY : plus aucun travail GPU ; destruction dans l'ordre inverse de la création.
        unsafe {
            ManuallyDrop::drop(&mut self.allocator);
            let pool = *self.command_pool.get_mut().expect("pool");
            self.device.destroy_command_pool(pool, None);
            self.device.destroy_device(None);
            if let Some((l, m)) = &self.debug {
                l.destroy_debug_utils_messenger(*m, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

/// Charge la bibliothèque Vulkan. Sous macOS, `libvulkan.dylib` est rarement installée : on
/// essaie ensuite `libMoltenVK.dylib` à côté de l'exécutable, puis dans les dossiers usuels
/// de Homebrew et du SDK Vulkan.
///
/// # Safety
/// Même contrat que [`ash::Entry::load`].
unsafe fn load_entry() -> std::result::Result<ash::Entry, ash::LoadingError> {
    // SAFETY : chargement dynamique de la bibliothèque Vulkan système.
    let first = unsafe { ash::Entry::load() };
    #[cfg(not(target_os = "macos"))]
    {
        first
    }
    #[cfg(target_os = "macos")]
    {
        let Err(err) = first else { return first };
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("libMoltenVK.dylib"));
            }
        }
        candidates.push("libMoltenVK.dylib".into());
        candidates.push("/opt/homebrew/lib/libMoltenVK.dylib".into());
        candidates.push("/usr/local/lib/libMoltenVK.dylib".into());
        for c in candidates {
            // SAFETY : idem.
            if let Ok(e) = unsafe { ash::Entry::load_from(&c) } {
                return Ok(e);
            }
        }
        Err(err)
    }
}
