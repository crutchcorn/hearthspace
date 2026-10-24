//! Drain compositor-owned pointer sequences, including after owner retirement.
use std::cell::Cell;

use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::{
        AxisFrame, ButtonEvent, ClickGrab, Focus, GestureHoldBeginEvent, GestureHoldEndEvent,
        GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
        GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData,
        MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
    },
    utils::{Logical, Point, Serial},
};
use wayland_server::protocol::wl_surface::WlSurface;

use crate::compositor::App;

impl App {
    pub(in crate::compositor) fn has_window_pointer_grab(&self) -> bool {
        self.pointer
            .with_grab(|_, grab| grab.is::<WindowGrab>())
            .unwrap_or(false)
    }

    pub(super) fn window_pointer_grab_owner_retired(&self) -> bool {
        self.pointer
            .with_grab(|_, grab| {
                grab.downcast_ref::<WindowGrab>().is_some_and(|grab| {
                    grab.owner_unmapped.get()
                        || !self
                            .windows
                            .iter()
                            .any(|window| window.id == grab.window_id && window.mapped)
                })
            })
            .unwrap_or(false)
    }

    pub(in crate::compositor) fn window_pointer_grab_owner_unmapped(&self, id: u64) {
        self.pointer.with_grab(|_, grab| {
            if let Some(grab) = grab.downcast_ref::<WindowGrab>()
                && grab.window_id == id
            {
                // A remap cannot undo the focus handoff deferred by this grab.
                // Geometry cancellation alone (such as X11 moveresize cancel)
                // does not retire the owner and must not set this latch.
                grab.owner_unmapped.set(true);
            }
        });
    }

    pub(super) fn restore_retired_window_grab_focus(&mut self, owner_retired: bool) {
        if owner_retired && self.keyboard.current_focus().is_none() {
            // Call only after pointer dispatch returns: navigation checks take
            // its mutex. They also guard lock state and any surviving grab.
            self.restore_window_focus();
        }
    }

    /// An implicit grab must survive its modal surface until all buttons drain.
    pub(in crate::compositor) fn retain_retiring_window_pointer_grab(&mut self, id: u64) {
        if self.pressed_pointer_buttons.is_empty() {
            return;
        }
        let Some((serial, start)) = self
            .pointer
            .with_grab(|serial, grab| {
                grab.is::<ClickGrab<App>>()
                    .then(|| (serial, grab.start_data().clone()))
            })
            .flatten()
        else {
            return;
        };
        if !start.focus.as_ref().is_some_and(|(surface, _)| {
            self.window_index_for_surface(surface)
                .is_some_and(|index| self.windows[index].id == id)
        }) {
            return;
        }
        // The initiating button may already be up while a secondary one is
        // held. Retirement must not use the left-button move/resize admission.
        self.install_window_pointer_grab(id, serial, start, true);
    }

    /// Reuse the implicit grab's serial and baseline only for a client request.
    /// Compositor decorations have no client press to take over.
    pub(in crate::compositor) fn start_window_pointer_grab(&mut self, window_index: usize) {
        if !self.pointer_grab_targets_window(window_index) {
            return;
        }
        let pointer = self.pointer.clone();
        let Some((serial, start)) =
            pointer.with_grab(|serial, grab| (serial, grab.start_data().clone()))
        else {
            return;
        };
        self.install_window_pointer_grab(self.windows[window_index].id, serial, start, false);
    }

    fn install_window_pointer_grab(
        &mut self,
        window_id: u64,
        serial: Serial,
        mut start: GrabStartData<App>,
        owner_unmapped: bool,
    ) {
        let pointer = self.pointer.clone();
        // Ownership belongs to the compositor now. Keeping a client surface in
        // start.focus would make Smithay discard the grab on surface teardown,
        // letting an unmatched release escape to the next focused client.
        start.focus = None;
        pointer.set_grab(
            self,
            WindowGrab {
                window_id,
                owner_unmapped: Cell::new(owner_unmapped),
                start,
            },
            serial,
            Focus::Clear,
        );
        // Clear pending focus as well as current focus, even if a reset occurs
        // before the next motion. A later unset must not restore stale content.
        pointer.motion(
            self,
            None,
            &MotionEvent {
                location: self.pointer_location,
                serial,
                time: InputTime::now(),
            },
        );
        pointer.frame(self);
    }
}

struct WindowGrab {
    window_id: u64,
    owner_unmapped: Cell<bool>,
    start: GrabStartData<App>,
}

impl WindowGrab {
    fn finish(&self, data: &mut App) {
        if data
            .drag
            .as_ref()
            .is_some_and(|drag| drag.window_id == self.window_id)
        {
            data.drag = None;
            data.request_redraw();
        }
        if data
            .resize
            .as_ref()
            .is_some_and(|resize| resize.window_id == self.window_id)
        {
            data.finish_resize();
            data.request_redraw();
        }
    }

    fn update(&self, data: &mut App) {
        if !data
            .windows
            .iter()
            .any(|window| window.id == self.window_id && window.mapped)
        {
            self.finish(data);
            return;
        }
        if data
            .drag
            .as_ref()
            .is_some_and(|drag| drag.window_id == self.window_id)
            || data
                .resize
                .as_ref()
                .is_some_and(|resize| resize.window_id == self.window_id)
        {
            data.update_window_pointer_operation();
        }
    }
}

impl PointerGrab<App> for WindowGrab {
    fn motion(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // PointerHandle is mutex-locked throughout every callback. Use only
        // this inner handle; geometry helpers must not re-enter the pointer.
        handle.motion(data, None, event);
        self.update(data);
    }

    fn relative_motion(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        _event: &RelativeMotionEvent,
    ) {
    }

    fn button(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &ButtonEvent,
    ) {
        if event.state == ButtonState::Released && event.button == self.start.button {
            self.update(data);
            self.finish(data);
        }
        // Smithay already updated current_pressed before this callback.
        // The compositor owns the released-focus sequence; do not forward
        // releases. XWayland reconciles held buttons on re-entry.
        // Additional held buttons must drain even after geometry has finished.
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, data, event.serial, event.time, false);
        }
    }

    fn axis(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _details: AxisFrame,
    ) {
    }

    fn frame(&mut self, _data: &mut App, _handle: &mut PointerInnerHandle<'_, App>) {}

    fn gesture_swipe_begin(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _event: &GestureSwipeBeginEvent,
    ) {
    }

    fn gesture_swipe_update(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _event: &GestureSwipeUpdateEvent,
    ) {
    }

    fn gesture_swipe_end(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _event: &GestureSwipeEndEvent,
    ) {
    }

    fn gesture_pinch_begin(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _event: &GesturePinchBeginEvent,
    ) {
    }

    fn gesture_pinch_update(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _event: &GesturePinchUpdateEvent,
    ) {
    }

    fn gesture_pinch_end(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _event: &GesturePinchEndEvent,
    ) {
    }

    fn gesture_hold_begin(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _event: &GestureHoldBeginEvent,
    ) {
    }

    fn gesture_hold_end(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _event: &GestureHoldEndEvent,
    ) {
    }

    fn start_data(&self) -> &GrabStartData<App> {
        &self.start
    }

    fn unset(&mut self, data: &mut App) {
        self.finish(data);
    }
}
