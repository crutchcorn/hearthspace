use std::cmp;

use smithay::{
    backend::allocator::Fourcc,
    reexports::drm::{
        Device as BasicDrmDevice, DriverCapability,
        buffer::Buffer as DrmBuffer,
        control::{
            AtomicCommitFlags, Device as ControlDevice, PlaneType, atomic::AtomicModeReq, crtc,
            dumbbuffer, property,
        },
    },
    utils::{Logical, Point, Rectangle},
};
use tracing::{debug, info, warn};

use super::{ScanoutNode, UdevDevice};
use crate::compositor::{
    App,
    cursor::{CursorIcon, CursorImage, cursor_image_for_icon, cursor_image_for_icon_at_size},
};

pub(super) struct HardwareCursor {
    image: CursorImage,
    buffer: dumbbuffer::DumbBuffer,
    hotspot: (i32, i32),
    icon: CursorIcon,
    scale: i32,
}

impl UdevDevice {
    pub(in crate::compositor::udev) fn clear_hardware_cursor(&mut self) {
        if !self.hardware_cursor_initialized {
            clear_all_drm_cursor_planes(&self.scanout_node);
            for output in &self.output_surfaces {
                clear_legacy_cursor(&self.scanout_node, output.target.crtc);
            }
            self.hardware_cursor_initialized = true;
        }
        if let Some(crtc) = self.hardware_cursor_crtc.take() {
            clear_legacy_cursor(&self.scanout_node, crtc);
        }
        self.hardware_cursor_position = None;
    }

    pub(in crate::compositor::udev) fn capture_cursor(
        &self,
        state: &App,
    ) -> Option<(crtc::Handle, crate::compositor::capture::CursorOverlay)> {
        if !state.cursor_visible || state.software_cursor_visible {
            return None;
        }
        let cursor = self.hardware_cursor.as_ref()?;
        let (x, y) = self.hardware_cursor_position?;
        Some((
            self.hardware_cursor_crtc?,
            crate::compositor::capture::CursorOverlay {
                image: cursor.image.clone(),
                plane_size: cursor.buffer.size(),
                x,
                y,
            },
        ))
    }

    pub(in crate::compositor::udev) fn update_hardware_cursor(&mut self, state: &mut App) {
        if !self.hardware_cursor_initialized {
            self.clear_hardware_cursor();
        }
        // Software presentation is required when a focused client is drawing its
        // own cursor surface, or when this device never obtained a usable
        // hardware cursor plane. In those cases the cursor is composited into the
        // scene by the render path, so any lingering hardware cursor is cleared.
        if state.cursor_surface.is_some() || self.hardware_cursor.is_none() {
            state.software_cursor_visible = true;
            self.clear_hardware_cursor();
            return;
        }

        if !state.cursor_visible {
            state.software_cursor_visible = false;
            if let Some(active_crtc) = self.hardware_cursor_crtc.take() {
                clear_legacy_cursor(&self.scanout_node, active_crtc);
            }
            self.hardware_cursor_position = None;
            return;
        }

        let pointer = state.pointer_location;
        let target = self.output_surfaces.iter().find_map(|output| {
            let view = state.output_render_view(output.target.connector_name())?;
            let rect = Rectangle::new(view.location, view.size);
            point_in_logical_rect(pointer, rect).then(|| {
                let scale = view.scale.max(1);
                let local = Point::<f64, Logical>::from((
                    (pointer.x - f64::from(view.location.x)) * f64::from(scale),
                    (pointer.y - f64::from(view.location.y)) * f64::from(scale),
                ));
                (output.target.crtc, local, scale)
            })
        });

        let Some((crtc, local, scale)) = target else {
            state.software_cursor_visible = false;
            if let Some(active_crtc) = self.hardware_cursor_crtc.take() {
                clear_legacy_cursor(&self.scanout_node, active_crtc);
            }
            self.hardware_cursor_position = None;
            return;
        };

        let cursor = self
            .hardware_cursor
            .as_mut()
            .expect("hardware cursor presence checked above");

        // Track whether every DRM cursor operation succeeded. A driver can reject
        // a cursor buffer or move at runtime, in which case the compositor falls
        // back to software presentation for this frame rather than losing the
        // cursor entirely.
        let mut hardware_ok = true;

        if cursor.icon != state.cursor_icon || cursor.scale != scale {
            update_hardware_cursor_image(&self.scanout_node, cursor, state.cursor_icon, scale);
            if let Some(crtc) = self.hardware_cursor_crtc {
                hardware_ok &= set_legacy_cursor(&self.scanout_node, crtc, cursor);
            }
            self.hardware_cursor_position = None;
        }

        let position = (
            local.x.round() as i32 - cursor.hotspot.0,
            local.y.round() as i32 - cursor.hotspot.1,
        );

        if self.hardware_cursor_crtc != Some(crtc) {
            if let Some(active_crtc) = self.hardware_cursor_crtc {
                clear_legacy_cursor(&self.scanout_node, active_crtc);
            }
            hardware_ok &= set_legacy_cursor(&self.scanout_node, crtc, cursor);
            self.hardware_cursor_crtc = Some(crtc);
            self.hardware_cursor_position = None;
        }

        if self.hardware_cursor_position != Some(position) {
            hardware_ok &= move_legacy_cursor(&self.scanout_node, crtc, position);
            self.hardware_cursor_position = Some(position);
        }

        // If any DRM cursor operation failed, present the cursor in software for
        // this frame instead of leaving it stuck or invisible.
        state.software_cursor_visible = !hardware_ok;
        if !hardware_ok {
            if let Some(active_crtc) = self.hardware_cursor_crtc.take() {
                clear_legacy_cursor(&self.scanout_node, active_crtc);
            }
            self.hardware_cursor_position = None;
        }
    }
}

fn set_legacy_cursor(
    scanout_node: &ScanoutNode,
    crtc: crtc::Handle,
    cursor: &HardwareCursor,
) -> bool {
    #[allow(deprecated)]
    match scanout_node.drm_fd.set_cursor(crtc, Some(&cursor.buffer)) {
        Ok(()) => {
            debug!(?crtc, "set legacy DRM cursor");
            true
        }
        Err(error) => {
            debug!(?crtc, %error, "legacy DRM cursor set failed");
            false
        }
    }
}

fn move_legacy_cursor(
    scanout_node: &ScanoutNode,
    crtc: crtc::Handle,
    position: (i32, i32),
) -> bool {
    #[allow(deprecated)]
    match scanout_node.drm_fd.move_cursor(crtc, position) {
        Ok(()) => {
            debug!(?crtc, ?position, "moved legacy DRM cursor");
            true
        }
        Err(error) => {
            debug!(?crtc, ?position, %error, "legacy DRM cursor move failed");
            false
        }
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

pub(super) fn create_hardware_cursor(scanout_node: &ScanoutNode) -> Option<HardwareCursor> {
    let icon = CursorIcon::Default;
    let image = cursor_image_for_icon(icon);
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
    Some(HardwareCursor {
        image,
        buffer,
        hotspot,
        icon,
        scale: 1,
    })
}

fn update_hardware_cursor_image(
    scanout_node: &ScanoutNode,
    cursor: &mut HardwareCursor,
    icon: CursorIcon,
    scale: i32,
) {
    let (width, height) = cursor.buffer.size();
    let desired_size = 24_u32
        .saturating_mul(u32::try_from(scale.max(1)).unwrap_or(1))
        .min(width.max(1))
        .min(height.max(1));
    let image = cursor_image_for_icon_at_size(icon, desired_size);
    let pitch = match usize::try_from(cursor.buffer.pitch()) {
        Ok(pitch) => pitch,
        Err(error) => {
            warn!(%error, "failed to read DRM hardware cursor pitch");
            return;
        }
    };
    let mut mapping = match scanout_node.drm_device.map_dumb_buffer(&mut cursor.buffer) {
        Ok(mapping) => mapping,
        Err(error) => {
            warn!(%error, ?icon, "failed to map DRM hardware cursor buffer for icon update");
            return;
        }
    };
    mapping.fill(0);
    copy_cursor_rgba_to_argb8888(&image, mapping.as_mut(), pitch, width, height);
    drop(mapping);

    cursor.hotspot = (
        cmp::min(image.hotspot.0.max(0), width.saturating_sub(1) as i32),
        cmp::min(image.hotspot.1.max(0), height.saturating_sub(1) as i32),
    );
    cursor.icon = icon;
    cursor.image = image;
    cursor.scale = scale.max(1);
    debug!(?icon, scale = cursor.scale, hotspot = ?cursor.hotspot, "updated DRM hardware cursor image");
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
