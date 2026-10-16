use std::time::Instant;

use smithay::backend::input::ButtonState;

use super::{App, Source};

impl App {
    pub(in crate::compositor) fn favorite_pointer_removed(&mut self) {
        if self.favorite_drawing.pointer_held {
            self.favorite_drawing.cancel();
            self.favorite_drawing.pointer_held = false;
            self.flush_favorite_events();
        }
    }

    pub(in crate::compositor) fn favorite_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
    ) -> bool {
        self.maintain_favorite_drawing();
        if button != 0x110 {
            if state == ButtonState::Pressed {
                self.favorite_drawing.cancel();
                self.flush_favorite_events();
            }
            return false;
        }
        if state == ButtonState::Released && self.favorite_drawing.pointer_held {
            self.favorite_drawing.pointer_held = false;
            self.favorite_drawing
                .end(&Source::Pointer, false, Instant::now());
            self.flush_favorite_events();
            return true;
        }
        if state != ButtonState::Pressed || self.touch_pointer_pressed {
            return false;
        }
        let Some(point) = self.favorite_local(self.pointer_location) else {
            return false;
        };
        self.favorite_drawing.pointer_held = true;
        if self.favorite_drawing.contacts.is_empty() {
            self.favorite_drawing
                .begin(Source::Pointer, point, Instant::now());
        } else {
            self.favorite_drawing.cancel();
        }
        self.flush_favorite_events();
        true
    }

    pub(in crate::compositor) fn favorite_pointer_motion(&mut self) -> bool {
        self.maintain_favorite_drawing();
        if !self.favorite_drawing.pointer_held {
            return false;
        }
        if let Some(point) = self.favorite_local(self.pointer_location) {
            self.favorite_drawing
                .update(&Source::Pointer, point, Instant::now());
        } else {
            self.favorite_drawing.cancel();
        }
        self.flush_favorite_events();
        true
    }
}
