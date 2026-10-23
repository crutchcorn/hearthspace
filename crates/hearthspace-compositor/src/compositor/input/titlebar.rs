//! Compositor decorations use stable window identities and release activation.
use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::MotionEvent,
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
};

use crate::compositor::{App, HitTarget, windows::TitlebarButton};

#[derive(Clone, Copy, Debug, PartialEq)]
struct ControlTarget {
    window_id: u64,
    button: TitlebarButton,
    bounds: Rectangle<f64, Logical>,
}

#[derive(Clone, Copy)]
struct TitleClick {
    window_id: u64,
    point: Point<f64, Logical>,
    time: InputTime,
    moved: bool,
}

#[derive(Default)]
pub(in crate::compositor) struct TitlebarInput {
    hovered: Option<ControlTarget>,
    hovered_window: Option<u64>,
    pressed: Option<ControlTarget>,
    held: bool,
    title_held: bool,
    title_press: Option<TitleClick>,
    last_click: Option<TitleClick>,
}

impl TitlebarInput {
    fn press(&mut self, target: ControlTarget) {
        self.pressed = Some(target);
        self.held = true;
        self.title_held = false;
        self.last_click = None;
        self.title_press = None;
    }

    fn cancel(&mut self) {
        self.pressed = None;
        self.title_press = None;
        self.last_click = None;
        self.hovered = None;
        self.hovered_window = None;
        // Keep ownership until release: the client never received our press.
    }

    fn release(&mut self, target: Option<ControlTarget>) -> Option<ControlTarget> {
        self.held = false;
        self.pressed.take().filter(|press| Some(*press) == target)
    }

    fn motion(&mut self, point: Point<f64, Logical>) {
        if let Some(press) = &mut self.title_press {
            press.moved |= distance_squared(press.point, point) > 25.0;
        }
    }

    fn title_down(&mut self, window_id: u64, point: Point<f64, Logical>, time: InputTime) -> bool {
        let double = self.last_click.take().is_some_and(|last| {
            last.window_id == window_id
                && time
                    .micros()
                    .checked_sub(last.time.micros())
                    .is_some_and(|elapsed| elapsed <= 400_000)
                && distance_squared(last.point, point) <= 25.0
        });
        self.title_press = (!double).then_some(TitleClick {
            window_id,
            point,
            time,
            moved: false,
        });
        self.title_held = !double;
        if double {
            self.held = true;
        }
        double
    }

    fn title_up(&mut self, time: InputTime) {
        self.title_held = false;
        self.last_click = self
            .title_press
            .take()
            .filter(|press| !press.moved)
            .map(|mut press| {
                press.time = time;
                press
            });
    }
}

fn distance_squared(a: Point<f64, Logical>, b: Point<f64, Logical>) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

impl App {
    fn chrome_pointer_target(&self) -> Option<HitTarget> {
        if self.session_locked()
            || self.overview_active()
            || self.window_switcher_active()
            || self.pointer.is_grabbed()
            || self.group_pointer_visible()
            || self.drag.is_some()
            || self.resize.is_some()
        {
            return None;
        }
        self.hit_test(self.pointer_location)
    }

    fn titlebar_control_at_pointer(&self) -> Option<ControlTarget> {
        let HitTarget::TitlebarButton {
            window_index,
            button,
        } = self.chrome_pointer_target()?
        else {
            return None;
        };
        Some(ControlTarget {
            window_id: self.windows[window_index].id,
            button,
            bounds: self.titlebar_button_screen_rect(window_index, button),
        })
    }

    pub(in crate::compositor) fn hovered_normal_window_id(&self) -> Option<u64> {
        let index = match self.chrome_pointer_target()? {
            HitTarget::Client { window_index, .. }
            | HitTarget::TitlebarButton { window_index, .. }
            | HitTarget::TitleBar { window_index }
            | HitTarget::ResizeBorder { window_index, .. } => window_index,
        };
        let id = self.windows[index].id;
        (self.is_navigable_window(index)
            && (!self.titlebar_input.held
                || self
                    .titlebar_input
                    .pressed
                    .is_some_and(|press| press.window_id == id)))
        .then_some(id)
    }

    pub(in crate::compositor) fn titlebar_button_state(
        &self,
        window_id: u64,
    ) -> (Option<TitlebarButton>, Option<TitlebarButton>) {
        let hovered = self.titlebar_control_at_pointer().filter(|target| {
            target.window_id == window_id
                && (!self.titlebar_input.held || Some(*target) == self.titlebar_input.pressed)
        });
        let pressed = hovered.filter(|target| Some(*target) == self.titlebar_input.pressed);
        (
            hovered.map(|target| target.button),
            pressed.map(|target| target.button),
        )
    }

    pub(super) fn refresh_titlebar_hover(&mut self) {
        let hovered = self.titlebar_control_at_pointer();
        let window = self.hovered_normal_window_id();
        if self.titlebar_input.hovered != hovered || self.titlebar_input.hovered_window != window {
            self.titlebar_input.hovered = hovered;
            self.titlebar_input.hovered_window = window;
            self.request_redraw();
        }
    }

    pub(super) fn titlebar_pointer_motion(&mut self) -> bool {
        self.titlebar_input.motion(self.raw_pointer_location);
        self.refresh_titlebar_hover();
        self.titlebar_input.held
    }

    pub(super) fn titlebar_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
        time: InputTime,
    ) -> bool {
        if button != 0x110 {
            return false;
        }
        if state == ButtonState::Released && self.titlebar_input.held {
            let target = self.titlebar_control_at_pointer();
            let activate = self.titlebar_input.release(target);
            self.request_redraw();
            if let Some(target) = activate {
                self.activate_titlebar_button(target);
            }
            return true;
        }
        if state == ButtonState::Released && self.titlebar_input.title_held {
            self.titlebar_input.title_up(time);
            self.drag = None;
            self.request_redraw();
            return true;
        }
        if state != ButtonState::Pressed || self.pointer.is_grabbed() {
            return false;
        }
        let Some(target) = self.titlebar_control_at_pointer() else {
            if !matches!(
                self.hit_test(self.pointer_location),
                Some(HitTarget::TitleBar { .. })
            ) {
                self.titlebar_input.last_click = None;
            }
            return false;
        };
        self.titlebar_input.press(target);
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
        self.cursor_surface = None;
        self.cancel_group_window_motion(target.window_id);
        if let Some(index) = self
            .windows
            .iter()
            .position(|window| window.id == target.window_id)
        {
            let index = self.raise_window(index);
            if let Some(surface) = self.windows[index].surface.wl_surface() {
                self.set_keyboard_focus_to_window(index, surface);
            }
        }
        self.request_redraw();
        true
    }

    fn activate_titlebar_button(&mut self, target: ControlTarget) {
        let Some(index) = self
            .windows
            .iter()
            .position(|window| window.id == target.window_id)
        else {
            return;
        };
        match target.button {
            TitlebarButton::Close => {
                if !self.request_pinned_window_close(target.window_id) {
                    self.windows[index].surface.send_close();
                }
            }
            TitlebarButton::Maximize => {
                if let Err(error) = self.toggle_window_focus_view(target.window_id) {
                    tracing::debug!(window_id = target.window_id, %error, "titlebar focused view was unavailable");
                }
            }
        }
    }

    fn toggle_titlebar_maximize(&mut self, id: u64) {
        if let Err(error) = self.toggle_window_canvas_maximize(id) {
            tracing::debug!(window_id = id, %error, "titlebar maximize was unavailable");
        }
    }

    pub(super) fn titlebar_title_press(&mut self, id: u64, time: InputTime) -> bool {
        if self
            .titlebar_input
            .title_down(id, self.raw_pointer_location, time)
        {
            self.drag = None;
            self.toggle_titlebar_maximize(id);
            true
        } else {
            false
        }
    }

    pub(in crate::compositor) fn cancel_titlebar_pointer(&mut self) {
        self.titlebar_input.cancel();
        self.request_redraw();
    }

    pub(super) fn reset_titlebar_input(&mut self) {
        self.titlebar_input = TitlebarInput::default();
        self.request_redraw();
    }
}

#[cfg(test)]
mod tests;
