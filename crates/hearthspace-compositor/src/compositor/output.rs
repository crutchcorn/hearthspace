use smithay::{
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    utils::{Logical, Physical, Point, Rectangle, Size, Transform},
};
use tracing::debug;
use wayland_server::{DisplayHandle, backend::GlobalId, protocol::wl_surface::WlSurface};

use super::App;

mod layout;

pub(in crate::compositor) use layout::OutputLayout;
#[cfg(feature = "udev")]
use layout::secondary_descriptors_for_layout;
use layout::{OutputGeometry, output_locations_for_layout};

pub(in crate::compositor) struct OutputSet {
    primary: OutputRecord,
    secondary: Vec<OutputRecord>,
    layout: OutputLayout,
}

pub(in crate::compositor) struct OutputRecord {
    #[cfg_attr(not(feature = "udev"), allow(dead_code))]
    name: String,
    output: Output,
    #[cfg_attr(not(feature = "udev"), allow(dead_code))]
    global_id: GlobalId,
    pub(in crate::compositor) size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
    location: Point<i32, Logical>,
}

type OutputDebugEntry = (
    String,
    Point<i32, Logical>,
    Size<i32, Physical>,
    Size<i32, Logical>,
    i32,
);

#[cfg(feature = "udev")]
pub(in crate::compositor) struct OutputRenderView {
    pub(in crate::compositor) name: String,
    pub(in crate::compositor) location: Point<i32, Logical>,
    pub(in crate::compositor) size: Size<i32, Logical>,
    pub(in crate::compositor) scale: i32,
}

#[derive(Clone)]
pub(in crate::compositor) struct OutputDescriptor {
    pub(in crate::compositor) name: String,
    pub(in crate::compositor) properties: PhysicalProperties,
    pub(in crate::compositor) size: Size<i32, Physical>,
    pub(in crate::compositor) scale: i32,
    pub(in crate::compositor) refresh: i32,
}

impl OutputSet {
    pub(in crate::compositor) fn new(primary: OutputRecord, layout: OutputLayout) -> Self {
        let mut outputs = Self {
            primary,
            secondary: Vec::new(),
            layout,
        };
        outputs.apply_layout_to_existing_outputs();
        outputs
    }

    pub(in crate::compositor) fn set_layout(&mut self, layout: OutputLayout) -> bool {
        if self.layout == layout {
            return false;
        }

        self.layout = layout;
        self.apply_layout_to_existing_outputs();
        true
    }

    #[cfg(feature = "udev")]
    fn sync_secondary_outputs(&mut self, dh: &DisplayHandle, descriptors: Vec<OutputDescriptor>) {
        let mut existing = std::mem::take(&mut self.secondary);
        let descriptors = secondary_descriptors_for_layout(descriptors, &self.primary.name);
        let plan = output_locations_for_layout(
            OutputGeometry::from(&self.primary),
            descriptors.iter().map(OutputGeometry::from).collect(),
            &self.layout,
        );
        self.primary
            .set_scale_and_location(plan.primary_scale, plan.primary);
        let mut next_secondary = Vec::with_capacity(descriptors.len());

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
                let output = create_output_record(dh, descriptor, location);
                tracing::info!(name = %output.name, ?location, "advertising newly connected Wayland output");
                output
            };
            next_secondary.push(output);
        }

        for removed in existing {
            tracing::info!(name = %removed.name, "disabling disconnected Wayland output");
            dh.disable_global::<App>(removed.global_id);
        }

        self.secondary = next_secondary;
    }

    fn apply_layout_to_existing_outputs(&mut self) {
        let plan = output_locations_for_layout(
            OutputGeometry::from(&self.primary),
            self.secondary.iter().map(OutputGeometry::from).collect(),
            &self.layout,
        );
        self.primary
            .set_scale_and_location(plan.primary_scale, plan.primary);
        for output in &mut self.secondary {
            let scale = plan.secondary_scale(&output.name).unwrap_or(output.scale);
            output.set_scale_and_location(scale, plan.secondary_location(&output.name));
        }
        debug!(outputs = ?self.output_debug(), "applied logical output layout");
    }

    fn output_debug(&self) -> Vec<OutputDebugEntry> {
        std::iter::once(&self.primary)
            .chain(self.secondary.iter())
            .map(|output| {
                (
                    output.name.clone(),
                    output.location,
                    output.size,
                    output.logical_size(),
                    output.scale,
                )
            })
            .collect()
    }

    fn logical_size(&self) -> Size<i32, Logical> {
        let mut width = 1;
        let mut height = 1;
        for rect in self.logical_rects() {
            width = width.max(rect.loc.x + rect.size.w);
            height = height.max(rect.loc.y + rect.size.h);
        }
        Size::from((width.max(1), height.max(1)))
    }

    fn logical_rects(&self) -> Vec<Rectangle<i32, Logical>> {
        std::iter::once(&self.primary)
            .chain(self.secondary.iter())
            .map(OutputRecord::logical_rect)
            .collect()
    }

    fn minimum_client_logical_width(&self) -> i32 {
        std::iter::once(&self.primary)
            .chain(self.secondary.iter())
            .map(|output| output.client_logical_size().w)
            .min()
            .unwrap_or(1)
    }

    fn output_for_name(&self, name: Option<&str>) -> Option<&OutputRecord> {
        match name {
            None => Some(&self.primary),
            Some(name) if self.primary.name == name => Some(&self.primary),
            Some(name) => self.secondary.iter().find(|output| output.name == name),
        }
    }
}

impl OutputRecord {
    fn logical_size(&self) -> Size<i32, Logical> {
        self.client_logical_size()
    }

    fn client_logical_size(&self) -> Size<i32, Logical> {
        Size::from((
            scaled_client_dimension(self.size.w, self.scale),
            scaled_client_dimension(self.size.h, self.scale),
        ))
    }

    fn logical_rect(&self) -> Rectangle<i32, Logical> {
        Rectangle::new(self.location, self.logical_size())
    }

    #[cfg(feature = "udev")]
    fn update(&mut self, descriptor: OutputDescriptor, location: Point<i32, Logical>) {
        self.size = descriptor.size;
        self.scale = descriptor.scale;
        self.refresh = descriptor.refresh;
        self.location = location;
        update_output_mode_with_refresh_at(
            &self.output,
            self.size,
            self.scale,
            self.refresh,
            self.location,
        );
    }

    fn set_scale_and_location(&mut self, scale: i32, location: Point<i32, Logical>) {
        let scale = scale.max(1);
        if self.scale == scale && self.location == location {
            return;
        }

        self.scale = scale;
        self.location = location;
        update_output_mode_with_refresh_at(
            &self.output,
            self.size,
            self.scale,
            self.refresh,
            self.location,
        );
    }
}

impl From<&OutputRecord> for OutputGeometry {
    fn from(output: &OutputRecord) -> Self {
        Self {
            name: output.name.clone(),
            size: output.size,
            scale: output.scale,
        }
    }
}

impl From<&OutputDescriptor> for OutputGeometry {
    fn from(descriptor: &OutputDescriptor) -> Self {
        Self {
            name: descriptor.name.clone(),
            size: descriptor.size,
            scale: descriptor.scale,
        }
    }
}

impl App {
    pub(super) fn output_size(&self) -> Size<i32, Physical> {
        self.outputs.primary.size
    }

    pub(super) fn output_logical_size(&self) -> Size<i32, Logical> {
        self.outputs.logical_size()
    }

    pub(super) fn output_logical_rects(&self) -> Vec<Rectangle<i32, Logical>> {
        self.outputs.logical_rects()
    }

    pub(super) fn primary_output_logical_rect(&self) -> Rectangle<i32, Logical> {
        self.outputs.primary.logical_rect()
    }

    pub(super) fn minimum_output_client_logical_width(&self) -> i32 {
        self.outputs.minimum_client_logical_width()
    }

    pub(super) fn output_client_logical_size_for_name(
        &self,
        name: Option<&str>,
    ) -> Option<Size<i32, Logical>> {
        self.outputs
            .output_for_name(name)
            .map(OutputRecord::client_logical_size)
    }

    pub(super) fn output_logical_rect_for_name(
        &self,
        name: Option<&str>,
    ) -> Option<Rectangle<i32, Logical>> {
        self.outputs
            .output_for_name(name)
            .map(OutputRecord::logical_rect)
    }

    pub(super) fn primary_output_scale(&self) -> i32 {
        self.outputs.primary.scale.max(1)
    }

    pub(in crate::compositor) fn set_output_layout(&mut self, layout: OutputLayout) -> bool {
        self.outputs.set_layout(layout)
    }

    #[cfg(feature = "udev")]
    pub(in crate::compositor) fn output_render_view(&self, name: &str) -> Option<OutputRenderView> {
        if self.outputs.primary.name == name {
            return Some(OutputRenderView {
                name: self.outputs.primary.name.clone(),
                location: self.outputs.primary.location,
                size: self.outputs.primary.logical_size(),
                scale: self.outputs.primary.scale.max(1),
            });
        }

        self.outputs
            .secondary
            .iter()
            .find(|output| output.name == name)
            .map(|output| OutputRenderView {
                name: output.name.clone(),
                location: output.location,
                size: output.logical_size(),
                scale: output.scale.max(1),
            })
    }

    #[cfg_attr(not(feature = "winit"), allow(dead_code))]
    pub(super) fn set_primary_output_size(&mut self, size: Size<i32, Physical>) {
        debug!(old_size = ?self.outputs.primary.size, new_size = ?size, "resizing primary output");
        self.outputs.primary.size = size;
        self.outputs.primary.refresh = 60_000;
        self.outputs.primary.location = (0, 0).into();
        update_output_mode(
            &self.outputs.primary.output,
            size,
            self.outputs.primary.scale,
        );
    }

    #[cfg(feature = "udev")]
    pub(in crate::compositor) fn set_primary_output_descriptor(
        &mut self,
        mut descriptor: OutputDescriptor,
    ) {
        if self.outputs.primary.name != descriptor.name {
            tracing::info!(
                wayland_name = %self.outputs.primary.name,
                kms_name = %descriptor.name,
                "retargeting primary KMS output while keeping the existing Wayland output global"
            );
        }
        descriptor.scale = self
            .outputs
            .layout
            .scale_for(&descriptor.name, "primary")
            .unwrap_or(descriptor.scale);
        self.outputs
            .primary
            .update(descriptor, self.outputs.primary.location);
        self.outputs.apply_layout_to_existing_outputs();
    }

    pub(super) fn enter_output(&self, surface: &WlSurface, name: Option<&str>) {
        for output in std::iter::once(&self.outputs.primary).chain(self.outputs.secondary.iter()) {
            if self
                .outputs
                .output_for_name(name)
                .is_some_and(|target| target.name == output.name)
            {
                output.output.enter(surface);
            } else {
                output.output.leave(surface);
            }
        }
    }

    pub(super) fn enter_outputs_intersecting(
        &self,
        surface: &WlSurface,
        rect: Rectangle<i32, Logical>,
    ) {
        for output in std::iter::once(&self.outputs.primary).chain(self.outputs.secondary.iter()) {
            if rectangles_overlap(rect, output.logical_rect()) {
                output.output.enter(surface);
            } else {
                output.output.leave(surface);
            }
        }
    }

    pub(super) fn enter_shell_output(&self, surface: &WlSurface, name: Option<&str>) {
        self.enter_output(surface, name);
    }

    pub(super) fn cleanup_outputs(&mut self) {
        self.outputs.primary.output.cleanup();
        for output in &mut self.outputs.secondary {
            output.output.cleanup();
        }
    }

    #[cfg(feature = "udev")]
    pub(in crate::compositor) fn sync_connector_outputs(
        &mut self,
        dh: &DisplayHandle,
        descriptors: Vec<OutputDescriptor>,
    ) {
        self.outputs.sync_secondary_outputs(dh, descriptors);
    }
}

pub(in crate::compositor) fn create_output(
    dh: &DisplayHandle,
    size: Size<i32, Physical>,
    scale: i32,
) -> OutputRecord {
    create_output_record(
        dh,
        OutputDescriptor {
            name: "hearthspace-0".into(),
            properties: PhysicalProperties {
                size: (340, 190).into(),
                subpixel: Subpixel::Unknown,
                make: "Hearthspace".into(),
                model: "Nested Canvas".into(),
                serial_number: "Unknown".into(),
            },
            size,
            scale,
            refresh: 60_000,
        },
        (0, 0).into(),
    )
}

#[cfg_attr(not(feature = "udev"), allow(dead_code))]
pub(in crate::compositor) fn create_output_with_properties(
    dh: &DisplayHandle,
    name: String,
    properties: PhysicalProperties,
    size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
) -> OutputRecord {
    create_output_record(
        dh,
        OutputDescriptor {
            name,
            properties,
            size,
            scale,
            refresh,
        },
        (0, 0).into(),
    )
}

fn create_output_record(
    dh: &DisplayHandle,
    descriptor: OutputDescriptor,
    location: Point<i32, Logical>,
) -> OutputRecord {
    let OutputDescriptor {
        name,
        properties,
        size,
        scale,
        refresh,
    } = descriptor;
    let output = Output::new(name.clone(), properties);
    let global_id = output.create_global::<App>(dh);
    update_output_mode_with_refresh_at(&output, size, scale, refresh, location);
    OutputRecord {
        name,
        output,
        global_id,
        size,
        scale,
        refresh,
        location,
    }
}

#[cfg_attr(not(feature = "winit"), allow(dead_code))]
fn update_output_mode(output: &Output, size: Size<i32, Physical>, scale: i32) {
    update_output_mode_with_refresh(output, size, scale, 60_000);
}

fn update_output_mode_with_refresh(
    output: &Output,
    size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
) {
    update_output_mode_with_refresh_at(output, size, scale, refresh, (0, 0).into());
}

fn update_output_mode_with_refresh_at(
    output: &Output,
    size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
    location: Point<i32, Logical>,
) {
    let mode = Mode { size, refresh };
    output.set_preferred(mode);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Integer(scale)),
        Some(location),
    );
}

fn scaled_client_dimension(size: i32, scale: i32) -> i32 {
    let size = size.max(1);
    let scale = scale.max(1);
    ((size + scale - 1) / scale).max(1)
}

fn rectangles_overlap(a: Rectangle<i32, Logical>, b: Rectangle<i32, Logical>) -> bool {
    a.loc.x < b.loc.x + b.size.w
        && a.loc.x + a.size.w > b.loc.x
        && a.loc.y < b.loc.y + b.size.h
        && a.loc.y + a.size.h > b.loc.y
}

#[cfg(test)]
mod tests {
    use super::scaled_client_dimension;

    #[test]
    fn client_logical_dimension_divides_physical_size_by_output_scale() {
        assert_eq!(scaled_client_dimension(2560, 4), 640);
        assert_eq!(scaled_client_dimension(1920, 2), 960);
    }

    #[test]
    fn client_logical_dimension_rounds_up_and_stays_positive() {
        assert_eq!(scaled_client_dimension(1366, 2), 683);
        assert_eq!(scaled_client_dimension(0, 4), 1);
        assert_eq!(scaled_client_dimension(2560, 0), 2560);
    }
}
