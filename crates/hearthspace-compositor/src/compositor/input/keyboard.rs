use std::collections::HashSet;

use hearthspace_shell::{ShellCommand, SpawnTarget};
use smithay::{
    backend::input::{InputTime, KeyState},
    input::keyboard::{FilterResult, Keycode, ModifiersState, keysyms},
    utils::SERIAL_COUNTER,
};

use crate::compositor::{App, idle::ActivityReason, shell_integration::ControlReply};

#[derive(Default)]
pub(in crate::compositor) struct KeyboardShortcuts {
    intercepted: HashSet<Keycode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shortcut {
    Terminal,
    Launcher,
    Next,
    Previous,
    Close,
    Left,
    Right,
    Up,
    Down,
    Reset,
}

impl KeyboardShortcuts {
    fn filter(
        &mut self,
        code: Keycode,
        state: KeyState,
        modifiers: &ModifiersState,
        symbol: u32,
        available: bool,
    ) -> FilterResult<Option<Shortcut>> {
        if state == KeyState::Released {
            return if self.intercepted.remove(&code) {
                FilterResult::Intercept(None)
            } else {
                FilterResult::Forward
            };
        }
        if self.intercepted.contains(&code) {
            return FilterResult::Intercept(None);
        }
        let action = available.then(|| shortcut(modifiers, symbol)).flatten();
        if let Some(action) = action {
            self.intercepted.insert(code);
            FilterResult::Intercept(Some(action))
        } else {
            FilterResult::Forward
        }
    }
}

impl App {
    /// Real and agent keyboard input use the same policy. Intercepted releases
    /// stay intercepted even if the modifier was released first.
    pub(super) fn handle_keyboard_input(
        &mut self,
        code: Keycode,
        state: KeyState,
        time: InputTime,
    ) {
        let keyboard = self.keyboard.clone();
        let available = self.navigation_available();
        let intercepted = keyboard.input(
            self,
            code,
            state,
            SERIAL_COUNTER.next_serial(),
            time,
            |app, modifiers, key| {
                let symbol = key
                    .raw_latin_sym_or_raw_current_sym()
                    .map(|symbol| symbol.raw())
                    .unwrap_or(0);
                app.shortcuts
                    .filter(code, state, modifiers, symbol, available)
            },
        );
        match intercepted {
            Some(Some(shortcut)) => self.run_shortcut(shortcut),
            Some(None) => {}
            None => self.record_focused_client_activity(ActivityReason::ClientInput),
        }
    }

    fn run_shortcut(&mut self, shortcut: Shortcut) {
        let command = match shortcut {
            Shortcut::Terminal => ShellCommand::Spawn(SpawnTarget::Foot),
            Shortcut::Launcher => ShellCommand::FocusLauncher,
            Shortcut::Next => ShellCommand::FocusNextWindow,
            Shortcut::Previous => ShellCommand::FocusPreviousWindow,
            Shortcut::Close => {
                let Some(id) = self.focused_normal_window_id else {
                    return;
                };
                ShellCommand::CloseWindow(id)
            }
            Shortcut::Left => ShellCommand::PanLeft,
            Shortcut::Right => ShellCommand::PanRight,
            Shortcut::Up => ShellCommand::PanUp,
            Shortcut::Down => ShellCommand::PanDown,
            Shortcut::Reset => ShellCommand::ResetView,
        };
        if let ControlReply::Err(error) = self.run_control_action(command) {
            tracing::warn!(%error, ?shortcut, "keyboard action failed");
        }
    }
}

fn shortcut(modifiers: &ModifiersState, symbol: u32) -> Option<Shortcut> {
    if modifiers.ctrl {
        return None;
    }
    if modifiers.alt && !modifiers.logo {
        return match symbol {
            keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab => Some(if modifiers.shift {
                Shortcut::Previous
            } else {
                Shortcut::Next
            }),
            keysyms::KEY_F4 if !modifiers.shift => Some(Shortcut::Close),
            _ => None,
        };
    }
    if !modifiers.logo || modifiers.alt || modifiers.shift {
        return None;
    }
    match symbol {
        keysyms::KEY_Return => Some(Shortcut::Terminal),
        keysyms::KEY_space => Some(Shortcut::Launcher),
        keysyms::KEY_Left => Some(Shortcut::Left),
        keysyms::KEY_Right => Some(Shortcut::Right),
        keysyms::KEY_Up => Some(Shortcut::Up),
        keysyms::KEY_Down => Some(Shortcut::Down),
        keysyms::KEY_0 => Some(Shortcut::Reset),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_respect_modifiers_and_leave_application_keys_alone() {
        let mut mods = ModifiersState::default();
        assert_eq!(shortcut(&mods, keysyms::KEY_Return), None);
        mods.logo = true;
        assert_eq!(
            shortcut(&mods, keysyms::KEY_Return),
            Some(Shortcut::Terminal)
        );
        assert_eq!(shortcut(&mods, keysyms::KEY_0), Some(Shortcut::Reset));
        assert_eq!(shortcut(&mods, keysyms::KEY_a), None);
        mods.ctrl = true;
        assert_eq!(shortcut(&mods, keysyms::KEY_Return), None);
        mods = ModifiersState {
            alt: true,
            ..Default::default()
        };
        assert_eq!(shortcut(&mods, keysyms::KEY_Tab), Some(Shortcut::Next));
        mods.shift = true;
        assert_eq!(shortcut(&mods, keysyms::KEY_Tab), Some(Shortcut::Previous));
    }

    #[test]
    fn release_is_consumed_after_modifier_released_and_repeats_do_not_retrigger() {
        let mut shortcuts = KeyboardShortcuts::default();
        let mods = ModifiersState {
            logo: true,
            ..Default::default()
        };
        let key = 36_u32.into();
        assert!(matches!(
            shortcuts.filter(key, KeyState::Pressed, &mods, keysyms::KEY_Return, true),
            FilterResult::Intercept(Some(Shortcut::Terminal))
        ));
        assert!(matches!(
            shortcuts.filter(key, KeyState::Pressed, &mods, keysyms::KEY_Return, true),
            FilterResult::Intercept(None)
        ));
        assert!(matches!(
            shortcuts.filter(
                key,
                KeyState::Released,
                &ModifiersState::default(),
                keysyms::KEY_Return,
                false
            ),
            FilterResult::Intercept(None)
        ));
        assert!(matches!(
            shortcuts.filter(
                key,
                KeyState::Pressed,
                &ModifiersState::default(),
                keysyms::KEY_Return,
                true
            ),
            FilterResult::Forward
        ));
    }

    #[test]
    fn popup_grabs_keep_shortcuts_and_their_releases() {
        let mut shortcuts = KeyboardShortcuts::default();
        let mods = ModifiersState {
            alt: true,
            ..Default::default()
        };
        assert!(matches!(
            shortcuts.filter(
                23_u32.into(),
                KeyState::Pressed,
                &mods,
                keysyms::KEY_Tab,
                false
            ),
            FilterResult::Forward
        ));
        assert!(matches!(
            shortcuts.filter(
                23_u32.into(),
                KeyState::Released,
                &mods,
                keysyms::KEY_Tab,
                true
            ),
            FilterResult::Forward
        ));
    }
}
