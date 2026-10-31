use super::{HardwareCursor, ScanoutNode};
use crate::compositor::cursor::{
    CursorIcon, CursorImage, cursor_image_for_icon, cursor_image_for_icon_at_size,
};
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
};
use std::cmp;
use tracing::{debug, info, warn};

pub(super) fn set_legacy_cursor(
    scanout_node: &ScanoutNode,
    crtc: crtc::Handle,
    cursor: &HardwareCursor,
) -> bool {
    let started = std::time::Instant::now();
    #[allow(deprecated)]
    match scanout_node.drm_fd.set_cursor(crtc, Some(&cursor.buffer)) {
        Ok(()) => {
            debug!(
                ?crtc,
                duration_us = started.elapsed().as_micros() as u64,
                "set legacy DRM cursor"
            );
            true
        }
        Err(error) => {
            debug!(?crtc, %error, duration_us = started.elapsed().as_micros() as u64, "legacy DRM cursor set failed");
            false
        }
    }
}

pub(super) fn move_legacy_cursor(
    scanout_node: &ScanoutNode,
    crtc: crtc::Handle,
    position: (i32, i32),
) -> bool {
    let started = std::time::Instant::now();
    #[allow(deprecated)]
    match scanout_node.drm_fd.move_cursor(crtc, position) {
        Ok(()) => {
            debug!(
                ?crtc,
                ?position,
                duration_us = started.elapsed().as_micros() as u64,
                "moved legacy DRM cursor"
            );
            true
        }
        Err(error) => {
            debug!(?crtc, ?position, %error, duration_us = started.elapsed().as_micros() as u64, "legacy DRM cursor move failed");
            false
        }
    }
}

pub(super) fn clear_legacy_cursor(scanout_node: &ScanoutNode, crtc: crtc::Handle) -> bool {
    let started = std::time::Instant::now();
    #[allow(deprecated)]
    match scanout_node
        .drm_fd
        .set_cursor(crtc, Option::<&dumbbuffer::DumbBuffer>::None)
    {
        Ok(()) => {
            debug!(
                ?crtc,
                duration_us = started.elapsed().as_micros() as u64,
                "cleared legacy DRM cursor"
            );
            true
        }
        Err(error) => {
            debug!(?crtc, %error, duration_us = started.elapsed().as_micros() as u64, "legacy DRM cursor clear failed");
            false
        }
    }
}

pub(super) fn clear_all_drm_cursor_planes(scanout_node: &ScanoutNode) {
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

pub(in crate::compositor::udev) fn create_hardware_cursor(
    scanout_node: &ScanoutNode,
) -> Option<HardwareCursor> {
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
        assigned: None,
    })
}

pub(super) fn update_hardware_cursor_image(
    scanout_node: &ScanoutNode,
    cursor: &mut HardwareCursor,
    icon: CursorIcon,
    scale: i32,
) -> bool {
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
            return false;
        }
    };
    let mut mapping = match scanout_node.drm_device.map_dumb_buffer(&mut cursor.buffer) {
        Ok(mapping) => mapping,
        Err(error) => {
            warn!(%error, ?icon, "failed to map DRM hardware cursor buffer for icon update");
            return false;
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
    cursor.assigned = None;
    debug!(?icon, scale = cursor.scale, hotspot = ?cursor.hotspot, "updated DRM hardware cursor image");
    true
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
