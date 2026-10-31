use std::process::Stdio;

use smithay::{
    wayland::compositor::CompositorHandler,
    xwayland::{X11Surface, XWayland, XWaylandEvent},
};
use tracing::{error, info, trace, warn};
use wayland_server::DisplayHandle;

use super::{
    App, ManagedSurface, ManagedWindow, ManagedWindowKind, WindowDecoration,
    windows::{RetirementCause, position_for_new_window},
};

mod clipboard;
mod coordinates;
mod dispatch;
mod handlers;
pub(super) mod identity;
mod interactive;
mod popups;

impl App {
    pub(super) fn start_xwayland(&mut self, dh: &DisplayHandle) {
        let Ok((xwayland, client)) = XWayland::spawn(
            dh,
            None,
            std::iter::empty::<(String, String)>(),
            std::iter::empty::<String>(),
            true,
            Stdio::null(),
            Stdio::null(),
            |_| (),
        ) else {
            warn!("XWayland is unavailable; X11 applications will not be supported");
            return;
        };

        let display_handle = dh.clone();
        let ret = self
            .loop_handle
            .insert_source(xwayland, move |event, _, data| match event {
                XWaylandEvent::Ready {
                    x11_socket,
                    display_number,
                } => {
                    data.state
                        .client_compositor_state(&client)
                        .set_client_scale(1.0);
                    match data.state.start_xwm(
                        &display_handle,
                        x11_socket,
                        client.clone(),
                        display_number,
                    ) {
                        Ok(()) => {
                            info!(
                                display = format_args!(":{display_number}"),
                                "XWayland is ready"
                            );
                        }
                        Err(error) => {
                            error!(%error, "failed to start XWayland window manager");
                        }
                    }
                }
                XWaylandEvent::Error => {
                    warn!("XWayland exited during startup");
                }
            });
        if let Err(error) = ret {
            error!(%error, "failed to register XWayland event source");
        }
    }

    fn map_x11_window(&mut self, window: X11Surface) {
        if self
            .windows
            .iter()
            .any(|managed| managed.surface.same_x11(&window))
        {
            return;
        }

        let Some(wl_surface) = window.wl_surface() else {
            trace!(
                window = window.window_id(),
                "X11 window has no wl_surface yet"
            );
            return;
        };

        let override_redirect = window.is_override_redirect();
        if !override_redirect {
            identity::remember_process(&window);
        }
        let geometry = window.geometry();
        let position = if override_redirect {
            crate::geometry::CanvasPoint {
                x: window.last_configure().loc.x,
                y: window.last_configure().loc.y,
            }
        } else {
            position_for_new_window(ManagedWindowKind::Normal, self.viewport_offset)
        };
        let decoration = if window.is_decorated() || override_redirect {
            WindowDecoration::ClientSide
        } else {
            WindowDecoration::ServerSide
        };

        let spawn_output_name = self.next_spawn_output_name.take();
        let id = self.windows.insert_new(|id| ManagedWindow {
            id,
            surface: ManagedSurface::X11(Box::new(window.clone())),
            identity: Default::default(),
            position,
            kind: ManagedWindowKind::Normal,
            shell_output_name: None,
            decoration,
            titlebar: None,
            content_bbox_size: geometry.size,
            mapped: false,
            initial_placement_done: false,
            initial_output_name: spawn_output_name.clone(),
            presentation: Default::default(),
            pending_resize: None,
            advertised_fractional_scale: 0.0,
        });
        info!(
            window_id = id,
            x11_window = window.window_id(),
            title = %window.title(),
            class = %window.class(),
            override_redirect,
            "new X11 window"
        );
        self.idle_daemon.register_window(id);
        self.enter_output(&wl_surface, spawn_output_name.as_deref());
        self.refresh_window_content_bbox(&wl_surface);
        if !override_redirect {
            if window.is_maximized() {
                let _ = self.set_window_maximized(id, true);
            }
            if window.is_fullscreen() {
                let _ = self.set_window_fullscreen(id, true, None);
            }
        }
        self.refresh_window_mapping(&wl_surface);
        self.request_redraw();
    }

    fn retire_x11_window(&mut self, window: &X11Surface, cause: RetirementCause) {
        if let Some(id) = self
            .windows
            .iter()
            .find(|managed| managed.surface.same_x11(window))
            .map(|managed| managed.id)
        {
            info!(
                window_id = id,
                x11_window = window.window_id(),
                ?cause,
                "X11 window retired"
            );
            self.retire_window(id, cause);
        }
    }

    fn window_index_for_x11(&self, window: &X11Surface) -> Option<usize> {
        self.windows
            .iter()
            .position(|managed| managed.surface.same_x11(window))
    }

    fn focus_x11_window(&mut self, window: &X11Surface) {
        if !self.navigation_available() {
            return;
        }
        let Some(window_index) = self.window_index_for_x11(window) else {
            return;
        };
        if !self.is_navigable_window(window_index) {
            return;
        }
        let id = self.windows[window_index].id;
        let Some(window_index) = self.raise_window_id(id) else {
            return;
        };
        if let Some(surface) = self.windows[window_index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(window_index, surface);
        }
        if let Err(error) = window.set_activated(true) {
            warn!(%error, "failed to activate X11 window");
        }
    }
}
