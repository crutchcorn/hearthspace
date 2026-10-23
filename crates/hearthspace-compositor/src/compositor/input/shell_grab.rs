//! Retain a shell control's click owner while its anchored native surface resizes.
use crate::compositor::App;
use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, ClickGrab, Focus, GestureHoldBeginEvent, GestureHoldEndEvent,
        GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
        GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData,
        MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
};

mod pending;

impl App {
    pub(super) fn track_shell_pointer_grab(&mut self) {
        let pointer = self.pointer.clone();
        let Some((serial, start)) = pointer
            .with_grab(|serial, grab| {
                grab.is::<ClickGrab<App>>()
                    .then(|| (serial, grab.start_data().clone()))
            })
            .flatten()
        else {
            return;
        };
        let Some((surface, origin)) = start.focus.as_ref() else {
            return;
        };
        let Some(index) = self.window_index_for_surface(surface) else {
            return;
        };
        if self.frame_widget_identity(index).is_none() && self.shell_corner(index).is_none() {
            return;
        }
        let Some(local) = self.shell_grab_local(index, start.location) else {
            return;
        };
        // Smithay's hit-test supplied the exact captured subsurface origin.
        // Preserve that offset rather than hit-testing a different child later.
        let child_offset = local - (start.location - *origin);
        pointer.set_grab(
            self,
            ShellGrab {
                window_id: self.windows[index].id,
                child_offset,
                original_focus: start.focus.clone().unwrap(),
                start,
                pending: None,
            },
            serial,
            Focus::Keep,
        );
    }

    fn shell_grab_local(
        &self,
        index: usize,
        screen: Point<f64, Logical>,
    ) -> Option<Point<f64, Logical>> {
        // Match shell hit-testing, including its integer output origin and any
        // canvas scaling applied to a detached frame widget.
        let origin = self.shell_bar_logical_origin(index)?.to_f64();
        let scale = self.window_render_scale(index).max(0.01);
        Some(((screen.x - origin.x) / scale, (screen.y - origin.y) / scale).into())
    }
}

#[derive(Debug)]
struct ShellGrab {
    window_id: u64,
    child_offset: Point<f64, Logical>,
    start: GrabStartData<App>,
    original_focus: (WlSurface, Point<f64, Logical>),
    pending: Option<pending::Pending>,
}

impl ShellGrab {
    fn focus(
        &self,
        data: &App,
        location: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let (surface, old_origin) = &self.original_focus;
        let origin = data
            .windows
            .iter()
            .position(|window| window.id == self.window_id && window.mapped)
            .and_then(|index| data.shell_grab_local(index, location))
            .map_or(*old_origin, |local| location - (local - self.child_offset));
        Some((surface.clone(), origin))
    }
}

impl PointerGrab<App> for ShellGrab {
    fn motion(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        if let Some(pending) = self.pending.as_mut() {
            pending.motion(event.clone());
            self.flush_pending(data, handle, event.serial, event.time);
            return;
        }
        handle.motion(data, self.focus(data, event.location), event);
    }

    fn relative_motion(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        if self.pending.is_some() {
            return;
        }
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &ButtonEvent,
    ) {
        if let Some(pending) = self.pending.as_mut() {
            pending.button(*event, data.pointer_location);
            self.flush_pending(data, handle, event.serial, event.time);
            return;
        }
        // A resize may happen under a stationary pointer between down and up.
        // Native buttons need the current local coordinates before their release.
        let location = handle.current_location();
        handle.motion(
            data,
            self.focus(data, location),
            &MotionEvent {
                location,
                serial: event.serial,
                time: event.time,
            },
        );
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, data, event.serial, event.time, false);
        }
    }

    fn axis(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        details: AxisFrame,
    ) {
        if self.pending.is_some() {
            return;
        }
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut App, handle: &mut PointerInnerHandle<'_, App>) {
        if self.pending.is_none() {
            handle.frame(data);
        }
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &GestureSwipeBeginEvent,
    ) {
        if self.pending.is_none() {
            handle.gesture_swipe_begin(data, event);
        }
    }
    fn gesture_swipe_update(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &GestureSwipeUpdateEvent,
    ) {
        if self.pending.is_none() {
            handle.gesture_swipe_update(data, event);
        }
    }
    fn gesture_swipe_end(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &GestureSwipeEndEvent,
    ) {
        if self.pending.is_none() {
            handle.gesture_swipe_end(data, event);
        }
    }
    fn gesture_pinch_begin(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &GesturePinchBeginEvent,
    ) {
        if self.pending.is_none() {
            handle.gesture_pinch_begin(data, event);
        }
    }
    fn gesture_pinch_update(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &GesturePinchUpdateEvent,
    ) {
        if self.pending.is_none() {
            handle.gesture_pinch_update(data, event);
        }
    }
    fn gesture_pinch_end(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &GesturePinchEndEvent,
    ) {
        if self.pending.is_none() {
            handle.gesture_pinch_end(data, event);
        }
    }
    fn gesture_hold_begin(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &GestureHoldBeginEvent,
    ) {
        if self.pending.is_none() {
            handle.gesture_hold_begin(data, event);
        }
    }
    fn gesture_hold_end(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &GestureHoldEndEvent,
    ) {
        if self.pending.is_none() {
            handle.gesture_hold_end(data, event);
        }
    }

    fn start_data(&self) -> &GrabStartData<App> {
        &self.start
    }

    fn unset(&mut self, _data: &mut App) {}
}
