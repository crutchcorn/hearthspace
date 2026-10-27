//! Capture admission is independent of virtual input enablement.
use super::*;

impl App {
    pub(in crate::compositor) fn capture_available(&self) -> bool {
        self.remote_capture.enabled && self.remote_input.active && !self.session_locked()
    }

    pub(super) fn capture_source_constraints(
        &self,
        source: &ImageCaptureSource,
    ) -> Option<BufferConstraints> {
        if let Some(output) = source_output(source) {
            if !self.capture_output_handles().contains(&output) {
                return None;
            }
            let size = output.current_mode()?.size;
            constraints((size.w, size.h).into())
        } else {
            let (_, geometry) = self.capture_window_geometry(source)?;
            constraints(geometry.size)
        }
    }

    pub(in crate::compositor) fn maintain_remote_capture(&mut self, active: bool) {
        if !active || !self.capture_available() {
            self.remote_capture.stop();
            return;
        }
        // Take ownership so source validation can inspect the live window owner.
        let sessions = std::mem::take(&mut self.remote_capture.sessions);
        for session in sessions {
            if !session.alive() {
                continue;
            }
            let Some(constraints) = self.capture_source_constraints(&session.source()) else {
                continue; // Drop sends stopped and fails an outstanding frame.
            };
            if session
                .current_constraints()
                .is_some_and(|old| old.size != constraints.size)
            {
                session.update_constraints(constraints);
                for request in self
                    .remote_capture
                    .requests
                    .extract_if(.., |r| r.session == session.as_ref())
                {
                    request.frame.fail(Failure::BufferConstraints);
                }
                for request in self
                    .remote_capture
                    .window_requests
                    .extract_if(.., |r| r.session == session.as_ref())
                {
                    request.frame.fail(Failure::BufferConstraints);
                }
            }
            self.remote_capture.sessions.push(session);
        }
        self.remote_capture
            .requests
            .retain(|r| r.session.alive() && !r.frame.has_failed() && r.deadline > Instant::now());
        self.remote_capture
            .window_requests
            .retain(|r| r.session.alive() && !r.frame.has_failed() && r.deadline > Instant::now());
        if self
            .remote_capture
            .requests
            .iter()
            .any(|r| r.pixels.is_none())
            || !self.remote_capture.window_requests.is_empty()
        {
            self.request_redraw();
        }
    }
}
