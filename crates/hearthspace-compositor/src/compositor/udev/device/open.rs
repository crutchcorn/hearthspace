use super::super::{
    gpu::GpuState,
    identity::{ConnectorNames, DeviceToken, device_identity},
};
use super::{ScanoutNode, UdevDevice, cursor::create_hardware_cursor, session_fd::SessionFd};
use smithay::{
    backend::{
        allocator::{dmabuf::Dmabuf, gbm::GbmDevice},
        drm::{DrmDevice, DrmDeviceFd, DrmDeviceNotifier, DrmNode},
        renderer::{Bind, gles::GlesRenderer},
        session::libseat::LibSeatSession,
    },
    reexports::rustix::fs::OFlags,
};
use std::path::PathBuf;
use tracing::{info, warn};

pub(in crate::compositor::udev) fn create_udev_device(
    session: &mut LibSeatSession,
    path: PathBuf,
    active: bool,
    token: DeviceToken,
    gpus: &mut GpuState,
    names: &mut ConnectorNames,
) -> Result<(UdevDevice, DrmDeviceNotifier), Box<dyn std::error::Error>> {
    info!(path = %path.display(), active, "opening udev DRM device");
    // Resolve before acquiring seat/renderer resources. Discovery keeps a
    // failed device eligible for recovery without disturbing healthy GPUs.
    let identity = device_identity(&path)?;
    let session_fd = SessionFd::open(session, &path, OFlags::RDWR | OFlags::CLOEXEC)?;
    let drm_fd = DrmDeviceFd::new(session_fd.fd());
    let node = DrmNode::from_dev_id(drm_fd.dev_id()?)?;
    let gbm_device = GbmDevice::new(drm_fd.clone())?;
    let (drm_device, notifier) = DrmDevice::new(drm_fd.clone(), false)?;
    let mut device = UdevDevice {
        identity,
        token,
        node,
        source: None,
        path,
        active,
        recovery_needed: true,
        discovery_complete: false,
        output_targets: Vec::new(),
        output_surfaces: Vec::new(),
        hardware_cursor: None,
        hardware_cursor_initialized: false,
        hardware_cursor_crtc: None,
        hardware_cursor_position: None,
        gbm_device,
        renderer_formats: Vec::new(),
        scanout_node: ScanoutNode { drm_fd, drm_device },
        _session_fd: session_fd,
    };
    let initialized = (|| -> Result<(), Box<dyn std::error::Error>> {
        gpus.manager
            .as_mut()
            .add_node(node, device.gbm_device.clone())?;
        let renderer = gpus.manager.single_renderer(&node)?;
        // Framebuffer bindings require renderable formats, not the larger
        // texture-import set (which can contain external-only modifiers).
        device.renderer_formats =
            <GlesRenderer as Bind<Dmabuf>>::supported_formats(renderer.as_ref())
                .ok_or("native renderer has no framebuffer formats")?
                .into_iter()
                .collect();
        (device.output_targets, device.discovery_complete) =
            device.connected_output_targets(names)?;
        Ok(())
    })();
    if let Err(error) = initialized {
        gpus.manager.as_mut().remove_node(&node);
        let _ = gpus.manager.devices_mut();
        drop(notifier);
        drop(device);
        return Err(error);
    }
    device.hardware_cursor = create_hardware_cursor(&device.scanout_node);
    if let Err(error) = device.rebuild_output_surfaces(device.output_targets.clone()) {
        warn!(%error, ?node, "some native outputs await recovery");
    }
    device.recovery_needed = device.output_surfaces.len() != device.output_targets.len();
    Ok((device, notifier))
}
