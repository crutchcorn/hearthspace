use super::{App, State};
use crate::compositor::{
    CalloopData,
    shell_integration::{ControlReply, command_socket_path},
};
use hearthspace_common::shell_event_socket_path;
use serde_json::json;
use std::{
    os::unix::net::UnixDatagram,
    time::{Duration, Instant},
};

const ACTION_LIFETIME: Duration = Duration::from_secs(1);

fn send(value: serde_json::Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    let socket = UnixDatagram::unbound().map_err(|e| e.to_string())?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    socket
        .send_to(&bytes, shell_event_socket_path(&command_socket_path()))
        .map_err(|e| e.to_string())?;
    Ok(())
}

impl State {
    fn issue_action(&mut self, action: &'static str, id: &str, now: Instant) -> u64 {
        self.actions.retain(|(_, until, _, _)| now < *until);
        self.next_action = self.next_action.wrapping_add(1).max(1);
        let token = self.next_action;
        if self.actions.len() == 16 {
            self.actions.pop_front();
        }
        self.actions
            .push_back((token, now + ACTION_LIFETIME, action, id.to_owned()));
        token
    }

    fn accept_action(
        &mut self,
        token: u64,
        now: Instant,
    ) -> Result<(&'static str, String), String> {
        self.actions.retain(|(_, until, _, _)| now < *until);
        let index = self
            .actions
            .iter()
            .position(|(pending, _, _, _)| *pending == token)
            .ok_or("desktop action expired, cancelled or already accepted")?;
        let (_, _, action, id) = self.actions.remove(index).unwrap();
        Ok((action, id))
    }
}

impl App {
    pub(in crate::compositor) fn desktop_changed(&mut self) {
        self.desktop.revision = self.desktop.revision.wrapping_add(1);
        let _ = send(json!({"version":1,"action":"desktop-changed"}));
        self.request_redraw();
    }

    pub(in crate::compositor) fn desktop_status(&self) -> Vec<u8> {
        let items: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter_map(|(index, window)| {
                if !window.mapped {
                    return None;
                }
                let id = self.desktop_id(index)?;
                Some(json!({"id":id,"x":window.position.x,"y":window.position.y,
                "selected":self.desktop.selected.contains(&id),
                "focused":self.desktop.focused.as_ref()==Some(&id),
                "keyboard_focus":self.desktop.keyboard_focus}))
            })
            .collect();
        serde_json::to_vec(&json!({"version":1,"revision":self.desktop.revision,"select_mode":self.desktop.select_mode,"items":items})).expect("serializable desktop state")
    }

    pub(super) fn desktop_action(&mut self, action: &'static str, id: &str) -> Result<(), String> {
        self.check_navigation_available()?;
        if self.desktop_index(id).is_none() {
            return Err("desktop item is unavailable".into());
        }
        let token = self.desktop.issue_action(action, id, Instant::now());
        if let Err(error) = send(json!({"version":1,"action":"desktop-action","token":token})) {
            self.desktop.actions.pop_back();
            return Err(format!("desktop shell is unavailable: {error}"));
        }
        Ok(())
    }
}

impl CalloopData {
    pub(in crate::compositor) fn run_desktop_command(
        &mut self,
        text: &str,
    ) -> Option<ControlReply> {
        if !text.starts_with("desktop-") {
            return None;
        }
        if !self.running || !self.capture_active() || self.state.session_locked() {
            self.state.reset_desktop_input();
            return Some(ControlReply::Err("desktop is inactive".into()));
        }
        let app = &mut self.state;
        let result = if text == "desktop-status" {
            return Some(ControlReply::Payload(app.desktop_status()));
        } else if let Some(registration) = text.strip_prefix("desktop-register ") {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Registration {
                offset: usize,
                ids: Vec<String>,
            }
            match serde_json::from_str::<Registration>(registration) {
                Ok(value) => app.register_desktop_ids(value.offset, value.ids),
                Err(_) => Err("invalid desktop registration".into()),
            }
        } else if text == "desktop-clear" {
            app.cancel_desktop_gesture();
            app.desktop.selected.clear();
            app.desktop_changed();
            Ok(())
        } else if text == "desktop-selection-toggle" {
            app.cancel_desktop_gesture();
            app.desktop.select_mode = !app.desktop.select_mode;
            if app.desktop.select_mode {
                app.clear_window_group();
            }
            app.desktop_changed();
            Ok(())
        } else if let Some(token) = text
            .strip_prefix("desktop-event-accept ")
            .and_then(|value| value.parse::<u64>().ok())
        {
            match app.desktop.accept_action(token, Instant::now()) {
                Ok((action, id)) => {
                    return Some(ControlReply::Payload(
                        serde_json::to_vec(&json!({"action":action,"id":id})).unwrap(),
                    ));
                }
                Err(error) => Err(error),
            }
        } else if let Some((command, argument)) = text.split_once(' ') {
            match serde_json::from_str::<String>(argument) {
                Ok(id) if app.desktop_index(&id).is_some() => match command {
                    "desktop-select" => app.focus_desktop(&id, true).map(|()| {
                        app.desktop.selected.clear();
                        app.desktop.selected.insert(id.clone());
                        app.desktop_changed();
                    }),
                    "desktop-focus" => app.focus_desktop(&id, true),
                    "desktop-activate" => app.desktop_action("activate", &id),
                    _ => Err("invalid desktop command".into()),
                },
                _ => Err("desktop item is unavailable".into()),
            }
        } else {
            Err("invalid desktop command".into())
        };
        Some(result.map_or_else(ControlReply::Err, |()| ControlReply::Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_actions_are_bounded_single_use_and_expire_across_inactive_sessions() {
        let mut state = State::default();
        let now = Instant::now();
        let first = state.issue_action("activate", "app:foot.desktop", now);
        assert_eq!(
            state.accept_action(first, now).unwrap(),
            ("activate", "app:foot.desktop".into())
        );
        assert!(state.accept_action(first, now).is_err());
        let expired = state.issue_action("favorite", "app:foot.desktop", now);
        assert!(state.accept_action(expired, now + ACTION_LIFETIME).is_err());
        let evicted = state.issue_action("activate", "app:foot.desktop", now);
        for _ in 0..16 {
            state.issue_action("favorite", "app:foot.desktop", now);
        }
        assert_eq!(state.actions.len(), 16);
        assert!(state.accept_action(evicted, now).is_err());
        let cleared = state.issue_action("activate", "app:foot.desktop", now);
        state.actions.clear();
        assert!(state.accept_action(cleared, now).is_err());
        assert_ne!(
            state.issue_action("activate", "app:foot.desktop", now),
            cleared
        );
    }
}
