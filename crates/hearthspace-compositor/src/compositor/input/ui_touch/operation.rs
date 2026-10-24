//! Client touch can transfer geometry ownership without creating a mouse press.
use std::collections::HashSet;

use smithay::{
    input::{Seat, touch::TouchDownGrab},
    utils::Serial,
};
use wayland_server::protocol::wl_seat::WlSeat;

use super::*;
use crate::compositor::windows::ResizeEdges;

pub(super) struct WindowOperation {
    pub(super) active: bool,
    location: Position,
    buttons: HashSet<u32>,
}

impl App {
    pub(in crate::compositor) fn ui_touch_has_client_grab(
        &self,
        surface: &WlSurface,
        serial: Serial,
    ) -> bool {
        let Some(touch) = self.ui_touch.as_ref() else {
            return false;
        };
        let Target::Client(client) = &touch.target else {
            return false;
        };
        client.serial == serial
            && client.surface == *surface
            && surface.is_alive()
            && self
                .touch
                .with_grab(|grab_serial, grab| {
                    grab_serial == serial
                        && grab.is::<TouchDownGrab<Self>>()
                        && grab.start_data().slot == touch.slot
                        && grab
                            .start_data()
                            .focus
                            .as_ref()
                            .is_some_and(|(target, _)| target == surface)
                })
                .unwrap_or(false)
    }

    pub(in crate::compositor) fn window_touch_grab_serial(&self, index: usize) -> Option<Serial> {
        let touch = self.ui_touch.as_ref()?;
        let Target::Client(client) = &touch.target else {
            return None;
        };
        (self.windows[index].mapped
            && touch.window == self.windows[index].id
            && self.window_index_for_surface(&client.surface) == Some(index)
            && touch.valid(
                Some(self.ui_touch_geometry(index)),
                &self.output_logical_rects(),
                self.navigation_available(),
                None,
            )
            && !self.has_pressed_pointer_sources()
            && self.ui_touch_has_client_grab(&client.surface, client.serial))
        .then_some(client.serial)
    }

    pub(in crate::compositor) fn valid_window_touch_request(
        &self,
        index: usize,
        seat: &WlSeat,
        serial: Serial,
    ) -> bool {
        Seat::<Self>::from_resource(seat).as_ref() == Some(&self._seat)
            && self.window_touch_grab_serial(index) == Some(serial)
    }

    fn claim_window_touch_operation(&mut self, index: usize) -> Option<(Position, Position)> {
        self.window_touch_grab_serial(index)?;
        let touch = self.ui_touch.as_mut()?;
        let Target::Client(client) = &touch.target else {
            return None;
        };
        let (origin, location) = (touch.origin, client.location);
        touch.target = Target::WindowOperation(WindowOperation {
            active: true,
            location,
            buttons: HashSet::new(),
        });
        self.gtk_shell.reset_input();
        // Revoke the client sequence once; the coordinator owns every remaining
        // contact event. In particular, no client up or synthetic press follows.
        self.touch.clone().cancel(self);
        self.pinch_zoom.interrupt();
        Some((origin, location))
    }

    pub(in crate::compositor) fn start_touch_window_move(&mut self, index: usize) {
        let Some((origin, location)) = self.claim_window_touch_operation(index) else {
            return;
        };
        self.grab_canvas_peek_target(index);
        self.cancel_group_window_motion(self.windows[index].id);
        self.begin_window_move_at(index, origin);
        self.update_window_operation_at(location);
    }

    pub(in crate::compositor) fn start_touch_window_resize(
        &mut self,
        index: usize,
        edges: ResizeEdges,
    ) {
        let Some((origin, location)) = self.claim_window_touch_operation(index) else {
            return;
        };
        self.cancel_group_window_motion(self.windows[index].id);
        self.grab_canvas_peek_target(index);
        self.begin_window_resize_at(index, edges, origin);
        self.update_window_operation_at(location);
    }

    pub(in crate::compositor) fn has_window_touch_operation(&self) -> bool {
        self.ui_touch
            .as_ref()
            .is_some_and(|touch| matches!(touch.target, Target::WindowOperation(_)))
    }

    pub(in crate::compositor) fn window_touch_operation_position(&self) -> Option<Position> {
        let touch = self.ui_touch.as_ref()?;
        match &touch.target {
            Target::WindowOperation(operation) if operation.active => Some(operation.location),
            _ => None,
        }
    }

    pub(super) fn motion_window_touch_operation(&mut self, location: Position) {
        let Some(touch) = self.ui_touch.as_mut() else {
            return;
        };
        let Target::WindowOperation(operation) = &mut touch.target else {
            return;
        };
        if operation.active {
            operation.location = location;
            self.update_window_operation_at(location);
        }
    }

    pub(super) fn finish_window_touch_operation(&mut self) {
        let Some(touch) = self.ui_touch.as_mut() else {
            return;
        };
        let Target::WindowOperation(operation) = &mut touch.target else {
            return;
        };
        if !operation.active {
            return;
        }
        operation.active = false;
        let id = touch.window;
        // Safe under the keyboard focus callback: final geometry was already
        // applied on motion. Do not query modifiers or dispatch pointer input.
        if self.drag.as_ref().is_some_and(|drag| drag.window_id == id) {
            self.drag = None;
        }
        if self
            .resize
            .as_ref()
            .is_some_and(|resize| resize.window_id == id)
        {
            self.finish_resize();
        }
        self.request_redraw();
    }

    pub(super) fn maintain_window_touch_operation(&mut self) -> bool {
        let Some(touch) = self.ui_touch.as_ref() else {
            return false;
        };
        if !matches!(touch.target, Target::WindowOperation(_)) {
            return false;
        }
        if !self
            .windows
            .iter()
            .any(|window| window.id == touch.window && window.mapped)
            || !self.output_logical_rects().contains(&touch.output)
            || self.session_locked()
            || self.canvas_search_active()
            || self.window_switcher_active()
            || self.layout_editing()
            || self.layout_confirmation_active()
        {
            self.cancel_ui_touch();
        }
        true
    }

    pub(in crate::compositor) fn capture_window_touch_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
        time: InputTime,
    ) -> bool {
        let Some(touch) = self.ui_touch.as_mut() else {
            return false;
        };
        let Target::WindowOperation(operation) = &mut touch.target else {
            return false;
        };
        match state {
            ButtonState::Pressed => {
                operation.buttons.insert(button);
            }
            ButtonState::Released => {
                operation.buttons.remove(&button);
            }
        }
        if !self.touch_input.has_contacts() && operation.buttons.is_empty() {
            self.ui_touch = None;
            self.apply_pointer_motion(self.pointer_location, time);
        }
        true
    }

    pub(super) fn window_touch_has_pointer_buttons(&self) -> bool {
        self.ui_touch.as_ref().is_some_and(|touch| {
            matches!(
                &touch.target, Target::WindowOperation(operation) if !operation.buttons.is_empty()
            )
        })
    }

    pub(in crate::compositor) fn retire_window_ui_touch(&mut self, id: u64) {
        self.invalidate_gtk_touch_window(id);
        if self
            .ui_touch
            .as_ref()
            .is_some_and(|touch| touch.window == id)
        {
            self.cancel_ui_touch();
        }
    }
}
