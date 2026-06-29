use std::{io, path::PathBuf};

use smithay::{
    backend::{
        allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        allocator::{Format, Fourcc},
        drm::{DrmDevice, DrmDeviceFd, GbmBufferedSurface},
        egl::{EGLContext, EGLDisplay},
        renderer::{ImportDma, gles::GlesRenderer},
        session::{Session, libseat::LibSeatSession},
        udev::{UdevBackend, all_gpus},
    },
    output::{PhysicalProperties, Subpixel},
    reexports::{
        drm::control::{Device as ControlDevice, Mode, connector, crtc},
        rustix::fs::{OFlags, stat},
    },
    utils::{DeviceFd, Physical, Size},
};

pub(super) struct UdevDevice {
    pub(super) path: PathBuf,
    pub(super) render_node: RenderNode,
    pub(super) scanout_node: ScanoutNode,
    pub(super) active: bool,
    pub(super) output_targets: Vec<KmsOutputTarget>,
    pub(super) output_target: Option<KmsOutputTarget>,
    pub(super) output_surface: Option<KmsOutputSurface>,
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
            eprintln!(
                "Failed to query DRM resource handles for {}",
                self.path.display()
            );
            return Vec::new();
        };

        let crtcs = resources.crtcs();
        println!(
            "DRM device {} exposes {} connector(s) and {} CRTC(s)",
            self.path.display(),
            resources.connectors().len(),
            crtcs.len()
        );

        let mut targets = Vec::new();
        for connector_handle in resources.connectors() {
            let Ok(info) = self
                .scanout_node
                .drm_device
                .get_connector(*connector_handle, true)
            else {
                eprintln!("Failed to query DRM connector {:?}", connector_handle);
                continue;
            };
            let connected = info.state() == connector::State::Connected;
            println!(
                "DRM connector {:?} {:?} connected={} mode_count={}",
                connector_handle,
                info.interface(),
                connected,
                info.modes().len()
            );
            if connected {
                if let Some(mode) = info.modes().first() {
                    println!(
                        "Preferred/first mode for {:?}: {:?}",
                        connector_handle, mode
                    );
                    let crtc = info
                        .current_encoder()
                        .and_then(|encoder| self.scanout_node.drm_device.get_encoder(encoder).ok())
                        .and_then(|encoder| encoder.crtc())
                        .or_else(|| {
                            info.encoders().iter().find_map(|encoder| {
                                self.scanout_node
                                    .drm_device
                                    .get_encoder(*encoder)
                                    .ok()
                                    .and_then(|encoder_info| {
                                        resources
                                            .filter_crtcs(encoder_info.possible_crtcs())
                                            .into_iter()
                                            .next()
                                    })
                            })
                        });
                    if let Some(crtc) = crtc {
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
                println!("CRTC candidates for {:?}: {:?}", connector_handle, crtcs);
            }
        }
        if let Some(target) = targets.first() {
            println!(
                "Selected KMS target connector {:?}, CRTC {:?}, mode {:?}",
                target.connector, target.crtc, target.mode
            );
        } else {
            println!("No connected DRM connector with a usable CRTC was found");
        }
        targets
    }
}

pub(super) fn create_udev_device(
    session: &mut LibSeatSession,
    path: PathBuf,
    active: bool,
) -> Result<(UdevDevice, smithay::backend::drm::DrmDeviceNotifier), Box<dyn std::error::Error>> {
    let fd = session.open(&path, OFlags::RDWR | OFlags::CLOEXEC)?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let gbm_device = GbmDevice::new(drm_fd.clone())?;
    let egl_display = unsafe { EGLDisplay::new(gbm_device.clone())? };
    let context = EGLContext::new(&egl_display)?;
    let renderer = unsafe { GlesRenderer::new(context)? };
    let gbm_allocator = GbmAllocator::new(
        gbm_device,
        GbmBufferFlags::SCANOUT | GbmBufferFlags::RENDERING,
    );
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
        output_surface: None,
    };
    device.output_targets = device.connected_output_targets();
    device.output_target = device.output_targets.first().cloned();
    if let Some(target) = device.output_target.clone() {
        let surface = device.scanout_node.drm_device.create_surface(
            target.crtc,
            target.mode,
            std::slice::from_ref(&target.connector),
        )?;
        println!(
            "Created DRM surface for connector {:?}, CRTC {:?}, mode {:?}",
            target.connector, target.crtc, target.mode
        );
        let renderer_formats = device
            .render_node
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
        println!(
            "Created GBM buffered surface for connector {:?}, CRTC {:?}",
            target.connector, target.crtc
        );
        device.output_surface = Some(KmsOutputSurface {
            target,
            gbm_surface,
        });
    }
    Ok((device, notifier))
}

pub(super) fn initial_device_list(udev_backend: &UdevBackend) -> Vec<UdevDeviceInfo> {
    udev_backend
        .device_list()
        .map(|(device_id, path)| UdevDeviceInfo {
            device_id: device_id as u64,
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
        println!("No DRM devices found for seat {seat_name}");
        return;
    }

    for device in devices {
        println!(
            "Found DRM device {} at {}",
            device.device_id,
            device.path.display()
        );
    }
}
