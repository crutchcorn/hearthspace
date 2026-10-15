//! Held switching consumes application input without losing paired releases.
use super::*;

impl KeyboardShortcuts {
    pub(super) fn filter_switcher(
        &mut self,
        code: Keycode,
        state: KeyState,
        modifiers: &ModifiersState,
        symbol: u32,
        source: SwitcherSource,
    ) -> FilterResult<Option<Shortcut>> {
        if state == KeyState::Released || self.intercepted.contains(&code) {
            return self.filter(code, state, modifiers, symbol, false);
        }
        self.intercepted.insert(code);
        FilterResult::Intercept(switcher_shortcut(modifiers, symbol, source))
    }
}

fn switcher_shortcut(
    modifiers: &ModifiersState,
    symbol: u32,
    source: SwitcherSource,
) -> Option<Shortcut> {
    if symbol == keysyms::KEY_Escape {
        return Some(Shortcut::SwitcherCancel);
    }
    match shortcut(modifiers, symbol) {
        Some(Shortcut::Lock) => Some(Shortcut::Lock),
        action @ Some(Shortcut::Next | Shortcut::Previous)
            if source == SwitcherSource::Keyboard =>
        {
            action
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_switcher_cycles_only_its_keyboard_owner_and_escape_cancels_either() {
        let mut mods = ModifiersState {
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            switcher_shortcut(&mods, keysyms::KEY_Tab, SwitcherSource::Keyboard),
            Some(Shortcut::Next)
        );
        mods.shift = true;
        assert_eq!(
            switcher_shortcut(&mods, keysyms::KEY_ISO_Left_Tab, SwitcherSource::Keyboard),
            Some(Shortcut::Previous)
        );
        assert_eq!(
            switcher_shortcut(&mods, keysyms::KEY_Tab, SwitcherSource::Gesture),
            None
        );
        for source in [SwitcherSource::Keyboard, SwitcherSource::Gesture] {
            assert_eq!(
                switcher_shortcut(&mods, keysyms::KEY_Escape, source),
                Some(Shortcut::SwitcherCancel)
            );
            assert_eq!(switcher_shortcut(&mods, keysyms::KEY_F4, source), None);
        }
    }

    #[test]
    fn modal_typing_and_repeats_stay_consumed_after_alt_is_released() {
        let mut shortcuts = KeyboardShortcuts::default();
        let mods = ModifiersState::default();
        let code = 38_u32.into();
        for _ in 0..2 {
            assert!(matches!(
                shortcuts.filter_switcher(
                    code,
                    KeyState::Pressed,
                    &mods,
                    keysyms::KEY_a,
                    SwitcherSource::Keyboard
                ),
                FilterResult::Intercept(None)
            ));
        }
        assert!(matches!(
            shortcuts.filter(code, KeyState::Released, &mods, keysyms::KEY_a, true),
            FilterResult::Intercept(None)
        ));
        assert!(matches!(
            shortcuts.filter(code, KeyState::Pressed, &mods, keysyms::KEY_a, true),
            FilterResult::Forward
        ));
    }

    #[test]
    fn modifiers_pressed_before_opening_keep_their_matched_releases() {
        let mut shortcuts = KeyboardShortcuts::default();
        let code = 64_u32.into();
        let mods = ModifiersState {
            alt: true,
            ..Default::default()
        };
        assert!(matches!(
            shortcuts.filter(code, KeyState::Pressed, &mods, keysyms::KEY_Alt_L, true),
            FilterResult::Forward
        ));
        assert!(matches!(
            shortcuts.filter_switcher(
                code,
                KeyState::Released,
                &ModifiersState::default(),
                keysyms::KEY_Alt_L,
                SwitcherSource::Keyboard
            ),
            FilterResult::Forward
        ));
    }
}
