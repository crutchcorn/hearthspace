use smithay::utils::{Logical, Point, Rectangle};

use crate::{compositor::App, config::TITLE_BAR_HEIGHT};

impl App {
    pub(in crate::compositor) fn window_surface_screen_origin(
        &self,
        index: usize,
    ) -> Point<f64, Logical> {
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
        if self.window_uses_output_coordinates(index) {
            let chrome = if self.has_compositor_chrome(index) {
                TITLE_BAR_HEIGHT
            } else {
                0
            };
            let size = self.window_geometry_size(index);
            let origin = self.window_surface_screen_origin(index).to_i32_round()
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
