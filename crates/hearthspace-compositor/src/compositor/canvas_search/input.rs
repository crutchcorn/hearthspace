use super::{App, Point, Rect};
use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::MotionEvent,
    utils::SERIAL_COUNTER,
};
fn contains(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.x < r.x + r.width && p.y >= r.y && p.y < r.y + r.height
}
impl App {
    pub(in crate::compositor) fn cancel_canvas_search_touch(&mut self) {
        if self.touch_pointer_pressed
            && let Some((target, _)) = self.canvas_search.buttons.get_mut(&0x110)
        {
            // Keep ownership of the eventual release without allowing a
            // cancelled contact to activate a preview or dismiss the field.
            *target = Some(String::new());
        }
    }

    fn canvas_search_target(&self) -> Option<String> {
        let s = self.canvas_search.session.as_ref()?;
        let p = Point {
            x: self.pointer_location.x,
            y: self.pointer_location.y,
        };
        s.results.iter().find_map(|result| {
            let window = result.window?;
            let (rect, alpha) = self
                .canvas_preview
                .poses
                .get(&window)?
                .current(std::time::Instant::now());
            (alpha > 0.05 && contains(rect, p)).then(|| result.id.clone())
        })
    }
    fn pointer_on_canvas_search_field(&self) -> bool {
        self.canvas_search.session.as_ref().is_some_and(|s| {
            contains(
                s.field,
                Point {
                    x: self.pointer_location.x,
                    y: self.pointer_location.y,
                },
            )
        })
    }
    pub(in crate::compositor) fn canvas_search_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
    ) -> bool {
        if state == ButtonState::Released && self.canvas_search.field_buttons.remove(&button) {
            return false;
        }
        if self.canvas_search_active()
            && (self.pointer_on_canvas_search_field()
                || !self.canvas_search.field_buttons.is_empty())
            && state == ButtonState::Pressed
        {
            self.canvas_search.field_buttons.insert(button);
            return false;
        }
        let point = Point {
            x: self.pointer_location.x,
            y: self.pointer_location.y,
        };
        if state == ButtonState::Released
            && let Some((id, start)) = self.canvas_search.buttons.remove(&button)
        {
            if self.canvas_search_active()
                && button == 0x110
                && id == self.canvas_search_target()
                && (start.x - point.x).hypot(start.y - point.y) < 8.
            {
                if let Some(id) = id {
                    let _ = self.activate_canvas_search(Some(id));
                } else {
                    self.close_canvas_search(true);
                }
            }
            return true;
        }
        if !self.canvas_search_active() {
            if self.canvas_search.mode
                && state == ButtonState::Pressed
                && button == 0x110
                && self.hit_test(self.pointer_location).is_none()
                && self.open_canvas_search(Some(point), None).is_ok()
            {
                self.canvas_search
                    .buttons
                    .insert(button, (Some(String::new()), point));
                return true;
            }
            return false;
        }
        if self.pointer_on_canvas_search_field() {
            return false;
        }
        if state == ButtonState::Pressed {
            let target = self.canvas_search_target();
            self.canvas_search.buttons.insert(button, (target, point));
        }
        true
    }
    pub(in crate::compositor) fn refresh_canvas_search_hover(&mut self) {
        let hovered = if self.pointer_on_canvas_search_field()
            || !self.canvas_search.field_buttons.is_empty()
        {
            None
        } else {
            self.canvas_search_target()
        };
        if hovered != self.canvas_search.hovered {
            self.canvas_search.hovered = hovered;
            self.canvas_search_changed();
        }
    }
    pub(in crate::compositor) fn canvas_search_pointer_motion(&mut self, time: InputTime) -> bool {
        self.refresh_canvas_search_hover();
        if !self.canvas_search_active()
            || self.pointer_on_canvas_search_field()
            || !self.canvas_search.field_buttons.is_empty()
        {
            return false;
        }
        self.cursor_surface = None;
        self.cursor_icon = crate::compositor::cursor::CursorIcon::Default;
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
    pub(in crate::compositor) fn canvas_search_key_action(
        &self,
        modifiers: &smithay::input::keyboard::ModifiersState,
        symbol: u32,
        field_focused: bool,
    ) -> Option<i32> {
        use smithay::input::keyboard::keysyms;
        if self.session_locked() {
            return None;
        }
        if self.canvas_search_active() {
            if symbol == keysyms::KEY_Escape {
                return Some(1);
            }
            if modifiers.ctrl || modifiers.alt || modifiers.logo {
                return None;
            }
            if field_focused && symbol != keysyms::KEY_Escape {
                return None;
            }
            return match symbol {
                keysyms::KEY_Escape => Some(1),
                keysyms::KEY_Up => Some(2),
                keysyms::KEY_Down => Some(3),
                keysyms::KEY_Return | keysyms::KEY_KP_Enter => Some(4),
                _ => None,
            };
        }
        if !self.navigation_available() {
            return None;
        }
        if (modifiers.ctrl || modifiers.logo)
            && !modifiers.alt
            && matches!(symbol, keysyms::KEY_k | keysyms::KEY_K)
        {
            return Some(0);
        }
        let desktop = self
            .keyboard
            .current_focus()
            .and_then(|f| self.window_index_for_surface(f.surface()))
            .is_none_or(|i| self.desktop_id(i).is_some());
        (desktop
            && !modifiers.ctrl
            && !modifiers.logo
            && !modifiers.alt
            && symbol == keysyms::KEY_slash)
            .then_some(0)
    }
    pub(in crate::compositor) fn run_canvas_search_key(&mut self, action: i32) {
        match action {
            0 => {
                let _ = self.open_canvas_search(None, None);
            }
            1 => self.close_canvas_search(true),
            2 => self.step_canvas_search(-1),
            3 => self.step_canvas_search(1),
            4 => {
                let _ = self.activate_canvas_search(None);
            }
            _ => {}
        }
    }
}
