use std::{io, path::PathBuf};

use smithay::{
    backend::{
        allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        allocator::{Format, Fourcc},
        drm::{DrmDevice, DrmDeviceFd, GbmBufferedSurface},
        egl::{EGLContext, EGLDisplay},
        renderer::{ImportDma, damage::OutputDamageTracker, gles::GlesRenderer},
        session::{Session, libseat::LibSeatSession},
        udev::{UdevBackend, all_gpus},
    },
    output::{PhysicalProperties, Subpixel},
    reexports::{
        drm::control::{Device as ControlDevice, Mode, connector, crtc},
        rustix::fs::{OFlags, stat},
    },
    utils::{DeviceFd, Physical, Size, Transform},
};
use tracing::{debug, error, info, warn};

pub(super) struct UdevDevice {
    pub(super) path: PathBuf,
    pub(super) render_node: RenderNode,
    pub(super) scanout_node: ScanoutNode,
    pub(super) active: bool,
    pub(super) output_targets: Vec<KmsOutputTarget>,
    pub(super) output_target: Option<KmsOutputTarget>,
    pub(super) output_surfaces: Vec<KmsOutputSurface>,
}

pub(super) struct RenderNode {
    pub(super) drm_fd: DrmDeviceFd,
    pub(super) renderer: GlesRenderer,
}

pub(super) struct ScanoutNode {
    pub(super) drm_fd: DrmDeviceFd,
    pub(super) drm_device: DrmDevice,
}

pub(super) struct KmsOutputSurface {
    pub(super) target: KmsOutputTarget,
    pub(super) gbm_surface: GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>,
    pub(super) damage_tracker: OutputDamageTracker,
    pub(super) frame_pending: bool,
    pub(super) frame_dirty: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct KmsOutputTarget {
    pub(super) connector: connector::Handle,
    pub(super) crtc: crtc::Handle,
    pub(super) mode: Mode,
    connector_name: String,
    physical_size_mm: (i32, i32),
}

pub(super) struct UdevDeviceInfo {
    pub(super) device_id: u64,
    pub(super) path: PathBuf,
}

impl KmsOutputTarget {
    pub(super) fn output_size(&self) -> Size<i32, Physical> {
        let (width, height) = self.mode.size();
        Size::from((i32::from(width), i32::from(height)))
    }

    pub(super) fn output_descriptor(&self) -> super::super::OutputDescriptor {
        super::super::OutputDescriptor {
            name: self.connector_name.clone(),
            properties: PhysicalProperties {
                size: self.physical_size_mm.into(),
                subpixel: Subpixel::Unknown,
                make: "DRM".into(),
                model: self.connector_name.clone(),
            },
            size: self.output_size(),
            scale: 1,
            refresh: self.refresh_millihz(),
        }
    }

    pub(super) fn connector_name(&self) -> &str {
        &self.connector_name
    }

    fn refresh_millihz(&self) -> i32 {
        let refresh_hz = self.mode.vrefresh();
        if refresh_hz == 0 {
            return 60_000;
        }
        i32::try_from(refresh_hz.saturating_mul(1000)).unwrap_or(60_000)
    }
}

impl UdevDevice {
    pub(super) fn output_descriptors(&self) -> Vec<super::super::OutputDescriptor> {
        self.output_targets
            .iter()
            .map(KmsOutputTarget::output_descriptor)
            .collect()
    }

    pub(super) fn connected_output_targets(&self) -> Vec<KmsOutputTarget> {
        let Ok(resources) = self.scanout_node.drm_device.resource_handles() else {
            error!(path = %self.path.display(), "failed to query DRM resource handles");
            return Vec::new();
        };

        let crtcs = resources.crtcs();
        debug!(
            path = %self.path.display(),
            connector_count = resources.connectors().len(),
            crtc_count = crtcs.len(),
            "DRM device resources queried"
        );

        let mut targets = Vec::new();
        let mut used_crtcs = Vec::new();
        for connector_handle in resources.connectors() {
            let Ok(info) = self
                .scanout_node
                .drm_device
                .get_connector(*connector_handle, true)
            else {
                warn!(connector = ?connector_handle, "failed to query DRM connector");
                continue;
            };
            let connected = info.state() == connector::State::Connected;
            debug!(
                connector = ?connector_handle,
                interface = ?info.interface(),
                connected,
                mode_count = info.modes().len(),
                "DRM connector queried"
            );
            if connected {
                if let Some(mode) = info.modes().first() {
                    debug!(connector = ?connector_handle, ?mode, "selected preferred/first DRM mode");
                    let current_crtc = info
                        .current_encoder()
                        .and_then(|encoder| self.scanout_node.drm_device.get_encoder(encoder).ok())
                        .and_then(|encoder| encoder.crtc());
                    let crtc_candidates = info
                        .encoders()
                        .iter()
                        .filter_map(|encoder| {
                            self.scanout_node.drm_device.get_encoder(*encoder).ok().map(
                                |encoder_info| {
                                    resources.filter_crtcs(encoder_info.possible_crtcs())
                                },
                            )
                        })
                        .flatten()
                        .collect::<Vec<_>>();
                    let crtc = current_crtc
                        .filter(|crtc| !used_crtcs.contains(crtc))
                        .or_else(|| {
                            crtc_candidates
                                .iter()
                                .copied()
                                .find(|crtc| !used_crtcs.contains(crtc))
                        });
                    debug!(
                        connector = ?connector_handle,
                        current_crtc = ?current_crtc,
                        crtcs = ?crtc_candidates,
                        selected_crtc = ?crtc,
                        "DRM CRTC candidates"
                    );
                    if let Some(crtc) = crtc {
                        used_crtcs.push(crtc);
                        let physical_size_mm = info
                            .size()
                            .map(|(width, height)| {
                                (
                                    i32::try_from(width).unwrap_or(0),
                                    i32::try_from(height).unwrap_or(0),
                                )
                            })
                            .unwrap_or((0, 0));
                        targets.push(KmsOutputTarget {
                            connector: *connector_handle,
                            crtc,
                            mode: *mode,
                            connector_name: format!(
                                "{}-{}",
                                info.interface().as_str(),
                                info.interface_id()
                            ),
                            physical_size_mm,
                        });
                    }
                }
            }
        }
        if let Some(target) = targets.first() {
            info!(connector = ?target.connector, crtc = ?target.crtc, mode = ?target.mode, "selected KMS output target");
        } else {
            warn!("no connected DRM connector with a usable CRTC was found");
        }
        targets
    }

    pub(super) fn rebuild_output_surfaces(
        &mut self,
        targets: Vec<KmsOutputTarget>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.output_surfaces.clear();
        self.output_target = targets.first().cloned();

        if targets.is_empty() {
            warn!(path = %self.path.display(), "no KMS output target selected; native scanout disabled");
            return Ok(());
        }

        for target in targets {
            match create_output_surface(&mut self.scanout_node, &self.render_node, target.clone()) {
                Ok(surface) => self.output_surfaces.push(surface),
                Err(error) => {
                    error!(connector = ?target.connector, crtc = ?target.crtc, %error, "failed to create KMS output surface");
                }
            }
        }

        if self.output_surfaces.is_empty() {
            return Err("failed to create any KMS output surfaces".into());
        }
        Ok(())
    }

    pub(super) fn has_output_surfaces(&self) -> bool {
        !self.output_surfaces.is_empty()
    }

    pub(super) fn mark_all_surfaces_dirty(&mut self) {
        for surface in &mut self.output_surfaces {
            surface.frame_dirty = true;
        }
    }

    pub(super) fn any_frame_pending(&self) -> bool {
        self.output_surfaces
            .iter()
            .any(|surface| surface.frame_pending)
    }

    pub(super) fn any_frame_dirty(&self) -> bool {
        self.output_surfaces
            .iter()
            .any(|surface| surface.frame_dirty)
    }
}

fn create_output_surface(
    scanout_node: &mut ScanoutNode,
    render_node: &RenderNode,
    target: KmsOutputTarget,
) -> Result<KmsOutputSurface, Box<dyn std::error::Error>> {
    let surface = scanout_node.drm_device.create_surface(
        target.crtc,
        target.mode,
        std::slice::from_ref(&target.connector),
    )?;
    info!(connector = ?target.connector, crtc = ?target.crtc, mode = ?target.mode, "created DRM surface");

    let gbm_device = GbmDevice::new(render_node.drm_fd.clone())?;
    let gbm_allocator = GbmAllocator::new(
        gbm_device,
        GbmBufferFlags::SCANOUT | GbmBufferFlags::RENDERING,
    );
    let renderer_formats = render_node
        .renderer
        .dmabuf_formats()
        .into_iter()
        .collect::<Vec<Format>>();
    let gbm_surface = GbmBufferedSurface::new(
        surface,
        gbm_allocator,
        &[Fourcc::Argb8888, Fourcc::Xrgb8888],
        renderer_formats,
    )?;
    info!(connector = ?target.connector, crtc = ?target.crtc, "created GBM buffered surface");

    Ok(KmsOutputSurface {
        damage_tracker: OutputDamageTracker::new(target.output_size(), 1.0, Transform::Normal),
        frame_pending: false,
        frame_dirty: true,
        target,
        gbm_surface,
    })
}

pub(super) fn create_udev_device(
    session: &mut LibSeatSession,
    path: PathBuf,
    active: bool,
) -> Result<(UdevDevice, smithay::backend::drm::DrmDeviceNotifier), Box<dyn std::error::Error>> {
    info!(path = %path.display(), active, "opening udev DRM device");
    let fd = session.open(&path, OFlags::RDWR | OFlags::CLOEXEC)?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let gbm_device = GbmDevice::new(drm_fd.clone())?;
    let egl_display = unsafe { EGLDisplay::new(gbm_device.clone())? };
    let context = EGLContext::new(&egl_display)?;
    let renderer = unsafe { GlesRenderer::new(context)? };
    // Avoid disabling existing connectors until the first real frame commit path is in place.
    let (drm_device, notifier) = DrmDevice::new(drm_fd.clone(), false)?;
    let mut device = UdevDevice {
        path,
        render_node: RenderNode {
            drm_fd: drm_fd.clone(),
            renderer,
        },
        scanout_node: ScanoutNode { drm_fd, drm_device },
        active,
        output_targets: Vec::new(),
        output_target: None,
        output_surfaces: Vec::new(),
    };
    device.output_targets = device.connected_output_targets();
    device.rebuild_output_surfaces(device.output_targets.clone())?;
    Ok((device, notifier))
}

pub(super) fn initial_device_list(udev_backend: &UdevBackend) -> Vec<UdevDeviceInfo> {
    udev_backend
        .device_list()
        .map(|(device_id, path)| UdevDeviceInfo {
            device_id,
            path: path.to_path_buf(),
        })
        .collect()
}

pub(super) fn current_device_list(seat_name: &str) -> io::Result<Vec<UdevDeviceInfo>> {
    all_gpus(seat_name)?
        .into_iter()
        .map(|path| {
            let device_id = stat(&path)?.st_rdev as u64;
            Ok(UdevDeviceInfo { device_id, path })
        })
        .collect()
}

pub(super) fn log_device_list(seat_name: &str, devices: &[UdevDeviceInfo]) {
    if devices.is_empty() {
        warn!(seat = seat_name, "no DRM devices found for seat");
        return;
    }

    for device in devices {
        debug!(seat = seat_name, device_id = device.device_id, path = %device.path.display(), "found DRM device");
    }
}
