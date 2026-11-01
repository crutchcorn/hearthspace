//! Retirement must progress while output rendering and seat ownership are idle.
use super::{UdevBackendState, udev_backend_mut};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use std::time::Duration;

const INTERVAL: Duration = Duration::from_millis(16);

impl UdevBackendState {
    pub(super) fn schedule_bridge_poll(&mut self) {
        self.gpus.bridges.poll();
        if self.bridge_poll_source.is_some()
            || self.bridge_poll_failed
            || !self.gpus.bridges.needs_poll()
        {
            return;
        }
        match self
            .loop_handle
            .insert_source(Timer::from_duration(INTERVAL), |_, _, data| {
                let Some(backend) = udev_backend_mut(data) else {
                    return TimeoutAction::Drop;
                };
                backend.gpus.bridges.poll();
                // Deliberately independent of session_active/recovery state. No
                // drawing or KMS operations occur in this callback.
                if backend.gpus.bridges.needs_poll() {
                    TimeoutAction::ToDuration(INTERVAL)
                } else {
                    backend.bridge_poll_source = None;
                    TimeoutAction::Drop
                }
            }) {
            Ok(token) => self.bridge_poll_source = Some(token),
            Err(error) => {
                self.bridge_poll_failed = true;
                self.gpus.bridges.stop();
                tracing::warn!(%error, "Vulkan transfer polling unavailable; retaining work and using Smithay");
            }
        }
    }
}
