//! Device resources and the legacy DRM operation boundary for cursor handoff.
use super::{ScanoutNode, UdevDevice};
use crate::compositor::{
    App,
    cursor::{CursorIcon, CursorImage},
};
use smithay::reexports::drm::{
    buffer::Buffer,
    control::{crtc, dumbbuffer},
};

mod drm;
pub(in crate::compositor::udev) mod policy;
pub(in crate::compositor::udev) use drm::create_hardware_cursor;
use policy::{CursorOps, ImageKey, Scene, Target};

pub(in crate::compositor::udev) struct HardwareCursor {
    image: CursorImage,
    buffer: dumbbuffer::DumbBuffer,
    hotspot: (i32, i32),
    icon: CursorIcon,
    scale: i32,
    assigned: Option<crtc::Handle>,
}

struct Operations<'a> {
    node: &'a ScanoutNode,
    cursor: &'a mut Option<HardwareCursor>,
}

impl CursorOps for Operations<'_> {
    fn clear_all_planes(&mut self) {
        drm::clear_all_drm_cursor_planes(self.node);
    }

    fn image(&mut self, key: ImageKey) -> Option<(i32, i32)> {
        let cursor = self.cursor.as_mut()?;
        if (cursor.icon != key.icon || cursor.scale != key.scale)
            && !drm::update_hardware_cursor_image(self.node, cursor, key.icon, key.scale)
        {
            return None;
        }
        Some(cursor.hotspot)
    }

    fn set(&mut self, crtc: crtc::Handle) -> bool {
        let Some(cursor) = self.cursor.as_mut() else {
            return false;
        };
        if cursor.assigned == Some(crtc) {
            return true;
        }
        if !drm::set_legacy_cursor(self.node, crtc, cursor) {
            return false;
        }
        cursor.assigned = Some(crtc);
        true
    }

    fn move_to(&mut self, crtc: crtc::Handle, position: (i32, i32)) -> bool {
        drm::move_legacy_cursor(self.node, crtc, position)
    }

    fn clear(&mut self, crtc: crtc::Handle) -> bool {
        if !drm::clear_legacy_cursor(self.node, crtc) {
            return false;
        }
        if let Some(cursor) = self.cursor.as_mut()
            && cursor.assigned == Some(crtc)
        {
            cursor.assigned = None;
        }
        true
    }
}

impl UdevDevice {
    pub(in crate::compositor::udev) fn initialize_cursor(&mut self) {
        self.cursor_plane.initialize(
            self.output_surfaces.iter().map(|output| output.target.crtc),
            &mut Operations {
                node: &self.scanout_node,
                cursor: &mut self.hardware_cursor,
            },
        );
    }

    pub(in crate::compositor::udev) fn clear_hardware_cursor(&mut self) {
        self.initialize_cursor();
        self.cursor_plane.clear(&mut Operations {
            node: &self.scanout_node,
            cursor: &mut self.hardware_cursor,
        });
    }

    /// Seat loss/rebuild invalidates both presentation history and cached KMS
    /// assignment. This does not issue ioctls after another session owns the seat.
    pub(in crate::compositor::udev) fn reset_cursor(&mut self) {
        self.cursor_plane.reset();
        if let Some(cursor) = self.hardware_cursor.as_mut() {
            cursor.assigned = None;
        }
    }

    pub(in crate::compositor::udev) fn cursor_requires_software(&self, target: Target) -> bool {
        self.hardware_cursor.is_none() || self.cursor_plane.failed(target)
    }

    pub(in crate::compositor::udev) fn update_cursor(
        &mut self,
        target: Target,
        software: bool,
        scene: Scene,
    ) -> bool {
        self.initialize_cursor();
        self.cursor_plane.update(
            target,
            software,
            scene,
            &mut Operations {
                node: &self.scanout_node,
                cursor: &mut self.hardware_cursor,
            },
        )
    }

    pub(in crate::compositor::udev) fn retire_previous_cursor(
        &mut self,
        scene: Scene,
        replacement_hardware: bool,
    ) {
        self.cursor_plane.retire_previous(
            scene,
            replacement_hardware,
            &mut Operations {
                node: &self.scanout_node,
                cursor: &mut self.hardware_cursor,
            },
        );
    }

    pub(in crate::compositor::udev) fn capture_cursor(
        &self,
        state: &App,
    ) -> Option<(crtc::Handle, crate::compositor::capture::CursorOverlay)> {
        if !state.cursor_visible {
            return None;
        }
        let cursor = self.hardware_cursor.as_ref()?;
        let active = self.cursor_plane.active?;
        let (x, y) = active.position?;
        Some((
            active.crtc,
            crate::compositor::capture::CursorOverlay {
                image: cursor.image.clone(),
                plane_size: cursor.buffer.size(),
                x,
                y,
            },
        ))
    }
}

#[cfg(test)]
pub(in crate::compositor::udev) mod tests;
