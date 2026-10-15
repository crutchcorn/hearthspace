//! Short-lived, single-use authorization for shell-owned hardware controls.
use std::{
    collections::VecDeque,
    os::unix::net::UnixDatagram,
    time::{Duration, Instant},
};

use hearthspace_common::shell_event_socket_path;
use smithay::utils::Rectangle;

use super::command_socket_path;
use crate::compositor::{App, ManagedWindowKind};

pub(in crate::compositor) const CONTROL_TOKEN_BIT: u64 = 1 << 63;
const REQUEST_LIFETIME: Duration = Duration::from_millis(500);
const MAX_PENDING: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::compositor) enum DeviceControl {
    VolumeUp,
    VolumeDown,
    VolumeMute,
    BrightnessUp,
    BrightnessDown,
}

impl DeviceControl {
    fn wire_name(self) -> &'static str {
        match self {
            Self::VolumeUp => "volume-up",
            Self::VolumeDown => "volume-down",
            Self::VolumeMute => "volume-mute",
            Self::BrightnessUp => "brightness-up",
            Self::BrightnessDown => "brightness-down",
        }
    }

    pub(in crate::compositor) fn repeats(self) -> bool {
        self != Self::VolumeMute
    }
}

#[derive(Default)]
pub(in crate::compositor) struct ControlEvents {
    sequence: u64,
    pending: VecDeque<(u64, Instant)>,
}

impl ControlEvents {
    fn issue(&mut self, now: Instant) -> u64 {
        self.pending.retain(|(_, deadline)| now < *deadline);
        self.sequence = (self.sequence.wrapping_add(1) & !CONTROL_TOKEN_BIT).max(1);
        let token = self.sequence | CONTROL_TOKEN_BIT;
        if self.pending.len() == MAX_PENDING {
            self.pending.pop_front();
        }
        self.pending.push_back((token, now + REQUEST_LIFETIME));
        token
    }

    pub(super) fn accept(&mut self, token: u64) -> Result<(), String> {
        let now = Instant::now();
        self.pending.retain(|(_, deadline)| now < *deadline);
        let index = self
            .pending
            .iter()
            .position(|(pending, _)| *pending == token)
            .ok_or_else(|| {
                "device control request expired, cancelled, or already accepted".to_string()
            })?;
        self.pending.remove(index);
        Ok(())
    }

    pub(in crate::compositor) fn clear(&mut self) {
        self.pending.clear();
    }
}

impl App {
    pub(in crate::compositor) fn send_device_control(
        &mut self,
        control: DeviceControl,
    ) -> Result<(), String> {
        if self.session_locked() {
            return Err("session is locked".into());
        }
        let target = self
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
        let output = self
            .windows
            .iter()
            .find(|window| {
                window.kind == ManagedWindowKind::ShellBar
                    && window.mapped
                    && window.shell_output_name == target
            })
            .or_else(|| {
                self.windows
                    .iter()
                    .find(|window| window.kind == ManagedWindowKind::ShellBar && window.mapped)
            })
            .ok_or_else(|| "shell controls are not ready".to_string())?
            .shell_output_name
            .clone();
        let token = self.control_events.issue(Instant::now());
        let event = serde_json::to_vec(&serde_json::json!({
            "version": 1, "action": "device-control", "control": control.wire_name(), "output": output, "token": token,
        })).expect("serializable device control event");
        if event.len() > 4096 {
            self.control_events.accept(token)?;
            return Err("shell event exceeds size limit".into());
        }
        let sent = (|| {
            let socket = UnixDatagram::unbound()?;
            socket.set_nonblocking(true)?;
            socket.send_to(&event, shell_event_socket_path(&command_socket_path()))
        })();
        if let Err(error) = sent {
            let _ = self.control_events.accept(token);
            return Err(format!("shell controls are not ready: {error}"));
        }
        Ok(())
    }

    pub(in crate::compositor) fn dismiss_control_osds(&self) {
        for window in &self.windows {
            if window.kind == ManagedWindowKind::ControlOsd {
                window.surface.send_close();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_have_bounded_single_use_expiring_authorization_separate_from_search() {
        let mut events = ControlEvents::default();
        let now = Instant::now();
        let first = events.issue(now);
        assert_ne!(first & CONTROL_TOKEN_BIT, 0);
        assert!(events.accept(first & !CONTROL_TOKEN_BIT).is_err());
        assert!(events.accept(first).is_ok());
        assert!(events.accept(first).is_err());
        let expired = events.issue(now - REQUEST_LIFETIME);
        assert!(events.accept(expired).is_err());
        let evicted = events.issue(now);
        for _ in 0..MAX_PENDING {
            events.issue(now);
        }
        assert_eq!(events.pending.len(), MAX_PENDING);
        assert!(events.accept(evicted).is_err());
        let last = events.issue(now);
        events.clear();
        assert!(events.accept(last).is_err());
        assert_ne!(events.issue(now), last, "reset cannot reuse a stale token");
    }
}
