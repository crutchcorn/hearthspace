use super::{AllocationInfo, BridgeError, BufferLayout, device::Device, layout::vk_format, memory};
use ash::vk;
use smithay::backend::allocator::dmabuf::Dmabuf;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct SourceImage(pub(super) Rc<SourceInner>);

pub(super) struct SourceInner {
    pub device: Rc<Device>,
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub layout: BufferLayout,
    pub allocation: AllocationInfo,
    // Keep the actual exported allocation, not just its fd number or dimensions.
    pub _dma: Dmabuf,
}

impl SourceImage {
    pub(super) fn import(device: Rc<Device>, dma: &Dmabuf) -> Result<Self, BridgeError> {
        let layout = BufferLayout::inspect(dma)?;
        let support = device
            .supports
            .iter()
            .find(|s| s.format == layout.format)
            .ok_or(BridgeError::Unsupported(
                "actual source format/modifier is not transfer-importable",
            ))?;
        if layout.width > support.limits.max_extent.width
            || layout.height > support.limits.max_extent.height
            || support.limits.max_mip_levels == 0
            || support.limits.max_array_layers == 0
        {
            return Err(BridgeError::Unsupported(
                "source dimensions exceed Vulkan format limits",
            ));
        }
        if let Some(node) = dma.node()
            && Some(node) != device.physical.render_node().ok().flatten()
            && Some(node) != device.physical.primary_node().ok().flatten()
        {
            return Err(BridgeError::Invalid(
                "source allocation belongs to a different DRM device",
            ));
        }
        let external_dedicated = support.dedicated;
        let planes = [vk::SubresourceLayout::default()
            .offset(layout.offset)
            .row_pitch(u64::from(layout.stride))];
        let mut modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(layout.format.modifier.into())
            .plane_layouts(&planes);
        let mut external = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk_format(layout.format.code)?)
            .extent(vk::Extent3D {
                width: layout.width,
                height: layout.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut modifier)
            .push_next(&mut external);
        let image = unsafe { device.raw.create_image(&info, None) }
            .map_err(|e| BridgeError::vk("create source image", e))?;
        let mut dedicated = vk::MemoryDedicatedRequirements::default();
        let mut requirements = vk::MemoryRequirements2::default().push_next(&mut dedicated);
        unsafe {
            device.raw.get_image_memory_requirements2(
                &vk::ImageMemoryRequirementsInfo2::default().image(image),
                &mut requirements,
            )
        };
        let requirements = requirements.memory_requirements;
        let needs_dedicated = external_dedicated
            || dedicated.requires_dedicated_allocation == vk::TRUE
            || dedicated.prefers_dedicated_allocation == vk::TRUE;
        let imported = memory::import(
            &device,
            dma,
            layout,
            requirements,
            needs_dedicated.then_some((image, vk::Buffer::null())),
        );
        let (memory, allocation) = match imported {
            Ok(result) => result,
            Err(error) => {
                unsafe { device.raw.destroy_image(image, None) };
                return Err(error);
            }
        };
        let inner = SourceInner {
            device,
            image,
            memory,
            layout,
            allocation,
            _dma: dma.clone(),
        };
        unsafe { inner.device.raw.bind_image_memory(image, memory, 0) }
            .map_err(|e| BridgeError::vk("bind source image memory", e))?;
        Ok(Self(Rc::new(inner)))
    }

    pub fn layout(&self) -> BufferLayout {
        self.0.layout
    }
    pub fn allocation(&self) -> AllocationInfo {
        self.0.allocation
    }
}

impl Drop for SourceInner {
    fn drop(&mut self) {
        // Pending jobs retain this Rc through completion, including failure paths.
        unsafe {
            self.device.raw.destroy_image(self.image, None);
            self.device.raw.free_memory(self.memory, None);
        }
    }
}
