//! Modal overview input never reaches the applications behind its previews.
use std::collections::{HashMap, HashSet};

use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::MotionEvent,
    utils::{Logical, Point, SERIAL_COUNTER},
};

use crate::compositor::{App, cursor::CursorIcon};

type Position = Point<f64, Logical>;

#[derive(Default)]
pub(in crate::compositor) struct OverviewInput {
    buttons: HashMap<u32, (Option<u64>, Position)>,
    switcher_buttons: HashSet<u32>,
}

#[derive(Debug, PartialEq)]
enum Action {
    Forward,
    Consume,
    Select(u64),
    Dismiss,
}

impl OverviewInput {
    fn button(
        &mut self,
        button: u32,
        state: ButtonState,
        active: bool,
        target: Option<u64>,
        location: Position,
    ) -> Action {
        if state == ButtonState::Released
            && let Some((pressed_target, origin)) = self.buttons.remove(&button)
        {
            if active
                && button == 0x110
                && pressed_target == target
                && (location.x - origin.x).hypot(location.y - origin.y) < 8.0
            {
                return target.map_or(Action::Dismiss, Action::Select);
            }
            return Action::Consume;
        }
        if !active {
            return Action::Forward;
        }
        if state == ButtonState::Pressed {
            self.buttons.entry(button).or_insert((target, location));
        }
        Action::Consume
    }

    pub(super) fn clear(&mut self) {
        self.buttons.clear();
        self.switcher_buttons.clear();
    }

    fn consume_switcher_button(&mut self, button: u32, state: ButtonState, active: bool) -> bool {
        if state == ButtonState::Released && self.switcher_buttons.remove(&button) {
            return true;
        }
        if active && state == ButtonState::Pressed {
            self.switcher_buttons.insert(button);
        }
        active
    }
}

impl App {
    pub(super) fn handle_overview_button(&mut self, button: u32, state: ButtonState) -> bool {
        let switching = self.window_switcher_active();
        if self
            .overview_input
            .consume_switcher_button(button, state, switching)
        {
            return true;
        }
        let target = self.overview_window_at(self.pointer_location);
        let active = self.overview_active();
        match self
            .overview_input
            .button(button, state, active, target, self.pointer_location)
        {
            Action::Forward => false,
            Action::Consume => true,
            Action::Dismiss => {
                self.close_overview();
                true
            }
            Action::Select(id) => {
                if let Err(error) = self.select_overview_window(id) {
                    tracing::debug!(%error, "overview selection became unavailable");
                }
                true
            }
        }
    }

    pub(super) fn overview_pointer_motion(&mut self, time: InputTime) -> bool {
        if !self.overview_active() && !self.window_switcher_active() {
            return false;
        }
        if self.overview_active() {
            self.overview_hover(self.pointer_location);
        }
        self.cursor_icon = CursorIcon::Default;
        self.cursor_surface = None;
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
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switcher_consumes_buttons_until_release_even_after_alt_or_fingers_lift() {
        let mut input = OverviewInput::default();
        assert!(input.consume_switcher_button(0x110, ButtonState::Pressed, true));
        assert!(input.consume_switcher_button(0x110, ButtonState::Released, false));
        assert!(!input.consume_switcher_button(0x110, ButtonState::Pressed, false));
        assert!(input.consume_switcher_button(0x111, ButtonState::Released, true));
        input.clear();
        assert!(!input.consume_switcher_button(0x111, ButtonState::Released, false));
    }

    #[test]
    fn selection_requires_a_matching_click_and_never_leaks_its_release() {
        let mut input = OverviewInput::default();
        let point = (100.0, 100.0).into();
        assert_eq!(
            input.button(0x110, ButtonState::Pressed, true, Some(1), point),
            Action::Consume
        );
        assert_eq!(
            input.button(0x110, ButtonState::Released, true, Some(1), point),
            Action::Select(1)
        );
        input.button(0x110, ButtonState::Pressed, true, Some(1), point);
        assert_eq!(
            input.button(0x110, ButtonState::Released, false, Some(1), point),
            Action::Consume
        );
        assert_eq!(
            input.button(0x110, ButtonState::Pressed, false, Some(1), point),
            Action::Forward
        );
    }

    #[test]
    fn drag_other_buttons_and_changed_targets_do_not_activate() {
        for (button, target, point) in [
            (0x110, Some(2), (100.0, 100.0)),
            (0x110, Some(1), (120.0, 100.0)),
            (0x111, Some(1), (100.0, 100.0)),
        ] {
            let mut input = OverviewInput::default();
            input.button(
                button,
                ButtonState::Pressed,
                true,
                Some(1),
                (100.0, 100.0).into(),
            );
            assert_eq!(
                input.button(button, ButtonState::Released, true, target, point.into()),
                Action::Consume
            );
        }
    }

    #[test]
    fn background_click_dismisses_but_unmatched_release_does_not() {
        let mut input = OverviewInput::default();
        let point = (0.0, 0.0).into();
        assert_eq!(
            input.button(0x110, ButtonState::Released, true, None, point),
            Action::Consume
        );
        input.button(0x110, ButtonState::Pressed, true, None, point);
        assert_eq!(
            input.button(0x110, ButtonState::Released, true, None, point),
            Action::Dismiss
        );
    }
}
