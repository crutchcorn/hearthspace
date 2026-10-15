//! Bare Meta is decided on release; modifier combinations remain paired.
use super::*;

#[derive(Default)]
pub(super) struct MetaTap {
    held: HashSet<Keycode>,
    candidate: Option<Keycode>,
}

impl MetaTap {
    pub(super) fn cancel(&mut self) {
        self.candidate = None;
    }

    pub(super) fn discard(&mut self, code: Keycode, state: KeyState) {
        self.cancel();
        match state {
            KeyState::Pressed => {
                self.held.insert(code);
            }
            KeyState::Released => {
                self.held.remove(&code);
            }
        }
    }

    pub(super) fn observe(
        &mut self,
        code: Keycode,
        state: KeyState,
        modifiers: &ModifiersState,
        symbol: u32,
        available: bool,
    ) -> bool {
        if !available {
            self.cancel();
        }
        match state {
            KeyState::Pressed => {
                if self.held.contains(&code) {
                    return false;
                }
                let alone = self.held.is_empty();
                self.cancel();
                self.held.insert(code);
                if available
                    && alone
                    && is_meta(symbol)
                    && !modifiers.ctrl
                    && !modifiers.alt
                    && !modifiers.shift
                {
                    self.candidate = Some(code);
                }
                false
            }
            KeyState::Released => {
                self.held.remove(&code);
                let toggle = available && self.candidate == Some(code) && self.held.is_empty();
                if self.candidate == Some(code) {
                    self.cancel();
                }
                toggle
            }
        }
    }
}

fn is_meta(symbol: u32) -> bool {
    matches!(
        symbol,
        keysyms::KEY_Super_L | keysyms::KEY_Super_R | keysyms::KEY_Meta_L | keysyms::KEY_Meta_R
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn either_meta_toggles_once_only_after_release() {
        for symbol in [
            keysyms::KEY_Super_L,
            keysyms::KEY_Super_R,
            keysyms::KEY_Meta_L,
            keysyms::KEY_Meta_R,
        ] {
            let mut tap = MetaTap::default();
            let mods = ModifiersState {
                logo: true,
                ..Default::default()
            };
            for _ in 0..2 {
                assert!(!tap.observe(133_u32.into(), KeyState::Pressed, &mods, symbol, true));
            }
            assert!(tap.observe(
                133_u32.into(),
                KeyState::Released,
                &Default::default(),
                symbol,
                true
            ));
            assert!(!tap.observe(
                133_u32.into(),
                KeyState::Released,
                &Default::default(),
                symbol,
                true
            ));
        }
    }

    #[test]
    fn any_other_key_including_unknown_chords_or_second_meta_cancels() {
        for symbol in [
            keysyms::KEY_space,
            keysyms::KEY_a,
            keysyms::KEY_0,
            keysyms::KEY_Shift_L,
            keysyms::KEY_Super_R,
        ] {
            let mut tap = MetaTap::default();
            let mods = ModifiersState {
                logo: true,
                ..Default::default()
            };
            tap.observe(
                133_u32.into(),
                KeyState::Pressed,
                &mods,
                keysyms::KEY_Super_L,
                true,
            );
            tap.observe(134_u32.into(), KeyState::Pressed, &mods, symbol, true);
            assert!(!tap.observe(134_u32.into(), KeyState::Released, &mods, symbol, true));
            assert!(!tap.observe(
                133_u32.into(),
                KeyState::Released,
                &Default::default(),
                keysyms::KEY_Super_L,
                true
            ));
        }
    }

    #[test]
    fn pointer_scroll_touch_or_session_cancellation_prevents_toggle() {
        let mut tap = MetaTap::default();
        let mods = ModifiersState {
            logo: true,
            ..Default::default()
        };
        tap.observe(
            133_u32.into(),
            KeyState::Pressed,
            &mods,
            keysyms::KEY_Super_L,
            true,
        );
        tap.cancel();
        assert!(!tap.observe(
            133_u32.into(),
            KeyState::Released,
            &Default::default(),
            keysyms::KEY_Super_L,
            true
        ));
        tap.observe(
            133_u32.into(),
            KeyState::Pressed,
            &mods,
            keysyms::KEY_Super_L,
            false,
        );
        assert!(!tap.observe(
            133_u32.into(),
            KeyState::Released,
            &Default::default(),
            keysyms::KEY_Super_L,
            true
        ));
    }

    #[test]
    fn held_application_key_and_grab_on_release_prevent_toggle() {
        let mut tap = MetaTap::default();
        let mods = ModifiersState {
            logo: true,
            ..Default::default()
        };
        tap.observe(
            38_u32.into(),
            KeyState::Pressed,
            &Default::default(),
            keysyms::KEY_a,
            true,
        );
        tap.observe(
            133_u32.into(),
            KeyState::Pressed,
            &mods,
            keysyms::KEY_Super_L,
            true,
        );
        tap.observe(
            38_u32.into(),
            KeyState::Released,
            &mods,
            keysyms::KEY_a,
            true,
        );
        assert!(!tap.observe(
            133_u32.into(),
            KeyState::Released,
            &Default::default(),
            keysyms::KEY_Super_L,
            true
        ));
        tap.observe(
            133_u32.into(),
            KeyState::Pressed,
            &mods,
            keysyms::KEY_Super_L,
            true,
        );
        assert!(!tap.observe(
            133_u32.into(),
            KeyState::Released,
            &Default::default(),
            keysyms::KEY_Super_L,
            false
        ));
    }
}
