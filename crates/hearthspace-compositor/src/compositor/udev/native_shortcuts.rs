//! Physical recovery chords run before client grabs or queued search input.
//! Keep each device's keys separate so releasing one of two keyboards' Ctrl
//! keys cannot disable the other keyboard's chord.
use std::collections::{HashMap, HashSet};

use smithay::backend::input::KeyState;

// Smithay keycodes include XKB's offset from Linux evdev codes.
const ESC: u32 = 1 + 8;
const BACKSPACE: u32 = 14 + 8;
const LEFT_CTRL: u32 = 29 + 8;
const LEFT_ALT: u32 = 56 + 8;
const RIGHT_CTRL: u32 = 97 + 8;
const RIGHT_ALT: u32 = 100 + 8;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum NativeAction {
    Forward,
    Consume,
    SwitchVt(i32),
    Exit,
}

#[derive(Default)]
struct DeviceKeys {
    pressed: HashSet<u32>,
    intercepted: HashSet<u32>,
}

#[derive(Default)]
pub(super) struct NativeShortcuts {
    devices: HashMap<String, DeviceKeys>,
}

impl NativeShortcuts {
    pub(super) fn key(&mut self, device: &str, code: u32, state: KeyState) -> NativeAction {
        if !tracked_key(code) {
            return NativeAction::Forward;
        }
        let keys = self.devices.entry(device.to_owned()).or_default();
        if state == KeyState::Released {
            keys.pressed.remove(&code);
            return if keys.intercepted.remove(&code) {
                NativeAction::Consume
            } else {
                NativeAction::Forward
            };
        }
        if keys.intercepted.contains(&code) {
            return NativeAction::Consume;
        }
        if !keys.pressed.insert(code) {
            return NativeAction::Forward;
        }
        let held = |left, right| {
            self.devices
                .values()
                .any(|keys| keys.pressed.contains(&left) || keys.pressed.contains(&right))
        };
        if !held(LEFT_CTRL, RIGHT_CTRL) || !held(LEFT_ALT, RIGHT_ALT) {
            return NativeAction::Forward;
        }
        let action = match code {
            ESC | BACKSPACE => NativeAction::Exit,
            _ => match function_vt(code) {
                Some(vt) => NativeAction::SwitchVt(vt),
                None => return NativeAction::Forward,
            },
        };
        self.devices
            .get_mut(device)
            .expect("the current keyboard is registered")
            .intercepted
            .insert(code);
        action
    }

    pub(super) fn remove_device(&mut self, device: &str) {
        self.devices.remove(device);
    }

    pub(super) fn clear(&mut self) {
        self.devices.clear();
    }
}

fn function_vt(code: u32) -> Option<i32> {
    match code.checked_sub(8)? {
        59..=68 => Some((code - 8 - 59 + 1) as i32),
        87..=88 => Some((code - 8 - 87 + 11) as i32),
        _ => None,
    }
}

fn tracked_key(code: u32) -> bool {
    matches!(
        code,
        ESC | BACKSPACE | LEFT_CTRL | RIGHT_CTRL | LEFT_ALT | RIGHT_ALT
    ) || function_vt(code).is_some()
}

#[cfg(test)]
mod tests;
