use super::super::{
    gpu::GpuState,
    identity::{ConnectorNames, DeviceToken, device_identity},
    presenter::{Presenter, PresenterEvent},
};
use super::{UdevDevice, session_fd::SessionFd, targets::discover_outputs};
use smithay::{
    backend::{
        allocator::{dmabuf::Dmabuf, gbm::GbmDevice},
        drm::{DrmDeviceFd, DrmNode},
        renderer::{Bind, gles::GlesRenderer},
        session::libseat::LibSeatSession,
    },
    reexports::{calloop::channel::Channel, rustix::fs::OFlags},
};
use std::path::PathBuf;
use tracing::info;

pub(in crate::compositor::udev) fn create_udev_device(
    session: &mut LibSeatSession,
    path: PathBuf,
    active: bool,
    token: DeviceToken,
    gpus: &mut GpuState,
    names: &mut ConnectorNames,
) -> Result<(UdevDevice, Channel<PresenterEvent>), Box<dyn std::error::Error>> {
    info!(path = %path.display(), active, "opening udev DRM device");
    let identity = device_identity(&path)?;
    let session_fd = SessionFd::open(session, &path, OFlags::RDWR | OFlags::CLOEXEC)?;
    let drm_fd = DrmDeviceFd::new(session_fd.fd());
    let node = DrmNode::from_dev_id(drm_fd.dev_id()?)?;
    let gbm_device = GbmDevice::new(drm_fd.clone())?;
    let initialized = (|| -> Result<_, Box<dyn std::error::Error>> {
        gpus.manager.as_mut().add_node(node, gbm_device.clone())?;
        let renderer = gpus.manager.single_renderer(&node)?;
        let formats = <GlesRenderer as Bind<Dmabuf>>::supported_formats(renderer.as_ref())
            .ok_or("native renderer has no framebuffer formats")?
            .into_iter()
            .collect();
        let (targets, complete) = discover_outputs(&drm_fd, &path, &identity, names)?;
        Ok((formats, targets, complete))
    })();
    let (renderer_formats, output_targets, discovery_complete) = match initialized {
        Ok(value) => value,
        Err(error) => {
            gpus.manager.as_mut().remove_node(&node);
            let _ = gpus.manager.devices_mut();
            return Err(error);
        }
    };
    let (presenter, events) = match Presenter::spawn(token, drm_fd.clone(), gbm_device.clone()) {
        Ok(value) => value,
        Err(error) => {
            gpus.manager.as_mut().remove_node(&node);
            let _ = gpus.manager.devices_mut();
            return Err(error.into());
        }
    };
    let mut device = UdevDevice {
        path,
        identity,
        token,
        node,
        source: None,
        active,
        recovery_needed: true,
        discovery_complete,
        configuration_pending: false,
        recovery_not_before: std::time::Instant::now(),
        generation: 0,
        control_generation: 0,
        output_targets,
        output_surfaces: Vec::new(),
        presenter,
        gbm_device,
        renderer_formats,
        drm_fd,
        _session_fd: session_fd,
    };
    if !active {
        device.presenter.pause();
    }
    device.rebuild_output_surfaces(device.output_targets.clone());
    Ok((device, events))
}
