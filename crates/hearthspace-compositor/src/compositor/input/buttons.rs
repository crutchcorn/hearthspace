//! Shared policy for physical, direct-touch, and agent pointer buttons.
use crate::compositor::{App, DragState, HitTarget, idle::ActivityReason};
use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::ButtonEvent,
    utils::SERIAL_COUNTER,
};
use tracing::{debug, trace};

impl App {
    pub(super) fn handle_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
        time: InputTime,
    ) {
        self.shortcuts.cancel_meta_tap();
        if state == ButtonState::Pressed {
            self.cancel_pending_search();
        }
        if self.session_locked() {
            self.deliver_lock_button(button, state, time);
            return;
        }
        if self.handle_overview_button(button, state) {
            return;
        }
        trace!(button, ?state, "pointer button input");
        if state == ButtonState::Pressed {
            self.interrupt_pinch_zoom();
            // Close-button and background presses do not always change focus.
            self.dismiss_shell_panels_outside_pointer();
        }
        let is_left_button = button == 0x110;

        if is_left_button
            && state == ButtonState::Released
            && (self.drag.is_some() || self.resize.is_some())
        {
            debug!("ending pointer drag/resize from button release");
            self.drag = None;
            self.finish_resize();
            // A client-initiated move/resize established an implicit grab;
            // forwarding its release is necessary to end that grab as well.
            let pointer = self.pointer.clone();
            self.deliver_pointer_button(&ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                time,
                button,
                state,
            });
            pointer.frame(self);
            return;
        }

        if is_left_button && state == ButtonState::Pressed {
            match self.hit_test(self.pointer_location) {
                Some(HitTarget::CloseButton { window_index }) => {
                    debug!(
                        window_id = self.windows[window_index].id,
                        "close button clicked"
                    );
                    self.windows[window_index].surface.send_close();
                    self.drag = None;
                    return;
                }
                Some(HitTarget::TitleBar { window_index }) => {
                    let window_index = self.raise_window(window_index);
                    if let Some(surface) = self.windows[window_index].surface.wl_surface() {
                        self.set_keyboard_focus_to_window(window_index, surface);
                    }
                    if self.window_uses_output_coordinates(window_index) {
                        return;
                    }
                    self.drag = Some(DragState {
                        window_id: self.windows[window_index].id,
                        pointer_start: self.raw_pointer_location,
                        window_start: self.windows[window_index].position,
                    });
                    debug!(
                        window_id = self.windows[window_index].id,
                        "started titlebar drag"
                    );
                    self.request_redraw();
                    return;
                }
                Some(HitTarget::ResizeBorder {
                    window_index,
                    edges,
                }) => {
                    let window_index = self.raise_window(window_index);
                    if let Some(surface) = self.windows[window_index].surface.wl_surface() {
                        self.set_keyboard_focus_to_window(window_index, surface);
                    }
                    debug!(
                        window_id = self.windows[window_index].id,
                        ?edges,
                        "started resize from pointer press"
                    );
                    self.start_resize(window_index, edges);
                    return;
                }
                Some(HitTarget::Client { window_index, .. }) => {
                    let window_index = self.raise_window(window_index);
                    if let Some(surface) = self.windows[window_index].surface.wl_surface() {
                        self.set_keyboard_focus_to_window(window_index, surface);
                    }
                }
                None => {
                    debug!("clearing keyboard focus from pointer press on canvas");
                    self.clear_keyboard_focus();
                }
            }
        } else if is_left_button
            && state == ButtonState::Released
            && matches!(
                self.hit_test(self.pointer_location),
                Some(HitTarget::CloseButton { .. })
            )
        {
            return;
        }

        let focus = match self.hit_test(self.pointer_location) {
            Some(HitTarget::Client {
                window_index,
                surface,
                surface_location,
            }) => {
                self.record_client_activity_for_window_index(
                    window_index,
                    ActivityReason::ClientInput,
                );
                Some((surface, surface_location))
            }
            _ => None,
        };

        match focus.clone() {
            Some((surface, _)) => {
                if state == ButtonState::Pressed
                    && let Some(window_index) = self.window_index_for_surface(&surface)
                {
                    self.set_keyboard_focus_to_window(window_index, surface);
                }
            }
            _ if is_left_button && state == ButtonState::Pressed => return,
            _ => {}
        }

        let pointer = self.pointer.clone();
        self.deliver_pointer_button(&ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time,
            button,
            state,
        });
        pointer.frame(self);
    }
}
