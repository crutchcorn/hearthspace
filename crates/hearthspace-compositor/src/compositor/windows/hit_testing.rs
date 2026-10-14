use smithay::{
    desktop::{PopupManager, WindowSurfaceType, utils::under_from_surface_tree},
    utils::{Logical, Point},
};

use crate::{
    config::{RESIZE_HANDLE_INSET, RESIZE_HANDLE_OUTSET},
    geometry::rect_contains,
};

use super::geometry::resize_edges_at;
use crate::compositor::{App, HitTarget, ManagedWindowKind};

impl App {
    pub(in crate::compositor) fn hit_test(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<HitTarget> {
        // Shell chrome (the bar and the launcher palette) is drawn in screen
        // space above the canvas, so it is hit-tested first using each surface's
        // own screen-space origin.
        for (window_index, window) in self.windows.iter().enumerate().rev() {
            let output = self
                .output_logical_rect_for_name(window.shell_output_name.as_deref())
                .unwrap_or_else(|| self.primary_output_logical_rect());
            if !window.mapped || !self.shell_visible_on_output(output) {
                continue;
            }
            match window.kind {
                ManagedWindowKind::ShellBar => {
                    if let Some(origin) = self.shell_bar_logical_origin(window_index)
                        && let scale = self.window_render_scale(window_index)
                        && let Some(target) =
                            self.hit_test_shell_surface(window_index, location, origin, scale)
                    {
                        return Some(target);
                    }
                }
                ManagedWindowKind::Launcher => {
                    if let Some(origin) = self.launcher_logical_origin(window_index)
                        && let scale = self.window_render_scale(window_index)
                        && let Some(target) =
                            self.hit_test_shell_surface(window_index, location, origin, scale)
                    {
                        return Some(target);
                    }
                }
                ManagedWindowKind::ShellWatcher => continue,
                ManagedWindowKind::Normal => continue,
            }
        }

        for (window_index, window) in self.windows.iter().enumerate().rev() {
            if window.kind != ManagedWindowKind::Normal || !window.mapped {
                continue;
            }
            if self
                .window_presentation_output(window_index)
                .is_some_and(|output| !output.contains(location.to_i32_round()))
            {
                continue;
            }
            let local = self.window_screen_to_surface(window_index, location);

            // Popups (menus) sit above this window's content and chrome, so
            // they are hit-tested first. Their location comes from the popup's
            // configured offset relative to the parent surface origin.
            let Some(root) = window.surface.wl_surface() else {
                continue;
            };
            let geometry_loc = self.window_surface_geometry_loc(window_index);
            for (popup, popup_offset) in PopupManager::popups_for_surface(&root) {
                let popup_origin = geometry_loc + popup_offset - popup.geometry().loc;
                let hit = under_from_surface_tree(
                    popup.wl_surface(),
                    local,
                    popup_origin,
                    WindowSurfaceType::ALL,
                );
                if let Some((surface, surface_location)) = hit {
                    let relative_surface_location = local - surface_location.to_f64();
                    let pointer_focus_origin = location - relative_surface_location;
                    return Some(HitTarget::Client {
                        window_index,
                        surface,
                        surface_location: pointer_focus_origin,
                    });
                }
            }

            if self.has_compositor_chrome(window_index) {
                let titlebar = self.title_bar_rect(window_index);
                let canvas_titlebar = self.title_bar_canvas_rect(window_index);
                let scale = self.window_render_scale(window_index);
                let chrome_location = Point::from((
                    f64::from(canvas_titlebar.loc.x)
                        + (location.x - f64::from(titlebar.loc.x)) / scale,
                    f64::from(canvas_titlebar.loc.y)
                        + (location.y - f64::from(titlebar.loc.y)) / scale,
                ));
                if rect_contains(self.close_button_canvas_rect(window_index), chrome_location) {
                    return Some(HitTarget::CloseButton { window_index });
                }

                if rect_contains(titlebar, location) {
                    return Some(HitTarget::TitleBar { window_index });
                }
            }

            // The interactive resize handle is a band centered on each window
            // edge, checked before the client surface tree so the visible edge
            // is grabbable for resizing. This applies to both server- and
            // client-side-decorated windows; for the latter `window_canvas_rect`
            // tracks the visible geometry so the band sits on the real edge.
            let window_rect = self.window_screen_bounds(window_index);
            if !self.window_uses_output_coordinates(window_index)
                && let Some(edges) = resize_edges_at(
                    window_rect,
                    location,
                    (f64::from(RESIZE_HANDLE_OUTSET) * self.window_render_scale(window_index))
                        .round() as i32,
                    (f64::from(RESIZE_HANDLE_INSET) * self.window_render_scale(window_index))
                        .round() as i32,
                )
            {
                return Some(HitTarget::ResizeBorder {
                    window_index,
                    edges,
                });
            }

            if let Some((surface, surface_location)) =
                under_from_surface_tree(&root, local, Point::from((0, 0)), WindowSurfaceType::ALL)
            {
                let relative_surface_location = local - surface_location.to_f64();
                let pointer_focus_origin = location - relative_surface_location;
                return Some(HitTarget::Client {
                    window_index,
                    surface,
                    surface_location: pointer_focus_origin,
                });
            }
            if window.presentation.fullscreen {
                return None;
            }
        }

        None
    }

    fn hit_test_shell_surface(
        &self,
        window_index: usize,
        location: Point<f64, Logical>,
        origin: Point<i32, Logical>,
        scale: f64,
    ) -> Option<HitTarget> {
        let window = &self.windows[window_index];
        let root = window.surface.wl_surface()?;
        let scale = scale.max(1.0);
        let origin_f64 = origin.to_f64();
        let unscaled_location = Point::from((
            origin_f64.x + (location.x - origin_f64.x) / scale,
            origin_f64.y + (location.y - origin_f64.y) / scale,
        ));
        let (surface, surface_location) =
            under_from_surface_tree(&root, unscaled_location, origin, WindowSurfaceType::ALL)?;
        let relative_surface_location = unscaled_location - surface_location.to_f64();
        let pointer_focus_origin = location - relative_surface_location;
        Some(HitTarget::Client {
            window_index,
            surface,
            surface_location: pointer_focus_origin,
        })
    }
}
