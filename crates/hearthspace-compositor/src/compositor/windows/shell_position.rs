//! Shell surfaces stay in output logical coordinates regardless of canvas zoom.
use hearthspace_common::{
    ShellCorner, canvas_map_output_name_from_app_id, notifications_output_name_from_app_id,
};
use smithay::utils::{Logical, Physical, Point, Rectangle};

use crate::{
    compositor::{App, ManagedWindowKind},
    config::CONTROL_BAR_HEIGHT,
};

impl App {
    fn content_screen_origin(&self, window_index: usize) -> Point<i32, Physical> {
        self.window_surface_screen_origin(window_index)
            .to_i32_round()
            .to_physical(1)
    }

    fn shell_bar_screen_origin(&self, window_index: usize) -> Option<Point<i32, Physical>> {
        self.shell_bar_logical_origin(window_index)
            .map(|origin| origin.to_physical(1))
    }

    pub(in crate::compositor) fn shell_bar_screen_origins(
        &self,
        window_index: usize,
    ) -> Vec<Point<i32, Physical>> {
        self.shell_bar_screen_origin(window_index)
            .into_iter()
            .collect()
    }

    pub(in crate::compositor) fn launcher_screen_origins(
        &self,
        window_index: usize,
    ) -> Vec<Point<i32, Physical>> {
        self.launcher_logical_origin(window_index)
            .map(|origin| origin.to_physical(1))
            .into_iter()
            .collect()
    }

    pub(in crate::compositor) fn shell_bar_logical_origin(
        &self,
        window_index: usize,
    ) -> Option<Point<i32, Logical>> {
        if self.is_layout_surface(window_index) {
            return self.layout_surface_origin(window_index);
        }
        let output_name = self.windows[window_index].shell_output_name.as_deref();
        let output = self.output_logical_rect_for_name(output_name)?;
        Some(if let Some(corner) = self.shell_corner(window_index) {
            super::shell_corners::corner_origin(
                corner,
                output,
                Rectangle::new(
                    self.window_surface_geometry_loc(window_index),
                    self.window_geometry_size(window_index),
                ),
                false,
            )
        } else {
            output.loc
        })
    }

    /// Logical screen-space origin of a launcher palette: pinned below its
    /// output's shell bar.
    pub(in crate::compositor) fn launcher_logical_origin(
        &self,
        window_index: usize,
    ) -> Option<Point<i32, Logical>> {
        if super::window_app_id(&self.windows[window_index].surface)
            .as_deref()
            .and_then(hearthspace_common::desktop_context::identity)
            .is_some()
        {
            return self.desktop_context_origin(window_index);
        }
        if let Some(origin) = self.frame_popup_origin(window_index) {
            return Some(origin);
        }
        if self.is_window_switcher_caption(window_index) {
            return self.window_switcher_caption_origin(window_index);
        }
        if self.is_canvas_search_panel(window_index)
            || self.canvas_search_result_identity(window_index).is_some()
        {
            return self.canvas_search_surface_origin(window_index);
        }
        let window = &self.windows[window_index];
        let output = self.output_logical_rect_for_name(window.shell_output_name.as_deref())?;
        let geometry = Rectangle::new(
            self.window_surface_geometry_loc(window_index),
            self.window_geometry_size(window_index),
        );
        if window.kind == ManagedWindowKind::ControlOsd {
            return Some(osd_surface_origin(output, geometry));
        }
        if self.has_shell_corners(output) {
            let app_id = super::window_app_id(&window.surface).unwrap_or_default();
            let corner = if self.is_search_panel(window_index) {
                ShellCorner::Apps
            } else if canvas_map_output_name_from_app_id(&app_id).is_some() {
                ShellCorner::Canvas
            } else if notifications_output_name_from_app_id(&app_id).is_some() {
                ShellCorner::Notifications
            } else {
                ShellCorner::System
            };
            return Some(super::shell_corners::corner_origin(
                corner, output, geometry, true,
            ));
        }
        Some(shell_panel_surface_origin(
            output,
            geometry,
            self.is_search_panel(window_index),
        ))
    }

    pub(in crate::compositor) fn surface_screen_origin(
        &self,
        window_index: usize,
    ) -> Point<i32, Physical> {
        match self.windows[window_index].kind {
            ManagedWindowKind::Normal | ManagedWindowKind::DesktopItem => {
                self.content_screen_origin(window_index)
            }
            ManagedWindowKind::ShellBar => self
                .shell_bar_screen_origin(window_index)
                .unwrap_or_else(|| Point::from((0, 0))),
            ManagedWindowKind::ShellWatcher => Point::from((0, 0)),
            ManagedWindowKind::Launcher | ManagedWindowKind::ControlOsd => self
                .launcher_logical_origin(window_index)
                .unwrap_or_else(|| Point::from((0, CONTROL_BAR_HEIGHT)))
                .to_physical(1),
        }
    }
}

/// Anchor the visible geometry; a client's optional shadow inset is not part
/// of the panel. Fixed-size constraints clamp the panel on small outputs.
fn shell_panel_surface_origin(
    output: Rectangle<i32, Logical>,
    geometry: Rectangle<i32, Logical>,
    centered: bool,
) -> Point<i32, Logical> {
    let right = output.loc.x.saturating_add(output.size.w);
    let bottom = output.loc.y.saturating_add(output.size.h);
    let max_x = right.saturating_sub(geometry.size.w).max(output.loc.x);
    let max_y = bottom.saturating_sub(geometry.size.h).max(output.loc.y);
    let x = if centered {
        output
            .loc
            .x
            .saturating_add((output.size.w - geometry.size.w).max(0) / 2)
    } else {
        max_x.saturating_sub(12).max(output.loc.x)
    };
    let y = output
        .loc
        .y
        .saturating_add(CONTROL_BAR_HEIGHT + 12)
        .min(max_y);
    Point::from((x, y)) - geometry.loc
}

/// Feedback stays centered near the bottom in output-logical coordinates.
fn osd_surface_origin(
    output: Rectangle<i32, Logical>,
    geometry: Rectangle<i32, Logical>,
) -> Point<i32, Logical> {
    let x = output
        .loc
        .x
        .saturating_add((output.size.w - geometry.size.w).max(0) / 2);
    let y = output
        .loc
        .y
        .saturating_add(output.size.h)
        .saturating_sub(geometry.size.h)
        .saturating_sub(48)
        .max(output.loc.y);
    Point::from((x, y)) - geometry.loc
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point, Rectangle};

    use super::{osd_surface_origin, shell_panel_surface_origin};

    #[test]
    fn osd_centers_on_scaled_offset_outputs_and_clamps_small_outputs() {
        let output = Rectangle::new((-1280, 120).into(), (1280, 800).into());
        let geometry = Rectangle::new((8, 6).into(), (300, 90).into());
        assert_eq!(osd_surface_origin(output, geometry), (-798, 776).into());
        let small = Rectangle::new((120, -100).into(), (200, 100).into());
        assert_eq!(osd_surface_origin(small, geometry), (112, -106).into());
    }

    #[test]
    fn map_origin_tracks_output_position_actual_size_and_geometry_inset() {
        let output = Rectangle::new((1280, 120).into(), (1024, 768).into());
        let geometry = Rectangle::new((8, 6).into(), (340, 336).into());
        assert_eq!(
            shell_panel_surface_origin(output, geometry, false),
            (1944, 180).into()
        );
        let resized = Rectangle::new((8, 6).into(), (420, 260).into());
        assert_eq!(
            shell_panel_surface_origin(output, resized, false),
            (1864, 180).into()
        );
        let negative = Rectangle::new((-1280, -200).into(), (1280, 800).into());
        assert_eq!(
            shell_panel_surface_origin(negative, geometry, false),
            (-360, -140).into()
        );
    }

    #[test]
    fn map_origin_clamps_small_outputs_and_uses_logical_dimensions() {
        let small = Rectangle::new((0, 0).into(), (320, 240).into());
        let geometry = Rectangle::new((0, 0).into(), (320, 192).into());
        assert_eq!(
            shell_panel_surface_origin(small, geometry, false),
            (0, 48).into()
        );
        // A 2560x1600 output at scale 2 exposes a 1280x800 logical rect.
        let scaled = Rectangle::new((0, 0).into(), (1280, 800).into());
        let geometry = Rectangle::new((0, 0).into(), (340, 336).into());
        assert_eq!(
            shell_panel_surface_origin(scaled, geometry, false),
            (928, 66).into()
        );
    }

    #[test]
    fn search_centers_below_bar_and_clamps_small_outputs() {
        let output = Rectangle::<i32, Logical>::new((1280, 120).into(), (1024, 768).into());
        let geometry = Rectangle::new((8, 6).into(), (600, 400).into());
        assert_eq!(
            shell_panel_surface_origin(output, geometry, true),
            Point::from((1484, 180))
        );
        let small = Rectangle::new((-320, 0).into(), (320, 240).into());
        let geometry = Rectangle::new((0, 0).into(), (320, 192).into());
        assert_eq!(
            shell_panel_surface_origin(small, geometry, true),
            Point::from((-320, 48))
        );
    }
}
