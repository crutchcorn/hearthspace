use super::{Request, Result, Slot, slot::Resources};
use crate::compositor::udev::{
    gpu::{
        GpuApi, GpuState,
        bridge::{Endpoint, Pair, SlotKey},
    },
    vulkan_bridge::{AllocationInfo, BufferLayout, FORMATS, NativeFence, VulkanBridge},
};
use smithay::backend::{
    allocator::{Buffer, Format, Fourcc, Modifier, dmabuf::Dmabuf},
    drm::DrmNode,
    egl::fence::EGLFence,
    renderer::{
        Bind, ImportDma, Renderer,
        gles::{GlesRenderer, ffi},
        multigpu::{ApiDevice, GpuManager},
    },
};
use std::ffi::CStr;

pub(super) fn key(gpus: &mut GpuState, request: &Request<'_>) -> Result<SlotKey> {
    let endpoint = |gpus: &mut GpuState, node| -> Result<Endpoint> {
        let (token, physical) = gpus
            .incarnations
            .get(&node)
            .ok_or("missing GPU incarnation")?
            .clone();
        let renderer = gpus.manager.single_renderer(&node)?;
        Ok(Endpoint {
            node,
            token,
            physical,
            context: renderer.as_ref().context_id(),
        })
    };
    Ok(SlotKey {
        pair: Pair {
            source: endpoint(gpus, request.source)?,
            target: endpoint(gpus, request.target)?,
        },
        output: request.output.into(),
        size: request.size,
        format: request.copy_format,
    })
}

pub(super) fn create(manager: &mut GpuManager<GpuApi>, key: &SlotKey) -> Result<Slot> {
    let mut bridge = VulkanBridge::new(key.pair.source.node)?;
    let (source_formats, target_formats) = {
        let source = manager.single_renderer(&key.pair.source.node)?;
        let renderer = source.as_ref();
        if renderer.is_software() || !EGLFence::supports_importing(renderer.egl_context().display())
        {
            return Err("canonical GPU lacks native fence support".into());
        }
        let formats =
            <GlesRenderer as Bind<Dmabuf>>::supported_formats(renderer).unwrap_or_default();
        drop(source);
        let mut target = manager.single_renderer(&key.pair.target.node)?;
        let renderer = target.as_mut();
        if renderer.is_software() || !EGLFence::supports_importing(renderer.egl_context().display())
        {
            return Err("target GPU lacks native fence support".into());
        }
        let nvidia = renderer.with_context(|gl| unsafe {
            let vendor = gl.GetString(ffi::VENDOR);
            !vendor.is_null()
                && CStr::from_ptr(vendor.cast())
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .contains("nvidia")
        })?;
        if nvidia {
            return Err("NVIDIA-owned Linear bridge allocations are not supported".into());
        }
        (formats, renderer.dmabuf_formats())
    };
    let format = std::iter::once(key.format)
        .chain(FORMATS)
        .find(|code| {
            target_formats.contains(&Format {
                code: *code,
                modifier: Modifier::Linear,
            }) && bridge.source_formats().iter().any(|f| {
                f.code == *code
                    && source_formats.contains(f)
                    && !matches!(f.modifier, Modifier::Invalid | Modifier::Linear)
            })
        })
        .ok_or("no GLES-render/Vulkan-copy/target-Linear format intersection")?;
    let modifiers: Vec<_> = bridge
        .source_formats()
        .iter()
        .filter(|f| {
            f.code == format
                && source_formats.contains(f)
                && !matches!(f.modifier, Modifier::Invalid | Modifier::Linear)
        })
        .map(|f| f.modifier)
        .collect();
    let source_dma = allocate(manager, key.pair.source.node, key.size, format, &modifiers)?;
    let target_dma = allocate(
        manager,
        key.pair.target.node,
        key.size,
        format,
        &[Modifier::Linear],
    )?;
    let source = bridge.import_source(&source_dma)?;
    let target = bridge.import_linear_target(&target_dma)?;
    let target_texture = manager
        .single_renderer(&key.pair.target.node)?
        .as_mut()
        .import_dmabuf(&target_dma, None)?;
    log_allocation("source", source.layout(), source.allocation());
    log_allocation("target", target.layout(), target.allocation());
    let identity = bridge.identity();
    tracing::info!(output = key.output, source = ?identity.node, target = ?key.pair.target.node,
        name = identity.name, device_uuid = ?identity.device_uuid, driver_uuid = ?identity.driver_uuid,
        ?format, size = ?key.size, "created Vulkan image-to-buffer transfer slot");
    Ok(Slot::new(Resources {
        bridge: Some(bridge),
        retired: None,
        source,
        target,
        source_dma,
        _target_dma: target_dma,
        target_texture,
        size: key.size,
        source_released: NativeFence::Signaled,
        target_released: NativeFence::Signaled,
        unknown_external: false,
        holds: Vec::with_capacity(super::slot::EXTERNAL_LIMIT),
    }))
}

fn allocate(
    manager: &mut GpuManager<GpuApi>,
    node: DrmNode,
    size: smithay::utils::Size<i32, smithay::utils::Physical>,
    format: Fourcc,
    modifiers: &[Modifier],
) -> Result<Dmabuf> {
    let mut devices = manager.devices_mut()?;
    let device = devices
        .find(|device| *device.node() == node)
        .ok_or("allocation GPU disappeared")?;
    // Reuse Smithay's existing RENDERING-only GbmAllocator. Its implicit
    // fallback is never accepted as an explicit layout by this adapter.
    let dma = device
        .allocator()
        .create_buffer(size.w as u32, size.h as u32, format, modifiers)
        .map_err(|error| error.to_string())?;
    if dma.format().code != format
        || !modifiers.contains(&dma.format().modifier)
        || dma.format().modifier == Modifier::Invalid
        || dma.num_planes() != 1
        || !dma.flags().is_empty()
    {
        return Err("GBM substituted an unsupported bridge layout".into());
    }
    Ok(dma)
}

fn log_allocation(role: &str, layout: BufferLayout, allocation: AllocationInfo) {
    tracing::debug!(role, format = ?layout.format, width = layout.width, height = layout.height,
        offset = layout.offset, stride = layout.stride, fd_size = allocation.fd_size,
        required_size = allocation.required_size, alignment = allocation.required_alignment,
        resource_memory_types = allocation.resource_memory_types, fd_memory_types = allocation.fd_memory_types,
        memory_type = allocation.memory_type, dedicated = allocation.dedicated, "imported bridge allocation");
}
