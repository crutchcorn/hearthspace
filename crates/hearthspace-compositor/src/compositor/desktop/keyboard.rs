use super::App;
use smithay::{
    backend::input::KeyState,
    input::keyboard::{ModifiersState, keysyms},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::compositor) enum Action {
    Toggle,
    Cancel,
    Cycle(bool),
    Direction(i32, i32),
    Select(bool),
    Activate,
}

impl App {
    pub(in crate::compositor) fn desktop_key_action(
        &self,
        state: KeyState,
        modifiers: &ModifiersState,
        symbol: u32,
    ) -> Option<Action> {
        if state != KeyState::Pressed || self.session_locked() {
            return None;
        }
        if symbol == keysyms::KEY_Escape && self.desktop.gesture.is_some() {
            return Some(Action::Cancel);
        }
        if modifiers.alt || modifiers.logo {
            return None;
        }
        let focused = self
            .keyboard
            .current_focus()
            .and_then(|focus| self.window_index_for_surface(focus.surface()));
        let desktop = focused.is_none_or(|index| self.desktop_id(index).is_some());
        if !desktop && self.desktop.gesture.is_none() {
            return None;
        }
        if symbol == keysyms::KEY_Escape && self.desktop.gesture.is_some() {
            return Some(Action::Cancel);
        }
        if modifiers.ctrl && symbol != keysyms::KEY_space {
            return None;
        }
        match symbol {
            keysyms::KEY_v | keysyms::KEY_V => Some(Action::Toggle),
            keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab => Some(Action::Cycle(
                modifiers.shift || symbol == keysyms::KEY_ISO_Left_Tab,
            )),
            keysyms::KEY_Left if focused.is_some() => Some(Action::Direction(-1, 0)),
            keysyms::KEY_Right if focused.is_some() => Some(Action::Direction(1, 0)),
            keysyms::KEY_Up if focused.is_some() => Some(Action::Direction(0, -1)),
            keysyms::KEY_Down if focused.is_some() => Some(Action::Direction(0, 1)),
            keysyms::KEY_space if focused.is_some() => {
                Some(Action::Select(modifiers.ctrl || modifiers.shift))
            }
            keysyms::KEY_Return | keysyms::KEY_KP_Enter if focused.is_some() => {
                Some(Action::Activate)
            }
            keysyms::KEY_Escape if !self.desktop.selected.is_empty() => Some(Action::Cancel),
            _ => None,
        }
    }

    pub(in crate::compositor) fn run_desktop_key(&mut self, action: Action) {
        match action {
            Action::Toggle => {
                self.disarm_canvas_search();
                self.cancel_desktop_gesture();
                self.desktop.select_mode = true;
                self.clear_window_group();
                self.desktop_changed();
            }
            Action::Cancel => {
                if self.desktop.gesture.is_some() {
                    self.cancel_desktop_gesture();
                } else {
                    self.desktop.selected.clear();
                    self.desktop_changed();
                }
            }
            Action::Cycle(reverse) => {
                let mut items: Vec<_> = self
                    .windows
                    .iter()
                    .enumerate()
                    .filter_map(|(index, window)| {
                        window
                            .mapped
                            .then(|| self.desktop_id(index))
                            .flatten()
                            .map(|id| (window.position, id))
                    })
                    .collect();
                items.sort_by(|(left, a), (right, b)| {
                    (left.x, left.y, a).cmp(&(right.x, right.y, b))
                });
                if items.is_empty() {
                    return;
                }
                let current = items
                    .iter()
                    .position(|(_, id)| Some(id) == self.desktop.focused.as_ref());
                let next = match current {
                    Some(index) if reverse => (index + items.len() - 1) % items.len(),
                    Some(index) => (index + 1) % items.len(),
                    None if reverse => items.len() - 1,
                    None => 0,
                };
                let _ = self.focus_desktop(&items[next].1, true);
            }
            Action::Direction(dx, dy) => {
                let Some(current) = self
                    .desktop
                    .focused
                    .as_deref()
                    .and_then(|id| self.desktop_index(id))
                else {
                    return;
                };
                let from = self.windows[current].position;
                let next = self
                    .windows
                    .iter()
                    .enumerate()
                    .filter_map(|(index, window)| {
                        if !window.mapped || index == current {
                            return None;
                        }
                        let id = self.desktop_id(index)?;
                        let x = i64::from(window.position.x) - i64::from(from.x);
                        let y = i64::from(window.position.y) - i64::from(from.y);
                        let along = x * i64::from(dx) + y * i64::from(dy);
                        let cross = x * i64::from(dy) - y * i64::from(dx);
                        (along > 0).then_some((along + cross.abs() * 4, id))
                    })
                    .min();
                if let Some((_, id)) = next {
                    let _ = self.focus_desktop(&id, true);
                }
            }
            Action::Select(additive) => {
                if let Some(id) = self.desktop.focused.clone() {
                    if additive {
                        if !self.desktop.selected.remove(&id) {
                            self.desktop.selected.insert(id);
                        }
                    } else {
                        self.desktop.selected.clear();
                        self.desktop.selected.insert(id);
                    }
                    self.desktop.keyboard_focus = true;
                    self.desktop_changed();
                }
            }
            Action::Activate => {
                if let Some(id) = self.desktop.focused.clone() {
                    let _ = self.desktop_action("activate", &id);
                }
            }
        }
    }
}
