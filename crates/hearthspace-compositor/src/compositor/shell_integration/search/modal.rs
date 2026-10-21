//! Escape and Alt+Tab remain immediate while the native field is still mapping.
use super::{App, HashSet, KeyState, Keycode};

impl App {
    pub(super) fn pending_canvas_modal_shortcut(&mut self, code: Keycode) -> bool {
        // Alt+Tab may supersede a Canvas Search field before it maps.
        // Buffered modifiers have not reached XKB yet; include their ordered
        // state before deciding whether Tab belongs to the compositor.
        let initial_alt = self.keyboard.modifier_state().alt;
        let pressed_keys = self.keyboard.pressed_keys();
        let pending_keys: Vec<_> = self
            .search_request
            .pending
            .as_ref()
            .unwrap()
            .keys
            .iter()
            .map(|key| (key.code, key.state))
            .collect();
        self.keyboard.clone().with_xkb_state(self, |context| {
            use smithay::input::keyboard::keysyms;
            let xkb = context.xkb().lock().unwrap();
            let symbols = |code| xkb.raw_syms_for_key_in_layout(code, xkb.active_layout());
            let current = symbols(code);
            if current.iter().any(|sym| sym.raw() == keysyms::KEY_Escape) {
                return true;
            }
            if !current
                .iter()
                .any(|sym| matches!(sym.raw(), keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab))
            {
                return false;
            }
            let is_alt = |code| {
                symbols(code)
                    .iter()
                    .any(|sym| matches!(sym.raw(), keysyms::KEY_Alt_L | keysyms::KEY_Alt_R))
            };
            let mut held_alt: HashSet<_> = pressed_keys
                .iter()
                .copied()
                .filter(|code| is_alt(*code))
                .collect();
            let external_alt = initial_alt && held_alt.is_empty();
            for (pending_code, pending_state) in &pending_keys {
                if is_alt(*pending_code) {
                    if *pending_state == KeyState::Pressed {
                        held_alt.insert(*pending_code);
                    } else {
                        held_alt.remove(pending_code);
                    }
                }
            }
            external_alt || !held_alt.is_empty()
        })
    }
}
