//! Native wake recovery is explicit: system sleep does not necessarily cause a
//! libseat deactivate/activate pair. Never infer success from receiving a signal.
use std::time::{Duration, Instant};

use smithay::{
    backend::{
        drm::{DrmDeviceNotifier, DrmEvent},
        session::Session,
    },
    reexports::calloop::{
        LoopHandle, RegistrationToken,
        timer::{TimeoutAction, Timer},
    },
};
use tracing::{error, info, trace, warn};

use super::{UdevBackendState, apply_connector_sync, udev_backend_mut};
use crate::compositor::{CalloopData, shell_integration::ControlReply};

const RECOVERY_INTERVAL: Duration = Duration::from_millis(250);
const RECOVERY_ATTEMPTS: u8 = 20;
const RECOVERY_BUDGET: Duration = Duration::from_secs(5);

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

    fn next_attempt(&mut self, now: Instant) -> bool {
        if now >= self.deadline || self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

impl UdevBackendState {
    pub(in crate::compositor) fn recovery_complete(&self) -> bool {
        self.session_active
            && self.session.is_active()
            && self.input_source.is_some()
            && !self.recovery_needed
            && self.primary_device.as_ref().is_some_and(|device| {
                device.output_surfaces.len() == device.output_targets.len()
                    && (device.output_targets.is_empty() || !self.drm_commits_paused)
                    && device
                        .output_surfaces
                        .iter()
                        .all(|output| output.presented_once)
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
    let mut budget = RecoveryBudget::new(Instant::now());
    data.state.reset_session_input();
    data.state.invalidate_lock_presentation();
    data.captures.invalidate("native session is recovering");
    let Some(backend) = udev_backend_mut(data) else {
        return;
    };
    if let Some(token) = backend.recovery_source.take() {
        backend.loop_handle.remove(token);
    }
    let connector_sync = backend.activate_session();
    let retry = !backend.recovery_complete();
    let handle = backend.loop_handle.clone();
    apply_connector_sync(data, connector_sync);
    data.state.request_redraw();
    if !retry {
        return;
    }

    match handle.insert_source(
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
            if !budget.next_attempt(Instant::now()) {
                backend.recovery_source = None;
                error!("native session recovery exhausted its bounded retries");
                return TimeoutAction::Drop;
            }
            // Setup success alone is insufficient: a failed first render or a
            // missing page flip must retry too, without depending on new input.
            let sync = backend.activate_session();
            data.state.invalidate_lock_presentation();
            apply_connector_sync(data, sync);
            data.state.request_redraw();
            TimeoutAction::ToDuration(RECOVERY_INTERVAL)
        },
    ) {
        Ok(token) => {
            if let Some(backend) = udev_backend_mut(data) {
                backend.recovery_source = Some(token);
            }
        }
        Err(error) => error!(%error, "failed to schedule native session recovery"),
    }
}

pub(super) fn insert_drm_source(
    handle: &LoopHandle<'static, CalloopData>,
    notifier: DrmDeviceNotifier,
) -> Result<RegistrationToken, Box<dyn std::error::Error>> {
    Ok(
        handle.insert_source(notifier, |event, metadata, data| match event {
            DrmEvent::VBlank(crtc) => {
                trace!(?crtc, ?metadata, "DRM vblank");
                let submitted = if let super::super::Backend::Udev(backend) = &mut data.backend {
                    match backend.frame_submitted(
                        crtc,
                        metadata.as_ref(),
                        &mut data.state,
                        &mut data.captures,
                    ) {
                        Ok(result) => result,
                        Err(error) => {
                            error!(%error, "failed to mark native frame submitted");
                            None
                        }
                    }
                } else {
                    None
                };
                if let Some(should_redraw) = submitted {
                    if let Err(error) = data.send_frame_callbacks() {
                        warn!(%error, "failed to send native frame callbacks");
                    }
                    if should_redraw || data.state.viewport_animation.is_some() {
                        data.state.request_redraw();
                    }
                }
            }
            DrmEvent::Error(error) => error!(%error, "DRM event error"),
        })?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_driver_calls_consume_the_absolute_recovery_budget() {
        let started = Instant::now();
        let mut budget = RecoveryBudget::new(started);
        assert!(budget.next_attempt(started + Duration::from_millis(2_700)));
        assert!(!budget.next_attempt(started + Duration::from_millis(5_400)));
    }

    #[test]
    fn initial_activation_can_exhaust_the_deadline_without_further_rebuilds() {
        let started = Instant::now();
        let mut budget = RecoveryBudget::new(started);
        assert!(!budget.next_attempt(started + RECOVERY_BUDGET));
    }

    #[test]
    fn retry_count_also_bounds_attempts_if_time_does_not_advance() {
        let started = Instant::now();
        let mut budget = RecoveryBudget::new(started);
        for _ in 0..RECOVERY_ATTEMPTS {
            assert!(budget.next_attempt(started));
        }
        assert!(!budget.next_attempt(started));
    }
}
