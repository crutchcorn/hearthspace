//! One composited attempt after a selected client or cursor plane is rejected at
//! queue time. Smithay still owns all earlier plane-assignment test fallback.
use super::output::RenderOutcome;
use smithay::backend::drm::{DrmError, compositor::FrameFlags};
use std::io::{self, ErrorKind};

#[derive(Debug)]
enum PlaneRejection {
    InvalidInput,
    TestFailed,
}

#[derive(Debug)]
pub(super) struct AttemptError {
    message: String,
    plane_rejection: Option<PlaneRejection>,
}

impl From<String> for AttemptError {
    fn from(message: String) -> Self {
        Self {
            message,
            plane_rejection: None,
        }
    }
}

impl AttemptError {
    /// Called only for FrameError::DrmError returned by queue_frame, with the
    /// plane selection copied from that attempt's actual RenderFrameResult.
    pub fn queue(scanout_plane: bool, error: DrmError) -> Self {
        let message = error.to_string();
        match error {
            DrmError::Access(access) => Self::access(scanout_plane, &access.source, message),
            DrmError::TestFailed(_) if scanout_plane => Self {
                message,
                plane_rejection: Some(PlaneRejection::TestFailed),
            },
            _ => message.into(),
        }
    }

    fn access(scanout_plane: bool, error: &io::Error, message: String) -> Self {
        Self {
            message,
            plane_rejection: (scanout_plane && error.kind() == ErrorKind::InvalidInput)
                .then_some(PlaneRejection::InvalidInput),
        }
    }
}

/// External operations for a complete render, capture, and queue attempt. The
/// native implementation keeps the source lease and immutable cursor snapshot
/// on its output throughout both attempts; tests substitute only these ops.
pub(super) trait AttemptOps {
    fn permitted(&self) -> bool;
    fn reset_buffer_ages(&mut self);
    fn attempt(&mut self, flags: FrameFlags) -> Result<RenderOutcome, AttemptError>;
}

/// Overlay planes stay disabled: only the eligible fullscreen client and the
/// independent cursor can supply storage across the presenter boundary.
pub(super) fn scanout_flags() -> FrameFlags {
    FrameFlags::ALLOW_PRIMARY_PLANE_SCANOUT | FrameFlags::ALLOW_CURSOR_PLANE_SCANOUT
}

pub(super) fn render_and_queue(ops: &mut impl AttemptOps) -> Result<RenderOutcome, String> {
    if !ops.permitted() {
        return Ok(RenderOutcome::Revoked);
    }
    match ops.attempt(scanout_flags()) {
        Err(first) if first.plane_rejection.is_some() => {
            if !ops.permitted() {
                return Ok(RenderOutcome::Revoked);
            }
            tracing::trace!(target: "hearthspace::presenter",
                rejection = ?first.plane_rejection, error = %first.message,
                "retrying rejected plane queue once with composition");
            // A rejected queue can advance Smithay's buffer ages and rendering
            // damage history. Force a fresh primary render and read fence.
            ops.reset_buffer_ages();
            if !ops.permitted() {
                return Ok(RenderOutcome::Revoked);
            }
            // Deliberately not recursive: even a second eligible rejection is
            // final. Each attempt rerenders and captures its own frame result.
            match ops.attempt(FrameFlags::empty()) {
                Ok(RenderOutcome::Empty) => Err(format!(
                    "plane queue rejected ({}); composition retry produced no submission",
                    first.message
                )),
                Err(second) => Err(format!(
                    "plane queue rejected ({}); composition retry failed: {}",
                    first.message, second.message
                )),
                Ok(outcome) => Ok(outcome),
            }
        }
        result => result.map_err(|error| error.message),
    }
}

#[cfg(test)]
#[path = "fallback_test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "fallback_tests.rs"]
mod tests;
