use std::{io, path::PathBuf};

use smithay::{
    backend::{
        allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        allocator::{Format, Fourcc},
        drm::{DrmDevice, DrmDeviceFd, GbmBufferedSurface},
        renderer::damage::OutputDamageTracker,
        udev::{UdevBackend, all_gpus},
    },
    output::{PhysicalProperties, Subpixel},
    reexports::{
        drm::control::{Device as ControlDevice, Mode, connector, crtc},
        rustix::fs::stat,
    },
    utils::{Physical, Size, Transform},
};
use tracing::{debug, error, info, warn};

mod cursor;
mod open;
mod session_fd;

use super::event_cutoff::SurfaceEventCutoff;
use super::{
    identity::{ConnectorNames, DeviceToken},
    presentation::OutputPresentation,
    transfer::SceneBuffer,
};
use cursor::HardwareCursor;
pub(super) use open::create_udev_device;
use smithay::{backend::drm::DrmNode, reexports::calloop::RegistrationToken};

pub(super) struct UdevDevice {
    pub(super) path: PathBuf,
    pub(super) identity: String,
    pub(super) token: DeviceToken,
    pub(super) node: DrmNode,
    pub(super) source: Option<RegistrationToken>,
    pub(super) active: bool,
    // Scanout repair only. Incomplete discovery independently keeps backend
    // recovery/lock readiness blocked without replacing healthy surfaces.
    pub(super) recovery_needed: bool,
    pub(super) discovery_complete: bool,
    pub(super) output_targets: Vec<KmsOutputTarget>,
    pub(super) output_surfaces: Vec<KmsOutputSurface>,
    hardware_cursor: Option<HardwareCursor>,
    hardware_cursor_initialized: bool,
    hardware_cursor_crtc: Option<crtc::Handle>,
    hardware_cursor_position: Option<(i32, i32)>,
    pub(super) gbm_device: GbmDevice<DrmDeviceFd>,
    pub(super) renderer_formats: Vec<Format>,
    pub(super) scanout_node: ScanoutNode,
    // Last field: release libseat ownership after every DRM/GBM holder drops.
    _session_fd: session_fd::SessionFd,
}

pub(super) struct ScanoutNode {
    pub(super) drm_fd: DrmDeviceFd,
    pub(super) drm_device: DrmDevice,
}

pub(super) struct KmsOutputSurface {
    pub(super) target: KmsOutputTarget,
    pub(super) gbm_surface: GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>,
    pub(super) damage_tracker: OutputDamageTracker,
    pub(super) presentation: OutputPresentation,
    pub(super) scene: Option<SceneBuffer>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct KmsOutputTarget {
    pub(super) connector: connector::Handle,
    pub(super) crtc: crtc::Handle,
    pub(super) mode: Mode,
    connector_name: String,
    connector_label: String,
    physical_size_mm: (i32, i32),
}

#[derive(Clone)]
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
                model: self.connector_label.clone(),
                serial_number: self.connector_name.clone(),
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
        self.output_surfaces
            .iter()
            .map(|surface| surface.target.output_descriptor())
            .collect()
    }

    pub(super) fn connected_output_targets(
        &self,
        names: &mut ConnectorNames,
    ) -> Result<(Vec<KmsOutputTarget>, bool), Box<dyn std::error::Error>> {
        let resources = self.scanout_node.drm_device.resource_handles()?;

        let crtcs = resources.crtcs();
        debug!(
            path = %self.path.display(),
            connector_count = resources.connectors().len(),
            crtc_count = crtcs.len(),
            "DRM device resources queried"
        );

        let mut targets = Vec::new();
        let mut complete = true;
        let mut used_crtcs = Vec::new();
        for connector_handle in resources.connectors() {
            let info = self
                .scanout_node
                .drm_device
                .get_connector(*connector_handle, true)?;
            let connected = info.state() == connector::State::Connected;
            if connected && info.modes().is_empty() {
                complete = false;
            }
            debug!(
                connector = ?connector_handle,
                interface = ?info.interface(),
                connected,
                mode_count = info.modes().len(),
                "DRM connector queried"
            );
            if connected && let Some(mode) = info.modes().first() {
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
                            |encoder_info| resources.filter_crtcs(encoder_info.possible_crtcs()),
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
                    let connector_label =
                        format!("{}-{}", info.interface().as_str(), info.interface_id());
                    targets.push(KmsOutputTarget {
                        connector: *connector_handle,
                        crtc,
                        mode: *mode,
                        connector_name: names.assign(&self.identity, &connector_label),
                        connector_label,
                        physical_size_mm,
                    });
                } else {
                    complete = false;
                    warn!(connector = ?connector_handle, "connected DRM output has no usable CRTC");
                }
            }
        }
        if let Some(target) = targets.first() {
            info!(connector = ?target.connector, crtc = ?target.crtc, mode = ?target.mode, "selected KMS output target");
        } else {
            warn!("no connected DRM connector with a usable CRTC was found");
        }
        Ok((targets, complete))
    }

    pub(super) fn rebuild_output_surfaces(
        &mut self,
        targets: Vec<KmsOutputTarget>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.output_surfaces.clear();
        // Dropping active surfaces completes their blocking DRM reset, but their
        // flip events can remain queued until the next event-loop dispatch.
        let event_cutoff = SurfaceEventCutoff::now();
        self.hardware_cursor_initialized = false;
        self.hardware_cursor_crtc = None;
        self.hardware_cursor_position = None;

        if targets.is_empty() {
            warn!(path = %self.path.display(), "no KMS output target selected; native scanout disabled");
            return Ok(());
        }

        let expected_surfaces = targets.len();
        for target in targets {
            match create_output_surface(
                &mut self.scanout_node,
                &self.gbm_device,
                &self.renderer_formats,
                target.clone(),
                self.token,
                event_cutoff,
            ) {
                Ok(surface) => self.output_surfaces.push(surface),
                Err(error) => {
                    error!(connector = ?target.connector, crtc = ?target.crtc, %error, "failed to create KMS output surface");
                }
            }
        }

        if self.output_surfaces.len() != expected_surfaces {
            return Err("failed to create all KMS output surfaces".into());
        }
        Ok(())
    }

    pub(super) fn has_output_surfaces(&self) -> bool {
        !self.output_surfaces.is_empty()
    }

    pub(super) fn mark_all_surfaces_dirty(&mut self) {
        for surface in &mut self.output_surfaces {
            surface.presentation.dirty = true;
        }
    }

    pub(super) fn any_frame_pending(&self) -> bool {
        self.output_surfaces
            .iter()
            .any(|surface| surface.presentation.pending)
    }

    pub(super) fn any_frame_dirty(&self) -> bool {
        self.output_surfaces
            .iter()
            .any(|surface| surface.presentation.dirty)
    }
}

fn create_output_surface(
    scanout_node: &mut ScanoutNode,
    gbm_device: &GbmDevice<DrmDeviceFd>,
    renderer_formats: &[Format],
    target: KmsOutputTarget,
    token: DeviceToken,
    event_cutoff: SurfaceEventCutoff,
) -> Result<KmsOutputSurface, Box<dyn std::error::Error>> {
    let surface = scanout_node.drm_device.create_surface(
        target.crtc,
        target.mode,
        std::slice::from_ref(&target.connector),
    )?;
    info!(connector = ?target.connector, crtc = ?target.crtc, mode = ?target.mode, "created DRM surface");

    let gbm_allocator = GbmAllocator::new(
        gbm_device.clone(),
        GbmBufferFlags::SCANOUT | GbmBufferFlags::RENDERING,
    );
    let gbm_surface = GbmBufferedSurface::new(
        surface,
        gbm_allocator,
        &[Fourcc::Argb8888, Fourcc::Xrgb8888],
        renderer_formats.iter().copied(),
    )?;
    info!(connector = ?target.connector, crtc = ?target.crtc, "created GBM buffered surface");

    Ok(KmsOutputSurface {
        damage_tracker: OutputDamageTracker::new(target.output_size(), 1.0, Transform::Normal),
        presentation: OutputPresentation::new(
            token,
            target.crtc.into(),
            target.connector_name().to_owned(),
            event_cutoff,
        ),
        scene: None,
        target,
        gbm_surface,
    })
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
