use super::{
    UdevBackendState,
    event_cutoff::{SurfaceEvent, SurfaceEventCutoff},
    identity::DeviceToken,
};
use crate::compositor::{App, capture::CaptureState};
use smithay::{
    backend::drm::{DrmEventMetadata, DrmEventTime},
    reexports::drm::control::crtc,
    utils::{Clock, Monotonic},
};
use std::time::Instant;
use tracing::{debug, trace};

/// Presentation ownership stays with the surface, independently of its GPU
/// resources. The completion test substitutes only GBM's external acknowledgement.
pub(super) struct OutputPresentation {
    device: DeviceToken,
    crtc: u32,
    name: String,
    cutoff: SurfaceEventCutoff,
    pub(super) pending: bool,
    pub(super) dirty: bool,
    pub(super) presented_once: bool,
    lock_generation: Option<u64>,
}

impl OutputPresentation {
    pub(super) fn new(
        device: DeviceToken,
        crtc: u32,
        name: String,
        cutoff: SurfaceEventCutoff,
    ) -> Self {
        Self {
            device,
            crtc,
            name,
            cutoff,
            pending: false,
            dirty: true,
            presented_once: false,
            lock_generation: None,
        }
    }

    pub(super) fn queued(&mut self, lock_generation: Option<u64>) {
        self.lock_generation = lock_generation;
        self.pending = true;
        self.dirty = false;
    }
}

pub(super) struct NativeFrameCompletion {
    pub(super) output: String,
    pub(super) redraw: Option<bool>,
}

impl NativeFrameCompletion {
    pub(super) fn dispatch<E>(
        &self,
        send: impl FnOnce(&str) -> Result<(), E>,
    ) -> Result<Option<bool>, E> {
        send(&self.output)?;
        Ok(self.redraw)
    }
}

enum Submission {
    Ignored,
    Untimed(String),
    Verified {
        completion: NativeFrameCompletion,
        lock_generation: Option<u64>,
    },
}

/// Route the event, acknowledge its buffer, transition the real output states,
/// and calculate aggregate redraw eligibility. `submit` is the GBM boundary;
/// production passes GbmBufferedSurface::frame_submitted, tests a driver result.
fn complete_frame<'a, Buffer: 'a>(
    outputs: impl Iterator<Item = (&'a mut OutputPresentation, &'a mut Buffer)>,
    device: DeviceToken,
    crtc: u32,
    metadata: Option<&DrmEventMetadata>,
    submit: impl FnOnce(&mut Buffer) -> Result<bool, Box<dyn std::error::Error>>,
) -> Result<Submission, Box<dyn std::error::Error>> {
    let mut submit = Some(submit);
    let mut result = Submission::Ignored;
    let mut pending = false;
    let mut dirty = false;
    for (output, buffer) in outputs {
        if output.device == device && output.crtc == crtc && output.pending {
            let event = output.cutoff.classify(metadata);
            if event != SurfaceEvent::Stale
                && let Some(submit) = submit.take()
                && submit(buffer)?
            {
                output.pending = false;
                let generation = output.lock_generation.take();
                if event == SurfaceEvent::Untimed {
                    output.dirty = true;
                    result = Submission::Untimed(output.name.clone());
                } else {
                    output.presented_once = true;
                    result = Submission::Verified {
                        completion: NativeFrameCompletion {
                            output: output.name.clone(),
                            redraw: None,
                        },
                        lock_generation: generation,
                    };
                }
            }
        }
        pending |= output.pending;
        dirty |= output.dirty;
    }
    if let Submission::Verified { completion, .. } = &mut result {
        // Never discard a verified completion because another GPU is pending.
        // Only the compositor redraw gate aggregates all output state.
        completion.redraw = (!pending).then_some(dirty);
    }
    Ok(result)
}

impl UdevBackendState {
    pub(super) fn frame_submitted(
        &mut self,
        token: DeviceToken,
        crtc: crtc::Handle,
        metadata: Option<&DrmEventMetadata>,
        state: &mut App,
        captures: &mut CaptureState,
    ) -> Result<Option<NativeFrameCompletion>, Box<dyn std::error::Error>> {
        if !self.session_active {
            return Ok(None);
        }
        let result = complete_frame(
            self.opened_devices
                .values_mut()
                .filter(|device| device.active)
                .flat_map(|device| &mut device.output_surfaces)
                .map(|output| (&mut output.presentation, &mut output.gbm_surface)),
            token,
            crtc.into(),
            metadata,
            |surface| Ok(surface.frame_submitted()?.is_some()),
        )?;
        match result {
            Submission::Ignored => Ok(None),
            Submission::Untimed(name) => {
                state.remote_capture.fail_output(&name);
                captures.fail(
                    &name,
                    "native presentation timestamp unavailable; retry capture",
                );
                self.recovery_progress_at = Some(Instant::now());
                state.request_redraw();
                debug!(
                    device_id = token.id,
                    incarnation = token.incarnation,
                    ?crtc,
                    output = name,
                    "untimed native modeset completed; requesting a verified page flip"
                );
                Ok(None)
            }
            Submission::Verified {
                completion,
                lock_generation,
            } => {
                if state.session_locked() {
                    captures.invalidate("session is locked");
                    state.remote_capture.stop();
                } else {
                    captures.presented(&completion.output, "page-flip");
                    let timestamp = match metadata.map(|m| m.time) {
                        Some(DrmEventTime::Monotonic(time)) => time,
                        _ => Clock::<Monotonic>::new().now().into(),
                    };
                    state
                        .remote_capture
                        .presented(&completion.output, timestamp);
                }
                if let Some(generation) = lock_generation {
                    state.lock_frame_presented(&completion.output, generation);
                }
                trace!(
                    device_id = token.id,
                    incarnation = token.incarnation,
                    ?crtc,
                    output = completion.output,
                    other_frames_pending = completion.redraw.is_none(),
                    "verified native output presentation"
                );
                Ok(Some(completion))
            }
        }
    }
}

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod tests;
