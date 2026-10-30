use smithay::{
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    utils::{Logical, Physical, Point, Rectangle, Size, Transform},
};
use tracing::debug;
use wayland_server::{DisplayHandle, backend::GlobalId, protocol::wl_surface::WlSurface};

use super::App;

mod layout;
#[cfg(feature = "udev")]
mod native;

pub(in crate::compositor) use layout::OutputLayout;
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
    advertised: bool,
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
    #[cfg(test)]
    pub(in crate::compositor) fn for_callback_tests(dh: &DisplayHandle) -> Self {
        let make = |name: &str, x| {
            create_output_record(
                dh,
                OutputDescriptor {
                    name: name.into(),
                    properties: PhysicalProperties {
                        size: (300, 200).into(),
                        subpixel: Subpixel::Unknown,
                        make: "Callback test".into(),
                        model: name.into(),
                        serial_number: name.into(),
                    },
                    size: (800, 600).into(),
                    scale: 1,
                    refresh: 60_000,
                },
                (x, 0).into(),
            )
        };
        Self {
            primary: make("A", 0),
            secondary: vec![make("B", 800)],
            layout: OutputLayout::default(),
        }
    }

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

    fn apply_layout_to_existing_outputs(&mut self) {
        if !self.primary.advertised {
            return;
        }
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

    fn active_outputs(&self) -> impl Iterator<Item = &OutputRecord> {
        std::iter::once(&self.primary)
            .chain(self.secondary.iter())
            .filter(|output| output.advertised)
    }

    fn output_debug(&self) -> Vec<OutputDebugEntry> {
        self.active_outputs()
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
        self.active_outputs()
            .map(OutputRecord::logical_rect)
            .collect()
    }

    fn minimum_client_logical_width(&self) -> i32 {
        self.active_outputs()
            .map(|output| output.client_logical_size().w)
            .min()
            .unwrap_or(1)
    }

    fn output_for_name(&self, name: Option<&str>) -> Option<&OutputRecord> {
        let name = name.unwrap_or(&self.primary.name);
        self.active_outputs().find(|output| output.name == name)
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
    pub(super) fn capture_output_handles(&self) -> Vec<Output> {
        self.outputs
            .active_outputs()
            .map(|record| record.output.clone())
            .collect()
    }

    pub(super) fn capture_outputs(&self) -> Vec<super::capture::OutputMetadata> {
        self.outputs
            .output_debug()
            .into_iter()
            .map(
                |(name, location, size, logical, scale)| super::capture::OutputMetadata {
                    name,
                    width: size.w,
                    height: size.h,
                    scale,
                    logical_x: location.x,
                    logical_y: location.y,
                    logical_width: logical.w,
                    logical_height: logical.h,
                    transform: "normal",
                    frame_id: 0,
                },
            )
            .collect()
    }

    pub(super) fn output_size(&self) -> Size<i32, Physical> {
        self.outputs.primary.size
    }

    pub(super) fn output_logical_size(&self) -> Size<i32, Logical> {
        self.outputs.logical_size()
    }

    pub(super) fn output_logical_rects(&self) -> Vec<Rectangle<i32, Logical>> {
        self.outputs.logical_rects()
    }

    /// Active identities and geometry, with the primary first for stable pacing.
    pub(super) fn callback_outputs(&self) -> Vec<(&str, Rectangle<i32, Logical>)> {
        self.outputs
            .active_outputs()
            .map(|output| (output.name.as_str(), output.logical_rect()))
            .collect()
    }

    pub(super) fn primary_output_logical_rect(&self) -> Rectangle<i32, Logical> {
        self.outputs.primary.logical_rect()
    }

    pub(super) fn primary_output_name(&self) -> Option<&str> {
        self.outputs
            .primary
            .advertised
            .then_some(self.outputs.primary.name.as_str())
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

    pub(super) fn titlebar_output_scale(&self, rect: Rectangle<i32, Logical>) -> i32 {
        self.outputs
            .active_outputs()
            .filter(|output| output.logical_rect().overlaps(rect))
            .map(|output| output.scale.max(1))
            .max()
            .unwrap_or_else(|| self.primary_output_scale())
    }

    /// The integer scale of whichever output contains `point` (in logical
    /// screen space), falling back to the primary output's scale. Used to pick
    /// the preferred fractional scale advertised to a surface's clients.
    pub(super) fn output_scale_at(&self, point: Point<f64, Logical>) -> i32 {
        self.outputs
            .active_outputs()
            .find(|output| point_in_rect(point, output.logical_rect()))
            .map(|output| output.scale.max(1))
            .unwrap_or_else(|| self.primary_output_scale())
    }

    pub(in crate::compositor) fn set_output_layout(&mut self, layout: OutputLayout) -> bool {
        self.outputs.set_layout(layout)
    }

    #[cfg(feature = "udev")]
    pub(in crate::compositor) fn output_render_view(&self, name: &str) -> Option<OutputRenderView> {
        self.outputs
            .output_for_name(Some(name))
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
            if output.advertised && rectangles_overlap(rect, output.logical_rect()) {
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
        advertised: true,
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

fn point_in_rect(point: Point<f64, Logical>, rect: Rectangle<i32, Logical>) -> bool {
    point.x >= f64::from(rect.loc.x)
        && point.y >= f64::from(rect.loc.y)
        && point.x < f64::from(rect.loc.x + rect.size.w)
        && point.y < f64::from(rect.loc.y + rect.size.h)
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
