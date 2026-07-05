use smithay::{
    input::{Seat, SeatHandler, SeatState, dnd::DndGrabHandler, pointer::CursorImageStatus},
    utils::{Logical, Rectangle},
    wayland::{
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
        selection::{
            SelectionHandler, SelectionTarget,
            data_device::{DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler},
        },
        xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    },
    xwayland::{
        X11Surface, X11Wm, XwmHandler,
        xwm::{Reorder, ResizeEdge as X11ResizeEdge, WmWindowProperty, XwmId},
    },
};
use tracing::{debug, warn};
use wayland_server::{Client, Resource, protocol::wl_surface::WlSurface};

use crate::compositor::{App, CalloopData, DragState, WindowDecoration, windows::ResizeEdges};

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

impl SelectionHandler for CalloopData {
    type SelectionUserData = ();
}

impl WaylandDndGrabHandler for CalloopData {}

impl DataDeviceHandler for CalloopData {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.state.data_device_state
    }
}

impl SeatHandler for CalloopData {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.xwayland_seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&Self::KeyboardFocus>) {}

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}
}

impl DndGrabHandler for CalloopData {}

impl CompositorHandler for CalloopData {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.state.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        self.state.client_compositor_state(client)
    }

    fn commit(&mut self, surface: &WlSurface) {
        CompositorHandler::commit(&mut self.state, surface);
    }
}

impl XWaylandShellHandler for CalloopData {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.state._xwayland_shell_state
    }

    fn surface_associated(&mut self, xwm: XwmId, wl_surface: WlSurface, surface: X11Surface) {
        XWaylandShellHandler::surface_associated(&mut self.state, xwm, wl_surface, surface);
    }
}

impl XwmHandler for App {
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
        let Some(index) = self.window_index_for_x11(&window) else {
            return;
        };
        let index = self.raise_window(index);
        self.start_resize(index, ResizeEdges::from(resize_edge));
    }

    fn move_request(&mut self, _xwm: XwmId, window: X11Surface, _button: u32) {
        let Some(index) = self.window_index_for_x11(&window) else {
            return;
        };
        let index = self.raise_window(index);
        self.drag = Some(DragState {
            window_id: self.windows[index].id,
            pointer_start: self.raw_pointer_location,
            window_start: self.windows[index].position,
        });
        self.request_redraw();
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

    fn allow_selection_access(&mut self, _xwm: XwmId, _selection: SelectionTarget) -> bool {
        false
    }
}

impl XwmHandler for CalloopData {
    fn xwm_state(&mut self, xwm: XwmId) -> &mut X11Wm {
        XwmHandler::xwm_state(&mut self.state, xwm)
    }

    fn new_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::new_window(&mut self.state, xwm, window);
    }

    fn new_override_redirect_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::new_override_redirect_window(&mut self.state, xwm, window);
    }

    fn map_window_request(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::map_window_request(&mut self.state, xwm, window);
    }

    fn map_window_notify(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::map_window_notify(&mut self.state, xwm, window);
    }

    fn mapped_override_redirect_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::mapped_override_redirect_window(&mut self.state, xwm, window);
    }

    fn unmapped_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::unmapped_window(&mut self.state, xwm, window);
    }

    fn destroyed_window(&mut self, xwm: XwmId, window: X11Surface) {
        XwmHandler::destroyed_window(&mut self.state, xwm, window);
    }

    fn configure_request(
        &mut self,
        xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        reorder: Option<Reorder>,
    ) {
        XwmHandler::configure_request(&mut self.state, xwm, window, x, y, w, h, reorder);
    }

    fn configure_notify(
        &mut self,
        xwm: XwmId,
        window: X11Surface,
        geometry: Rectangle<i32, Logical>,
        above: Option<u32>,
    ) {
        XwmHandler::configure_notify(&mut self.state, xwm, window, geometry, above);
    }

    fn property_notify(&mut self, xwm: XwmId, window: X11Surface, property: WmWindowProperty) {
        XwmHandler::property_notify(&mut self.state, xwm, window, property);
    }

    fn resize_request(
        &mut self,
        xwm: XwmId,
        window: X11Surface,
        button: u32,
        resize_edge: X11ResizeEdge,
    ) {
        XwmHandler::resize_request(&mut self.state, xwm, window, button, resize_edge);
    }

    fn move_request(&mut self, xwm: XwmId, window: X11Surface, button: u32) {
        XwmHandler::move_request(&mut self.state, xwm, window, button);
    }

    fn active_window_request(
        &mut self,
        xwm: XwmId,
        window: X11Surface,
        timestamp: u32,
        currently_active_window: Option<X11Surface>,
    ) {
        XwmHandler::active_window_request(
            &mut self.state,
            xwm,
            window,
            timestamp,
            currently_active_window,
        );
    }

    fn allow_selection_access(&mut self, xwm: XwmId, selection: SelectionTarget) -> bool {
        XwmHandler::allow_selection_access(&mut self.state, xwm, selection)
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
