use smithay::{
    desktop::{PopupManager, WindowSurfaceType, utils::under_from_surface_tree},
    utils::{Logical, Point},
};

use crate::{
    config::{CONTROL_BAR_HEIGHT, RESIZE_HANDLE_INSET, RESIZE_HANDLE_OUTSET},
    geometry::rect_contains,
};

use super::geometry::resize_edges_at;
use crate::compositor::{App, HitTarget, ManagedWindowKind, rendering::toplevel_geometry_loc};

impl App {
    pub(in crate::compositor) fn hit_test(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<HitTarget> {
        // Shell chrome (the bar and the launcher palette) is drawn in screen
        // space above the canvas, so it is hit-tested first using each surface's
        // own screen-space origin.
        for (window_index, window) in self.windows.iter().enumerate().rev() {
            match window.kind {
                ManagedWindowKind::ShellBar => {
                    if let Some(origin) = self.shell_bar_logical_origin(window_index)
                        && let Some(target) =
                            self.hit_test_shell_surface(window_index, location, origin)
                    {
                        return Some(target);
                    }
                }
                ManagedWindowKind::Launcher => {
                    if let Some(origin) = self.launcher_logical_origin(window_index)
                        && let Some(target) =
                            self.hit_test_shell_surface(window_index, location, origin)
                    {
                        return Some(target);
                    }
                }
                ManagedWindowKind::ShellWatcher => continue,
                ManagedWindowKind::Normal => continue,
            }
        }

        if location.y < f64::from(CONTROL_BAR_HEIGHT) {
            return None;
        }

        let canvas_location = self.screen_to_canvas(location);

        for (window_index, window) in self.windows.iter().enumerate().rev() {
            if window.kind != ManagedWindowKind::Normal {
                continue;
            }

            let content_origin = self.content_canvas_origin(window_index);

            // Popups (menus) sit above this window's content and chrome, so
            // they are hit-tested first. Their location comes from the popup's
            // configured offset relative to the parent surface origin.
            let geometry_loc = toplevel_geometry_loc(window.surface.wl_surface());
            for (popup, popup_offset) in
                PopupManager::popups_for_surface(window.surface.wl_surface())
            {
                let popup_origin =
                    content_origin + geometry_loc + popup_offset - popup.geometry().loc;
                let hit = under_from_surface_tree(
                    popup.wl_surface(),
                    canvas_location,
                    popup_origin,
                    WindowSurfaceType::ALL,
                );
                if let Some((surface, surface_location)) = hit {
                    let relative_surface_location = canvas_location - surface_location.to_f64();
                    let pointer_focus_origin = location - relative_surface_location;
                    return Some(HitTarget::Client {
                        window_index,
                        surface,
                        surface_location: pointer_focus_origin,
                    });
                }
            }

            if self.has_compositor_chrome(window_index) {
                if rect_contains(self.close_button_canvas_rect(window_index), canvas_location) {
                    return Some(HitTarget::CloseButton { window_index });
                }

                if rect_contains(self.title_bar_canvas_rect(window_index), canvas_location) {
                    return Some(HitTarget::TitleBar { window_index });
                }
            }

            // The interactive resize handle is a band centered on each window
            // edge, checked before the client surface tree so the visible edge
            // is grabbable for resizing. This applies to both server- and
            // client-side-decorated windows; for the latter `window_canvas_rect`
            // tracks the visible geometry so the band sits on the real edge.
            let window_rect = self.window_canvas_rect(window_index);
            if let Some(edges) = resize_edges_at(
                window_rect,
                canvas_location,
                RESIZE_HANDLE_OUTSET,
                RESIZE_HANDLE_INSET,
            ) {
                return Some(HitTarget::ResizeBorder {
                    window_index,
                    edges,
                });
            }

            if let Some((surface, surface_location)) = under_from_surface_tree(
                window.surface.wl_surface(),
                canvas_location,
                content_origin,
                WindowSurfaceType::ALL,
            ) {
                let relative_surface_location = canvas_location - surface_location.to_f64();
                let pointer_focus_origin = location - relative_surface_location;
                return Some(HitTarget::Client {
                    window_index,
                    surface,
                    surface_location: pointer_focus_origin,
                });
            }
        }

        None
    }

    fn hit_test_shell_surface(
        &self,
        window_index: usize,
        location: Point<f64, Logical>,
        origin: Point<i32, Logical>,
    ) -> Option<HitTarget> {
        let window = &self.windows[window_index];
        let (surface, surface_location) = under_from_surface_tree(
            window.surface.wl_surface(),
            location,
            origin,
            WindowSurfaceType::ALL,
        )?;
        let relative_surface_location = location - surface_location.to_f64();
        let pointer_focus_origin = location - relative_surface_location;
        Some(HitTarget::Client {
            window_index,
            surface,
            surface_location: pointer_focus_origin,
        })
    }
}
