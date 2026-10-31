//! Failed clears need progress even when input and primary presentation are idle.
use smithay::reexports::calloop::{
    LoopHandle, RegistrationToken,
    timer::{TimeoutAction, Timer},
};
use std::time::Duration;

use crate::compositor::{Backend, CalloopData, udev::UdevBackendState};

pub(in crate::compositor::udev) const INTERVAL: Duration = Duration::from_millis(50);

impl UdevBackendState {
    pub(super) fn cursor_retry_pending(&self) -> bool {
        self.opened_devices
            .values()
            .filter(|device| device.active)
            .any(|device| device.cursor_plane.clear_retry_pending())
    }

    pub(super) fn schedule_cursor_retry(&mut self) {
        if self.cursor_retry_source.is_some()
            || self.drm_commits_paused
            || !self.cursor_retry_pending()
        {
            return;
        }
        match insert_retry(&self.loop_handle, retry_cursor) {
            Ok(token) => self.cursor_retry_source = Some(token),
            Err(error) => {
                // Do not retry registration every input tick either. Lifecycle
                // reset provides a fresh bounded opportunity.
                for device in self.opened_devices.values_mut() {
                    device.cursor_plane.stop_clear_retries();
                }
                tracing::warn!(%error, "failed to schedule native cursor clear retry");
            }
        }
    }

    pub(in crate::compositor::udev) fn cancel_cursor_retry(&mut self) {
        if let Some(token) = self.cursor_retry_source.take() {
            self.loop_handle.remove(token);
        }
    }
}

fn retry_cursor(data: &mut CalloopData) -> bool {
    let CalloopData { backend, state, .. } = data;
    let Backend::Udev(backend) = backend else {
        return false;
    };
    if !backend.session_active || backend.drm_commits_paused {
        backend.cursor_retry_source = None;
        return false;
    }
    for device in backend
        .opened_devices
        .values_mut()
        .filter(|device| device.active)
    {
        device.cursor_plane.retry_clear();
    }
    // Re-evaluate current intent/output rather than replaying a stale clear.
    // schedule_cursor_retry sees the existing token and cannot add another timer.
    backend.update_hardware_cursor(state);
    let pending = backend.cursor_retry_pending();
    if !pending {
        backend.cursor_retry_source = None;
    }
    pending
}

/// The real calloop boundary is also used by deterministic cursor regressions;
/// their callback uses the production cursor policy with substituted DRM ops.
pub(in crate::compositor::udev) fn insert_retry<Data: 'static>(
    handle: &LoopHandle<'static, Data>,
    mut retry: impl FnMut(&mut Data) -> bool + 'static,
) -> Result<RegistrationToken, Box<dyn std::error::Error>> {
    Ok(
        handle.insert_source(Timer::from_duration(INTERVAL), move |_, _, data| {
            if retry(data) {
                TimeoutAction::ToDuration(INTERVAL)
            } else {
                TimeoutAction::Drop
            }
        })?,
    )
}
