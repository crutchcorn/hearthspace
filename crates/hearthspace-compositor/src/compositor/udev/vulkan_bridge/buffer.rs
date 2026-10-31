use super::{AllocationInfo, BridgeError, BufferLayout, device::Device, memory};
use ash::vk;
use smithay::backend::allocator::dmabuf::Dmabuf;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct LinearTarget(pub(super) Rc<TargetInner>);

pub(super) struct TargetInner {
    pub device: Rc<Device>,
    pub buffer: vk::Buffer,
    pub memory: vk::DeviceMemory,
    pub layout: BufferLayout,
    pub allocation: AllocationInfo,
    pub _dma: Dmabuf,
}

impl LinearTarget {
    pub(super) fn import(device: Rc<Device>, dma: &Dmabuf) -> Result<Self, BridgeError> {
        let layout = BufferLayout::inspect(dma)?;
        let size = layout.linear_extent()?;
        let mut external = vk::ExternalMemoryBufferCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .push_next(&mut external);
        let buffer = unsafe { device.raw.create_buffer(&info, None) }
            .map_err(|e| BridgeError::vk("create Linear destination buffer", e))?;
        let mut dedicated = vk::MemoryDedicatedRequirements::default();
        let mut requirements = vk::MemoryRequirements2::default().push_next(&mut dedicated);
        unsafe {
            device.raw.get_buffer_memory_requirements2(
                &vk::BufferMemoryRequirementsInfo2::default().buffer(buffer),
                &mut requirements,
            )
        };
        let requirements = requirements.memory_requirements;
        // The whole imported allocation is dedicated to this one buffer, even
        // when the driver does not report a dedicated-allocation requirement.
        // This also supplies the external allocation's concrete resource type.
        let imported = memory::import(
            &device,
            dma,
            layout,
            requirements,
            Some((vk::Image::null(), buffer)),
        );
        let (memory, allocation) = match imported {
            Ok(result) => result,
            Err(error) => {
                unsafe { device.raw.destroy_buffer(buffer, None) };
                return Err(error);
            }
        };
        let inner = TargetInner {
            device,
            buffer,
            memory,
            layout,
            allocation,
            _dma: dma.clone(),
        };
        unsafe { inner.device.raw.bind_buffer_memory(buffer, memory, 0) }
            .map_err(|e| BridgeError::vk("bind Linear destination memory", e))?;
        Ok(Self(Rc::new(inner)))
    }

    pub fn layout(&self) -> BufferLayout {
        self.0.layout
    }
    pub fn allocation(&self) -> AllocationInfo {
        self.0.allocation
    }
}

impl Drop for TargetInner {
    fn drop(&mut self) {
        unsafe {
            self.device.raw.destroy_buffer(self.buffer, None);
            self.device.raw.free_memory(self.memory, None);
        }
    }
}
