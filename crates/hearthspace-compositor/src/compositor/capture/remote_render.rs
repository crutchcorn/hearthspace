use smithay::utils::{Clock, Monotonic};

#[cfg(feature = "udev")]
use super::Backend;
use super::CalloopData;

impl CalloopData {
    /// Nested/headless capture has the same offscreen contract as agent capture.
    /// Native capture is handled by the scanout submission and page-flip paths.
    pub(super) fn capture_remote_offscreen(&mut self) {
        #[cfg(feature = "udev")]
        if matches!(self.backend, Backend::Udev(_)) {
            return;
        }
        let Some(output) = self.state.capture_outputs().into_iter().next() else {
            return;
        };
        if !self.state.remote_capture.wanted(&output.name) {
            return;
        }
        let result = (|| {
            let plain = self.screenshot_pixels(false)?;
            let painted = self.screenshot_pixels(true)?;
            Ok::<_, Box<dyn std::error::Error>>((plain, painted))
        })();
        match result {
            Ok((plain, painted)) => {
                self.state.remote_capture.stage(
                    &output.name,
                    (output.width, output.height).into(),
                    plain,
                    Some(painted),
                );
                self.state
                    .remote_capture
                    .presented(&output.name, Clock::<Monotonic>::new().now().into());
            }
            Err(error) => {
                tracing::warn!(%error, "remote capture readback failed");
                self.state.remote_capture.fail_output(&output.name);
            }
        }
        // A separate damage tracker rendered this framebuffer, including a
        // possible cursor pass, so the regular backend must redraw it fully.
        self.full_redraw = self.full_redraw.max(1);
    }
}
