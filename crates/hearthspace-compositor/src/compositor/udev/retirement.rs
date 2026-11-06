//! Never join a presenter from a hotplug or session callback.
use super::{UdevBackendState, apply_connector_sync, udev_backend_mut};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use std::time::Duration;

impl UdevBackendState {
    pub(super) fn schedule_retirement(&mut self) {
        if self.retirement_source.is_some() || self.retired_devices.is_empty() {
            return;
        }
        match self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(50)),
            |_, _, data| {
                let Some(backend) = udev_backend_mut(data) else {
                    return TimeoutAction::Drop;
                };
                let before = backend.retired_devices.len();
                // Finished workers have dropped their renderer, notifier and DRM
                // surfaces. Dropping the main record now releases libseat last.
                backend.retired_devices.retain(|device| {
                    !device.presenter.is_finished()
                        || device.output_surfaces.iter().any(|output| {
                            output
                                .rejected_scene
                                .as_ref()
                                .is_some_and(|frame| !frame.ready.is_reached())
                        })
                });
                let changed = backend.retired_devices.len() != before;
                let done = backend.retired_devices.is_empty();
                if done {
                    backend.retirement_source = None;
                }
                if changed && backend.session_active {
                    let sync = backend.activate_session();
                    backend.schedule_recovery_timer();
                    apply_connector_sync(data, sync);
                    data.state.request_redraw();
                }
                if done {
                    TimeoutAction::Drop
                } else {
                    TimeoutAction::ToDuration(Duration::from_millis(50))
                }
            },
        ) {
            Ok(token) => self.retirement_source = Some(token),
            Err(error) => tracing::error!(%error, "failed to schedule native device retirement"),
        }
    }
}
