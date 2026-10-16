//! A bounded, private event bridge opens shell search without synthesizing keys.
use std::{
    collections::HashSet,
    os::unix::net::UnixDatagram,
    time::{Duration, Instant},
};

use hearthspace_common::{launcher_request_token_from_app_id, shell_event_socket_path};
use smithay::{
    backend::input::{InputTime, KeyState},
    input::keyboard::Keycode,
    utils::Rectangle,
};

use super::command_socket_path;
use crate::compositor::{App, ManagedWindowKind, switcher::SwitcherSource};

const REQUEST_LIFETIME: Duration = Duration::from_secs(1);
const MAX_PENDING_KEYS: usize = 64;

pub(in crate::compositor) struct PendingKey {
    pub code: Keycode,
    pub state: KeyState,
    pub time: InputTime,
    finish_existing: bool,
}

struct Pending {
    token: u64,
    expires: Instant,
    accepted: bool,
    output: Option<String>,
    keys: Vec<PendingKey>,
    held_before: HashSet<Keycode>,
}

#[derive(Default)]
pub(in crate::compositor) struct SearchRequest {
    sequence: u64,
    pending: Option<Pending>,
}

impl SearchRequest {
    fn valid(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| Instant::now() < pending.expires)
    }

    fn accept(&mut self, token: u64) -> Result<(), String> {
        if !self.valid() {
            return Err("search request expired or cancelled".into());
        }
        let pending = self.pending.as_mut().unwrap();
        if token != pending.token || pending.accepted {
            return Err("search request superseded or already accepted".into());
        }
        pending.accepted = true;
        Ok(())
    }

    fn take_keys(&mut self) -> Vec<PendingKey> {
        self.pending
            .take()
            .map(|pending| pending.keys)
            .unwrap_or_default()
    }

    fn push_key(&mut self, code: Keycode, state: KeyState, time: InputTime) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if pending.keys.len() == MAX_PENDING_KEYS {
            return false;
        }
        let finish_existing = state == KeyState::Released && pending.held_before.remove(&code);
        pending.keys.push(PendingKey {
            code,
            state,
            time,
            finish_existing,
        });
        true
    }
}

impl App {
    pub(in crate::compositor) fn pending_search_reserves_focus(&self) -> bool {
        self.search_request.valid()
    }

    pub(in crate::compositor) fn focus_launcher(&mut self) -> Result<(), String> {
        self.request_search(false)
    }

    pub(in crate::compositor) fn toggle_launcher(&mut self) -> Result<(), String> {
        self.request_search(true)
    }

    fn request_search(&mut self, toggle: bool) -> Result<(), String> {
        self.check_navigation_available()?;
        let target_output = self
            .capture_outputs()
            .into_iter()
            .find(|output| {
                Rectangle::new(
                    (output.logical_x, output.logical_y).into(),
                    (output.logical_width, output.logical_height).into(),
                )
                .contains(self.pointer_location.to_i32_round())
            })
            .map(|output| output.name);
        let bar = self
            .windows
            .iter()
            .position(|window| {
                window.kind == ManagedWindowKind::ShellBar
                    && window.mapped
                    && window.shell_output_name == target_output
            })
            .or_else(|| {
                self.windows
                    .iter()
                    .position(|window| window.kind == ManagedWindowKind::ShellBar && window.mapped)
            })
            .ok_or_else(|| "application launcher is not ready".to_string())?;
        let output = self.windows[bar].shell_output_name.clone();
        // Repeated activation while the shell is building its window must not
        // invalidate the token already attached to that in-flight surface.
        if self.search_request.valid()
            && self
                .search_request
                .pending
                .as_ref()
                .is_some_and(|pending| pending.output == output)
        {
            return Ok(());
        }
        self.cancel_pending_search();
        if let Some(index) = self
            .windows
            .iter()
            .enumerate()
            .find(|(index, window)| {
                self.is_search_panel(*index) && window.mapped && window.shell_output_name == output
            })
            .map(|(index, _)| index)
        {
            if toggle {
                self.clear_favorite_drawing();
                self.cancel_ui_touch();
                self.windows[index].surface.send_close();
            } else if let Some(surface) = self.windows[index].surface.wl_surface() {
                self.set_keyboard_focus_to_window(index, surface);
            }
            return Ok(());
        }
        self.search_request.sequence = (self.search_request.sequence.wrapping_add(1)
            & !super::device_control::CONTROL_TOKEN_BIT)
            .max(1);
        let token = self.search_request.sequence;
        let event = serde_json::to_vec(&serde_json::json!({
            "version": 1, "action": "open-search", "output": output, "token": token,
        }))
        .expect("serializable shell event");
        if event.len() > 4096 {
            return Err("shell event exceeds size limit".into());
        }
        let socket = UnixDatagram::unbound().map_err(|error| error.to_string())?;
        socket
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        socket
            .send_to(&event, shell_event_socket_path(&command_socket_path()))
            .map_err(|error| format!("application launcher is not ready: {error}"))?;
        // Reserve focus immediately. Actual keystrokes are retained until the
        // new search surface maps, so a fast typist neither loses the first
        // characters nor types them into the previously focused application.
        if let Some(surface) = self.windows[bar].surface.wl_surface() {
            self.set_keyboard_focus_to_window(bar, surface);
        }
        self.search_request.pending = Some(Pending {
            token,
            expires: Instant::now() + REQUEST_LIFETIME,
            accepted: false,
            output,
            keys: Vec::new(),
            held_before: self.keyboard.pressed_keys(),
        });
        Ok(())
    }

    pub(in crate::compositor) fn accept_shell_event(&mut self, token: u64) -> Result<(), String> {
        if token & super::device_control::CONTROL_TOKEN_BIT != 0 {
            return self.control_events.accept(token);
        }
        self.check_navigation_available()?;
        self.search_request.accept(token)
    }

    /// Keep modifier changes in order with typed keys, including modifiers
    /// already held when a command opened search.
    pub(in crate::compositor) fn buffer_pending_search_key(
        &mut self,
        code: Keycode,
        state: KeyState,
        time: InputTime,
    ) -> bool {
        if self.search_request.pending.is_none() {
            return false;
        }
        if !self.search_request.valid() || !self.navigation_available() {
            self.cancel_pending_search();
            return false;
        }
        if !self.search_request.push_key(code, state, time) {
            self.cancel_pending_search();
            return false;
        }
        true
    }

    pub(in crate::compositor) fn search_mapping_allowed(&mut self, index: usize) -> bool {
        if !self.is_search_panel(index) {
            return true;
        }
        let token = crate::compositor::windows::window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(launcher_request_token_from_app_id);
        let Some(token) = token else {
            // An explicit icon activation supersedes an earlier keyboard request.
            self.cancel_pending_search();
            return true;
        };
        let allowed = self.search_request.valid()
            && self.search_request.pending.as_ref().is_some_and(|pending| {
                pending.accepted
                    && pending.token == token
                    && pending.output == self.windows[index].shell_output_name
            });
        if !allowed
            && self
                .search_request
                .pending
                .as_ref()
                .is_some_and(|pending| pending.token == token)
        {
            self.cancel_pending_search();
        }
        allowed
    }

    pub(in crate::compositor) fn finish_pending_search(&mut self, index: usize) {
        let ready = self.search_request.valid()
            && self.is_search_panel(index)
            && self.search_request.pending.as_ref().is_some_and(|pending| {
                pending.accepted && pending.output == self.windows[index].shell_output_name
            });
        if !ready {
            return;
        }
        let keys = self.search_request.take_keys();
        let mut replayed = HashSet::new();
        for mut key in keys {
            // A queued shortcut can lock the session or change focus. Stop
            // typing immediately if that happened during replay.
            if !self.navigation_available() || self.focused_shell_panel_index() != Some(index) {
                // Earlier replay may already have forwarded this press before
                // a shortcut moved focus. Preserve its matching release.
                if key.state == KeyState::Released && replayed.remove(&key.code) {
                    key.finish_existing = true;
                }
                self.discard_search_key(key);
            } else {
                match key.state {
                    KeyState::Pressed => {
                        replayed.insert(key.code);
                    }
                    KeyState::Released => {
                        replayed.remove(&key.code);
                    }
                }
                self.handle_keyboard_input(key.code, key.state, key.time);
            }
        }
    }

    pub(in crate::compositor) fn cancel_pending_search(&mut self) {
        for key in self.search_request.take_keys() {
            self.discard_search_key(key);
        }
    }

    fn discard_search_key(&mut self, key: PendingKey) {
        let keyboard = self.keyboard.clone();
        let previous_modifiers = keyboard.modifier_state();
        keyboard.input(
            self,
            key.code,
            key.state,
            smithay::utils::SERIAL_COUNTER.next_serial(),
            key.time,
            |app, _, _| {
                app.shortcuts
                    .discard_pending_key(key.code, key.state, key.finish_existing)
            },
        );
        keyboard.advertise_modifier_state(self);
        // A shortcut replayed before focus changed may own the held switcher.
        // Discarding subsequent typing must still finish its real Alt release.
        if previous_modifiers.alt && !keyboard.modifier_state().alt {
            self.finish_window_switcher(SwitcherSource::Keyboard, false);
        }
    }

    pub(in crate::compositor) fn cancel_search_outside_focus(&mut self, focus: Option<usize>) {
        let keep = self.search_request.pending.as_ref().is_some_and(|pending| {
            focus.is_some_and(|index| {
                (self.windows[index].kind == ManagedWindowKind::ShellBar
                    || self.is_search_panel(index))
                    && self.windows[index].shell_output_name == pending.output
            })
        });
        if !keep {
            // This callback runs inside Smithay's keyboard focus lock. Only
            // invalidate here; drain keys at the next input outside that lock.
            if let Some(pending) = self.search_request.pending.as_mut() {
                pending.expires = Instant::now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_acknowledgement_requires_current_unexpired_token_once() {
        let mut request = SearchRequest::default();
        assert!(request.accept(1).is_err());
        request.pending = Some(Pending {
            token: 4,
            expires: Instant::now() + REQUEST_LIFETIME,
            accepted: false,
            output: None,
            keys: Vec::new(),
            held_before: HashSet::new(),
        });
        assert!(request.accept(3).is_err());
        assert!(request.accept(4).is_ok());
        assert!(request.accept(4).is_err());
        request.pending.as_mut().unwrap().expires = Instant::now() - REQUEST_LIFETIME;
        assert!(request.accept(4).is_err());
        request.take_keys();
        assert!(request.accept(4).is_err());
    }

    #[test]
    fn pending_keys_preserve_modifier_order_and_bound_memory() {
        let shift = 50_u32.into();
        let a = 38_u32.into();
        let time = InputTime::now();
        let mut request = SearchRequest {
            sequence: 1,
            pending: Some(Pending {
                token: 1,
                expires: Instant::now() + REQUEST_LIFETIME,
                accepted: false,
                output: None,
                keys: Vec::new(),
                held_before: HashSet::from([shift]),
            }),
        };
        // Shift was already held when search opened. Its release stays behind
        // the first typed letter; it can be paired if creation is cancelled.
        assert!(request.push_key(a, KeyState::Pressed, time));
        assert!(request.push_key(a, KeyState::Released, time));
        assert!(request.push_key(shift, KeyState::Released, time));
        assert!(request.push_key(shift, KeyState::Pressed, time));
        assert!(request.push_key(shift, KeyState::Released, time));
        for _ in 5..MAX_PENDING_KEYS {
            assert!(request.push_key(a, KeyState::Pressed, time));
        }
        assert!(!request.push_key(a, KeyState::Pressed, time));
        let keys = request.take_keys();
        assert_eq!(keys.len(), MAX_PENDING_KEYS);
        assert_eq!(keys[0].code, a);
        assert_eq!(keys[1].code, a);
        assert_eq!(keys[2].code, shift);
        assert!(keys[2].finish_existing);
        assert!(!keys[4].finish_existing);
        assert!(request.take_keys().is_empty());
    }
}
