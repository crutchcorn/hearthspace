//! Native wake recovery is explicit: system sleep does not necessarily cause a
//! libseat deactivate/activate pair. Never infer success from receiving a signal.
use std::time::{Duration, Instant};

use smithay::{
    backend::session::Session,
    reexports::calloop::timer::{TimeoutAction, Timer},
};
use tracing::{error, info};

use super::{UdevBackendState, apply_connector_sync, udev_backend_mut};
use crate::compositor::{CalloopData, shell_integration::ControlReply};

const RECOVERY_INTERVAL: Duration = Duration::from_millis(250);
const RECOVERY_ATTEMPTS: u8 = 20;
const RECOVERY_BUDGET: Duration = Duration::from_secs(5);
pub(super) const CONFIRMATION_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, PartialEq, Eq)]
enum RecoveryStep {
    Rebuild,
    Exhausted,
}

/// Driver calls are synchronous and cannot be interrupted, but a slow attempt
/// must consume the wall-clock budget rather than start another five seconds
/// of retries. The deadline starts before the initial activation.
struct RecoveryBudget {
    deadline: Instant,
    remaining: u8,
}

impl RecoveryBudget {
    fn new(started: Instant) -> Self {
        Self {
            deadline: started + RECOVERY_BUDGET,
            remaining: RECOVERY_ATTEMPTS,
        }
    }

    fn next_step(&mut self, now: Instant) -> RecoveryStep {
        if now >= self.deadline || self.remaining == 0 {
            return RecoveryStep::Exhausted;
        }
        self.remaining -= 1;
        RecoveryStep::Rebuild
    }
}

impl UdevBackendState {
    pub(in crate::compositor) fn recovery_complete(&self) -> bool {
        self.native_outputs_ready()
            && self.input_source.is_some()
            && self.opened_devices.values().all(|device| {
                device.output_surfaces.len() == device.output_targets.len()
                    && (device.output_targets.is_empty() || !self.drm_commits_paused)
                    && device
                        .output_surfaces
                        .iter()
                        .all(|output| output.presentation.presented_once)
            })
    }
}

impl CalloopData {
    pub(in crate::compositor) fn resume_native_session(&mut self) -> ControlReply {
        let Some(backend) = udev_backend_mut(self) else {
            return ControlReply::Err("session-resume requires the native backend".into());
        };
        if !backend.session_active || !backend.session.is_active() {
            // A different VT still owns the devices. Its eventual libseat
            // activation will rebuild our scanout without disturbing that VT.
            self.state.set_native_lock_outputs_available(false);
            self.state.reset_session_input();
            self.state.invalidate_lock_presentation();
            return ControlReply::Ok;
        }
        backend.pause_session();
        activate_session(self);
        ControlReply::Ok
    }
}

pub(super) fn activate_session(data: &mut CalloopData) {
    let budget = RecoveryBudget::new(Instant::now());
    data.state.set_native_lock_outputs_available(false);
    data.state.set_remote_input_active(false);
    data.state.remote_capture.stop();
    data.state.reset_session_input();
    data.state.invalidate_lock_presentation();
    data.captures.invalidate("native session is recovering");
    let Some(backend) = udev_backend_mut(data) else {
        return;
    };
    if let Some(token) = backend.recovery_source.take() {
        backend.loop_handle.remove(token);
    }
    backend.recovery_exhausted = false;
    let connector_sync = backend.activate_session();
    apply_connector_sync(data, connector_sync);
    data.state.request_redraw();
    if let Some(backend) = udev_backend_mut(data) {
        backend.start_recovery_timer(budget);
    }
}

pub(super) fn schedule_recovery(data: &mut CalloopData) {
    if let Some(backend) = udev_backend_mut(data) {
        // A new hotplug event may resolve the condition that exhausted retries.
        backend.recovery_exhausted = false;
        backend.schedule_recovery_timer();
    }
}

impl UdevBackendState {
    pub(super) fn schedule_recovery_timer(&mut self) {
        self.start_recovery_timer(RecoveryBudget::new(Instant::now()));
    }

    fn start_recovery_timer(&mut self, mut budget: RecoveryBudget) {
        if self.recovery_source.is_some()
            || self.recovery_exhausted
            || self.recovery_complete()
            || !self.session_active
        {
            return;
        }
        match self.loop_handle.insert_source(
            Timer::from_duration(RECOVERY_INTERVAL),
            move |_, _, data| {
                let Some(backend) = udev_backend_mut(data) else {
                    return TimeoutAction::Drop;
                };
                if !backend.session_active || !backend.session.is_active() {
                    backend.recovery_source = None;
                    return TimeoutAction::Drop;
                }
                if backend.recovery_complete() {
                    backend.recovery_source = None;
                    info!("native session recovery presented on every output");
                    return TimeoutAction::Drop;
                }
                match budget.next_step(Instant::now()) {
                    RecoveryStep::Exhausted => {
                        backend.recovery_source = None;
                        backend.recovery_exhausted = true;
                        error!("native session recovery exhausted its bounded retries");
                        return TimeoutAction::Drop;
                    }
                    RecoveryStep::Rebuild => {}
                }
                // Setup success alone is insufficient: a failed first render or a
                // missing page flip must retry too, without depending on new input.
                data.state.set_native_lock_outputs_available(false);
                let Some(backend) = udev_backend_mut(data) else {
                    return TimeoutAction::Drop;
                };
                let sync = backend.activate_session();
                data.state.invalidate_lock_presentation();
                apply_connector_sync(data, sync);
                data.state.request_redraw();
                TimeoutAction::ToDuration(RECOVERY_INTERVAL)
            },
        ) {
            Ok(token) => self.recovery_source = Some(token),
            Err(error) => error!(%error, "failed to schedule native session recovery"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_driver_calls_consume_the_absolute_recovery_budget() {
        let started = Instant::now();
        let mut budget = RecoveryBudget::new(started);
        assert_eq!(
            budget.next_step(started + Duration::from_millis(2_700)),
            RecoveryStep::Rebuild
        );
        assert_eq!(
            budget.next_step(started + Duration::from_millis(5_400)),
            RecoveryStep::Exhausted
        );
    }

    #[test]
    fn initial_activation_can_exhaust_the_deadline_without_further_rebuilds() {
        let started = Instant::now();
        let mut budget = RecoveryBudget::new(started);
        assert_eq!(
            budget.next_step(started + RECOVERY_BUDGET),
            RecoveryStep::Exhausted
        );
    }

    #[test]
    fn retry_count_also_bounds_attempts_if_time_does_not_advance() {
        let started = Instant::now();
        let mut budget = RecoveryBudget::new(started);
        for _ in 0..RECOVERY_ATTEMPTS {
            assert_eq!(budget.next_step(started), RecoveryStep::Rebuild);
        }
        assert_eq!(budget.next_step(started), RecoveryStep::Exhausted);
    }
}
