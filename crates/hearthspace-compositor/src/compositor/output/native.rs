//! Keep each advertised Wayland output attached to one physical connector.
use wayland_server::DisplayHandle;

use super::{
    App, OutputDescriptor, OutputGeometry, OutputSet, create_output_record,
    layout::{output_locations_for_layout, secondary_descriptors_for_layout},
};

impl OutputSet {
    fn sync_connectors(&mut self, dh: &DisplayHandle, descriptors: Vec<OutputDescriptor>) {
        let Some(primary) = descriptors.first().cloned() else {
            if self.primary.advertised {
                tracing::info!(name = %self.primary.name, "disabling last disconnected Wayland output");
                dh.disable_global::<App>(self.primary.global_id.clone());
                self.primary.advertised = false;
            }
            for removed in self.secondary.drain(..) {
                dh.disable_global::<App>(removed.global_id);
            }
            // Retain geometry for recovery, but expose no fictitious output to
            // capture, lock barriers, window metadata, or new clients.
            return;
        };

        if !self.primary.advertised || self.primary.name != primary.name {
            tracing::info!(previous = %self.primary.name, next = %primary.name, "changing primary Wayland output without retargeting its connector identity");
            if let Some(index) = self
                .secondary
                .iter()
                .position(|output| output.name == primary.name)
            {
                std::mem::swap(&mut self.primary, &mut self.secondary[index]);
            } else {
                let replacement = create_output_record(dh, primary.clone(), (0, 0).into());
                let previous = std::mem::replace(&mut self.primary, replacement);
                if previous.advertised {
                    self.secondary.push(previous);
                }
            }
        }
        self.primary.update(primary, self.primary.location);

        let mut existing = std::mem::take(&mut self.secondary);
        let descriptors = secondary_descriptors_for_layout(descriptors, &self.primary.name);
        let plan = output_locations_for_layout(
            OutputGeometry::from(&self.primary),
            descriptors.iter().map(OutputGeometry::from).collect(),
            &self.layout,
        );
        self.primary
            .set_scale_and_location(plan.primary_scale, plan.primary);
        for mut descriptor in descriptors {
            let location = plan.secondary_location(&descriptor.name);
            descriptor.scale = plan
                .secondary_scale(&descriptor.name)
                .unwrap_or(descriptor.scale);
            let output = if let Some(index) = existing
                .iter()
                .position(|output| output.name == descriptor.name)
            {
                let mut output = existing.remove(index);
                output.update(descriptor, location);
                output
            } else {
                tracing::info!(name = %descriptor.name, ?location, "advertising newly connected Wayland output");
                create_output_record(dh, descriptor, location)
            };
            self.secondary.push(output);
        }
        for removed in existing {
            tracing::info!(name = %removed.name, "disabling disconnected Wayland output");
            dh.disable_global::<App>(removed.global_id);
        }
    }
}

impl App {
    pub(in crate::compositor) fn sync_connector_outputs(
        &mut self,
        dh: &DisplayHandle,
        descriptors: Vec<OutputDescriptor>,
    ) {
        self.outputs.sync_connectors(dh, descriptors);
    }
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
