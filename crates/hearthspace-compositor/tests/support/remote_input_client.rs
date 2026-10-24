//! Real protocol requests, including client keymaps and disconnects with holds.
use std::{
    io::Write,
    os::{fd::AsFd, unix::net::UnixStream},
    path::Path,
};

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};

#[derive(Default)]
struct State {
    seat: Option<wl_seat::WlSeat>,
    output: Option<wl_output::WlOutput>,
    keyboard_manager: Option<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1>,
    pointer_manager: Option<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1>,
}

pub(crate) struct RemoteClient {
    queue: EventQueue<State>,
    state: State,
    keyboard: Option<zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1>,
    pointer: Option<zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1>,
    _connection: Connection,
}

impl RemoteClient {
    pub(crate) fn connect(socket: &Path) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        connection.display().get_registry(&handle, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).unwrap();
        let keyboard = state.keyboard_manager.as_ref().map(|manager| {
            manager.create_virtual_keyboard(state.seat.as_ref().unwrap(), &handle, ())
        });
        let pointer = state.pointer_manager.as_ref().map(|manager| {
            manager.create_virtual_pointer_with_output(
                state.seat.as_ref(),
                state.output.as_ref(),
                &handle,
                (),
            )
        });
        let mut client = Self {
            queue,
            state,
            keyboard,
            pointer,
            _connection: connection,
        };
        client.sync();
        client
    }

    pub(super) fn enabled(&self) -> bool {
        self.keyboard.is_some() && self.pointer.is_some()
    }

    pub(super) fn set_keymap(&mut self, text: &str) {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.write_all(&[0]).unwrap();
        self.keyboard.as_ref().unwrap().keymap(
            wl_keyboard::KeymapFormat::XkbV1 as u32,
            file.as_fd(),
            text.len() as u32 + 1,
        );
        self.sync();
    }

    pub(super) fn key(&mut self, code: u32, pressed: bool) {
        self.keyboard
            .as_ref()
            .unwrap()
            .key(0, code, u32::from(pressed));
        self.sync();
    }

    pub(super) fn modifiers(&mut self, depressed: u32) {
        self.keyboard
            .as_ref()
            .unwrap()
            .modifiers(depressed, 0, 0, 0);
        self.sync();
    }

    pub(crate) fn motion(&mut self, x: u32, y: u32, width: u32, height: u32) {
        let pointer = self.pointer.as_ref().unwrap();
        pointer.motion_absolute(0, x, y, width, height);
        pointer.frame();
        self.sync();
    }

    pub(crate) fn button(&mut self, pressed: bool) {
        let pointer = self.pointer.as_ref().unwrap();
        pointer.button(
            0,
            0x110,
            if pressed {
                wl_pointer::ButtonState::Pressed
            } else {
                wl_pointer::ButtonState::Released
            },
        );
        pointer.frame();
        self.sync();
    }

    pub(crate) fn axis(&mut self, horizontal: f64, vertical: f64) {
        let pointer = self.pointer.as_ref().unwrap();
        pointer.axis_source(wl_pointer::AxisSource::Wheel);
        pointer.axis(0, wl_pointer::Axis::HorizontalScroll, horizontal);
        pointer.axis(0, wl_pointer::Axis::VerticalScroll, vertical);
        pointer.frame();
        self.sync();
    }

    pub(super) fn destroy_keyboard(&mut self) {
        self.keyboard.take().unwrap().destroy();
        self.sync();
    }

    pub(crate) fn destroy_pointer(&mut self) {
        self.pointer.take().unwrap().destroy();
        self.sync();
    }

    fn sync(&mut self) {
        self.queue.roundtrip(&mut self.state).unwrap();
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), handle, ())),
                "wl_output" if state.output.is_none() => {
                    state.output = Some(registry.bind(name, version.min(4), handle, ()))
                }
                "zwp_virtual_keyboard_manager_v1" => {
                    state.keyboard_manager = Some(registry.bind(name, 1, handle, ()))
                }
                "zwlr_virtual_pointer_manager_v1" => {
                    state.pointer_manager = Some(registry.bind(name, 2, handle, ()))
                }
                _ => {}
            }
        }
    }
}

delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore wl_output::WlOutput);
delegate_noop!(State: ignore zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
delegate_noop!(State: ignore zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);
delegate_noop!(State: ignore zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ignore zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1);
