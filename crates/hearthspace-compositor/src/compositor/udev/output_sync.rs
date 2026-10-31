//! Publish topology changes separately from replacement of a scanout source.
use std::collections::BTreeSet;

use crate::compositor::{App, OutputDescriptor, capture::CaptureState};
use wayland_server::DisplayHandle;

#[derive(Default)]
pub(super) struct PublishedOutputs {
    descriptors: Option<Vec<OutputDescriptor>>,
    invalidated: BTreeSet<String>,
}

#[derive(Default)]
pub(super) struct ConnectorSync {
    // None means unchanged/deferred, not that all outputs disappeared.
    descriptors: Option<Vec<OutputDescriptor>>,
    invalidated: BTreeSet<String>,
}

impl PublishedOutputs {
    pub(super) fn invalidate<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        self.invalidated
            .extend(names.into_iter().map(str::to_owned));
    }

    pub(super) fn update(&mut self, descriptors: Vec<OutputDescriptor>) -> ConnectorSync {
        let changed = self.descriptors.as_ref().is_none_or(|previous| {
            previous.len() != descriptors.len()
                || previous
                    .iter()
                    .zip(&descriptors)
                    .any(|(a, b)| !same_descriptor(a, b))
        });
        if let Some(previous) = &self.descriptors {
            for output in previous {
                if !descriptors.iter().any(|next| same_descriptor(output, next)) {
                    self.invalidated.insert(output.name.clone());
                }
            }
        }
        let update = changed.then(|| descriptors.clone());
        self.descriptors = Some(descriptors);
        ConnectorSync {
            descriptors: update,
            invalidated: std::mem::take(&mut self.invalidated),
        }
    }
}

impl ConnectorSync {
    /// The production effects boundary also runs without DRM/EGL in regression
    /// tests. A failed secondary retry must not stop a healthy capture session.
    pub(super) fn apply(
        self,
        state: &mut App,
        captures: &mut CaptureState,
        dh: &DisplayHandle,
    ) -> bool {
        let changed = self.descriptors.is_some() || !self.invalidated.is_empty();
        for name in &self.invalidated {
            captures.fail(name, "native output source changed");
            state.remote_capture.fail_output(name);
        }
        if changed {
            state.invalidate_lock_presentation();
        }
        if let Some(descriptors) = self.descriptors {
            let previous = state.capture_outputs();
            state.sync_connector_outputs(dh, descriptors);
            state.configure_shell_bars();
            state.reconcile_pointer_after_output_geometry_change();
            let current = state.capture_outputs();
            // Layout normalization can move/scale a surviving output too. Only
            // invalidate captures whose actual metadata changed with the layout.
            for output in previous {
                if !current.iter().any(|next| {
                    output.name == next.name
                        && output.width == next.width
                        && output.height == next.height
                        && output.scale == next.scale
                        && output.logical_x == next.logical_x
                        && output.logical_y == next.logical_y
                        && output.logical_width == next.logical_width
                        && output.logical_height == next.logical_height
                        && output.transform == next.transform
                }) {
                    captures.fail(&output.name, "native output geometry changed");
                    state.remote_capture.fail_output(&output.name);
                }
            }
        } else if changed {
            state.reconfigure_lock_outputs();
        }
        if changed {
            state.request_redraw();
        }
        changed
    }
}

fn same_descriptor(a: &OutputDescriptor, b: &OutputDescriptor) -> bool {
    a.name == b.name
        && a.size == b.size
        && a.scale == b.scale
        && a.refresh == b.refresh
        && a.properties.size == b.properties.size
        && a.properties.subpixel == b.properties.subpixel
        && a.properties.make == b.properties.make
        && a.properties.model == b.properties.model
        && a.properties.serial_number == b.properties.serial_number
}

#[cfg(test)]
#[path = "output_sync_tests.rs"]
mod tests;
