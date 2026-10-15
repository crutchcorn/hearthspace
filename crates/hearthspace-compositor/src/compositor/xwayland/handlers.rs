use std::os::fd::OwnedFd;

use smithay::{
    input::dnd::DndGrabHandler,
    utils::{Logical, Rectangle},
    wayland::{
        selection::SelectionTarget,
        xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    },
    xwayland::{
        X11Surface, X11Wm, XwmHandler,
        xwm::{Reorder, ResizeEdge as X11ResizeEdge, WmWindowProperty, XwmId},
    },
};
use tracing::{debug, warn};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::compositor::{App, WindowDecoration, windows::ResizeEdges};

impl XWaylandShellHandler for App {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self._xwayland_shell_state
    }

    fn surface_associated(&mut self, _xwm: XwmId, wl_surface: WlSurface, surface: X11Surface) {
        debug!(
            x11_window = surface.window_id(),
            wl_surface = ?wl_surface.id().protocol_id(),
            "X11 window associated with wl_surface"
        );
        if surface.is_mapped() {
            self.map_x11_window(surface, false);
        }
    }
}

impl DndGrabHandler for App {}

impl XwmHandler for App {
    fn maximize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(index) = self.window_index_for_x11(&window) {
            let _ = self.set_window_maximized(self.windows[index].id, true);
        }
    }

    fn unmaximize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(index) = self.window_index_for_x11(&window) {
            let _ = self.set_window_maximized(self.windows[index].id, false);
        }
    }

    fn fullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(index) = self.window_index_for_x11(&window) {
            let _ = self.set_window_fullscreen(self.windows[index].id, true, None);
        }
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(index) = self.window_index_for_x11(&window) {
            let _ = self.set_window_fullscreen(self.windows[index].id, false, None);
        }
    }

    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwm.as_mut().expect("XWM callback without X11Wm")
    }

    fn new_window(&mut self, _xwm: XwmId, window: X11Surface) {
        debug!(x11_window = window.window_id(), "new unmapped X11 window");
    }

    fn new_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        debug!(
            x11_window = window.window_id(),
            "new unmapped override-redirect X11 window"
        );
    }

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Err(error) = window.set_mapped(true) {
            warn!(%error, x11_window = window.window_id(), "failed to map X11 window");
            return;
        }
        self.map_x11_window(window, false);
    }

    fn map_window_notify(&mut self, _xwm: XwmId, window: X11Surface) {
        self.map_x11_window(window, false);
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.map_x11_window(window, true);
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.unmap_x11_window(&window);
        if !window.is_override_redirect()
            && let Err(error) = window.set_mapped(false)
        {
            warn!(%error, x11_window = window.window_id(), "failed to unmap X11 window");
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.unmap_x11_window(&window);
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        let mut geometry = window.last_configure();
        if window.is_override_redirect() {
            if let Some(x) = x {
                geometry.loc.x = x;
            }
            if let Some(y) = y {
                geometry.loc.y = y;
            }
        }
        if let Some(w) = w {
            geometry.size.w = w as i32;
        }
        if let Some(h) = h {
            geometry.size.h = h as i32;
        }
        if let Err(error) = window.configure(Some(geometry)) {
            warn!(%error, x11_window = window.window_id(), "failed to configure X11 window");
        }
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
        if let Some(index) = self.window_index_for_x11(&window) {
            self.windows[index].content_bbox_size = geometry.size;
            if window.is_override_redirect() {
                self.windows[index].position = crate::geometry::CanvasPoint {
                    x: geometry.loc.x,
                    y: geometry.loc.y,
                };
            }
            self.request_redraw();
        }
    }

    fn property_notify(&mut self, _xwm: XwmId, window: X11Surface, property: WmWindowProperty) {
        if matches!(
            property,
            WmWindowProperty::Title | WmWindowProperty::Class | WmWindowProperty::MotifHints
        ) && let Some(index) = self.window_index_for_x11(&window)
        {
            self.windows[index].titlebar = None;
            self.windows[index].decoration = if window.is_decorated() {
                WindowDecoration::ClientSide
            } else {
                WindowDecoration::ServerSide
            };
            self.request_redraw();
        }
    }

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _button: u32,
        resize_edge: X11ResizeEdge,
    ) {
        if self.session_locked() {
            return;
        }
        let Some(index) = self.window_index_for_x11(&window) else {
            return;
        };
        if self.window_uses_output_coordinates(index)
            || self.drag.is_some()
            || self.resize.is_some()
            || !self.pointer_grab_targets_window(index)
        {
            return;
        }
        let index = self.raise_window(index);
        self.start_resize(index, ResizeEdges::from(resize_edge));
    }

    fn move_request(&mut self, _xwm: XwmId, window: X11Surface, _button: u32) {
        if self.session_locked() {
            return;
        }
        let Some(index) = self.window_index_for_x11(&window) else {
            return;
        };
        if self.window_uses_output_coordinates(index)
            || self.drag.is_some()
            || self.resize.is_some()
            || !self.pointer_grab_targets_window(index)
        {
            return;
        }
        let index = self.raise_window(index);
        self.start_client_window_move(index);
    }

    fn active_window_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _timestamp: u32,
        _currently_active_window: Option<X11Surface>,
    ) {
        self.focus_x11_window(&window);
    }

    fn allow_selection_access(&mut self, xwm: XwmId, _selection: SelectionTarget) -> bool {
        self.xwm_has_keyboard_focus(xwm)
    }

    fn send_selection(
        &mut self,
        xwm: XwmId,
        selection: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
    ) {
        self.send_wayland_clipboard_to_x11(xwm, selection, mime_type, fd);
    }

    fn new_selection(&mut self, xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        self.set_x11_clipboard(xwm, selection, mime_types);
    }

    fn cleared_selection(&mut self, xwm: XwmId, selection: SelectionTarget) {
        self.clear_x11_selection(xwm, selection);
    }

    fn disconnected(&mut self, xwm: XwmId) {
        self.xwayland_disconnected(xwm);
    }
}

impl From<X11ResizeEdge> for ResizeEdges {
    fn from(edge: X11ResizeEdge) -> Self {
        match edge {
            X11ResizeEdge::Top => Self {
                top: true,
                ..Self::default()
            },
            X11ResizeEdge::Bottom => Self {
                bottom: true,
                ..Self::default()
            },
            X11ResizeEdge::Left => Self {
                left: true,
                ..Self::default()
            },
            X11ResizeEdge::Right => Self {
                right: true,
                ..Self::default()
            },
            X11ResizeEdge::TopLeft => Self {
                top: true,
                left: true,
                ..Self::default()
            },
            X11ResizeEdge::TopRight => Self {
                top: true,
                right: true,
                ..Self::default()
            },
            X11ResizeEdge::BottomLeft => Self {
                bottom: true,
                left: true,
                ..Self::default()
            },
            X11ResizeEdge::BottomRight => Self {
                bottom: true,
                right: true,
                ..Self::default()
            },
        }
    }
}
