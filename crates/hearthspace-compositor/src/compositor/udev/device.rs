use std::{cmp, io, path::PathBuf};

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
        drm::{
            Device as BasicDrmDevice, DriverCapability,
            buffer::Buffer as DrmBuffer,
            control::{
                AtomicCommitFlags, Device as ControlDevice, Mode, PlaneType, atomic::AtomicModeReq,
                connector, crtc, dumbbuffer, property,
            },
        },
        rustix::fs::{OFlags, stat},
    },
    utils::{DeviceFd, Logical, Physical, Point, Rectangle, Size, Transform},
};
use tracing::{debug, error, info, warn};

use super::super::cursor::{CursorImage, standard_cursor_image};

pub(super) struct UdevDevice {
    pub(super) path: PathBuf,
    pub(super) render_node: RenderNode,
    pub(super) scanout_node: ScanoutNode,
    pub(super) active: bool,
    pub(super) output_targets: Vec<KmsOutputTarget>,
    pub(super) output_target: Option<KmsOutputTarget>,
    pub(super) output_surfaces: Vec<KmsOutputSurface>,
    hardware_cursor: Option<HardwareCursor>,
    hardware_cursor_initialized: bool,
    hardware_cursor_crtc: Option<crtc::Handle>,
    hardware_cursor_position: Option<(i32, i32)>,
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

struct HardwareCursor {
    buffer: dumbbuffer::DumbBuffer,
    hotspot: (i32, i32),
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
        self.hardware_cursor_initialized = false;
        self.hardware_cursor_crtc = None;
        self.hardware_cursor_position = None;

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

    pub(super) fn update_hardware_cursor(&mut self, state: &super::super::App) {
        let Some(cursor) = self.hardware_cursor.as_ref() else {
            return;
        };

        if !self.hardware_cursor_initialized {
            clear_all_drm_cursor_planes(&self.scanout_node);
            for output in &self.output_surfaces {
                clear_legacy_cursor(&self.scanout_node, output.target.crtc);
            }
            self.hardware_cursor_initialized = true;
        }

        let pointer = state.pointer_location;
        let target = self.output_surfaces.iter().find_map(|output| {
            let view = state.output_render_view(output.target.connector_name())?;
            let rect = Rectangle::new(view.location, view.size);
            point_in_logical_rect(pointer, rect).then(|| {
                let x = (pointer.x - f64::from(view.location.x)).round() as i32 - cursor.hotspot.0;
                let y = (pointer.y - f64::from(view.location.y)).round() as i32 - cursor.hotspot.1;
                (output.target.crtc, (x, y))
            })
        });

        let Some((crtc, position)) = target else {
            if let Some(active_crtc) = self.hardware_cursor_crtc.take() {
                clear_legacy_cursor(&self.scanout_node, active_crtc);
            }
            self.hardware_cursor_position = None;
            return;
        };

        if self.hardware_cursor_crtc != Some(crtc) {
            if let Some(active_crtc) = self.hardware_cursor_crtc {
                clear_legacy_cursor(&self.scanout_node, active_crtc);
            }
            set_legacy_cursor(&self.scanout_node, crtc, cursor);
            self.hardware_cursor_crtc = Some(crtc);
            self.hardware_cursor_position = None;
        }

        if self.hardware_cursor_position != Some(position) {
            move_legacy_cursor(&self.scanout_node, crtc, position);
            self.hardware_cursor_position = Some(position);
        }
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

fn set_legacy_cursor(scanout_node: &ScanoutNode, crtc: crtc::Handle, cursor: &HardwareCursor) {
    #[allow(deprecated)]
    match scanout_node.drm_fd.set_cursor(crtc, Some(&cursor.buffer)) {
        Ok(()) => debug!(?crtc, "set legacy DRM cursor"),
        Err(error) => debug!(?crtc, %error, "legacy DRM cursor set failed"),
    }
}

fn move_legacy_cursor(scanout_node: &ScanoutNode, crtc: crtc::Handle, position: (i32, i32)) {
    #[allow(deprecated)]
    match scanout_node.drm_fd.move_cursor(crtc, position) {
        Ok(()) => debug!(?crtc, ?position, "moved legacy DRM cursor"),
        Err(error) => debug!(?crtc, ?position, %error, "legacy DRM cursor move failed"),
    }
}

fn clear_legacy_cursor(scanout_node: &ScanoutNode, crtc: crtc::Handle) {
    #[allow(deprecated)]
    match scanout_node
        .drm_fd
        .set_cursor(crtc, Option::<&dumbbuffer::DumbBuffer>::None)
    {
        Ok(()) => debug!(?crtc, "cleared legacy DRM cursor"),
        Err(error) => debug!(?crtc, %error, "legacy DRM cursor clear failed"),
    }
}

fn clear_all_drm_cursor_planes(scanout_node: &ScanoutNode) {
    let Ok(planes) = scanout_node.drm_device.plane_handles() else {
        debug!("failed to query DRM planes for cursor clear");
        return;
    };

    let mut request = AtomicModeReq::new();
    let mut cursor_planes = Vec::new();
    for plane in planes {
        let Ok(properties) = scanout_node.drm_device.get_properties(plane) else {
            debug!(
                ?plane,
                "failed to query DRM plane properties for cursor clear"
            );
            continue;
        };
        let Ok(property_info) = properties.as_hashmap(&scanout_node.drm_device) else {
            debug!(
                ?plane,
                "failed to map DRM plane properties for cursor clear"
            );
            continue;
        };
        let Some(type_property) = property_info.get("type") else {
            continue;
        };
        let Some((_, type_value)) = properties
            .iter()
            .find(|(property, _)| **property == type_property.handle())
        else {
            continue;
        };
        if *type_value != PlaneType::Cursor as u64 {
            continue;
        }
        let Some(crtc_id) = property_info.get("CRTC_ID") else {
            continue;
        };
        let Some(fb_id) = property_info.get("FB_ID") else {
            continue;
        };
        request.add_property(plane, crtc_id.handle(), property::Value::CRTC(None));
        request.add_property(plane, fb_id.handle(), property::Value::Framebuffer(None));
        cursor_planes.push(plane);
    }

    if cursor_planes.is_empty() {
        return;
    }

    match scanout_node
        .drm_device
        .atomic_commit(AtomicCommitFlags::empty(), request)
    {
        Ok(()) => debug!(?cursor_planes, "cleared all DRM cursor planes"),
        Err(error) => debug!(?cursor_planes, %error, "all DRM cursor plane clear failed"),
    }
}

fn create_hardware_cursor(scanout_node: &ScanoutNode) -> Option<HardwareCursor> {
    let image = standard_cursor_image();
    let width = cursor_capability(scanout_node, DriverCapability::CursorWidth)
        .unwrap_or(image.width)
        .max(image.width);
    let height = cursor_capability(scanout_node, DriverCapability::CursorHeight)
        .unwrap_or(image.height)
        .max(image.height);
    let mut buffer =
        match scanout_node
            .drm_device
            .create_dumb_buffer((width, height), Fourcc::Argb8888, 32)
        {
            Ok(buffer) => buffer,
            Err(error) => {
                warn!(%error, width, height, "failed to create DRM hardware cursor buffer");
                return None;
            }
        };

    let pitch = usize::try_from(buffer.pitch()).ok()?;
    let mut mapping = match scanout_node.drm_device.map_dumb_buffer(&mut buffer) {
        Ok(mapping) => mapping,
        Err(error) => {
            warn!(%error, "failed to map DRM hardware cursor buffer");
            return None;
        }
    };
    mapping.fill(0);
    copy_cursor_rgba_to_argb8888(&image, mapping.as_mut(), pitch, width, height);
    drop(mapping);

    let hotspot = (
        cmp::min(image.hotspot.0.max(0), width.saturating_sub(1) as i32),
        cmp::min(image.hotspot.1.max(0), height.saturating_sub(1) as i32),
    );
    info!(
        width,
        height,
        ?hotspot,
        "created DRM hardware cursor buffer"
    );
    Some(HardwareCursor { buffer, hotspot })
}

fn cursor_capability(scanout_node: &ScanoutNode, capability: DriverCapability) -> Option<u32> {
    scanout_node
        .drm_device
        .get_driver_capability(capability)
        .ok()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn copy_cursor_rgba_to_argb8888(
    image: &CursorImage,
    target: &mut [u8],
    pitch: usize,
    cursor_width: u32,
    cursor_height: u32,
) {
    let copy_width = cmp::min(image.width, cursor_width) as usize;
    let copy_height = cmp::min(image.height, cursor_height) as usize;
    let image_width = image.width as usize;
    for y in 0..copy_height {
        for x in 0..copy_width {
            let source = (y * image_width + x) * 4;
            let destination = y * pitch + x * 4;
            let [red, green, blue, alpha] = image.pixels_rgba[source..source + 4] else {
                continue;
            };
            // DRM_FORMAT_ARGB8888 is stored as BGRA bytes on little-endian hosts.
            target[destination..destination + 4].copy_from_slice(&[blue, green, red, alpha]);
        }
    }
}

fn point_in_logical_rect(point: Point<f64, Logical>, rect: Rectangle<i32, Logical>) -> bool {
    point.x >= f64::from(rect.loc.x)
        && point.y >= f64::from(rect.loc.y)
        && point.x < f64::from(rect.loc.x + rect.size.w)
        && point.y < f64::from(rect.loc.y + rect.size.h)
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
        hardware_cursor: None,
        hardware_cursor_initialized: false,
        hardware_cursor_crtc: None,
        hardware_cursor_position: None,
    };
    device.hardware_cursor = create_hardware_cursor(&device.scanout_node);
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
