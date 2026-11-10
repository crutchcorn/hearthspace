//! Read-only diagnostics from accepted page flips, never from admission alone.
use super::{UdevBackendState, presentation::OutputPresentation};
use crate::compositor::App;
use serde::Serialize;
use serde_json::{Value, json};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct PresentedPlanes {
    pub primary_scanout: bool,
    pub window_id: Option<u64>,
    pub cursor_plane: bool,
}

impl PresentedPlanes {
    pub fn new(primary_scanout: bool, window_id: Option<u64>, cursor_plane: bool) -> Self {
        Self {
            primary_scanout,
            // An admitted client can still be composed by Smithay/KMS. Never
            // label it as scanned out after the plane fallback has succeeded.
            window_id: window_id.filter(|_| primary_scanout),
            cursor_plane,
        }
    }
}

pub(super) struct CompletedScanout {
    submission: u64,
    scene: u64,
    planes: PresentedPlanes,
    received_at: Instant,
}

impl CompletedScanout {
    pub fn new(submission: u64, scene: u64, planes: PresentedPlanes) -> Self {
        Self {
            submission,
            scene,
            planes,
            received_at: Instant::now(),
        }
    }
}

impl OutputPresentation {
    pub(super) fn scanout_status(&self, available: bool) -> Value {
        let Some(completed) = self.scanout.as_ref().filter(|_| available) else {
            return json!({
                "state": "unknown", "primary_scanout": null, "window_id": null,
                "cursor_plane": null, "submission_sequence": null,
                "scene_sequence": null, "completion_age_ms": null,
            });
        };
        json!({
            "state": if completed.planes.primary_scanout { "direct-scanout" } else { "composited" },
            "primary_scanout": completed.planes.primary_scanout,
            "window_id": completed.planes.window_id,
            "cursor_plane": completed.planes.cursor_plane,
            "submission_sequence": completed.submission,
            "scene_sequence": completed.scene,
            "completion_age_ms": completed.received_at.elapsed().as_millis() as u64,
        })
    }
}

impl UdevBackendState {
    pub(in crate::compositor) fn scanout_status_available(&self, state: &App) -> bool {
        self.capture_active() && self.lock_generation == state.lock_frame_generation()
    }

    pub(in crate::compositor) fn scanout_status(&self) -> Vec<Value> {
        self.opened_devices
            .values()
            .flat_map(|device| {
                device.output_surfaces.iter().map(|output| {
                    let available = device.active
                        && !device.recovery_needed
                        && !device.configuration_pending
                        && output.failed_slot.is_none()
                        && output.rejected_scene.is_none();
                    let mut status = output.presentation.scanout_status(available);
                    status["output"] = json!(output.target.connector_name());
                    status
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
