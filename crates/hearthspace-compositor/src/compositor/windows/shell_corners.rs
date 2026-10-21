//! Floating shell controls and the work area between awareness and action edges.
use hearthspace_common::{
    CANVAS_CORNER_WIDTH, CONTROL_BAR_HEIGHT, NOTIFICATION_CORNER_WIDTH, SHELL_CORNER_CLEARANCE,
    SHELL_CORNER_MARGIN, ShellCorner, shell_corner_from_app_id,
};
use smithay::utils::{Logical, Point, Rectangle, Size};

use super::window_app_id;
use crate::compositor::{App, ManagedWindowKind};

impl App {
    pub(in crate::compositor) fn focused_shell_corner_index(&self) -> Option<usize> {
        let focus = self.keyboard.current_focus()?;
        let index = self.window_index_for_surface(focus.surface())?;
        (self.shell_corner(index).is_some() || self.frame_widget_identity(index).is_some())
            .then_some(index)
    }

    pub(in crate::compositor) fn cycle_shell_corner(&mut self, reverse: bool) {
        if !self.navigation_available() {
            return;
        }
        if let Some(index) = self.focused_shell_corner_index()
            && let Some(target) = self.frame_target_for_window(index)
        {
            let _ = self.focus_next_frame_control(target, reverse);
            return;
        }
        if let Some(target) = self
            .windows
            .iter()
            .enumerate()
            .filter(|(_, w)| w.mapped)
            .find_map(|(index, _)| {
                self.frame_target_for_window(index).filter(|t| {
                    t.target.kind == hearthspace_common::frame_layout::TargetKind::Widget
                })
            })
        {
            let _ = self.focus_next_frame_control(target, reverse);
            return;
        }
        let output = self
            .output_logical_rects()
            .into_iter()
            .find(|output| output.contains(self.pointer_location.to_i32_round()))
            .unwrap_or_else(|| self.primary_output_logical_rect());
        if !self.shell_visible_on_output(output) {
            return;
        }
        let mut corners: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter_map(|(index, window)| {
                let corner = self.shell_corner(index)?;
                (window.mapped
                    && self.output_logical_rect_for_name(window.shell_output_name.as_deref())
                        == Some(output))
                .then_some((
                    match corner {
                        ShellCorner::Notifications => 0,
                        ShellCorner::System => 1,
                        ShellCorner::Canvas => 2,
                        ShellCorner::Apps => 3,
                    },
                    index,
                ))
            })
            .collect();
        corners.sort_unstable_by_key(|(order, _)| *order);
        if corners.is_empty() {
            return;
        }
        let focused = self.focused_shell_corner_index();
        let current = corners
            .iter()
            .position(|(_, index)| Some(*index) == focused);
        let next = match current {
            Some(index) if reverse => (index + corners.len() - 1) % corners.len(),
            Some(index) => (index + 1) % corners.len(),
            None if reverse => corners.len() - 1,
            None => 0,
        };
        let index = corners[next].1;
        if let Some(surface) = self.windows[index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(index, surface);
        }
    }

    pub(in crate::compositor) fn shell_corners_status(&self) -> Vec<serde_json::Value> {
        self.windows
            .iter()
            .enumerate()
            .filter_map(|(index, window)| {
                let corner = self.shell_corner(index)?;
                let output =
                    self.output_logical_rect_for_name(window.shell_output_name.as_deref())?;
                if !window.mapped || !self.shell_visible_on_output(output) {
                    return None;
                }
                let origin =
                    self.shell_bar_logical_origin(index)? + self.window_surface_geometry_loc(index);
                let size = self.window_geometry_size(index);
                let corner = match corner {
                    ShellCorner::Notifications => "notifications",
                    ShellCorner::System => "system",
                    ShellCorner::Canvas => "canvas",
                    ShellCorner::Apps => "apps",
                };
                Some(
                    serde_json::json!({"corner": corner, "output": window.shell_output_name,
                "x": origin.x, "y": origin.y, "width": size.w, "height": size.h}),
                )
            })
            .collect()
    }

    pub(in crate::compositor) fn shell_corner(&self, index: usize) -> Option<ShellCorner> {
        window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(shell_corner_from_app_id)
    }

    pub(in crate::compositor) fn has_shell_corners(&self, output: Rectangle<i32, Logical>) -> bool {
        self.windows.iter().enumerate().any(|(index, window)| {
            window.kind == ManagedWindowKind::ShellBar
                && self.shell_corner(index).is_some()
                && self.output_logical_rect_for_name(window.shell_output_name.as_deref())
                    == Some(output)
        })
    }

    pub(in crate::compositor) fn usable_output(
        &self,
        output: Rectangle<i32, Logical>,
    ) -> Rectangle<i32, Logical> {
        if self.has_shell_corners(output) {
            corner_work_area(output)
        } else {
            crate::compositor::navigation::usable_output(output)
        }
    }
}

pub(super) fn corner_size(
    corner: ShellCorner,
    requested: Option<Size<i32, Logical>>,
    output: Size<i32, Logical>,
) -> Size<i32, Logical> {
    let left_width = if corner.is_bottom() {
        CANVAS_CORNER_WIDTH
    } else {
        NOTIFICATION_CORNER_WIDTH
    };
    let max_width = if corner.is_right() {
        output
            .w
            .saturating_sub(left_width + 3 * SHELL_CORNER_MARGIN)
    } else {
        left_width.min(output.w.saturating_sub(2 * SHELL_CORNER_MARGIN))
    }
    .max(1);
    let width = requested
        .map(|size| size.w)
        .unwrap_or(320)
        .clamp(1, max_width);
    let height = CONTROL_BAR_HEIGHT.min(output.h.saturating_sub(2 * SHELL_CORNER_MARGIN).max(1));
    (width, height).into()
}

pub(super) fn corner_origin(
    corner: ShellCorner,
    output: Rectangle<i32, Logical>,
    geometry: Rectangle<i32, Logical>,
    panel: bool,
) -> Point<i32, Logical> {
    let inset = if panel {
        SHELL_CORNER_CLEARANCE
    } else {
        SHELL_CORNER_MARGIN
    };
    let max_x = (output.size.w - geometry.size.w).max(0);
    let max_y = (output.size.h - geometry.size.h).max(0);
    let x = if corner.is_right() {
        max_x.saturating_sub(SHELL_CORNER_MARGIN).max(0)
    } else {
        SHELL_CORNER_MARGIN.min(max_x)
    };
    let y = if corner.is_bottom() {
        max_y.saturating_sub(inset).max(0)
    } else {
        inset.min(max_y)
    };
    output.loc + Point::from((x, y)) - geometry.loc
}

fn corner_work_area(mut output: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let inset = SHELL_CORNER_CLEARANCE.min(output.size.h.saturating_sub(1).max(0) / 2);
    output.loc.y += inset;
    output.size.h = (output.size.h - 2 * inset).max(1);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_fit_small_scaled_outputs_without_overlapping() {
        let output = (320, 240).into();
        assert_eq!(
            corner_size(ShellCorner::Notifications, None, output),
            (110, 54).into()
        );
        assert_eq!(
            corner_size(ShellCorner::System, None, output),
            (138, 54).into()
        );
        assert_eq!(
            corner_size(ShellCorner::Canvas, None, output),
            (198, 54).into()
        );
        assert_eq!(
            corner_size(ShellCorner::Apps, None, output),
            (50, 54).into()
        );
        assert_eq!(
            corner_size(ShellCorner::Apps, Some((120, 48).into()), output),
            (50, 54).into()
        );
        assert_eq!(
            corner_size(ShellCorner::Apps, None, (10, 10).into()),
            (1, 1).into()
        );
    }

    #[test]
    fn corners_and_panels_anchor_geometry_on_offset_outputs() {
        let output = Rectangle::new((-1280, 120).into(), (1280, 800).into());
        let geometry = Rectangle::new((8, 6).into(), (320, 48).into());
        assert_eq!(
            corner_origin(ShellCorner::Notifications, output, geometry, false),
            (-1264, 138).into()
        );
        assert_eq!(
            corner_origin(ShellCorner::System, output, geometry, false),
            (-352, 138).into()
        );
        assert_eq!(
            corner_origin(ShellCorner::Canvas, output, geometry, false),
            (-1264, 842).into()
        );
        assert_eq!(
            corner_origin(ShellCorner::Apps, output, geometry, false),
            (-352, 842).into()
        );
        let panel = Rectangle::new((8, 6).into(), (360, 240).into());
        assert_eq!(
            corner_origin(ShellCorner::Canvas, output, panel, true),
            (-1264, 584).into()
        );
        assert_eq!(
            corner_origin(ShellCorner::Apps, output, panel, true),
            (-392, 584).into()
        );
        assert_eq!(
            corner_work_area(output),
            Rectangle::new((-1280, 210).into(), (1280, 620).into())
        );
        assert_eq!(
            corner_work_area(Rectangle::from_size((1, 1).into())).size,
            (1, 1).into()
        );
    }
}
