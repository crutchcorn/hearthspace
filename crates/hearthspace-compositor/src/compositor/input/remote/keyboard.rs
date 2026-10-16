use std::{collections::HashSet, sync::Arc};

use smithay::{
    backend::input::{Event, InputEvent, InputTime, KeyState, KeyboardKeyEvent},
    input::{
        Seat,
        keyboard::{KeyboardSource, Keycode},
    },
    wayland::virtual_keyboard::{
        VirtualKeyboardBackend, VirtualKeyboardDevice, VirtualKeyboardHandler,
        VirtualKeyboardSpecialEvent,
    },
};

use crate::compositor::App;

pub(super) struct RemoteKeyboard {
    keymap: Option<Arc<str>>,
    held: HashSet<Keycode>,
    modifiers: Option<[u32; 4]>,
}

impl VirtualKeyboardHandler for App {
    fn process_virtual_keyboard_event(&mut self, event: InputEvent<VirtualKeyboardBackend>) {
        match event {
            InputEvent::DeviceAdded { device }
            | InputEvent::Special(VirtualKeyboardSpecialEvent::KeymapChanged { device }) => {
                if !self.remote_keyboard_seat(&device) {
                    return;
                }
                self.release_remote_keyboard(device.source());
                self.remote_input.keyboards.insert(
                    device.source(),
                    RemoteKeyboard {
                        keymap: device.keymap(),
                        held: HashSet::new(),
                        modifiers: None,
                    },
                );
            }
            InputEvent::DeviceRemoved { device } => {
                self.release_remote_keyboard(device.source());
                self.remote_input.keyboards.remove(&device.source());
                self.activate_keyboard_source(KeyboardSource::MAIN);
            }
            InputEvent::Keyboard { event } => {
                let device = event.device();
                if device.client().is_none() || !self.remote_input_available() {
                    self.release_remote_keyboard(device.source());
                    return;
                }
                let Some(remote) = self.remote_input.keyboards.get_mut(&device.source()) else {
                    return;
                };
                match event.state() {
                    KeyState::Pressed => {
                        remote.held.insert(event.key_code());
                    }
                    KeyState::Released => {
                        remote.held.remove(&event.key_code());
                    }
                }
                self.handle_keyboard_input_from_source(
                    device.source(),
                    event.key_code(),
                    event.state(),
                    event.time(),
                );
            }
            InputEvent::Special(VirtualKeyboardSpecialEvent::Modifiers {
                device,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            }) if self.remote_input_available() && self.remote_keyboard_seat(&device) => {
                self.apply_remote_modifiers(
                    device.source(),
                    [mods_depressed, mods_latched, mods_locked, group],
                );
            }
            _ => {}
        }
    }
}

impl App {
    fn remote_keyboard_seat(&self, device: &VirtualKeyboardDevice) -> bool {
        self.remote_input.enabled
            && Seat::<App>::from_resource(device.wl_seat()).as_ref() == Some(&self._seat)
    }

    /// Restore the configured map for physical keys; virtual keycodes must be
    /// interpreted using their client's map, including WayVNC's remapped symbols.
    pub(in crate::compositor) fn activate_keyboard_source(
        &mut self,
        source: KeyboardSource,
    ) -> bool {
        if self.remote_input.main_keymap.is_none() {
            if source == KeyboardSource::MAIN {
                return true;
            }
            let keyboard = self.keyboard.clone();
            let map = keyboard.with_xkb_state(self, |context| {
                context.xkb().lock().unwrap().keymap_as_string()
            });
            self.remote_input.main_keymap = Some(Arc::from(map));
        }
        let target = if source == KeyboardSource::MAIN {
            self.remote_input.main_keymap.clone()
        } else {
            let Some(remote) = self.remote_input.keyboards.get(&source) else {
                return false;
            };
            remote
                .keymap
                .clone()
                .or_else(|| self.remote_input.main_keymap.clone())
        };
        if self.remote_input.active_keyboard == source && self.remote_input.active_keymap == target
        {
            return true;
        }
        let keyboard = self.keyboard.clone();
        if self.remote_input.active_keyboard == KeyboardSource::MAIN {
            self.remote_input.main_modifiers = self.keyboard_modifier_masks();
        }
        // Rebuild even for equal maps when switching sources. Explicit virtual
        // modifier masks must never remain active when local typing resumes.
        if let Some(map) = target.as_ref()
            && let Err(error) = keyboard.set_keymap_from_string(self, map.to_string())
        {
            tracing::warn!(?error, "could not activate virtual keyboard keymap");
            return false;
        }
        self.remote_input.active_keymap = target;
        self.remote_input.active_keyboard = source;
        let modifiers = if source == KeyboardSource::MAIN {
            let mut masks = self.remote_input.main_modifiers;
            // Depressed modifiers follow all currently held keys, not a stale
            // snapshot; preserve the local locked/latched state and layout.
            masks[0] = self.keyboard_modifier_masks()[0];
            Some(masks)
        } else {
            self.remote_input
                .keyboards
                .get(&source)
                .and_then(|remote| remote.modifiers)
        };
        if let Some(masks) = modifiers {
            self.set_keyboard_modifier_masks(masks);
        }
        true
    }

    pub(in crate::compositor) fn remote_key_modifiers(
        &self,
        source: KeyboardSource,
    ) -> Option<[u32; 4]> {
        (source != KeyboardSource::MAIN).then(|| self.keyboard_modifier_masks())
    }

    pub(in crate::compositor) fn apply_remote_modifiers(
        &mut self,
        source: KeyboardSource,
        masks: [u32; 4],
    ) {
        if !self.activate_keyboard_source(source) {
            return;
        }
        if let Some(remote) = self.remote_input.keyboards.get_mut(&source) {
            remote.modifiers = Some(masks);
            let previous_alt = self.keyboard.modifier_state().alt;
            self.set_keyboard_modifier_masks(masks);
            // WayVNC sends absolute masks before the corresponding key-up.
            // Observe this edge here as well as in normal keyboard dispatch.
            if previous_alt && !self.keyboard.modifier_state().alt {
                self.finish_window_switcher(
                    crate::compositor::switcher::SwitcherSource::Keyboard,
                    false,
                );
            }
        }
    }

    fn keyboard_modifier_masks(&self) -> [u32; 4] {
        let mods = self.keyboard.modifier_state().serialized;
        [
            mods.depressed,
            mods.latched,
            mods.locked,
            mods.layout_effective,
        ]
    }

    fn set_keyboard_modifier_masks(&mut self, masks: [u32; 4]) {
        self.keyboard.clone().with_xkb_state(self, |mut context| {
            context.set_modifier_mask(masks[0], masks[1], masks[2], masks[3]);
        });
    }

    pub(super) fn release_remote_keyboard(&mut self, source: KeyboardSource) {
        let Some(remote) = self.remote_input.keyboards.get(&source) else {
            return;
        };
        if remote.held.is_empty() && remote.modifiers.is_none_or(|masks| masks == [0; 4]) {
            return;
        }
        // Disconnect, lock and VT changes cancel modal chords before releasing
        // keys, so a lost Meta/Alt release cannot launch or select an app.
        self.cancel_pending_search();
        self.shortcuts.cancel_meta_tap();
        self.shortcuts.cancel_device_repeat();
        self.reset_window_switcher();
        let held = std::mem::take(&mut self.remote_input.keyboards.get_mut(&source).unwrap().held);
        for key in held {
            self.handle_keyboard_input_from_source(
                source,
                key,
                KeyState::Released,
                InputTime::now(),
            );
        }
        self.keyboard.clone().release_source(self, source);
        self.remote_input
            .keyboards
            .get_mut(&source)
            .unwrap()
            .modifiers = None;
        // Force a rebuild when the remote and local maps happened to be equal.
        self.remote_input.active_keymap = None;
        self.activate_keyboard_source(KeyboardSource::MAIN);
    }
}
