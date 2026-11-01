//! Produce cursor-free scenes; presentation and cursor composition are worker-owned.
use super::{
    UdevBackendState,
    device::KmsOutputSurface,
    gpu::GpuState,
    presenter::{Presenter, SceneFrame},
    transfer::render_pass,
};
use crate::compositor::{App, capture::CaptureState};
use smithay::backend::{allocator::dmabuf::AsDmabuf, drm::DrmNode};
use std::{sync::Arc, time::Instant};
use tracing::{trace, warn};

impl UdevBackendState {
    pub(in crate::compositor) fn scene_work_pending(&self) -> bool {
        !self.drm_commits_paused
            && self
                .opened_devices
                .values()
                .filter(|d| d.active)
                .any(|device| {
                    device.output_surfaces.iter().any(|output| {
                        output.failed_slot.is_none() && output.presentation.needs_scene()
                    })
                })
    }

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
        self.sync_security(state);
        state.software_cursor_visible = false;
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
        let changed = state.needs_redraw || force_full_redraw;
        let mut submitted = false;
        let mut failed = false;
        for device in self.opened_devices.values_mut().filter(|d| d.active) {
            for output in &mut device.output_surfaces {
                if !output.presentation.request_scene(changed) || output.failed_slot.is_some() {
                    continue;
                }
                self.next_scene_sequence = self
                    .next_scene_sequence
                    .checked_add(1)
                    .expect("scene sequence exhausted");
                match render_output(
                    &mut self.gpus,
                    device.node,
                    &device.presenter,
                    output,
                    state,
                    force_full_redraw,
                    captures,
                    self.next_scene_sequence,
                    self.security_epoch,
                ) {
                    Ok(queued) => submitted |= queued,
                    Err(error) => {
                        let name = output.target.connector_name();
                        captures.fail(name, &format!("native scene failed: {error}"));
                        state.remote_capture.fail_output(name);
                        if let Some(scene) = output.scene.as_mut() {
                            scene.invalidate();
                        }
                        output.presentation.dirty = true;
                        device.recovery_needed = true;
                        failed = true;
                        warn!(%error, device_id = device.token.id, incarnation = device.token.incarnation,
                            crtc = ?output.target.crtc, output = name, "native scene failed; continuing healthy outputs");
                    }
                }
            }
        }
        let rejected_devices: Vec<_> = self
            .opened_devices
            .iter()
            .filter(|(_, device)| {
                device
                    .output_surfaces
                    .iter()
                    .any(|o| o.rejected_scene.is_some())
            })
            .map(|(id, _)| *id)
            .collect();
        for id in rejected_devices {
            self.close_device(id);
        }
        self.refresh_backend_state();
        state.set_native_lock_outputs_available(self.native_outputs_ready());
        if failed {
            self.schedule_recovery_timer();
        }
        Ok(submitted)
    }
}

#[allow(clippy::too_many_arguments)]
fn render_output(
    gpus: &mut GpuState,
    node: DrmNode,
    presenter: &Presenter,
    output: &mut KmsOutputSurface,
    state: &mut App,
    force: bool,
    captures: &CaptureState,
    sequence: u64,
    security_epoch: u64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let Some(view) = state.output_render_view(output.target.connector_name()) else {
        output.presentation.dirty = true;
        return Ok(false);
    };
    let Some(slot) = output.pool.acquire()? else {
        // Every slot is leased by the presenter. Its capacity event wakes scene
        // production; input dispatch never waits for a KMS completion here.
        output.presentation.dirty = true;
        return Ok(false);
    };
    let slot = Arc::new(slot);
    let mut dmabuf = slot.export()?;
    let age = usize::from(slot.age());
    let rendered = match render_pass(gpus, node, output, &mut dmabuf, state, &view, age, force) {
        Ok(rendered) => rendered,
        Err(error) => {
            // No producer fence is guaranteed on error. Quarantine this slot
            // until the entire pool is retired; it must never be overwritten.
            output.failed_slot = Some(slot);
            return Err(error);
        }
    };
    output.pool.submitted(&slot);
    let wanted = captures.wanted(&view.name) || state.remote_capture.wanted(&view.name);
    let metadata = wanted
        .then(|| {
            state
                .capture_outputs()
                .into_iter()
                .find(|o| o.name == view.name)
        })
        .flatten();
    let frame = SceneFrame {
        output: output.key,
        sequence,
        security_epoch,
        lock_generation: state.lock_frame_generation(),
        slot,
        dmabuf,
        ready: rendered.sync,
        capture: wanted,
    };
    if let Err(frame) = presenter.submit_scene(frame) {
        // Retain the rejected producer lease and fence with this output until
        // recovery. Its pool cannot be acquired again while quarantined.
        output.failed_slot = Some(frame.slot.clone());
        output.rejected_scene = Some(frame);
        return Err("presenter rejected scene admission".into());
    }
    output.capture = metadata.map(|metadata| (sequence, metadata, started));
    output.presentation.queued(sequence);
    trace!(device = ?node, scene_node = ?gpus.scene_node, target_node = ?node,
        crtc = ?output.target.crtc, name = view.name, scene_sequence = sequence,
        age = if force { 0 } else { age }, render_us = started.elapsed().as_micros() as u64,
        "published native scene");
    Ok(true)
}
