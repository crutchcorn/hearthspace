//! No surface owns this grab: teardown cannot redirect a held release.
use crate::compositor::App;
use smithay::{
    backend::input::ButtonState,
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
        GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData, MotionEvent, PointerGrab,
        PointerInnerHandle, RelativeMotionEvent,
    },
    utils::{Logical, Point},
};
use wayland_server::protocol::wl_surface::WlSurface;

pub(super) struct PanGrab {
    pub(super) start: GrabStartData<App>,
}

impl PointerGrab<App> for PanGrab {
    fn motion(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);
        if let Some(drag) = &data.canvas_pan.drag {
            let offset = drag.offset_at(event.location);
            if data.viewport_offset != offset {
                data.viewport_offset = offset;
                data.viewport_animation = None;
                data.request_redraw();
            }
        }
    }

    fn button(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        event: &ButtonEvent,
    ) {
        if event.state == ButtonState::Released && event.button == self.start.button {
            data.cancel_canvas_pan();
        }
        // The initiating source can release before other sources/buttons. Keep
        // every remaining event captured until Smithay's aggregate drains.
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, data, event.serial, event.time, false);
        }
    }

    fn relative_motion(
        &mut self,
        _data: &mut App,
        _handle: &mut PointerInnerHandle<'_, App>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        _event: &RelativeMotionEvent,
    ) {
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
        data.canvas_pan.drag = None;
        data.canvas_pan.owner = None;
        data.canvas_pan_cursor();
        data.request_redraw();
    }
}
