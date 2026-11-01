//! Shared policy for physical, direct-touch, and agent pointer buttons.
use crate::compositor::{App, DragState, HitTarget, idle::ActivityReason};
use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::{ButtonEvent, MotionEvent},
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
        self.handle_pointer_button_from_source(
            super::remote::PointerSource::Local,
            button,
            state,
            time,
        );
    }

    pub(in crate::compositor::input) fn handle_pointer_button_policy(
        &mut self,
        button: u32,
        state: ButtonState,
        time: InputTime,
    ) {
        self.shortcuts.cancel_meta_tap();
        if self.canvas_pan_button(button, state, time) {
            return;
        }
        if self.capture_window_touch_pointer_button(button, state, time) {
            return;
        }
        if self.has_window_pointer_grab() {
            let owner_retired = self.window_pointer_grab_owner_retired();
            // Route every button to the owner, including releases after its
            // window disappeared. Neither chrome nor another client may claim
            // the remainder of this sequence.
            self.deliver_pointer_button(&ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                time,
                button,
                state,
            });
            self.pointer.clone().frame(self);
            if !self.has_window_pointer_grab() {
                self.apply_pointer_motion(self.pointer_location, time);
                self.restore_retired_window_grab_focus(owner_retired);
            }
            return;
        }
        if state == ButtonState::Pressed {
            self.abandon_released_shell_resize_click();
        }
        // An overlay or gesture may have cancelled a compositor-owned press.
        // Consume its release before another input mode can redirect it.
        if state == ButtonState::Released && self.titlebar_pointer_button(button, state, time) {
            self.apply_pointer_motion(self.pointer_location, time);
            return;
        }
        if state == ButtonState::Pressed {
            self.cancel_pending_search();
            let group_was_active = self.group_active();
            self.dismiss_group_outside_press(self.pointer_location, button == 0x110);
            if group_was_active && !self.group_active() {
                // Selection hover may have deliberately removed client focus.
                // Reconcile it before forwarding the outside press unchanged.
                self.apply_pointer_motion(self.pointer_location, time);
            }
            // First touch-down already invalidates using its own coordinates.
            if !self.dispatching_touch {
                self.cancel_shell_popup_outside_press(self.pointer_location);
            }
        }
        if self.favorite_pointer_button(button, state) {
            if state == ButtonState::Released {
                // Captured motion bypassed Wayland hover. Reconcile its final
                // position before the next wheel/button event reaches a client.
                self.apply_pointer_motion(self.pointer_location, time);
            }
            return;
        }
        if self.session_locked() {
            self.deliver_lock_button(button, state, time);
            return;
        }
        if self.layout_pointer_button(button, state) {
            return;
        }
        if self.canvas_search_pointer_button(button, state) {
            return;
        }
        if self.group_pointer_button(button, state) {
            self.apply_pointer_motion(self.pointer_location, time);
            return;
        }
        if self.desktop_context_button(button, state) {
            return;
        }
        if self.desktop_pointer_button(button, state, time) {
            return;
        }
        if self.handle_window_switcher_button(button, state) {
            return;
        }
        trace!(button, ?state, "pointer button input");
        if state == ButtonState::Pressed {
            self.interrupt_pinch_zoom();
            // Close-button and background presses do not always change focus.
            self.dismiss_shell_panels_outside_pointer();
        }
        let is_left_button = button == 0x110;
        if self.titlebar_pointer_button(button, state, time) {
            return;
        }
        if is_left_button
            && state == ButtonState::Released
            && (self.drag.is_some() || self.resize.is_some())
        {
            debug!("ending pointer drag/resize from button release");
            self.gtk_shell.reset_input();
            self.update_window_pointer_operation();
            self.drag = None;
            self.finish_resize();
            // Client requests are owned by WindowGrab above. This also drains
            // a pre-existing implicit grab if compositor chrome used it.
            let pointer = self.pointer.clone();
            if self.pressed_pointer_buttons.contains(&button) {
                self.deliver_pointer_button(&ButtonEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    button,
                    state,
                });
            }
            pointer.frame(self);
            self.apply_pointer_motion(self.pointer_location, time);
            return;
        }

        if self.pointer.is_grabbed() {
            // Smithay's existing client grab owns additional buttons too.
            // Chrome under the pointer must not steal a press or its release.
            self.deliver_pointer_button(&ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                time,
                button,
                state,
            });
            self.pointer.clone().frame(self);
            self.refresh_titlebar_hover();
            return;
        }

        if is_left_button && state == ButtonState::Pressed {
            match self.hit_test(self.pointer_location) {
                Some(HitTarget::TitlebarButton { .. }) => return,
                Some(HitTarget::TitleBar { window_index }) => {
                    self.grab_canvas_peek_target(window_index);
                    self.cancel_group_window_motion(self.windows[window_index].id);
                    let window_index = self.raise_window(window_index);
                    if let Some(surface) = self.windows[window_index].surface.wl_surface() {
                        self.set_keyboard_focus_to_window(window_index, surface);
                    }
                    if self.titlebar_title_press(self.windows[window_index].id, time) {
                        return;
                    }
                    if self.window_uses_output_coordinates(window_index)
                        || self.windows[window_index].presentation.maximized
                    {
                        return;
                    }
                    self.windows[window_index].pending_resize = None;
                    self.drag = Some(DragState {
                        window_id: self.windows[window_index].id,
                        pointer_start: self.raw_pointer_location,
                        window_start: self.windows[window_index].position,
                        visible_start: self.window_canvas_rect(window_index).loc,
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
            && !self.pressed_pointer_buttons.contains(&button)
            && matches!(
                self.hit_test(self.pointer_location),
                Some(HitTarget::TitlebarButton { .. })
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
        if state == ButtonState::Pressed {
            // A corner can resize and move under a stationary pointer. Refresh
            // Smithay's surface origin before the press establishes its grab.
            // Existing grabs keep their owner and release coordinates.
            pointer.motion(
                self,
                focus.clone(),
                &MotionEvent {
                    location: self.pointer_location,
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                },
            );
        }
        let event = ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time,
            button,
            state,
        };
        if state == ButtonState::Pressed {
            self.defer_shell_resize_click(focus, &event);
        }
        self.deliver_pointer_button(&event);
        if state == ButtonState::Pressed {
            self.track_shell_pointer_grab();
        }
        pointer.frame(self);
        self.refresh_titlebar_hover();
    }
}
