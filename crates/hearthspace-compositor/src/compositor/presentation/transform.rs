use smithay::utils::{Logical, Point, Rectangle};

use crate::{compositor::App, config::TITLE_BAR_HEIGHT};

impl App {
    pub(in crate::compositor) fn window_surface_screen_origin(
        &self,
        index: usize,
    ) -> Point<f64, Logical> {
        let origin = self.base_window_surface_screen_origin(index);
        if let Some((pose, _)) = self.canvas_preview_pose(index) {
            let base = self.base_window_screen_bounds(index);
            let scale = pose.width / f64::from(base.size.w.max(1));
            return Point::from((
                pose.x + (origin.x - f64::from(base.loc.x)) * scale,
                pose.y + (origin.y - f64::from(base.loc.y)) * scale,
            ));
        }
        origin
    }
    pub(in crate::compositor) fn base_window_surface_screen_origin(
        &self,
        index: usize,
    ) -> Point<f64, Logical> {
        if !self.windows[index].presentation.pinned()
            && let Some(rect) = self
                .layout_motion_rect(index)
                .or_else(|| self.layout_target_rect(index))
        {
            let inset = self.window_surface_geometry_loc(index);
            let chrome = if self.has_compositor_chrome(index) {
                TITLE_BAR_HEIGHT
            } else {
                0
            };
            let scale = self.layout_render_scale(index).unwrap_or(1.);
            return Point::from((
                rect.x - f64::from(inset.x) * scale,
                rect.y + f64::from(chrome - inset.y) * scale,
            ));
        }
        if let Some(origin) = self.transient_screen_origin(index) {
            return origin;
        }
        if let Some(mut output) = self.presentation_output_rect(index) {
            let fullscreen = self.windows[index].presentation.fullscreen;
            let chrome = if self.has_compositor_chrome(index) {
                TITLE_BAR_HEIGHT
            } else {
                0
            };
            let size = self.window_geometry_size(index);
            if !fullscreen {
                output = self.usable_output(output);
            }
            let inset = self.window_surface_geometry_loc(index);
            return Point::from((
                f64::from(output.loc.x - inset.x)
                    + if fullscreen {
                        f64::from((output.size.w - size.w).max(0)) / 2.0
                    } else {
                        0.0
                    },
                f64::from(output.loc.y + chrome - inset.y)
                    + if fullscreen {
                        f64::from((output.size.h - size.h).max(0)) / 2.0
                    } else {
                        0.0
                    },
            ));
        }
        self.canvas_to_screen(self.content_canvas_origin(index).to_f64())
    }

    pub(in crate::compositor) fn window_screen_bounds(
        &self,
        index: usize,
    ) -> Rectangle<i32, Logical> {
        if let Some((pose, _)) = self.canvas_preview_pose(index) {
            return Rectangle::new(
                (pose.x.round() as i32, pose.y.round() as i32).into(),
                (pose.width.round() as i32, pose.height.round() as i32).into(),
            );
        }
        self.base_window_screen_bounds(index)
    }
    pub(in crate::compositor) fn base_window_screen_bounds(
        &self,
        index: usize,
    ) -> Rectangle<i32, Logical> {
        if !self.windows[index].presentation.pinned()
            && let Some(rect) = self
                .layout_motion_rect(index)
                .or_else(|| self.layout_target_rect(index))
        {
            return Rectangle::new(
                (rect.x.round() as i32, rect.y.round() as i32).into(),
                (rect.width.round() as i32, rect.height.round() as i32).into(),
            );
        }
        if self.window_uses_output_coordinates(index) {
            let chrome = if self.has_compositor_chrome(index) {
                TITLE_BAR_HEIGHT
            } else {
                0
            };
            let size = self.window_geometry_size(index);
            let origin = self.base_window_surface_screen_origin(index).to_i32_round()
                + self.window_surface_geometry_loc(index)
                - Point::from((0, chrome));
            return Rectangle::new(origin, (size.w, size.h + chrome).into());
        }
        let canvas = self.window_canvas_rect(index);
        Rectangle::new(
            self.canvas_to_screen(canvas.loc.to_f64()).to_i32_round(),
            (
                (f64::from(canvas.size.w) * self.viewport_scale).round() as i32,
                (f64::from(canvas.size.h) * self.viewport_scale).round() as i32,
            )
                .into(),
        )
    }

    pub(in crate::compositor) fn window_screen_to_surface(
        &self,
        index: usize,
        screen: Point<f64, Logical>,
    ) -> Point<f64, Logical> {
        let origin = self.window_surface_screen_origin(index);
        let scale = self.window_render_scale(index);
        Point::from(((screen.x - origin.x) / scale, (screen.y - origin.y) / scale))
    }

    pub(in crate::compositor) fn fullscreen_on_output(
        &self,
        output: Rectangle<i32, Logical>,
    ) -> bool {
        self.windows.iter().enumerate().any(|(index, window)| {
            window.mapped
                && window.presentation.fullscreen
                && self.presentation_output_rect(index) == Some(output)
        })
    }

    pub(in crate::compositor) fn shell_visible_on_output(
        &self,
        output: Rectangle<i32, Logical>,
    ) -> bool {
        if self.canvas_preview_active() {
            return false;
        }
        // Launcher activation always provides an escape from fullscreen.
        if self
            .keyboard
            .current_focus()
            .and_then(|focus| self.window_index_for_surface(focus.surface()))
            .is_some_and(|index| self.windows[index].kind.is_shell_chrome())
        {
            return true;
        }
        !self
            .windows
            .iter()
            .enumerate()
            .rev()
            .find(|(index, window)| {
                window.mapped
                    && self.is_navigable_window(*index)
                    && self
                        .normal_window_screen_rect(*index)
                        .is_some_and(|rect| rect.overlaps(output))
            })
            .is_some_and(|(index, _)| {
                self.presentation_anchor(index).is_some_and(|anchor| {
                    self.windows[anchor].presentation.fullscreen
                        && self.presentation_output_rect(anchor) == Some(output)
                })
            })
    }

    pub(in crate::compositor) fn window_visible_on_output(
        &self,
        index: usize,
        output: Rectangle<i32, Logical>,
    ) -> bool {
        if !self.windows[index].mapped {
            return false;
        }
        if self.is_layout_surface(index) {
            return self.layout_surface_visible(index);
        }
        if self.is_window_switcher_caption(index) {
            return self.window_switcher_caption_visible(index);
        }
        if self.is_canvas_search_panel(index) {
            return self.canvas_search_panel_allowed(index);
        }
        if self.canvas_search_result_identity(index).is_some() {
            return self.canvas_search_result_visible(index);
        }
        if self.canvas_preview_active() && !self.windows[index].kind.is_shell_chrome() {
            return true;
        }
        if self.windows[index].kind == super::super::ManagedWindowKind::ControlOsd {
            return !self.session_locked();
        }
        if self.windows[index].kind.is_shell_chrome() {
            return self.shell_visible_on_output(output);
        }
        self.window_presentation_output(index)
            .is_none_or(|target| target == output)
    }
}
