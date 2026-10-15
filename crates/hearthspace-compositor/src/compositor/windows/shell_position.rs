//! Shell surfaces stay in output logical coordinates regardless of canvas zoom.
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
        let output_name = self.windows[window_index].shell_output_name.as_deref();
        self.output_logical_rect_for_name(output_name)
            .map(|rect| rect.loc)
    }

    /// Logical screen-space origin of a launcher palette: pinned below its
    /// output's shell bar.
    pub(in crate::compositor) fn launcher_logical_origin(
        &self,
        window_index: usize,
    ) -> Option<Point<i32, Logical>> {
        let window = &self.windows[window_index];
        let output = self.output_logical_rect_for_name(window.shell_output_name.as_deref())?;
        let geometry = Rectangle::new(
            self.window_surface_geometry_loc(window_index),
            self.window_geometry_size(window_index),
        );
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
            ManagedWindowKind::Normal => self.content_screen_origin(window_index),
            ManagedWindowKind::ShellBar => self
                .shell_bar_screen_origin(window_index)
                .unwrap_or_else(|| Point::from((0, 0))),
            ManagedWindowKind::ShellWatcher => Point::from((0, 0)),
            ManagedWindowKind::Launcher => self
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

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point, Rectangle};

    use super::shell_panel_surface_origin;

    #[test]
    fn map_origin_tracks_output_position_actual_size_and_geometry_inset() {
        let output = Rectangle::new((1280, 120).into(), (1024, 768).into());
        let geometry = Rectangle::new((8, 6).into(), (340, 336).into());
        assert_eq!(
            shell_panel_surface_origin(output, geometry, false),
            (1944, 174).into()
        );
        let resized = Rectangle::new((8, 6).into(), (420, 260).into());
        assert_eq!(
            shell_panel_surface_origin(output, resized, false),
            (1864, 174).into()
        );
        let negative = Rectangle::new((-1280, -200).into(), (1280, 800).into());
        assert_eq!(
            shell_panel_surface_origin(negative, geometry, false),
            (-360, -146).into()
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
            (928, 60).into()
        );
    }

    #[test]
    fn search_centers_below_bar_and_clamps_small_outputs() {
        let output = Rectangle::<i32, Logical>::new((1280, 120).into(), (1024, 768).into());
        let geometry = Rectangle::new((8, 6).into(), (600, 400).into());
        assert_eq!(
            shell_panel_surface_origin(output, geometry, true),
            Point::from((1484, 174))
        );
        let small = Rectangle::new((-320, 0).into(), (320, 240).into());
        let geometry = Rectangle::new((0, 0).into(), (320, 192).into());
        assert_eq!(
            shell_panel_surface_origin(small, geometry, true),
            Point::from((-320, 48))
        );
    }
}
