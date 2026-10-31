use super::{BridgeError, BufferLayout, admission::IMPORTS, device::Device};
use ash::vk;
use smithay::backend::allocator::dmabuf::Dmabuf;
use std::os::fd::AsRawFd;

#[derive(Clone, Copy, Debug)]
pub(crate) struct AllocationInfo {
    pub fd_size: u64,
    pub required_size: u64,
    pub required_alignment: u64,
    pub resource_memory_types: u32,
    pub fd_memory_types: u32,
    pub memory_type: u32,
    pub dedicated: bool,
}

pub(super) fn import(
    device: &Device,
    dma: &Dmabuf,
    layout: BufferLayout,
    requirements: vk::MemoryRequirements,
    dedicated: Option<(vk::Image, vk::Buffer)>,
) -> Result<(vk::DeviceMemory, AllocationInfo), BridgeError> {
    if requirements.size == 0 || requirements.size > layout.fd_size {
        return Err(BridgeError::Invalid(
            "Vulkan memory requirements exceed DMA-BUF size",
        ));
    }
    let fd = dma
        .handles()
        .next()
        .ok_or(BridgeError::Invalid("missing DMA-BUF fd"))?
        .try_clone_to_owned()
        .map_err(|e| BridgeError::io("duplicate memory fd", e))?;
    let mut properties = vk::MemoryFdPropertiesKHR::default();
    // The properties query does not transfer ownership. Allocation uses the
    // process-wide admission guard for the observed failure-consumes-FD defect.
    tracing::trace!(fd = fd.as_raw_fd(), "querying DMA-BUF memory properties");
    unsafe {
        device.memory_fd.get_memory_fd_properties(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            fd.as_raw_fd(),
            &mut properties,
        )
    }
    .map_err(|e| BridgeError::vk("query actual DMA-BUF memory types", e))?;
    tracing::trace!(
        fd = fd.as_raw_fd(),
        memory_types = properties.memory_type_bits,
        "queried DMA-BUF memory properties"
    );
    let eligible = requirements.memory_type_bits & properties.memory_type_bits;
    let types = unsafe {
        device
            .physical
            .instance()
            .handle()
            .get_physical_device_memory_properties(device.physical.handle())
    };
    tracing::trace!(types = ?types, eligible, "import memory candidates");
    let forbidden = vk::MemoryPropertyFlags::PROTECTED
        | vk::MemoryPropertyFlags::LAZILY_ALLOCATED
        | vk::MemoryPropertyFlags::DEVICE_COHERENT_AMD;
    let memory_type = (0..types.memory_type_count)
        .find(|i| {
            eligible & (1 << i) != 0
                && !types.memory_types[*i as usize]
                    .property_flags
                    .intersects(forbidden)
        })
        .ok_or(BridgeError::Unsupported(
            "no compatible enabled memory type for this DMA-BUF",
        ))?;
    let raw_fd = fd.as_raw_fd();
    let mut imported = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(fd.as_raw_fd());
    let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default();
    let mut allocate = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type)
        .push_next(&mut imported);
    if let Some((image, buffer)) = dedicated {
        dedicated_info.image = image;
        dedicated_info.buffer = buffer;
        allocate = allocate.push_next(&mut dedicated_info);
    }
    tracing::trace!(
        fd = fd.as_raw_fd(),
        memory_type,
        size = requirements.size,
        dedicated = dedicated.is_some(),
        "allocating imported DMA-BUF memory"
    );
    let result = IMPORTS.allocate(fd, |_| unsafe {
        device.raw.allocate_memory(&allocate, None)
    });
    tracing::trace!(fd = raw_fd, result = ?result, "allocated imported DMA-BUF memory");
    let memory = result?;
    Ok((
        memory,
        AllocationInfo {
            fd_size: layout.fd_size,
            required_size: requirements.size,
            required_alignment: requirements.alignment,
            resource_memory_types: requirements.memory_type_bits,
            fd_memory_types: properties.memory_type_bits,
            memory_type,
            dedicated: dedicated.is_some(),
        },
    ))
}
