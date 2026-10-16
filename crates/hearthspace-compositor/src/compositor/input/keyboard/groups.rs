use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Toggle,
    Clear,
    Done,
    Next,
    Previous,
    Select,
    Gather,
}

impl KeyboardShortcuts {
    pub(super) fn filter_group(
        &mut self,
        code: Keycode,
        state: KeyState,
        modifiers: &ModifiersState,
        symbol: u32,
        available: bool,
        selecting: bool,
    ) -> FilterResult<Option<Shortcut>> {
        if state == KeyState::Released || self.intercepted.contains(&code) {
            return self.filter(code, state, modifiers, symbol, false);
        }
        if symbol == keysyms::KEY_Escape {
            self.intercepted.insert(code);
            return FilterResult::Intercept(Some(Shortcut::Group(Action::Clear)));
        }
        if !selecting {
            return self.filter(code, state, modifiers, symbol, available);
        }
        let action = if modifiers.ctrl || modifiers.alt || modifiers.logo {
            shortcut(modifiers, symbol)
        } else {
            match symbol {
                keysyms::KEY_Return | keysyms::KEY_KP_Enter => Some(Shortcut::Group(Action::Done)),
                keysyms::KEY_space => Some(Shortcut::Group(Action::Select)),
                keysyms::KEY_g => Some(Shortcut::Group(Action::Gather)),
                keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab => {
                    Some(Shortcut::Group(if modifiers.shift {
                        Action::Previous
                    } else {
                        Action::Next
                    }))
                }
                _ => None,
            }
        };
        self.intercepted.insert(code);
        FilterResult::Intercept(action)
    }
}

impl App {
    pub(super) fn run_group_shortcut(&mut self, action: Action) {
        match action {
            Action::Clear => self.clear_window_group(),
            Action::Toggle | Action::Done => {
                let _ = self.toggle_group_selection();
            }
            Action::Next | Action::Previous => {
                self.group_keyboard_cycle(action == Action::Previous)
            }
            Action::Select => self.group_keyboard_toggle_target(),
            Action::Gather => {
                let _ = self.start_group_gather();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_keys_remain_paired_after_selection_or_group_is_cleared() {
        let mut shortcuts = KeyboardShortcuts::default();
        let modifiers = ModifiersState::default();
        for (code, symbol, action) in [
            (9, keysyms::KEY_Escape, Action::Clear),
            (65, keysyms::KEY_space, Action::Select),
            (42, keysyms::KEY_g, Action::Gather),
        ] {
            let code = Keycode::from(code as u32);
            assert!(
                matches!(shortcuts.filter_group(code, KeyState::Pressed, &modifiers, symbol, true, true), FilterResult::Intercept(Some(Shortcut::Group(found))) if found == action)
            );
            assert!(matches!(
                shortcuts.filter(code, KeyState::Released, &modifiers, symbol, true),
                FilterResult::Intercept(None)
            ));
        }
    }
    #[test]
    fn selected_group_does_not_capture_application_typing_after_enter() {
        let mut shortcuts = KeyboardShortcuts::default();
        assert!(matches!(
            shortcuts.filter_group(
                38_u32.into(),
                KeyState::Pressed,
                &ModifiersState::default(),
                keysyms::KEY_a,
                true,
                false
            ),
            FilterResult::Forward
        ));
    }
}
