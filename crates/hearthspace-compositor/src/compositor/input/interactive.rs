//! Preserve the input sequence when a client takes over moving or resizing.
use smithay::{
    backend::input::InputTime,
    input::Seat,
    utils::{Logical, Point, Serial},
};
use wayland_server::protocol::wl_seat::WlSeat;

use super::super::{App, DragState};

impl App {
    pub(in crate::compositor) fn valid_window_pointer_request(
        &self,
        window_index: usize,
        seat: &WlSeat,
        serial: Serial,
    ) -> bool {
        let Some(seat) = Seat::<Self>::from_resource(seat) else {
            return false;
        };
        seat == self._seat
            && self.drag.is_none()
            && self.resize.is_none()
            && self.pointer.has_grab(serial)
            && self.pointer_grab_targets_window(window_index)
    }

    pub(in crate::compositor) fn pointer_grab_targets_window(&self, window_index: usize) -> bool {
        let Some(start) = self.pointer.grab_start_data() else {
            return false;
        };
        // Interactive operations currently end on left-button release. Touch
        // emulates that same button, including its Smithay implicit grab.
        start.button == 0x110
            && start.focus.as_ref().is_some_and(|(surface, _)| {
                self.window_index_for_surface(surface) == Some(window_index)
            })
    }

    pub(in crate::compositor) fn prepare_window_pointer_operation(&mut self) {
        if self.touch_pointer_pressed {
            // xdg/XWayland requests arrive after touch dispatch has returned.
            // They continue this finger's pointer sequence; interrupting that
            // sequence would release the button and suppress all later motion.
            self.pinch_zoom.interrupt();
        } else {
            self.interrupt_pinch_zoom();
        }
    }

    pub(in crate::compositor) fn window_pointer_start(
        &self,
        window_index: usize,
    ) -> Point<f64, Logical> {
        if self.pointer_grab_targets_window(window_index) {
            self.pointer.grab_start_data().unwrap().location
        } else {
            // Compositor decorations start synchronously, before a client grab.
            self.raw_pointer_location
        }
    }

    pub(in crate::compositor) fn start_client_window_move(&mut self, window_index: usize) {
        self.cancel_group_window_motion(self.windows[window_index].id);
        self.prepare_window_pointer_operation();
        self.drag = Some(DragState {
            window_id: self.windows[window_index].id,
            pointer_start: self.window_pointer_start(window_index),
            window_start: self.windows[window_index].position,
        });
        // Recognition and the client's request can arrive after the finger has
        // moved. Include that motion instead of leaving the window behind it.
        self.apply_pointer_motion(self.raw_pointer_location, InputTime::now());
        self.request_redraw();
    }
}
