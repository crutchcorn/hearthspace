//! Mode previews use the existing presenter/recovery path; only Keep persists.
use super::{UdevBackendState, apply_connector_sync, device::KmsOutputTarget, udev_backend_mut};
use crate::compositor::{CalloopData, shell_integration::ControlReply};
use hearthspace_ipc::display_modes::{Output, Preview, Request, Snapshot};
use smithay::{
    backend::session::Session,
    reexports::calloop::timer::{TimeoutAction, Timer},
};
use std::time::{Duration, Instant};
mod store;
pub(super) use store::State;

pub(super) struct Pending {
    token: u64,
    output: String,
    previous: String,
    requested: String,
    deadline: Instant,
    ready: bool,
    device: super::identity::DeviceToken,
}

impl UdevBackendState {
    fn catalog(&self) -> Snapshot {
        let outputs = self
            .opened_devices
            .values()
            .filter(|d| d.active)
            .flat_map(|d| &d.output_targets)
            .map(|t| Output {
                id: t.connector_name().into(),
                current: KmsOutputTarget::describe_mode(t.mode),
                modes: t
                    .modes
                    .iter()
                    .copied()
                    .map(KmsOutputTarget::describe_mode)
                    .collect(),
            })
            .collect();
        Snapshot {
            available: self.native_outputs_ready(),
            outputs,
            preview: self.mode_settings.pending.as_ref().map(|p| Preview {
                token: p.token,
                output: p.output.clone(),
                ready: p.ready,
                seconds_remaining: p
                    .deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs()
                    + 1,
            }),
            message: self.mode_settings.message.clone(),
        }
    }

    fn reconfigure_modes(&mut self) {
        if !self.session_active || !self.session.is_active() {
            return;
        }
        let ids: Vec<_> = self.opened_devices.keys().copied().collect();
        for id in ids {
            self.refresh_device(id, false);
        }
        self.refresh_backend_state();
    }

    fn revert_mode(&mut self, message: &str) {
        if let Some(p) = self.mode_settings.pending.take() {
            self.mode_settings.selected.insert(p.output, p.previous);
            self.mode_settings.message = Some(message.into());
            self.reconfigure_modes();
        }
    }

    fn mode_request(&mut self, request: Request) -> Result<(), String> {
        match request {
            Request::Preview { output, mode } => {
                if self.mode_settings.pending.is_some() {
                    return Err("Confirm or revert the current display change first.".into());
                }
                if !self.native_outputs_ready() {
                    return Err("Displays are not ready for a mode change.".into());
                }
                let catalog = self.catalog();
                let head = catalog
                    .outputs
                    .iter()
                    .find(|o| o.id == output)
                    .ok_or("Display is no longer connected.")?;
                if !head.modes.iter().any(|m| m.id == mode) {
                    return Err("That mode is no longer supported.".into());
                }
                if mode == head.current.id {
                    return Ok(());
                }
                let device = self
                    .opened_devices
                    .values()
                    .find(|d| {
                        d.output_targets
                            .iter()
                            .any(|t| t.connector_name() == output)
                    })
                    .unwrap()
                    .token;
                self.mode_settings.serial += 1;
                self.mode_settings.pending = Some(Pending {
                    token: self.mode_settings.serial,
                    output: output.clone(),
                    previous: head.current.id.clone(),
                    requested: mode.clone(),
                    deadline: Instant::now() + Duration::from_secs(5),
                    ready: false,
                    device,
                });
                self.mode_settings.selected.insert(output, mode);
                self.mode_settings.message = None;
                self.reconfigure_modes();
            }
            Request::Keep { token } => {
                if !self.recovery_complete() {
                    return Err("The new display mode has not been presented.".into());
                }
                self.mode_settings.confirm(token, Instant::now())?;
            }
            Request::Revert { token } => {
                if self
                    .mode_settings
                    .pending
                    .as_ref()
                    .is_none_or(|p| p.token != token)
                {
                    return Err("That display preview has expired.".into());
                }
                self.revert_mode("Previous display mode restored.");
            }
        }
        Ok(())
    }

    fn tick_mode(&mut self, locked: bool) {
        let Some(p) = &self.mode_settings.pending else {
            return;
        };
        let valid = self.opened_devices.values().any(|d| {
            d.token == p.device
                && d.output_targets.iter().any(|t| {
                    t.connector_name() == p.output
                        && KmsOutputTarget::describe_mode(t.mode).id == p.requested
                })
        });
        let presented = self.recovery_complete();
        let invalid = locked || !self.session_active || !valid;
        if self
            .mode_settings
            .pending
            .as_mut()
            .unwrap()
            .poll(Instant::now(), presented, invalid)
        {
            self.revert_mode("Display change reverted automatically.");
        }
    }
}

impl CalloopData {
    pub(in crate::compositor) fn run_display_mode_command(
        &mut self,
        text: &str,
    ) -> Option<ControlReply> {
        if text != "display-modes" && !text.starts_with("display-mode ") {
            return None;
        }
        if !self.capture_active() || self.state.session_locked() {
            return Some(ControlReply::Err(
                "Display controls are unavailable while the session is inactive or locked.".into(),
            ));
        }
        let Some(backend) = udev_backend_mut(self) else {
            return Some(if text == "display-modes" {
                ControlReply::Payload(serde_json::to_vec(&Snapshot::default()).unwrap())
            } else {
                ControlReply::Err("Mode changes require the native display backend.".into())
            });
        };
        if let Some(payload) = text.strip_prefix("display-mode ") {
            let request = match serde_json::from_str(payload) {
                Ok(v) => v,
                Err(e) => return Some(ControlReply::Err(format!("Invalid mode request: {e}"))),
            };
            backend.tick_mode(false);
            if let Err(error) = backend.mode_request(request) {
                return Some(ControlReply::Err(error));
            }
            let sync = backend.connector_sync();
            apply_connector_sync(self, sync);
            self.state.request_redraw();
            let handle = self.state.loop_handle.clone();
            // One timer per active transaction; Keep/Revert makes it drop.
            if let Some(backend) = udev_backend_mut(self)
                && backend.mode_settings.pending.is_some()
            {
                let token = backend.mode_settings.pending.as_ref().unwrap().token;
                if let Err(e) = handle.insert_source(
                    Timer::from_duration(Duration::from_millis(100)),
                    move |_, _, data| {
                        let locked = data.state.session_locked();
                        let Some(backend) = udev_backend_mut(data) else {
                            return TimeoutAction::Drop;
                        };
                        if backend
                            .mode_settings
                            .pending
                            .as_ref()
                            .is_none_or(|p| p.token != token)
                        {
                            return TimeoutAction::Drop;
                        }
                        backend.tick_mode(locked);
                        let active = backend.mode_settings.pending.is_some();
                        let sync = backend.connector_sync();
                        apply_connector_sync(data, sync);
                        data.state.request_redraw();
                        if active {
                            TimeoutAction::ToDuration(Duration::from_millis(100))
                        } else {
                            TimeoutAction::Drop
                        }
                    },
                ) {
                    if let Some(b) = udev_backend_mut(self) {
                        b.revert_mode("Could not start display rollback timer.");
                    }
                    return Some(ControlReply::Err(e.to_string()));
                }
            }
        }
        let backend = udev_backend_mut(self).unwrap();
        Some(ControlReply::Payload(
            serde_json::to_vec(&backend.catalog()).unwrap(),
        ))
    }
}

impl Pending {
    fn poll(&mut self, now: Instant, presented: bool, invalid: bool) -> bool {
        if invalid || now >= self.deadline {
            return true;
        }
        if !self.ready && presented {
            self.ready = true;
            self.deadline = now + Duration::from_secs(15);
        }
        false
    }

    fn can_keep(&self, token: u64, now: Instant) -> bool {
        self.token == token && self.ready && now < self.deadline
    }
}

#[cfg(test)]
mod tests;
