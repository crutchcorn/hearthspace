//! Floating shell controls and the work area between awareness and action edges.
use hearthspace_common::{
    CONTROL_BAR_HEIGHT, SHELL_CORNER_CLEARANCE, SHELL_CORNER_MARGIN, ShellCorner,
    shell_corner_from_app_id,
};
use smithay::utils::{Logical, Point, Rectangle, Size};

use super::window_app_id;
use crate::compositor::{App, ManagedWindowKind};

impl App {
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
    let left_width = if corner.is_bottom() { 88 } else { 48 };
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
            (48, 48).into()
        );
        assert_eq!(
            corner_size(ShellCorner::System, None, output),
            (236, 48).into()
        );
        assert_eq!(
            corner_size(ShellCorner::Canvas, None, output),
            (88, 48).into()
        );
        assert_eq!(
            corner_size(ShellCorner::Apps, None, output),
            (196, 48).into()
        );
        assert_eq!(
            corner_size(ShellCorner::Apps, Some((120, 48).into()), output),
            (120, 48).into()
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
            (-1276, 126).into()
        );
        assert_eq!(
            corner_origin(ShellCorner::System, output, geometry, false),
            (-340, 126).into()
        );
        assert_eq!(
            corner_origin(ShellCorner::Canvas, output, geometry, false),
            (-1276, 854).into()
        );
        assert_eq!(
            corner_origin(ShellCorner::Apps, output, geometry, false),
            (-340, 854).into()
        );
        let panel = Rectangle::new((8, 6).into(), (360, 240).into());
        assert_eq!(
            corner_origin(ShellCorner::Canvas, output, panel, true),
            (-1276, 602).into()
        );
        assert_eq!(
            corner_origin(ShellCorner::Apps, output, panel, true),
            (-380, 602).into()
        );
        assert_eq!(
            corner_work_area(output),
            Rectangle::new((-1280, 192).into(), (1280, 656).into())
        );
        assert_eq!(
            corner_work_area(Rectangle::from_size((1, 1).into())).size,
            (1, 1).into()
        );
    }
}
