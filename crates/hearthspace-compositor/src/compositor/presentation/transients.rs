use smithay::{
    desktop::{PopupKind, find_popup_root_surface, get_popup_toplevel_coords},
    utils::{Logical, Point, Rectangle},
    wayland::shell::xdg::PopupSurface,
};

use super::super::{App, ManagedSurface};
use crate::{config::TITLE_BAR_HEIGHT, geometry::CanvasPoint};

impl App {
    pub(in crate::compositor) fn transient_descendant_of(
        &self,
        mut index: usize,
        ancestor: u64,
    ) -> bool {
        for _ in 0..self.windows.len() {
            let Some(parent) = self.transient_parent_index(index) else {
                return false;
            };
            if self.windows[parent].id == ancestor {
                return true;
            }
            index = parent;
        }
        false
    }

    pub(in crate::compositor) fn transient_parent_index(&self, index: usize) -> Option<usize> {
        let parent = match &self.windows[index].surface {
            ManagedSurface::Xdg(surface) => surface.parent().and_then(|root| self.window_index_for_surface(&root)),
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(surface) => surface.is_transient_for().and_then(|parent| {
                self.windows.iter().position(|window| matches!(&window.surface, ManagedSurface::X11(surface) if surface.window_id() == parent))
            }),
        };
        parent.filter(|parent| *parent != index)
    }

    /// Nearest output-pinned ancestor, including the window itself. Bound the
    /// walk so a broken client-supplied parent chain cannot trap the compositor.
    pub(in crate::compositor) fn presentation_anchor(&self, mut index: usize) -> Option<usize> {
        for _ in 0..self.windows.len() {
            if self.windows[index].presentation.pinned() {
                return Some(index);
            }
            index = self.transient_parent_index(index)?;
        }
        None
    }

    pub(in crate::compositor) fn window_uses_output_coordinates(&self, index: usize) -> bool {
        self.presentation_anchor(index).is_some()
    }

    pub(in crate::compositor) fn window_presentation_output(
        &self,
        index: usize,
    ) -> Option<Rectangle<i32, Logical>> {
        self.presentation_anchor(index)
            .and_then(|anchor| self.presentation_output_rect(anchor))
    }

    pub(in crate::compositor) fn transient_screen_origin(
        &self,
        index: usize,
    ) -> Option<Point<f64, Logical>> {
        if self.windows[index].presentation.pinned() {
            return None;
        }
        let parent = self.presentation_anchor(index)?;
        let mut output = self.presentation_output_rect(parent)?;
        if !self.windows[parent].presentation.fullscreen {
            output = self.usable_output(output);
        }
        let size = self.window_geometry_size(index);
        let chrome = if self.has_compositor_chrome(index) {
            TITLE_BAR_HEIGHT
        } else {
            0
        };
        let origin = centered_origin(output, (size.w, size.h + chrome).into());
        Some((origin + Point::from((0, chrome)) - self.window_surface_geometry_loc(index)).to_f64())
    }

    pub(in crate::compositor) fn position_transient(&mut self, index: usize) {
        let Some(parent) = self.transient_parent_index(index) else {
            return;
        };
        if self.windows[index].presentation.pinned() {
            return;
        }
        let parent = self.window_canvas_rect(parent);
        let child = self.window_canvas_rect(index);
        self.windows[index].pending_resize = None;
        self.windows[index].position =
            centered_canvas_position(parent, child, self.windows[index].position);
        self.request_redraw();
    }

    pub(in crate::compositor) fn unconstrain_popup(&self, popup: &PopupSurface) {
        let kind = PopupKind::Xdg(popup.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Some(index) = self.window_index_for_surface(&root) else {
            return;
        };
        let output_name = self.presentation_output_for_window(index, None);
        let Some(mut output) = self.output_logical_rect_for_name(Some(&output_name)) else {
            return;
        };
        if !self
            .presentation_anchor(index)
            .is_some_and(|anchor| self.windows[anchor].presentation.fullscreen)
        {
            output = self.usable_output(output);
        }
        let mut origin = self
            .window_screen_to_surface(index, output.loc.to_f64())
            .to_i32_round();
        origin -= self.window_surface_geometry_loc(index);
        origin -= get_popup_toplevel_coords(&kind);
        let scale = self.window_render_scale(index);
        let target = Rectangle::new(
            origin,
            (
                (f64::from(output.size.w) / scale).floor() as i32,
                (f64::from(output.size.h) / scale).floor() as i32,
            )
                .into(),
        );
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

fn centered_origin(
    bounds: Rectangle<i32, Logical>,
    size: smithay::utils::Size<i32, Logical>,
) -> Point<i32, Logical> {
    bounds.loc
        + Point::from((
            (bounds.size.w - size.w).max(0) / 2,
            (bounds.size.h - size.h).max(0) / 2,
        ))
}

fn centered_canvas_position(
    parent: Rectangle<i32, Logical>,
    child: Rectangle<i32, Logical>,
    position: CanvasPoint,
) -> CanvasPoint {
    // Center the visible window geometry, then remove the child's CSD shadow
    // inset to recover its surface/canvas origin. SSD geometry has no inset.
    let origin =
        Point::from((position.x, position.y)) + centered_origin(parent, child.size) - child.loc;
    CanvasPoint {
        x: origin.x,
        y: origin.y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_size_csd_dialog_aligns_visible_geometry_with_parent() {
        let parent = Rectangle::new((94, 108).into(), (1280, 752).into());
        let child = Rectangle::new((94, 108).into(), (1280, 752).into());
        assert_eq!(
            centered_canvas_position(parent, child, CanvasPoint { x: 80, y: 96 }),
            CanvasPoint { x: 80, y: 96 }
        );
    }

    #[test]
    fn server_decorated_dialog_keeps_its_canvas_origin_at_geometry_origin() {
        let parent = Rectangle::new((100, 200).into(), (800, 600).into());
        let child = Rectangle::new((0, 0).into(), (300, 200).into());
        assert_eq!(
            centered_canvas_position(parent, child, CanvasPoint { x: 0, y: 0 }),
            CanvasPoint { x: 350, y: 400 }
        );
    }
}
