//! Preserve the input sequence when a client takes over moving or resizing.
use smithay::{
    backend::input::InputTime,
    input::Seat,
    utils::{Logical, Point, Serial},
};
use wayland_server::protocol::wl_seat::WlSeat;

use super::super::{App, DragState};
use crate::{
    compositor::windows::resize_cursor_icon,
    geometry::{CanvasPoint, grid::snap_coordinate},
};

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
        use super::policy::{Action, Phase, Source};
        seat == self._seat
            && self.interaction_allowed(
                Action::ClientWindowOperation,
                Source::Client,
                Phase::Continue,
            )
            && self.pointer.has_grab(serial)
            && self.pointer_grab_targets_window(window_index)
    }

    pub(in crate::compositor) fn pointer_grab_targets_window(&self, window_index: usize) -> bool {
        if !self.windows[window_index].mapped
            || self.has_window_pointer_grab()
            || !self.pressed_pointer_buttons.contains(&0x110)
        {
            return false;
        }
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
        // The press now belongs to this operation, including after its release.
        self.gtk_shell.reset_input();
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
        self.grab_canvas_peek_target(window_index);
        self.cancel_group_window_motion(self.windows[window_index].id);
        self.prepare_window_pointer_operation();
        self.windows[window_index].pending_resize = None;
        self.drag = Some(DragState {
            window_id: self.windows[window_index].id,
            pointer_start: self.window_pointer_start(window_index),
            window_start: self.windows[window_index].position,
            visible_start: self.window_canvas_rect(window_index).loc,
        });
        self.start_window_pointer_grab(window_index);
        // Recognition and the client's request can arrive after the finger has
        // moved. Include that motion instead of leaving the window behind it.
        self.apply_pointer_motion(self.raw_pointer_location, InputTime::now());
        self.request_redraw();
    }

    /// Modifier changes use the same original gesture baseline as pointer motion.
    pub(in crate::compositor) fn refresh_grid_snapping(&mut self) {
        self.layout_pointer_motion();
        if self.session_locked() {
            return;
        }
        if !self.desktop_pointer_motion() && !self.group_pointer_motion(InputTime::now()) {
            self.update_window_pointer_operation();
        }
    }

    pub(in crate::compositor) fn update_window_pointer_operation(&mut self) -> bool {
        if let Some(drag) = self.drag.as_ref() {
            let motion = self.raw_pointer_location - drag.pointer_start;
            let delta = Point::<f64, Logical>::from((
                motion.x / self.viewport_scale,
                motion.y / self.viewport_scale,
            ));
            let snap = self.keyboard.modifier_state().alt;
            let axis = |start: i32, visible: i32, delta: f64| {
                if snap {
                    let inset = f64::from(visible) - f64::from(start);
                    (snap_coordinate(
                        f64::from(visible) + delta,
                        f64::from(i32::MIN) + inset,
                        f64::from(i32::MAX) + inset,
                    ) - inset) as i32
                } else {
                    (f64::from(start) + delta.round()) as i32
                }
            };
            let position = CanvasPoint {
                x: axis(drag.window_start.x, drag.visible_start.x, delta.x),
                y: axis(drag.window_start.y, drag.visible_start.y, delta.y),
            };
            let id = drag.window_id;
            let start = drag.window_start;
            self.cursor_surface = None;
            if let Some(window) = self.window_mut_by_id(id)
                // An unmoved titlebar press remains a click, even with Alt held.
                && (delta != (0.0, 0.0).into() || window.position != start)
                && window.position != position
            {
                window.position = position;
                #[cfg(feature = "xwayland")]
                self.sync_x11_window_position(id);
                self.request_redraw();
            }
            return true;
        }
        if let Some(resize) = self.resize.as_ref() {
            self.cursor_icon = resize_cursor_icon(resize.edges);
            self.cursor_surface = None;
            self.update_resize(self.raw_pointer_location);
            return true;
        }
        false
    }
}
