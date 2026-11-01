use smithay::{
    desktop::{PopupManager, WindowSurfaceType, utils::under_from_surface_tree},
    utils::{Logical, Point},
};

use crate::{
    config::{RESIZE_HANDLE_INSET, RESIZE_HANDLE_OUTSET},
    geometry::rect_contains,
};

use super::{TitlebarButton, geometry::resize_edges_at};
use crate::compositor::{App, HitTarget, ManagedWindowKind};

impl App {
    pub(in crate::compositor) fn hit_test(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<HitTarget> {
        if self.session_locked() || self.window_switcher_active() {
            return None;
        }
        let mut order: Vec<_> = (0..self.windows.len()).collect();
        order.sort_by_cached_key(|index| {
            (
                self.is_power_confirmation(*index),
                self.layout_render_layer(*index).unwrap_or_else(|| {
                    match self.windows[*index].kind {
                        ManagedWindowKind::DesktopItem => 1,
                        ManagedWindowKind::Normal => 10,
                        ManagedWindowKind::ControlOsd => 130,
                        _ => 110,
                    }
                }),
                *index,
            )
        });
        for window_index in order.into_iter().rev() {
            let window = &self.windows[window_index];
            if !window.mapped {
                continue;
            }
            if self.compact_frame_backing(window_index)
                || self
                    .layout_group_clip(window_index)
                    .is_some_and(|clip| !clip.contains(location.to_i32_round()))
            {
                continue;
            }
            if self.modal_confirmation_active() && !self.modal_confirmation_surface(window_index) {
                continue;
            }
            let output = self
                .output_logical_rect_for_name(window.shell_output_name.as_deref())
                .unwrap_or_else(|| self.primary_output_logical_rect());
            if !self.window_visible_on_output(window_index, output) {
                continue;
            }
            if self.canvas_search_active() && !self.canvas_search_panel_allowed(window_index) {
                continue;
            }
            if self.canvas_search_result_identity(window_index).is_some()
                || self.is_window_switcher_caption(window_index)
                || self.is_layout_tooltip(window_index)
            {
                continue;
            }
            let projection = self.window_projection(window_index);
            match window.kind {
                ManagedWindowKind::ShellBar | ManagedWindowKind::Launcher => {
                    let origin = if window.kind == ManagedWindowKind::ShellBar {
                        self.shell_bar_logical_origin(window_index)
                    } else {
                        self.launcher_logical_origin(window_index)
                    };
                    if let Some(origin) = origin
                        && let Some(target) = self.hit_test_shell_surface(
                            window_index,
                            location,
                            origin,
                            projection.scale,
                        )
                    {
                        return Some(target);
                    }
                    continue;
                }
                ManagedWindowKind::DesktopItem => {
                    if !projection.bounds.contains(location.to_i32_round()) {
                        continue;
                    }
                    let Some(root) = window.surface.wl_surface() else {
                        continue;
                    };
                    let local = projection.screen_to_surface(location);
                    if let Some((surface, origin)) =
                        under_from_surface_tree(&root, local, (0, 0), WindowSurfaceType::ALL)
                    {
                        return Some(HitTarget::Client {
                            window_index,
                            surface,
                            surface_location: location - (local - origin.to_f64()),
                        });
                    }
                    continue;
                }
                ManagedWindowKind::ShellWatcher | ManagedWindowKind::ControlOsd => continue,
                ManagedWindowKind::Normal => {}
            }
            if self.frame_window_attached(window_index)
                && self.layout_editor.expanded_window != Some(window.id)
            {
                continue;
            }
            if self
                .window_presentation_output(window_index)
                .is_some_and(|output| !output.contains(location.to_i32_round()))
            {
                continue;
            }
            let local = projection.screen_to_surface(location);

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
                for button in [TitlebarButton::Close, TitlebarButton::Maximize] {
                    if self
                        .titlebar_button_screen_rect(window_index, button)
                        .contains(location)
                    {
                        return Some(HitTarget::TitlebarButton {
                            window_index,
                            button,
                        });
                    }
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
            let window_rect = projection.bounds;
            if !self.window_uses_output_coordinates(window_index)
                && !self.windows[window_index].presentation.maximized
                && let Some(edges) = resize_edges_at(
                    window_rect,
                    location,
                    (f64::from(RESIZE_HANDLE_OUTSET) * projection.scale).round() as i32,
                    (f64::from(RESIZE_HANDLE_INSET) * projection.scale).round() as i32,
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
        let scale = scale.max(0.01);
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
