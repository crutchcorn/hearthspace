//! Capture group touch before native UI delivery or delayed pointer emulation.
use super::*;

impl App {
    pub(super) fn begin_group_ui_touch(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Position,
        time: InputTime,
    ) -> bool {
        if !self.group_active() && !self.group_has_pointer_capture() {
            return false;
        }
        let blocked = self.group_touch_blocked();
        if !blocked {
            let was_dispatching = std::mem::replace(&mut self.dispatching_touch, true);
            self.apply_pointer_motion(location, time);
            // An outside press clears the group but stays on the normal UI route.
            // An owned press never enters the shared physical pointer button ledger.
            let owned = self.group_pointer_button(0x110, ButtonState::Pressed);
            if owned || !self.group_active() {
                self.apply_pointer_motion(location, time);
            }
            self.dispatching_touch = was_dispatching;
            if !owned {
                return false;
            }
        }
        self.ui_touch = Some(UiTouch {
            device: device.into(),
            slot,
            window: 0,
            geometry: [0.0; 5],
            output: self.primary_output_logical_rect(),
            origin: location,
            local: location,
            target: if blocked {
                Target::GroupCancelled
            } else {
                Target::Group
            },
        });
        true
    }

    pub(in crate::compositor) fn group_touch_owns_input(&self) -> bool {
        self.ui_touch
            .as_ref()
            .is_some_and(|touch| matches!(touch.target, Target::Group))
    }

    pub(in crate::compositor::input) fn group_touch_suppresses_gestures(&self) -> bool {
        self.ui_touch
            .as_ref()
            .is_some_and(|touch| matches!(touch.target, Target::Group | Target::GroupCancelled))
    }

    pub(super) fn motion_group_ui_touch(&mut self, location: Position, time: InputTime) {
        if !location.x.is_finite() || !location.y.is_finite() {
            self.cancel_ui_touch();
            return;
        }
        let was_dispatching = std::mem::replace(&mut self.dispatching_touch, true);
        self.apply_pointer_motion(location, time);
        self.dispatching_touch = was_dispatching;
    }

    pub(super) fn finish_group_ui_touch(&mut self, time: InputTime) {
        let was_dispatching = std::mem::replace(&mut self.dispatching_touch, true);
        self.group_pointer_button(0x110, ButtonState::Released);
        self.apply_pointer_motion(self.pointer_location, time);
        self.dispatching_touch = was_dispatching;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::input::touch_tracker::TouchTracker;

    fn group_contact() -> UiTouch {
        UiTouch {
            device: "screen".into(),
            slot: Some(0).into(),
            window: 0,
            geometry: [0.0; 5],
            output: Rectangle::new((0, 0).into(), (1280, 800).into()),
            origin: (12.0, 18.0).into(),
            local: (12.0, 18.0).into(),
            target: Target::Group,
        }
    }

    #[test]
    fn group_chord_never_emulates_a_delayed_tap_or_drag_after_raw_capture() {
        for cancel in [false, true] {
            let mut tracker = TouchTracker::default();
            let mut contact = group_contact();
            tracker.down("screen", Some(0).into(), contact.origin);
            assert!(tracker.interrupt().is_empty());
            tracker.motion("screen", Some(0).into(), (300.0, 200.0).into());
            if cancel {
                // A second contact aborts the raw group gesture first; the
                // cancelled UI owner still suppresses every remaining finger.
                assert!(!contact.cancel());
                tracker.down("screen", Some(1).into(), (400.0, 300.0).into());
            }
            assert!(tracker.frame_all().is_empty());
            tracker.up("screen", Some(0).into());
            if cancel {
                tracker.motion("screen", Some(1).into(), (450.0, 350.0).into());
                tracker.up("screen", Some(1).into());
            }
            assert!(tracker.frame_all().is_empty());
            assert!(!tracker.has_contacts());
            for action in [
                TouchAction::Hover(contact.origin),
                TouchAction::Press(contact.origin),
                TouchAction::Motion(contact.origin),
                TouchAction::Tap(contact.origin),
                TouchAction::Release,
            ] {
                assert!(contact.suppresses_pointer("screen", &action));
            }
        }
    }

    #[test]
    fn reset_and_device_removal_cannot_finish_a_captured_lasso_as_a_click() {
        for remove in [false, true] {
            let mut tracker = TouchTracker::default();
            tracker.down("screen", Some(0).into(), (12.0, 18.0).into());
            tracker.interrupt();
            tracker.motion("screen", Some(0).into(), (500.0, 400.0).into());
            let cleanup = if remove {
                tracker.remove_device("screen")
            } else {
                tracker.reset()
            };
            assert!(cleanup.is_empty());
            tracker.up("screen", Some(0).into());
            assert!(tracker.frame_all().is_empty());
        }
    }
}
