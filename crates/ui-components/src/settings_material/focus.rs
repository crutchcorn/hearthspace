//! Focus stays native; only the visible ring follows keyboard versus pointer.
use xilem::masonry::core::{PointerEvent, TextEvent, Update};

/// Bubble from the native control to its owner before restoring native focus.
/// Material decorators observe the origin without consuming the action.
#[derive(Debug, Clone, Copy)]
pub struct RestoreFocus {
    pub keyboard: bool,
}

#[derive(Debug, Default)]
pub struct FocusIndicator {
    pointer_down: bool,
    visible: bool,
    restored_visibility: Option<bool>,
}

impl FocusIndicator {
    pub fn pointer(&mut self, event: &PointerEvent) {
        match event {
            PointerEvent::Down(_) => {
                self.pointer_down = true;
                self.visible = false;
                self.restored_visibility = None;
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => self.pointer_down = false,
            _ => {}
        }
    }

    pub fn text(&mut self, event: &TextEvent) {
        if matches!(event, TextEvent::Keyboard(_)) {
            self.visible = true;
            self.restored_visibility = None;
        }
    }

    /// Prepare the next focus event, or repaint immediately when already focused.
    /// The override is single-use so later Tab navigation keeps its normal ring.
    pub fn restore(&mut self, keyboard: bool, already_focused: bool) {
        self.visible = keyboard;
        self.restored_visibility = (!already_focused).then_some(keyboard);
    }

    pub fn update(&mut self, event: &Update) {
        match event {
            Update::FocusChanged(focused) | Update::ChildFocusChanged(focused) => {
                let restored = self.restored_visibility.take();
                self.visible = *focused && restored.unwrap_or(!self.pointer_down);
            }
            _ => {}
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xilem::masonry::core::keyboard::{Key, NamedKey};

    #[test]
    fn pointer_restore_is_one_shot_and_does_not_hide_later_keyboard_focus() {
        let mut indicator = FocusIndicator::default();
        indicator.restore(false, false);
        indicator.update(&Update::ChildFocusChanged(true));
        assert!(!indicator.visible());
        indicator.update(&Update::ChildFocusChanged(false));
        indicator.update(&Update::ChildFocusChanged(true));
        assert!(
            indicator.visible(),
            "a later focus must not reuse pointer origin"
        );
        indicator.restore(false, false);
        indicator.update(&Update::ChildFocusChanged(false));
        indicator.update(&Update::ChildFocusChanged(true));
        assert!(
            indicator.visible(),
            "an aborted restoration must not hide later focus"
        );
        indicator.restore(false, true);
        assert!(
            !indicator.visible(),
            "already-focused controls also lose the pointer ring"
        );
        indicator.update(&Update::ChildFocusChanged(false));
        indicator.update(&Update::ChildFocusChanged(true));
        assert!(
            indicator.visible(),
            "same-target restoration must not arm a future override"
        );
    }

    #[test]
    fn keyboard_input_supersedes_a_pending_pointer_restore() {
        let mut indicator = FocusIndicator::default();
        indicator.restore(false, false);
        indicator.text(&TextEvent::key_down(Key::Named(NamedKey::Tab)));
        indicator.update(&Update::ChildFocusChanged(true));
        assert!(indicator.visible());
        indicator.restore(true, false);
        indicator.update(&Update::ChildFocusChanged(true));
        assert!(
            indicator.visible(),
            "keyboard/AT restoration remains visible"
        );
    }
}
