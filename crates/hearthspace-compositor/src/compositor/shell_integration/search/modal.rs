//! Modal exits remain immediate while the native search field is still mapping.
use super::{App, HashSet, KeyState, Keycode};
use hearthspace_ipc::{
    command::CanvasTool,
    shortcuts::{CANVAS_SHORTCUTS, CanvasAction},
};

impl App {
    pub(super) fn pending_canvas_modal_shortcut(&mut self, code: Keycode) -> bool {
        // Buffered modifiers have not reached XKB yet. Observe their ordered
        // state before deciding whether this key supersedes the pending field.
        let initial = self.keyboard.modifier_state();
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
            let mut held: HashSet<_> = pressed_keys.clone();
            for (pending_code, pending_state) in &pending_keys {
                if *pending_state == KeyState::Pressed {
                    held.insert(*pending_code);
                } else {
                    held.remove(pending_code);
                }
            }
            let modifier = |initial, names: &[u32]| {
                let matches = |code| symbols(code).iter().any(|sym| names.contains(&sym.raw()));
                let external = initial && !pressed_keys.iter().any(|code| matches(*code));
                external || held.iter().any(|code| matches(*code))
            };
            let alt = modifier(initial.alt, &[keysyms::KEY_Alt_L, keysyms::KEY_Alt_R]);
            if current
                .iter()
                .any(|sym| matches!(sym.raw(), keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab))
            {
                return alt;
            }
            if held.contains(&code)
                || alt
                || modifier(
                    initial.ctrl,
                    &[keysyms::KEY_Control_L, keysyms::KEY_Control_R],
                )
                || modifier(initial.shift, &[keysyms::KEY_Shift_L, keysyms::KEY_Shift_R])
                || !modifier(
                    initial.logo,
                    &[
                        keysyms::KEY_Super_L,
                        keysyms::KEY_Super_R,
                        keysyms::KEY_Meta_L,
                        keysyms::KEY_Meta_R,
                    ],
                )
            {
                return false;
            }
            // Match Smithay's raw Latin fallback used by normal shortcut input.
            let base = current.first().and_then(|sym| sym.key_char());
            let key = match base {
                Some(key) if key.is_ascii() => Some(key),
                Some(_) => xkb.layouts().find_map(|layout| {
                    xkb.raw_syms_for_key_in_layout(code, layout)
                        .first()
                        .and_then(|sym| sym.key_char())
                        .filter(char::is_ascii)
                }),
                None => None,
            };
            CANVAS_SHORTCUTS.iter().any(|binding| {
                // Selecting Search again must keep its pending field and text.
                binding.action != CanvasAction::Tool(CanvasTool::Search)
                    && Some(binding.key) == key.map(|key| key.to_ascii_lowercase())
            })
        })
    }
}
