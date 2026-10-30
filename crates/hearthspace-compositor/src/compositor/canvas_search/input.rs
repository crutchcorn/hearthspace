use super::{App, Point, Rect};
use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::MotionEvent,
    utils::SERIAL_COUNTER,
};
fn contains(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.x < r.x + r.width && p.y >= r.y && p.y < r.y + r.height
}

impl super::Session {
    fn target_at(
        &self,
        preview: &crate::compositor::canvas_preview::State,
        p: Point,
    ) -> Option<String> {
        self.results.iter().find_map(|result| {
            let (rect, alpha) = preview.pose(result.window?)?;
            (alpha > 0.05 && contains(rect, p)).then(|| result.id.clone())
        })
    }
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
        s.target_at(&self.canvas_preview, p)
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
        None
    }
    pub(in crate::compositor) fn run_canvas_search_key(&mut self, action: i32) {
        match action {
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

#[cfg(test)]
mod sampling_tests {
    use super::*;
    use crate::compositor::canvas_preview::test_support::held_preview;

    fn session() -> super::super::Session {
        super::super::Session {
            id: 1,
            output: None,
            view: Rect::default(),
            anchor: Point { x: 0., y: 0. },
            query: String::new(),
            field: Rect::default(),
            above: false,
            selected: None,
            results: vec![super::super::ResultItem {
                id: "window:7".into(),
                title: "Preview".into(),
                kind: "window",
                window: Some(7),
                natural: Rect::default(),
                search_text: String::new(),
            }],
            previous_focus: None,
            signature: None,
        }
    }

    #[test]
    fn search_selection_holds_fractional_frame_bounds_after_wall_time_advances() {
        let session = session();
        let (mut preview, now) = held_preview();
        let (bounds, _) = preview.pose(7).unwrap();
        let p = Point {
            x: bounds.x + 0.001,
            y: bounds.y + bounds.height / 2.,
        };
        assert!(!contains(preview.poses[&7].current(now).0, p));
        assert_eq!(session.target_at(&preview, p).as_deref(), Some("window:7"));
        assert_eq!(
            session.target_at(
                &preview,
                Point {
                    x: bounds.x - 0.001,
                    ..p
                }
            ),
            None
        );
        assert_eq!(
            session
                .target_at(
                    &preview,
                    Point {
                        x: bounds.x + bounds.width - 0.001,
                        ..p
                    }
                )
                .as_deref(),
            Some("window:7")
        );
        assert_eq!(
            session.target_at(
                &preview,
                Point {
                    x: bounds.x + bounds.width,
                    ..p
                }
            ),
            None
        );
        preview.advance(now, true);
        assert_eq!(session.target_at(&preview, p), None);
        let endpoint = preview.pose(7).unwrap().0;
        assert_eq!(
            session
                .target_at(
                    &preview,
                    Point {
                        x: endpoint.x + 0.001,
                        y: endpoint.y + 0.001
                    }
                )
                .as_deref(),
            Some("window:7")
        );
    }

    #[test]
    fn search_selection_uses_sampled_opacity_and_skips_missing_poses() {
        let mut session = session();
        let mut missing = session.results[0].clone();
        missing.window = Some(99);
        missing.id = "missing".into();
        session.results.insert(0, missing);
        let (mut preview, now) = held_preview();
        let pose = preview.poses.get_mut(&7).unwrap();
        pose.from = pose.to;
        pose.from_opacity = 0.;
        pose.to_opacity = 1.;
        let started = pose.started;
        let p = Point {
            x: pose.to.x + 0.001,
            y: pose.to.y + 0.001,
        };
        preview.advance(started, true);
        assert_eq!(preview.poses[&7].current(now).1, 1.);
        assert_eq!(session.target_at(&preview, p), None);
        preview.advance(now, true);
        assert_eq!(session.target_at(&preview, p).as_deref(), Some("window:7"));
    }
}
