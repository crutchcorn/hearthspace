//! Focus stays native; only the visible ring follows keyboard versus pointer.
use xilem::masonry::core::{PointerEvent, TextEvent, Update};

#[derive(Debug, Default)]
pub struct FocusIndicator {
    pointer_down: bool,
    visible: bool,
}

impl FocusIndicator {
    pub fn pointer(&mut self, event: &PointerEvent) {
        match event {
            PointerEvent::Down(_) => {
                self.pointer_down = true;
                self.visible = false;
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => self.pointer_down = false,
            _ => {}
        }
    }

    pub fn text(&mut self, event: &TextEvent) {
        if matches!(event, TextEvent::Keyboard(_)) {
            self.visible = true;
        }
    }

    pub fn update(&mut self, event: &Update) {
        match event {
            Update::FocusChanged(focused) | Update::ChildFocusChanged(focused) => {
                self.visible = *focused && !self.pointer_down;
            }
            _ => {}
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }
}
