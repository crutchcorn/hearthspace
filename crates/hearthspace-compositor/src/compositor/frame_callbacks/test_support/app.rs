//! Real App geometry and callback dispatch over the existing Wayland fixture.
//! Managed mapping/placement starts as fixture input; callbacks and committed xdg
//! metadata are real protocol state. Tests can exercise App identity/lifetime
//! paths without session startup, service workers or a renderer.
use smithay::{
    reexports::calloop::{EventLoop, LoopHandle},
    utils::{Logical, Rectangle},
};
use wayland_server::{Display, DisplayHandle};

use super::Fixture;
use crate::{
    compositor::{
        App, CalloopData, ManagedSurface, ManagedWindow, ManagedWindowKind, WindowDecoration,
    },
    geometry::CanvasPoint,
};

mod state;

/// Reuse the real App constructor for protocol tests without starting services,
/// a renderer or a native session.
pub(in crate::compositor) fn new_app(
    display: &DisplayHandle,
    handle: LoopHandle<'static, CalloopData>,
) -> App {
    state::new(display, handle)
}

pub(in crate::compositor::frame_callbacks) struct AppFixture {
    pub(in crate::compositor::frame_callbacks) app: App,
    wire: Fixture,
    _display: Display<App>,
    _event_loop: EventLoop<'static, CalloopData>,
}

impl AppFixture {
    pub(in crate::compositor::frame_callbacks) fn new() -> Self {
        let display = Display::new().unwrap();
        let event_loop = EventLoop::try_new().unwrap();
        Self {
            app: new_app(&display.handle(), event_loop.handle()),
            wire: Fixture::new(),
            _display: display,
            _event_loop: event_loop,
        }
    }

    pub(in crate::compositor::frame_callbacks) fn window(
        &mut self,
        kind: ManagedWindowKind,
        output: Option<&str>,
        geometry: Rectangle<i32, Logical>,
    ) -> usize {
        let surface = self.wire.window("A");
        let xdg = self.wire.surfaces[surface].xdg.as_ref().unwrap();
        xdg.set_window_geometry(
            geometry.loc.x,
            geometry.loc.y,
            geometry.size.w,
            geometry.size.h,
        );
        self.wire.surfaces[surface].client.commit();
        self.wire.sync();
        let toplevel = self
            .wire
            .server
            .toplevel(&self.wire.surfaces[surface].server);
        self.app.windows.insert_new(|id| ManagedWindow {
            id,
            surface: ManagedSurface::Xdg(toplevel),
            identity: Default::default(),
            position: CanvasPoint { x: 0, y: 0 },
            kind,
            shell_output_name: output.map(str::to_owned),
            decoration: WindowDecoration::ClientSide,
            titlebar: None,
            content_bbox_size: geometry.size,
            mapped: true,
            initial_placement_done: true,
            initial_output_name: None,
            presentation: Default::default(),
            pending_resize: None,
            advertised_fractional_scale: 0.,
        });
        assert_eq!(self.app.windows.len() - 1, surface);
        surface
    }

    pub(in crate::compositor::frame_callbacks) fn request_frame(&mut self, surface: usize) {
        self.wire.request_frame(surface);
    }

    pub(in crate::compositor::frame_callbacks) fn set_app_id(&mut self, surface: usize, id: &str) {
        self.wire.surfaces[surface]
            .toplevel
            .as_ref()
            .unwrap()
            .set_app_id(id.into());
        self.wire.sync();
    }

    pub(in crate::compositor::frame_callbacks) fn commit_buffer(&mut self, surface: usize) {
        let toplevel = self
            .wire
            .server
            .toplevel(&self.wire.surfaces[surface].server);
        if !toplevel.is_initial_configure_sent() {
            // The small wire server does not run App's initial-configure
            // handler. Complete that handshake after Smithay resets the role.
            toplevel.send_configure();
            self.wire.sync();
            self.wire.sync();
        }
        self.wire.make_ready(surface);
    }

    pub(in crate::compositor::frame_callbacks) fn unmap_buffer(&mut self, surface: usize) {
        self.wire.surfaces[surface].client.attach(None, 0, 0);
        self.wire.surfaces[surface].client.commit();
        self.wire.sync();
    }

    pub(in crate::compositor::frame_callbacks) fn send_output(
        &self,
        output: &str,
        time: u32,
    ) -> usize {
        self.app.dispatch_frame_callbacks(Some(output), time)
    }

    pub(in crate::compositor::frame_callbacks) fn assert_done(
        &mut self,
        expected: &[(usize, u32)],
    ) {
        self.wire.assert_done(expected);
    }
}
