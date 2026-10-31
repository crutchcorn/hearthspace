use super::{UdevBackendState, device::KmsOutputSurface, gpu::GpuState, transfer::render_pass};
use crate::compositor::{
    App,
    capture::{CaptureState, CursorOverlay},
};
use smithay::{backend::drm::DrmNode, reexports::drm::control::crtc};
use std::time::Instant;
use tracing::{trace, warn};

impl UdevBackendState {
    pub(in crate::compositor) fn capture_toplevels(&mut self, state: &mut App) {
        if !self.capture_active() || !state.capture_available() {
            state.remote_capture.stop();
            return;
        }
        if let Some(node) = self.gpus.scene_node {
            match self.gpus.manager.single_renderer(&node) {
                Ok(mut renderer) => state.render_window_captures(renderer.as_mut()),
                Err(error) => {
                    warn!(%error, "native window capture renderer unavailable");
                    state.remote_capture.stop();
                }
            }
        }
    }

    pub(in crate::compositor) fn capture_active(&self) -> bool {
        self.session_active && !self.drm_commits_paused && self.kms_devices_active
    }

    pub(in crate::compositor) fn render_frame(
        &mut self,
        state: &mut App,
        force_full_redraw: bool,
        captures: &mut CaptureState,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        self.schedule_bridge_poll();
        if state.session_locked() {
            captures.invalidate("session is locked");
            state.remote_capture.stop();
        }
        if self.drm_commits_paused {
            state.set_native_lock_outputs_available(self.native_outputs_ready());
            state.remote_capture.stop();
            for device in self.opened_devices.values_mut() {
                device.mark_all_surfaces_dirty();
            }
            return Ok(false);
        }
        let mut submitted = false;
        let mut failed = false;
        for device in self.opened_devices.values_mut() {
            if !device.active {
                continue;
            }
            let cursor = device.capture_cursor(state);
            for output in &mut device.output_surfaces {
                if output.presentation.pending {
                    output.presentation.dirty = true;
                    continue;
                }
                match render_output(
                    &mut self.gpus,
                    device.node,
                    output,
                    state,
                    force_full_redraw,
                    captures,
                    cursor.as_ref(),
                ) {
                    Ok(queued) => submitted |= queued,
                    Err(error) => {
                        let name = output.target.connector_name();
                        self.gpus.bridges.retire_output(name);
                        captures.fail(name, &format!("native presentation failed: {error}"));
                        state.remote_capture.fail_output(name);
                        output.scene = None;
                        output.presentation.dirty = true;
                        device.recovery_needed = true;
                        failed = true;
                        warn!(%error, device_id = device.token.id, incarnation = device.token.incarnation,
                            crtc = ?output.target.crtc, output = name, "native output failed; continuing healthy outputs");
                    }
                }
            }
        }
        // This frame may have created the first pending bridge work. Arm its
        // retirement timer now, even if no later redraw is requested.
        self.schedule_bridge_poll();
        self.refresh_backend_state();
        state.set_native_lock_outputs_available(self.native_outputs_ready());
        if failed {
            self.schedule_recovery_timer();
        }
        Ok(submitted)
    }
}

fn render_output(
    gpus: &mut GpuState,
    node: DrmNode,
    output: &mut KmsOutputSurface,
    state: &mut App,
    force_full_redraw: bool,
    captures: &mut CaptureState,
    cursor: Option<&(crtc::Handle, CursorOverlay)>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let Some(view) = state.output_render_view(output.target.connector_name()) else {
        output.presentation.dirty = true;
        warn!(
            output = output.target.connector_name(),
            "native output has no Wayland view"
        );
        return Ok(false);
    };
    let (mut dmabuf, buffer_age) = output.gbm_surface.next_buffer()?;
    let wants_clean =
        state.software_cursor_visible && state.remote_capture.wants_cursorless(&view.name);
    let clean = if wants_clean {
        let visible = std::mem::replace(&mut state.software_cursor_visible, false);
        let result = render_pass(gpus, node, output, &mut dmabuf, state, &view, 0, true, true);
        state.software_cursor_visible = visible;
        // A transfer/render failure can leave submitted work with uncertain
        // completion. Do not restore into the same scanout via another route.
        // Readback alone remains recoverable after a successful render pass.
        match result?.pixels {
            Ok(pixels) => pixels,
            Err(error) => {
                warn!(%error, "cursor-free remote readback failed");
                state.remote_capture.fail_output(&view.name);
                None
            }
        }
    } else {
        None
    };
    // Always restore the full scene after a temporary cursorless pass, even if
    // its readback failed. The final fence belongs to the restored image.
    let force = force_full_redraw || state.software_cursor_visible || wants_clean;
    let wanted = captures.wanted(&view.name) || state.remote_capture.wanted(&view.name);
    let rendered = render_pass(
        gpus,
        node,
        output,
        &mut dmabuf,
        state,
        &view,
        usize::from(buffer_age),
        force,
        wanted,
    )?;
    // Keep renderer-side damage, but do not send FB_DAMAGE_CLIPS to KMS. The
    // final fence is source GLES on same-GPU, target completion on a transfer.
    let submit_started = Instant::now();
    output
        .gbm_surface
        .queue_buffer(Some(rendered.sync), None, ())?;
    let submit_duration = submit_started.elapsed();
    let pixels = match rendered.pixels {
        Ok(pixels) => pixels,
        Err(error) => {
            captures.fail(&view.name, &format!("native readback failed: {error}"));
            state.remote_capture.fail_output(&view.name);
            None
        }
    };
    if let Some(mut pixels) = pixels {
        let size = output.target.output_size();
        let plain = clean.unwrap_or_else(|| pixels.clone());
        let mut cursor_source = "framebuffer";
        if let Some((crtc, cursor)) = cursor
            && *crtc == output.gbm_surface.crtc()
        {
            cursor.paint(&mut pixels, size);
            cursor_source = "hardware-overlay";
        }
        if state.remote_capture.wanted(&view.name) {
            state
                .remote_capture
                .stage(&view.name, size, plain, Some(pixels.clone()));
        }
        if captures.wanted(&view.name) {
            if let Some(metadata) = state
                .capture_outputs()
                .into_iter()
                .find(|o| o.name == view.name)
            {
                if let Err(error) = captures.stage_pixels(metadata, size, pixels, cursor_source) {
                    captures.fail(&view.name, &format!("native encoding failed: {error}"));
                }
            } else {
                captures.fail(&view.name, "capture output disappeared");
            }
        }
    }
    output.presentation.queued(state.lock_frame_generation());
    trace!(device = ?node, scene_node = ?gpus.scene_node, target_node = ?node,
        crtc = ?output.target.crtc, name = view.name, age = if force { 0 } else { buffer_age },
        submit_us = submit_duration.as_micros() as u64, render_us = started.elapsed().as_micros() as u64,
        "queued native frame");
    Ok(true)
}
