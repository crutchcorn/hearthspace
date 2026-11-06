//! Conservative fullscreen admission. Smithay/KMS decide hardware suitability.
use super::{App, HearthspaceRenderElement};
use smithay::{
    backend::{
        allocator::{Buffer as _, dmabuf::Dmabuf},
        renderer::{
            element::{Element, RenderElement, UnderlyingStorage},
            gles::GlesRenderer,
            utils::Buffer,
        },
    },
    utils::{Logical, Physical, Rectangle, Size, Transform},
    wayland::dmabuf::get_dmabuf,
};

impl App {
    /// Snapshot only immutable buffer ownership, never a live surface or GLES texture.
    pub(in crate::compositor) fn fullscreen_scanout(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
        scale: i32,
        size: Size<i32, Physical>,
    ) -> Option<(Buffer, Dmabuf)> {
        if self.session_locked()
            || !self.fullscreen_on_output(output)
            || self.canvas_preview_presenting()
            || self.viewport_scale != 1.0
            || self.viewport_animation.is_some()
            || self.layout_requires_composition()
        {
            return None;
        }
        // Use the same stack as composition so shell, OSD, popup, selection,
        // subsurface, and other overlays cannot be silently discarded.
        let scene = self.collect_render_elements(renderer, output);
        if !scene.window_backdrops.is_empty() {
            return None;
        }
        let [HearthspaceRenderElement::Surface(element)] = scene.elements.as_slice() else {
            return None;
        };
        let UnderlyingStorage::Wayland(buffer) = element.underlying_storage(renderer)? else {
            return None;
        };
        let dmabuf = get_dmabuf(buffer).ok()?;
        let geometry = element.geometry(1.0.into());
        let expected = Rectangle::new(output.loc.to_physical(scale), size);
        // Elements are collected in logical-sized physical coordinates and then
        // rescaled per output by render_frame_at. Opacity is element-local.
        let geometry = Rectangle::new(
            geometry.loc.to_logical(1).to_physical(scale),
            geometry.size.to_logical(1).to_physical(scale),
        );
        if !native_opaque_surface(element, geometry, expected, dmabuf.size()) {
            return None;
        }
        Some((buffer.clone(), dmabuf.clone()))
    }
}

fn native_opaque_surface(
    element: &impl Element,
    geometry: Rectangle<i32, Physical>,
    expected: Rectangle<i32, Physical>,
    buffer_size: Size<i32, smithay::utils::Buffer>,
) -> bool {
    geometry == expected
        && element.alpha() == 1.0
        && element.transform() == Transform::Normal
        && element.src() == Rectangle::from_size(buffer_size).to_f64()
        && (buffer_size.w, buffer_size.h) == (expected.size.w, expected.size.h)
        && Rectangle::from_size(element.geometry(1.0.into()).size)
            .subtract_rects(element.opaque_regions(1.0.into()).iter().copied())
            .is_empty()
}

#[cfg(test)]
mod tests;
