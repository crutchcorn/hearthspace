//! Mouse canvas navigation owns a pointer sequence independently of its transform.
use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::{ButtonEvent, Focus, GrabStartData, MotionEvent},
    utils::{Logical, Point, SERIAL_COUNTER},
};

use super::{
    policy::{Action, Phase, Source},
    remote::PointerSource,
};
use crate::{
    compositor::{App, HitTarget, cursor::CursorIcon},
    geometry::CanvasPoint,
};

mod grab;
#[cfg(test)]
mod tests;
mod tools;

#[derive(Default)]
pub(in crate::compositor) struct CanvasPan {
    selected: bool,
    // Retained until the grab drains, even after drag cancellation.
    owner: Option<PointerSource>,
    drag: Option<Drag>,
}

struct Drag {
    pointer: Point<f64, Logical>,
    offset: CanvasPoint,
    scale: f64,
}

impl Drag {
    fn offset_at(&self, point: Point<f64, Logical>) -> CanvasPoint {
        let delta = point - self.pointer;
        CanvasPoint {
            x: (f64::from(self.offset.x) - delta.x / self.scale)
                .round()
                .clamp(-1_000_000., 1_000_000.) as i32,
            y: (f64::from(self.offset.y) - delta.y / self.scale)
                .round()
                .clamp(-1_000_000., 1_000_000.) as i32,
        }
    }
}

impl App {
    pub(in crate::compositor) fn canvas_pan_captured(&self) -> bool {
        self.canvas_pan.owner.is_some()
    }

    pub(in crate::compositor) fn cancel_canvas_pan(&mut self) {
        if self.canvas_pan.drag.take().is_some() {
            self.canvas_pan_cursor();
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn disarm_canvas_pan(&mut self) {
        if self.revoke_canvas_pan() {
            // Mode entry runs outside Smithay callbacks. Reconcile even when
            // the pointer is stationary (for example desktop V or legacy IPC).
            self.apply_pointer_motion(self.pointer_location, InputTime::now());
        }
    }

    /// Focus callbacks can hold Smithay's keyboard mutex. Revoke only our
    /// state and named cursor here; keep the grab until its buttons drain.
    pub(in crate::compositor) fn revoke_canvas_pan(&mut self) -> bool {
        let selected = std::mem::take(&mut self.canvas_pan.selected);
        self.cancel_canvas_pan();
        if selected {
            self.cursor_icon = CursorIcon::Default;
            self.layout_changed();
        }
        selected
    }

    pub(in crate::compositor::input) fn canvas_pan_source_released(
        &mut self,
        source: &PointerSource,
        button: u32,
    ) {
        if button == 0x110 && self.canvas_pan.owner.as_ref() == Some(source) {
            self.cancel_canvas_pan();
        }
    }

    pub(in crate::compositor::input) fn cancel_canvas_pan_source(
        &mut self,
        source: &PointerSource,
    ) {
        if self.canvas_pan.owner.as_ref() == Some(source) {
            self.cancel_canvas_pan();
        }
    }

    pub(in crate::compositor::input) fn cancel_remote_canvas_pan(
        &mut self,
        device: Option<&smithay::wayland::virtual_pointer::VirtualPointerDevice>,
    ) {
        if matches!(&self.canvas_pan.owner, Some(PointerSource::Remote(owner)) if device.is_none_or(|device| device == owner))
        {
            self.cancel_canvas_pan();
        }
    }

    fn canvas_pan_target(&self) -> bool {
        if self.output_logical_rects().into_iter().any(|output| {
            output.contains(self.pointer_location.to_i32_round())
                && self.fullscreen_on_output(output)
        }) {
            return false;
        }
        match self.hit_test(self.pointer_location) {
            None => true,
            Some(HitTarget::Client { window_index, .. }) => {
                self.desktop_id(window_index).is_some()
                    && !self.layout_target_on_frame(window_index)
            }
            _ => false,
        }
    }

    fn canvas_pan_available(&self) -> bool {
        self.canvas_pan.selected
            && !self.dispatching_touch
            && !self.touch_input.has_contacts()
            && self.ui_touch.is_none()
            && !self.touch_pointer_pressed
            && !self.desktop_gesture_active()
            && !self.group_has_pointer_capture()
            && self.interaction_allowed(Action::CanvasPan, Source::Local, Phase::Begin)
    }

    pub(in crate::compositor::input) fn begin_canvas_pan(
        &mut self,
        source: PointerSource,
        button: u32,
        time: InputTime,
    ) -> bool {
        if button != 0x110
            || source == PointerSource::Touch
            || !self.canvas_pan_available()
            || !self.canvas_pan_target()
        {
            return false;
        }
        self.interrupt_pinch_zoom();
        self.advance_viewport_animation();
        // Sampling may have moved an app over the previously eligible target.
        if !self.canvas_pan_target() {
            return false;
        }
        self.viewport_animation = None;
        self.shortcuts.cancel_meta_tap();
        self.cancel_pending_search();
        // Match an ordinary outside press before taking the pointer mutex.
        // Pending palettes must be revoked too, and a closing palette must not
        // retain keyboard focus throughout the new drag.
        self.cancel_shell_popup_outside_press(self.pointer_location);
        self.desktop_context_button(button, ButtonState::Pressed);
        let panel_focused = self.focused_shell_panel_index().is_some();
        self.dismiss_shell_panels_outside_pointer();
        if panel_focused {
            self.clear_keyboard_focus();
            self.restore_focus_after_shell_panel();
        }
        // Restoring focus can activate a pending client maximize and revoke
        // Pan. Keep this outside press captured even then: it must not become
        // a click on the application that just appeared beneath the pointer.
        let drag = (self.canvas_pan_available() && self.canvas_pan_target()).then_some(Drag {
            pointer: self.pointer_location,
            offset: self.viewport_offset,
            scale: self.viewport_scale,
        });
        self.canvas_pan.owner = Some(source);
        self.canvas_pan.drag = drag;
        let serial = SERIAL_COUNTER.next_serial();
        let pointer = self.pointer.clone();
        pointer.set_grab(
            self,
            grab::PanGrab {
                start: GrabStartData {
                    focus: None,
                    button,
                    location: self.pointer_location,
                },
            },
            serial,
            Focus::Clear,
        );
        pointer.motion(
            self,
            None,
            &MotionEvent {
                location: self.pointer_location,
                serial,
                time,
            },
        );
        self.deliver_pointer_button(&ButtonEvent {
            button,
            state: ButtonState::Pressed,
            serial,
            time,
        });
        pointer.frame(self);
        self.canvas_pan_cursor();
        self.request_redraw();
        true
    }

    pub(in crate::compositor::input) fn canvas_pan_button(
        &mut self,
        button: u32,
        state: ButtonState,
        time: InputTime,
    ) -> bool {
        if !self.canvas_pan_captured() {
            return false;
        }
        self.deliver_pointer_button(&ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time,
            button,
            state,
        });
        self.pointer.clone().frame(self);
        if !self.canvas_pan_captured() {
            self.apply_pointer_motion(self.pointer_location, time);
        }
        true
    }

    pub(in crate::compositor::input) fn canvas_pan_pointer_motion(
        &mut self,
        time: InputTime,
    ) -> bool {
        if !self.canvas_pan_captured() {
            return false;
        }
        let pointer = self.pointer.clone();
        pointer.motion(
            self,
            None,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
        self.canvas_pan_cursor();
        true
    }

    pub(in crate::compositor::input) fn canvas_pan_hover(&mut self, time: InputTime) -> bool {
        if !self.canvas_pan_available() || !self.canvas_pan_target() {
            return false;
        }
        self.pointer.clone().motion(
            self,
            None,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        self.pointer.clone().frame(self);
        self.canvas_pan_cursor();
        true
    }

    fn canvas_pan_cursor(&mut self) {
        self.cursor_surface = None;
        self.cursor_visible = true;
        self.cursor_icon = if self.canvas_pan.drag.is_some() {
            CursorIcon::Grabbing
        } else if self.canvas_pan.selected {
            CursorIcon::Grab
        } else {
            CursorIcon::Default
        };
    }

    pub(in crate::compositor::input) fn apply_pointer_motion_from_source(
        &mut self,
        source: PointerSource,
        location: Point<f64, Logical>,
        time: InputTime,
    ) {
        if self
            .canvas_pan
            .owner
            .as_ref()
            .is_some_and(|owner| owner != &source)
        {
            return;
        }
        self.apply_pointer_motion(location, time);
    }
}
