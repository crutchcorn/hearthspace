//! Advisory initial bounds keep ordinary clients reachable without fixing sizes.
use smithay::{
    utils::{Logical, Point, Size},
    wayland::shell::xdg::ToplevelSurface,
};
use wayland_server::protocol::wl_surface::WlSurface;

use crate::{compositor::App, config::TITLE_BAR_HEIGHT};

impl App {
    pub(in crate::compositor) fn configure_initial_toplevel(&self, root: &WlSurface) {
        let Some(index) = self.window_index_for_surface(root) else {
            return;
        };
        let window = &self.windows[index];
        let Some(surface) = window.surface.xdg_toplevel() else {
            return;
        };
        if surface.wl_surface() != root || surface.is_initial_configure_sent() {
            return;
        }
        self.configure_toplevel(surface, window.kind, window.shell_output_name.as_deref());
        surface.send_configure();
    }

    pub(super) fn normal_toplevel_bounds(
        &self,
        surface: &ToplevelSurface,
    ) -> Option<Size<i32, Logical>> {
        let index = self.window_index_for_surface(surface.wl_surface())?;
        let parent = self.transient_parent_index(index).unwrap_or(index);
        let anchor = self.presentation_anchor(index);
        let output = self.window_presentation_output(index).or_else(|| {
            if !self.windows[parent].initial_placement_done {
                return Some(self.initial_window_output(parent));
            }
            let position = self.windows[parent].position;
            let point = self
                .canvas_to_screen(Point::from((f64::from(position.x), f64::from(position.y))))
                .to_i32_round();
            let outputs = self.output_logical_rects();
            outputs
                .iter()
                .find(|output| output.contains(point))
                .copied()
                .or_else(|| outputs.first().copied())
        })?;
        let fullscreen = anchor.is_some_and(|index| self.windows[index].presentation.fullscreen);
        let scale = if anchor.is_some() {
            1.0
        } else {
            self.viewport_scale
        };
        Some(advisory_bounds(
            if fullscreen {
                output.size
            } else {
                self.usable_output(output).size
            },
            scale,
            self.has_compositor_chrome(index),
        ))
    }
}

fn advisory_bounds(
    output: Size<i32, Logical>,
    scale: f64,
    server_chrome: bool,
) -> Size<i32, Logical> {
    let chrome = if server_chrome { TITLE_BAR_HEIGHT } else { 0 };
    // Surface coordinates scale with the canvas, independently of output DPI.
    // Floor so rounding a client size back onto the output cannot exceed it.
    (
        (f64::from(output.w.max(1)) / scale).floor().max(1.0) as i32,
        ((f64::from(output.h.max(1)) / scale).floor() as i32 - chrome).max(1),
    )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_exclude_panel_and_only_server_drawn_chrome() {
        assert_eq!(
            advisory_bounds((1280, 752).into(), 1.0, false),
            (1280, 752).into()
        );
        assert_eq!(
            advisory_bounds((1280, 752).into(), 1.0, true),
            (1280, 708).into()
        );
    }

    #[test]
    fn canvas_scale_and_fullscreen_dialogs_have_distinct_bounds() {
        assert_eq!(
            advisory_bounds((1280, 752).into(), 0.8, true),
            (1600, 896).into()
        );
        assert_eq!(
            advisory_bounds((1280, 752).into(), 1.25, false),
            (1024, 601).into()
        );
        assert_eq!(
            advisory_bounds((1280, 800).into(), 1.0, false),
            (1280, 800).into()
        );
        assert_eq!(advisory_bounds((1, 1).into(), 3.0, true), (1, 1).into());
    }
}
