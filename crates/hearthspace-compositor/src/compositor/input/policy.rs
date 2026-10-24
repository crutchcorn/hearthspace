//! Admission is separate from sequence ownership: Smithay grabs still drain releases.
use crate::compositor::{App, switcher::SwitcherSource};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::compositor) enum Action {
    Navigation,
    SwitcherGesture,
    CanvasGesture,
    TouchRecognition { group_suppressed: bool },
    ClientWindowOperation,
    Input,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::compositor) enum Source {
    Local,
    Touch,
    Remote,
    Client,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::compositor) enum Phase {
    Begin,
    Continue,
}

#[derive(Default)]
struct Policy {
    locked: bool,
    search: bool,
    switcher: bool,
    keyboard_switcher: bool,
    layout: bool,
    confirmation: bool,
    keyboard_grab: bool,
    pointer_grab: bool,
    window_operation: bool,
    group: bool,
    focused_view: bool,
    remote_enabled: bool,
    remote_active: bool,
}

impl Policy {
    fn allows(&self, action: Action, source: Source, phase: Phase) -> bool {
        if source == Source::Remote && (!self.remote_enabled || !self.remote_active || self.locked)
        {
            return false;
        }
        let gesture = !self.locked
            && !self.search
            && !self.confirmation
            && !self.window_operation
            && !self.group
            && !self.pointer_grab
            && !self.keyboard_grab;
        match action {
            Action::Navigation => gesture && !self.switcher && !self.layout,
            // Focused view still permits the switcher gesture, while canvas
            // navigation cannot move an output-pinned scene underneath it.
            Action::SwitcherGesture => gesture,
            Action::CanvasGesture => gesture && !self.focused_view,
            Action::ClientWindowOperation => {
                source == Source::Client
                    && phase == Phase::Continue
                    && !self.locked
                    && !self.window_operation
            }
            Action::Input => !self.locked,
            Action::TouchRecognition { group_suppressed } => {
                // Lock-surface touch routing and active switcher recognition
                // retain their existing source-owned paths. An extra contact
                // cancels a tap on a search/editor/keyboard-switcher preview.
                if phase == Phase::Continue {
                    !(self.search || self.keyboard_switcher || self.layout)
                } else {
                    self.locked || self.search || self.switcher || (!group_suppressed && gesture)
                }
            }
        }
    }
}

impl App {
    /// Call outside pointer callbacks: sampling Smithay grab state takes its mutex.
    pub(in crate::compositor) fn interaction_allowed(
        &self,
        action: Action,
        source: Source,
        phase: Phase,
    ) -> bool {
        Policy {
            locked: self.session_locked(),
            search: self.canvas_search_active(),
            switcher: self.window_switcher_active(),
            keyboard_switcher: self.window_switcher_source() == Some(SwitcherSource::Keyboard),
            layout: self.layout_editing(),
            confirmation: self.layout_confirmation_active(),
            keyboard_grab: self.keyboard.is_grabbed(),
            pointer_grab: self.pointer.is_grabbed(),
            window_operation: self.drag.is_some() || self.resize.is_some(),
            group: self.group_interaction_active(),
            focused_view: self.window_focus_view_active(),
            remote_enabled: self.remote_input.enabled,
            remote_active: self.remote_input.active,
        }
        .allows(action, source, phase)
    }
}

#[cfg(test)]
mod tests;
