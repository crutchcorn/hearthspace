//! Opt-in virtual devices use the same focus, shortcuts and grabs as local input.
use std::{collections::HashMap, sync::Arc};

use smithay::{
    input::keyboard::KeyboardSource,
    wayland::{
        virtual_keyboard::VirtualKeyboardManagerState, virtual_pointer::VirtualPointerManagerState,
    },
};
use wayland_server::DisplayHandle;

use crate::compositor::App;

mod keyboard;
mod pointer;

use keyboard::RemoteKeyboard;
pub(super) use pointer::PointerSource;

pub(in crate::compositor) struct RemoteInput {
    _keyboard_manager: Option<VirtualKeyboardManagerState>,
    _pointer_manager: Option<VirtualPointerManagerState>,
    pub(in crate::compositor) enabled: bool,
    pub(in crate::compositor) active: bool,
    keyboards: HashMap<KeyboardSource, RemoteKeyboard>,
    main_keymap: Option<Arc<str>>,
    main_modifiers: [u32; 4],
    active_keymap: Option<Arc<str>>,
    active_keyboard: KeyboardSource,
    buttons: pointer::ButtonSources,
    pointers: HashMap<smithay::wayland::virtual_pointer::VirtualPointerDevice, Option<String>>,
}

impl RemoteInput {
    pub(in crate::compositor) fn new(display: &DisplayHandle, enabled: bool) -> Self {
        Self {
            // The command-line opt-in and private session socket define access.
            // Normal sessions do not advertise either privileged global.
            _keyboard_manager: enabled
                .then(|| VirtualKeyboardManagerState::new::<App, _>(display, |_| true)),
            _pointer_manager: enabled
                .then(|| VirtualPointerManagerState::new::<App, _>(display, |_| true)),
            enabled,
            active: true,
            keyboards: HashMap::new(),
            main_keymap: None,
            main_modifiers: [0; 4],
            active_keymap: None,
            active_keyboard: KeyboardSource::MAIN,
            buttons: Default::default(),
            pointers: HashMap::new(),
        }
    }
}

impl App {
    pub(in crate::compositor) fn remote_input_available(&self) -> bool {
        self.remote_input.enabled && self.remote_input.active && !self.session_locked()
    }

    pub(in crate::compositor) fn set_remote_input_active(&mut self, active: bool) {
        if self.remote_input.active && !active {
            self.reset_remote_input();
        }
        self.remote_input.active = active;
    }

    /// Called before changing lock/VT ownership, while old focus still exists.
    pub(in crate::compositor) fn reset_remote_input(&mut self) {
        let sources = self
            .remote_input
            .keyboards
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for source in sources {
            self.release_remote_keyboard(source);
        }
        self.release_remote_pointer_buttons();
        self.activate_keyboard_source(KeyboardSource::MAIN);
    }
}
