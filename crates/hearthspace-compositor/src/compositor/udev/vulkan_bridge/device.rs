use super::{BridgeError, FORMATS, layout::vk_format};
use ash::{ext, khr, vk};
use smithay::backend::{
    allocator::{Format, Modifier},
    drm::DrmNode,
    vulkan::{Instance, PhysicalDevice, version::Version},
};
use std::{cell::Cell, ffi::CStr, rc::Rc};

#[derive(Debug)]
pub(crate) struct DeviceIdentity {
    pub node: DrmNode,
    pub name: String,
    pub device_uuid: [u8; 16],
    pub driver_uuid: [u8; 16],
    pub queue_family: u32,
}

#[derive(Clone, Copy)]
pub(super) struct SourceSupport {
    pub format: Format,
    pub limits: vk::ImageFormatProperties,
    pub dedicated: bool,
}

pub(super) struct Device {
    pub raw: ash::Device,
    pub physical: PhysicalDevice,
    pub memory_fd: khr::external_memory_fd::Device,
    pub semaphore_fd: khr::external_semaphore_fd::Device,
    pub queue: vk::Queue,
    pub identity: DeviceIdentity,
    pub supports: Vec<SourceSupport>,
    pub lost: Cell<bool>,
}

impl Device {
    pub fn new(node: DrmNode) -> Result<Rc<Self>, BridgeError> {
        let instance = Instance::new(Version::VERSION_1_2, None)
            .map_err(|e| BridgeError::Setup(e.to_string()))?;
        if instance.api_version() < Version::VERSION_1_1 {
            return Err(BridgeError::Unsupported("Vulkan instance 1.1 is required"));
        }
        let matches: Vec<_> = PhysicalDevice::enumerate(&instance)
            .map_err(|e| BridgeError::vk("enumerate physical devices", e))?
            .filter(|p| {
                p.render_node().ok().flatten() == Some(node)
                    || p.primary_node().ok().flatten() == Some(node)
            })
            .collect();
        let [physical] = matches.as_slice() else {
            return Err(BridgeError::Unsupported(
                "canonical DRM identity has no unique Vulkan device",
            ));
        };
        let physical = physical.clone();
        if physical.ty() == vk::PhysicalDeviceType::CPU
            || physical.api_version() < Version::VERSION_1_1
        {
            return Err(BridgeError::Unsupported(
                "hardware Vulkan 1.1 device required",
            ));
        }
        let mut extensions: Vec<&CStr> = vec![
            ext::image_drm_format_modifier::NAME,
            ext::external_memory_dma_buf::NAME,
            khr::external_memory_fd::NAME,
            ext::queue_family_foreign::NAME,
            khr::external_semaphore_fd::NAME,
        ];
        if instance.api_version() < Version::VERSION_1_2
            || physical.api_version() < Version::VERSION_1_2
        {
            extensions.push(khr::image_format_list::NAME);
        }
        if extensions
            .iter()
            .any(|extension| !physical.has_device_extension(extension))
        {
            return Err(BridgeError::Unsupported(
                "required external-memory/sync/foreign extension missing",
            ));
        }
        let handle = instance.handle();
        // SAFETY: queried physical handle is owned by the retained Smithay instance.
        let families =
            unsafe { handle.get_physical_device_queue_family_properties(physical.handle()) };
        let family = families
            .iter()
            .position(|q| q.queue_count > 0 && q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .ok_or(BridgeError::Unsupported(
                "graphics-capable copy queue unavailable",
            ))? as u32;
        let mut external = vk::ExternalBufferProperties::default();
        let mut semaphore = vk::ExternalSemaphoreProperties::default();
        unsafe {
            handle.get_physical_device_external_buffer_properties(
                physical.handle(),
                &vk::PhysicalDeviceExternalBufferInfo::default()
                    .usage(vk::BufferUsageFlags::TRANSFER_DST)
                    .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
                &mut external,
            );
            handle.get_physical_device_external_semaphore_properties(
                physical.handle(),
                &vk::PhysicalDeviceExternalSemaphoreInfo::default()
                    .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD),
                &mut semaphore,
            );
        }
        if !external
            .external_memory_properties
            .external_memory_features
            .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
            || !external
                .external_memory_properties
                .compatible_handle_types
                .contains(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            || !semaphore.external_semaphore_features.contains(
                vk::ExternalSemaphoreFeatureFlags::IMPORTABLE
                    | vk::ExternalSemaphoreFeatureFlags::EXPORTABLE,
            )
            || !semaphore
                .compatible_handle_types
                .contains(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
        {
            return Err(BridgeError::Unsupported(
                "DMA-BUF transfer destination or SYNC_FD unsupported",
            ));
        }
        let supports = source_support(&physical)?;
        if supports.is_empty() {
            return Err(BridgeError::Unsupported(
                "no packed-8888 source image imports",
            ));
        }
        let names: Vec<_> = extensions.iter().map(|name| name.as_ptr()).collect();
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        let info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&names);
        let raw = unsafe { handle.create_device(physical.handle(), &info, None) }
            .map_err(|e| BridgeError::vk("create copy device", e))?;
        let memory_fd = khr::external_memory_fd::Device::new(handle, &raw);
        let semaphore_fd = khr::external_semaphore_fd::Device::new(handle, &raw);
        let queue = unsafe { raw.get_device_queue(family, 0) };
        let ids = physical.id_properties();
        let identity = DeviceIdentity {
            node,
            name: physical.name().into(),
            device_uuid: ids.device_uuid,
            driver_uuid: ids.driver_uuid,
            queue_family: family,
        };
        Ok(Rc::new(Self {
            raw,
            physical,
            memory_fd,
            semaphore_fd,
            queue,
            identity,
            supports,
            lost: Cell::new(false),
        }))
    }
}

fn source_support(physical: &PhysicalDevice) -> Result<Vec<SourceSupport>, BridgeError> {
    let mut supports = Vec::new();
    for code in FORMATS {
        let format = vk_format(code)?;
        let modifiers = physical
            .get_format_modifier_properties(format)
            .map_err(|e| BridgeError::Setup(e.to_string()))?;
        for modifier in modifiers {
            if modifier.drm_format_modifier_plane_count != 1
                || !modifier
                    .drm_format_modifier_tiling_features
                    .contains(vk::FormatFeatureFlags::TRANSFER_SRC)
            {
                continue;
            }
            let mut drm = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
                .drm_format_modifier(modifier.drm_format_modifier)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let mut external = vk::PhysicalDeviceExternalImageFormatInfo::default()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
            let info = vk::PhysicalDeviceImageFormatInfo2::default()
                .format(format)
                .ty(vk::ImageType::TYPE_2D)
                .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
                .usage(vk::ImageUsageFlags::TRANSFER_SRC)
                .push_next(&mut drm)
                .push_next(&mut external);
            let mut ext = vk::ExternalImageFormatProperties::default();
            let mut output = vk::ImageFormatProperties2::default().push_next(&mut ext);
            let result = unsafe {
                physical
                    .instance()
                    .handle()
                    .get_physical_device_image_format_properties2(
                        physical.handle(),
                        &info,
                        &mut output,
                    )
            };
            match result {
                Err(vk::Result::ERROR_FORMAT_NOT_SUPPORTED) => continue,
                Err(e) => return Err(BridgeError::vk("query source format", e)),
                Ok(()) => {}
            }
            let limits = output.image_format_properties;
            let external = ext.external_memory_properties;
            if external
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
                && external
                    .compatible_handle_types
                    .contains(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
                && limits.sample_counts.contains(vk::SampleCountFlags::TYPE_1)
            {
                supports.push(SourceSupport {
                    format: Format {
                        code,
                        modifier: Modifier::from(modifier.drm_format_modifier),
                    },
                    limits,
                    dedicated: external
                        .external_memory_features
                        .contains(vk::ExternalMemoryFeatureFlags::DEDICATED_ONLY),
                });
            }
        }
    }
    Ok(supports)
}

impl Drop for Device {
    fn drop(&mut self) {
        // All resources retain this Rc; unresolved submissions deliberately keep
        // their Rc alive on final drop. No live command can reach this destructor.
        unsafe { self.raw.destroy_device(None) };
    }
}
