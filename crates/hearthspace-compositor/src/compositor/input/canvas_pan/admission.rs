//! Qualify only a fresh press; modifiers never govern an admitted drag's lifetime.
use smithay::{backend::input::InputTime, input::keyboard::ModifiersState};

use super::{App, PointerSource};
use crate::compositor::{
    HitTarget,
    input::policy::{Action, Phase, Source},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Admission {
    SelectedTool,
    Meta,
}

fn meta_pan(modifiers: &ModifiersState) -> bool {
    modifiers.logo && !modifiers.ctrl && !modifiers.alt && !modifiers.shift
}

impl App {
    pub(super) fn canvas_pan_admission(&self) -> Option<Admission> {
        if self.canvas_pan.selected {
            Some(Admission::SelectedTool)
        } else {
            meta_pan(&self.keyboard.modifier_state()).then_some(Admission::Meta)
        }
    }

    pub(super) fn canvas_pan_target(&self, admission: Admission) -> bool {
        if self.output_logical_rects().into_iter().any(|output| {
            output.contains(self.pointer_location.to_i32_round())
                && self.fullscreen_on_output(output)
        }) {
            return false;
        }
        match self.hit_test(self.pointer_location) {
            None => admission == Admission::SelectedTool || !self.group_action_under_pointer(),
            Some(HitTarget::Client { window_index, .. }) => {
                admission == Admission::SelectedTool
                    && self.desktop_id(window_index).is_some()
                    && !self.layout_target_on_frame(window_index)
            }
            _ => false,
        }
    }

    pub(super) fn canvas_pan_available(&self, source: &PointerSource) -> bool {
        let source = match source {
            PointerSource::Local => Source::Local,
            PointerSource::Remote(_) => Source::Remote,
            PointerSource::Touch => Source::Touch,
        };
        !self.dispatching_touch
            && !self.touch_input.has_contacts()
            && self.ui_touch.is_none()
            && !self.touch_pointer_pressed
            && !self.desktop_gesture_active()
            && !self.group_has_pointer_capture()
            && self.interaction_allowed(Action::CanvasPan, source, Phase::Begin)
    }

    /// Only call after keyboard dispatch returns. Focus callbacks can hold its
    /// mutex; revocation there must remain state-only.
    pub(in crate::compositor::input) fn refresh_canvas_pan_modifiers(
        &mut self,
        previous: &ModifiersState,
        time: InputTime,
    ) {
        if !self.canvas_pan.selected
            && meta_pan(previous) != meta_pan(&self.keyboard.modifier_state())
            && !self.has_pressed_pointer_sources()
            && self.canvas_pan_available(&PointerSource::Local)
            && self.hit_test(self.pointer_location).is_none()
        {
            // This also restores the selected tool's cursor on Meta release.
            self.apply_pointer_motion(self.pointer_location, time);
            self.request_redraw();
        }
    }
}
