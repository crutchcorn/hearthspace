//! Modal switcher input never reaches the applications behind its previews.
use std::collections::HashMap;

use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::MotionEvent,
    utils::{Logical, Point, SERIAL_COUNTER},
};

use crate::compositor::{App, cursor::CursorIcon};

type Position = Point<f64, Logical>;

#[derive(Default)]
pub(in crate::compositor) struct SwitcherInput {
    buttons: HashMap<u32, (Option<u64>, Position)>,
}

#[derive(Debug, PartialEq)]
enum Action {
    Forward,
    Consume,
    Select(u64),
    Dismiss,
}

impl SwitcherInput {
    fn button(
        buttons: &mut HashMap<u32, (Option<u64>, Position)>,
        button: u32,
        state: ButtonState,
        active: bool,
        target: Option<u64>,
        location: Position,
    ) -> Action {
        if state == ButtonState::Released
            && let Some((pressed_target, origin)) = buttons.remove(&button)
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
            buttons.entry(button).or_insert((target, location));
        }
        Action::Consume
    }

    pub(super) fn clear(&mut self) {
        self.buttons.clear();
    }
}

impl App {
    pub(in crate::compositor) fn invalidate_window_switcher_tap(&mut self) {
        if self.touch_pointer_pressed
            && let Some((target, _)) = self.switcher_input.buttons.get_mut(&0x110)
        {
            *target = Some(0);
        }
    }

    pub(in crate::compositor) fn cancel_window_switcher_touch(&mut self) {
        self.invalidate_window_switcher_tap();
        if self.touch_input.has_contacts() {
            let actions = self.touch_input.interrupt();
            self.apply_touch_actions(actions, InputTime::now());
        }
    }

    pub(super) fn handle_window_switcher_button(
        &mut self,
        button: u32,
        state: ButtonState,
    ) -> bool {
        let switching = self.window_switcher_active();
        let target = self.window_switcher_window_at(self.pointer_location);
        match SwitcherInput::button(
            &mut self.switcher_input.buttons,
            button,
            state,
            switching,
            target,
            self.pointer_location,
        ) {
            Action::Forward => {}
            Action::Consume => return true,
            Action::Dismiss => {
                self.dismiss_window_switcher(true);
                return true;
            }
            Action::Select(id) => {
                let _ = self.select_window_switcher(id);
                return true;
            }
        }
        false
    }

    pub(super) fn window_switcher_pointer_motion(&mut self, time: InputTime) -> bool {
        if !self.window_switcher_active() {
            return false;
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
    fn selection_requires_a_matching_click_and_never_leaks_its_release() {
        let mut input = SwitcherInput::default();
        let point = (100.0, 100.0).into();
        assert_eq!(
            SwitcherInput::button(
                &mut input.buttons,
                0x110,
                ButtonState::Pressed,
                true,
                Some(1),
                point
            ),
            Action::Consume
        );
        assert_eq!(
            SwitcherInput::button(
                &mut input.buttons,
                0x110,
                ButtonState::Released,
                true,
                Some(1),
                point
            ),
            Action::Select(1)
        );
        SwitcherInput::button(
            &mut input.buttons,
            0x110,
            ButtonState::Pressed,
            true,
            Some(1),
            point,
        );
        assert_eq!(
            SwitcherInput::button(
                &mut input.buttons,
                0x110,
                ButtonState::Released,
                false,
                Some(1),
                point
            ),
            Action::Consume
        );
        assert_eq!(
            SwitcherInput::button(
                &mut input.buttons,
                0x110,
                ButtonState::Pressed,
                false,
                Some(1),
                point
            ),
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
            let mut input = SwitcherInput::default();
            SwitcherInput::button(
                &mut input.buttons,
                button,
                ButtonState::Pressed,
                true,
                Some(1),
                (100.0, 100.0).into(),
            );
            assert_eq!(
                SwitcherInput::button(
                    &mut input.buttons,
                    button,
                    ButtonState::Released,
                    true,
                    target,
                    point.into()
                ),
                Action::Consume
            );
        }
    }

    #[test]
    fn background_click_dismisses_but_unmatched_release_does_not() {
        let mut input = SwitcherInput::default();
        let point = (0.0, 0.0).into();
        assert_eq!(
            SwitcherInput::button(
                &mut input.buttons,
                0x110,
                ButtonState::Released,
                true,
                None,
                point
            ),
            Action::Consume
        );
        SwitcherInput::button(
            &mut input.buttons,
            0x110,
            ButtonState::Pressed,
            true,
            None,
            point,
        );
        assert_eq!(
            SwitcherInput::button(
                &mut input.buttons,
                0x110,
                ButtonState::Released,
                true,
                None,
                point
            ),
            Action::Dismiss
        );
    }
}
