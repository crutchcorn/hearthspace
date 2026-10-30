//! Explicit client surface trees, viewport buffers, and cursor ownership.
use super::{
    MapClient, QueueHandle, capture::shm_buffer, delegate_noop, wl_pointer, wl_registry, wl_surface,
};
use wayland_client::protocol::{wl_output, wl_subcompositor, wl_subsurface};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

#[derive(Default)]
pub(super) struct State {
    subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    viewporter: Option<wp_viewporter::WpViewporter>,
    pub pointer: Option<wl_pointer::WlPointer>,
    focus: Option<wl_surface::WlSurface>,
    serial: u32,
}

pub(super) fn global(
    state: &mut super::State,
    registry: &wl_registry::WlRegistry,
    name: u32,
    interface: &str,
    q: &QueueHandle<super::State>,
) {
    match interface {
        "wl_subcompositor" => {
            state.capture_tree.subcompositor = Some(registry.bind(name, 1, q, ()))
        }
        "wp_viewporter" => state.capture_tree.viewporter = Some(registry.bind(name, 1, q, ())),
        _ => {}
    }
}

pub(super) fn pointer_event(state: &mut super::State, event: &wl_pointer::Event) {
    match event {
        wl_pointer::Event::Enter {
            surface, serial, ..
        } => {
            state.capture_tree.focus = Some(surface.clone());
            state.capture_tree.serial = *serial;
        }
        wl_pointer::Event::Leave { .. } => state.capture_tree.focus = None,
        _ => {}
    }
}

impl MapClient {
    pub(in super::super) fn capture_geometry(&mut self, number: usize, rect: (i32, i32, i32, i32)) {
        let window = &mut self.state.windows[number];
        window.defer_buffer = true;
        window
            .xdg_surface
            .set_window_geometry(rect.0, rect.1, rect.2, rect.3);
        window.surface.commit();
        self.sync();
    }

    pub(in super::super) fn capture_stretched_root(
        &mut self,
        number: usize,
    ) -> wl_surface::WlSurface {
        let window = &mut self.state.windows[number];
        window.defer_buffer = true;
        let root = window.surface.clone();
        let viewport = self
            .state
            .capture_tree
            .viewporter
            .as_ref()
            .unwrap()
            .get_viewport(&root, &self.queue.handle(), ());
        viewport.set_destination(window.size.0, window.size.1);
        self.capture_paint(&root, (1, 1), wl_output::Transform::Normal, |_, _| {
            [0, 0, 255, 255]
        });
        root
    }

    pub(in super::super) fn capture_child(
        &mut self,
        parent: &wl_surface::WlSurface,
        position: (i32, i32),
        destination: (i32, i32),
        backing: (i32, i32),
        transform: wl_output::Transform,
        paint: impl Fn(i32, i32) -> [u8; 4],
    ) -> wl_surface::WlSurface {
        let q = self.queue.handle();
        let surface = self
            .state
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&q, ());
        let child = self
            .state
            .capture_tree
            .subcompositor
            .as_ref()
            .unwrap()
            .get_subsurface(&surface, parent, &q, ());
        child.set_position(position.0, position.1);
        child.set_desync();
        let viewport = self
            .state
            .capture_tree
            .viewporter
            .as_ref()
            .unwrap()
            .get_viewport(&surface, &q, ());
        viewport.set_destination(destination.0, destination.1);
        self.capture_paint(&surface, backing, transform, paint);
        parent.commit(); // Commit subsurface position/stacking.
        self.sync();
        surface
    }

    pub(in super::super) fn capture_paint(
        &mut self,
        surface: &wl_surface::WlSurface,
        size: (i32, i32),
        transform: wl_output::Transform,
        paint: impl Fn(i32, i32) -> [u8; 4],
    ) {
        let mut bytes = Vec::new();
        for y in 0..size.1 {
            for x in 0..size.0 {
                bytes.extend_from_slice(&paint(x, y));
            }
        }
        let (file, buffer) = shm_buffer(&self.state, &self.queue.handle(), size, &bytes);
        surface.set_buffer_transform(transform);
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, size.0, size.1);
        surface.commit();
        self.state.buffers.push((file, buffer));
        self.sync();
    }

    pub(in super::super) fn capture_cursor(&mut self, expected_focus: &wl_surface::WlSurface) {
        self.sync();
        assert_eq!(
            self.state.capture_tree.focus.as_ref(),
            Some(expected_focus),
            "pointer must focus the popup subsurface"
        );
        let surface = self
            .state
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&self.queue.handle(), ());
        self.state
            .capture_tree
            .pointer
            .as_ref()
            .unwrap()
            .set_cursor(self.state.capture_tree.serial, Some(&surface), 2, 3);
        self.capture_paint(&surface, (8, 8), wl_output::Transform::Normal, |_, _| {
            [0, 255, 255, 255]
        });
    }
}

delegate_noop!(super::State: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(super::State: ignore wl_subsurface::WlSubsurface);
delegate_noop!(super::State: ignore wp_viewporter::WpViewporter);
delegate_noop!(super::State: ignore wp_viewport::WpViewport);
