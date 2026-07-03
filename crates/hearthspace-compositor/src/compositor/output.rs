use smithay::{
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    utils::{Logical, Physical, Point, Rectangle, Size, Transform},
};
use tracing::debug;
use wayland_server::{DisplayHandle, backend::GlobalId, protocol::wl_surface::WlSurface};

use super::App;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::compositor) struct OutputLayout {
    monitors: Vec<hearthspace_ipc::MonitorConfig>,
}

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

#[cfg(feature = "udev")]
pub(in crate::compositor) struct OutputRenderView {
    pub(in crate::compositor) name: String,
    pub(in crate::compositor) location: Point<i32, Logical>,
    pub(in crate::compositor) size: Size<i32, Logical>,
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

    #[cfg(feature = "udev")]
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

    fn minimum_logical_width(&self) -> i32 {
        minimum_output_width(self.logical_rects().iter().copied()).unwrap_or(1)
    }
}

/// Native layout policy: selected primary at (0, 0), all other connectors placed
/// horizontally to the right in stable connector-name order.
#[cfg(feature = "udev")]
fn secondary_descriptors_for_layout(
    descriptors: Vec<OutputDescriptor>,
    primary_name: &str,
) -> Vec<OutputDescriptor> {
    let mut secondary = descriptors
        .into_iter()
        .skip(1)
        .filter(|descriptor| descriptor.name != primary_name)
        .collect::<Vec<_>>();
    secondary.sort_by(|a, b| a.name.cmp(&b.name));
    secondary
}

impl OutputRecord {
    fn logical_size(&self) -> Size<i32, Logical> {
        Size::from((self.size.w, self.size.h))
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

impl OutputLayout {
    fn from_monitors(monitors: Vec<hearthspace_ipc::MonitorConfig>) -> Self {
        Self { monitors }
    }

    fn location_for(&self, output_name: &str, role_id: &str) -> Option<Point<i32, Logical>> {
        let use_role_fallback = !default_placeholder_monitors(&self.monitors);
        self.monitor_for(output_name, role_id, use_role_fallback)
            .map(|monitor| Point::from((monitor.x, monitor.y)))
    }

    fn scale_for(&self, output_name: &str, role_id: &str) -> Option<i32> {
        self.monitor_for(output_name, role_id, true)
            .map(|monitor| monitor.scale.max(1))
    }

    fn monitor_for(
        &self,
        output_name: &str,
        role_id: &str,
        use_role_fallback: bool,
    ) -> Option<&hearthspace_ipc::MonitorConfig> {
        self.monitors
            .iter()
            .find(|monitor| monitor.id == output_name || monitor.name == output_name)
            .or_else(|| {
                use_role_fallback
                    .then(|| self.monitors.iter().find(|monitor| monitor.id == role_id))
                    .flatten()
            })
    }
}

fn default_placeholder_monitors(monitors: &[hearthspace_ipc::MonitorConfig]) -> bool {
    monitors == hearthspace_ipc::Settings::default().monitors
}

pub(in crate::compositor) fn load_output_layout() -> OutputLayout {
    match hearthspace_ipc::client_get_settings() {
        Ok(settings) => OutputLayout::from_monitors(settings.monitors),
        Err(error) => {
            debug!(%error, "using fallback output layout because settingsd is unavailable");
            OutputLayout::default()
        }
    }
}

#[derive(Clone)]
struct OutputGeometry {
    name: String,
    size: Size<i32, Physical>,
    scale: i32,
}

impl OutputGeometry {
    fn logical_size(&self) -> Size<i32, Logical> {
        Size::from((self.size.w, self.size.h))
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

struct OutputLayoutPlan {
    primary: Point<i32, Logical>,
    primary_scale: i32,
    secondary: Vec<(String, Point<i32, Logical>, i32)>,
}

#[derive(Debug, Clone)]
struct OutputPlacement {
    name: String,
    size: Size<i32, Logical>,
    location: Point<i32, Logical>,
    scale: i32,
}

impl OutputLayoutPlan {
    fn secondary_location(&self, name: &str) -> Point<i32, Logical> {
        self.secondary
            .iter()
            .find(|(secondary_name, _, _)| secondary_name == name)
            .map(|(_, location, _)| *location)
            .unwrap_or((0, 0).into())
    }

    fn secondary_scale(&self, name: &str) -> Option<i32> {
        self.secondary
            .iter()
            .find(|(secondary_name, _, _)| secondary_name == name)
            .map(|(_, _, scale)| *scale)
    }
}

fn output_locations_for_layout(
    mut primary: OutputGeometry,
    mut secondary: Vec<OutputGeometry>,
    layout: &OutputLayout,
) -> OutputLayoutPlan {
    secondary.sort_by(|a, b| a.name.cmp(&b.name));
    if let Some(scale) = layout.scale_for(&primary.name, "primary") {
        primary.scale = scale;
    }
    for (index, output) in secondary.iter_mut().enumerate() {
        let role_id = secondary_role_id(index);
        if let Some(scale) = layout.scale_for(&output.name, &role_id) {
            output.scale = scale;
        }
    }
    let fallback_primary = primary.clone();
    let fallback_secondary = secondary.clone();

    let primary_location = layout
        .location_for(&primary.name, "primary")
        .unwrap_or((0, 0).into());
    let mut placed = vec![OutputPlacement {
        name: primary.name.clone(),
        size: primary.logical_size(),
        location: primary_location,
        scale: primary.scale,
    }];
    let mut unplaced = Vec::new();

    for (index, output) in secondary.into_iter().enumerate() {
        let role_id = secondary_role_id(index);
        if let Some(location) = layout.location_for(&output.name, &role_id) {
            let size = output.logical_size();
            placed.push(OutputPlacement {
                name: output.name,
                size,
                location,
                scale: output.scale,
            });
        } else {
            unplaced.push(output);
        }
    }

    let fallback_y = primary_location.y;
    let mut next_x = placed
        .iter()
        .map(|placement| placement.location.x + placement.size.w)
        .max()
        .unwrap_or(0);
    for output in unplaced {
        let size = output.logical_size();
        let location = Point::from((next_x, fallback_y));
        next_x += size.w;
        placed.push(OutputPlacement {
            name: output.name,
            size,
            location,
            scale: output.scale,
        });
    }

    if placements_overlap(&placed) {
        debug!(placements = ?placement_debug(&placed), "repairing overlapping active monitor layout");
        placed = fallback_output_placements(fallback_primary, fallback_secondary);
    }

    let min_x = placed
        .iter()
        .map(|placement| placement.location.x)
        .min()
        .unwrap_or(0);
    let min_y = placed
        .iter()
        .map(|placement| placement.location.y)
        .min()
        .unwrap_or(0);

    let mut primary_plan = Point::from((0, 0));
    let mut primary_scale = primary.scale;
    let mut secondary_plan = Vec::new();
    for placement in placed {
        let normalized = Point::from((placement.location.x - min_x, placement.location.y - min_y));
        if placement.name == primary.name {
            primary_plan = normalized;
            primary_scale = placement.scale;
        } else {
            secondary_plan.push((placement.name, normalized, placement.scale));
        }
    }

    OutputLayoutPlan {
        primary: primary_plan,
        primary_scale,
        secondary: secondary_plan,
    }
}

fn fallback_output_placements(
    primary: OutputGeometry,
    secondary: Vec<OutputGeometry>,
) -> Vec<OutputPlacement> {
    let mut next_x = 0;
    std::iter::once(primary)
        .chain(secondary)
        .map(|output| {
            let size = output.logical_size();
            let location = Point::from((next_x, 0));
            next_x += size.w;
            OutputPlacement {
                name: output.name,
                size,
                location,
                scale: output.scale,
            }
        })
        .collect()
}

fn placements_overlap(placements: &[OutputPlacement]) -> bool {
    placements.iter().enumerate().any(|(index, placement)| {
        let rect = Rectangle::new(placement.location, placement.size);
        placements
            .iter()
            .skip(index + 1)
            .any(|other| rectangles_overlap(rect, Rectangle::new(other.location, other.size)))
    })
}

fn rectangles_overlap(a: Rectangle<i32, Logical>, b: Rectangle<i32, Logical>) -> bool {
    a.loc.x < b.loc.x + b.size.w
        && a.loc.x + a.size.w > b.loc.x
        && a.loc.y < b.loc.y + b.size.h
        && a.loc.y + a.size.h > b.loc.y
}

fn placement_debug(placements: &[OutputPlacement]) -> Vec<(String, i32, i32, i32, i32)> {
    placements
        .iter()
        .map(|placement| {
            (
                placement.name.clone(),
                placement.location.x,
                placement.location.y,
                placement.size.w,
                placement.size.h,
            )
        })
        .collect()
}

fn secondary_role_id(index: usize) -> String {
    if index == 0 {
        "secondary".to_string()
    } else {
        format!("secondary-{}", index + 1)
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

    pub(super) fn minimum_output_logical_width(&self) -> i32 {
        self.outputs.minimum_logical_width()
    }

    #[cfg(feature = "udev")]
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

    pub(super) fn enter_primary_output(&self, surface: &WlSurface) {
        self.outputs.primary.output.enter(surface);
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

fn minimum_output_width(rects: impl IntoIterator<Item = Rectangle<i32, Logical>>) -> Option<i32> {
    rects.into_iter().map(|rect| rect.size.w.max(1)).min()
}

#[cfg(all(test, feature = "udev"))]
mod tests {
    use smithay::{
        output::{PhysicalProperties, Subpixel},
        utils::{Physical, Point, Rectangle, Size},
    };

    use super::{
        OutputDescriptor, OutputGeometry, OutputLayout, minimum_output_width,
        output_locations_for_layout, secondary_descriptors_for_layout,
    };

    fn descriptor(name: &str) -> OutputDescriptor {
        OutputDescriptor {
            name: name.into(),
            properties: PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "test".into(),
                model: name.into(),
            },
            size: Size::<i32, Physical>::from((100, 100)),
            scale: 1,
            refresh: 60_000,
        }
    }

    fn geometry(name: &str) -> OutputGeometry {
        OutputGeometry::from(&descriptor(name))
    }

    fn geometry_with_size(name: &str, width: i32, height: i32) -> OutputGeometry {
        OutputGeometry {
            name: name.into(),
            size: Size::<i32, Physical>::from((width, height)),
            scale: 1,
        }
    }

    fn layout(monitors: &[(&str, i32, i32)]) -> OutputLayout {
        OutputLayout::from_monitors(
            monitors
                .iter()
                .map(|(id, x, y)| hearthspace_ipc::MonitorConfig {
                    id: (*id).into(),
                    name: (*id).into(),
                    width: 100,
                    height: 100,
                    scale: 1,
                    x: *x,
                    y: *y,
                })
                .collect(),
        )
    }

    #[test]
    fn secondary_layout_skips_selected_primary_and_sorts_by_connector_name() {
        let descriptors = vec![
            descriptor("eDP-1"),
            descriptor("DP-2"),
            descriptor("HDMI-A-1"),
            descriptor("DP-1"),
        ];

        let names = secondary_descriptors_for_layout(descriptors, "eDP-1")
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect::<Vec<_>>();

        assert_eq!(names, ["DP-1", "DP-2", "HDMI-A-1"]);
    }

    #[test]
    fn secondary_layout_avoids_duplicate_primary_global_name() {
        let descriptors = vec![
            descriptor("DP-1"),
            descriptor("eDP-1"),
            descriptor("HDMI-A-1"),
        ];

        let names = secondary_descriptors_for_layout(descriptors, "eDP-1")
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect::<Vec<_>>();

        assert_eq!(names, ["HDMI-A-1"]);
    }

    #[test]
    fn output_layout_uses_connector_positions_and_normalizes_to_zero_origin() {
        let plan = output_locations_for_layout(
            geometry("eDP-1"),
            vec![geometry("HDMI-A-1")],
            &layout(&[("eDP-1", 1920, 0), ("HDMI-A-1", 0, -200)]),
        );

        assert_eq!(plan.primary, Point::from((1920, 200)));
        assert_eq!(plan.secondary_location("HDMI-A-1"), Point::from((0, 0)));
    }

    #[test]
    fn output_layout_maps_placeholder_roles_by_output_order() {
        let plan = output_locations_for_layout(
            geometry("eDP-1"),
            vec![geometry("HDMI-A-1")],
            &layout(&[("primary", 0, 0), ("secondary", -100, 100)]),
        );

        assert_eq!(plan.primary, Point::from((100, 0)));
        assert_eq!(plan.secondary_location("HDMI-A-1"), Point::from((0, 100)));
    }

    #[test]
    fn output_layout_ignores_default_placeholder_positions_for_mixed_resolutions() {
        let plan = output_locations_for_layout(
            geometry_with_size("eDP-1", 2560, 1440),
            vec![geometry_with_size("HDMI-A-1", 1366, 768)],
            &OutputLayout::from_monitors(hearthspace_ipc::Settings::default().monitors),
        );

        assert_eq!(plan.primary, Point::from((0, 0)));
        assert_eq!(plan.secondary_location("HDMI-A-1"), Point::from((2560, 0)));
    }

    #[test]
    fn output_layout_keeps_scaled_outputs_at_physical_layout_size() {
        let plan = output_locations_for_layout(
            geometry_with_size("eDP-1", 2560, 1440),
            vec![geometry_with_size("HDMI-A-1", 1920, 1080)],
            &OutputLayout::from_monitors(vec![
                hearthspace_ipc::MonitorConfig {
                    id: "eDP-1".into(),
                    name: "eDP-1".into(),
                    width: 2560,
                    height: 1440,
                    scale: 2,
                    x: 0,
                    y: 0,
                },
                hearthspace_ipc::MonitorConfig {
                    id: "HDMI-A-1".into(),
                    name: "HDMI-A-1".into(),
                    width: 1920,
                    height: 1080,
                    scale: 1,
                    x: 1280,
                    y: 0,
                },
            ]),
        );

        assert_eq!(plan.primary, Point::from((0, 0)));
        assert_eq!(plan.primary_scale, 2);
        assert_eq!(plan.secondary_location("HDMI-A-1"), Point::from((2560, 0)));
        assert_eq!(plan.secondary_scale("HDMI-A-1"), Some(1));
    }

    #[test]
    fn output_layout_repairs_overlapping_saved_active_monitors() {
        let plan = output_locations_for_layout(
            geometry_with_size("eDP-1", 2560, 1440),
            vec![geometry_with_size("HDMI-A-1", 1366, 768)],
            &layout(&[("primary", -1280, -736), ("secondary", -1920, -540)]),
        );

        assert_eq!(plan.primary, Point::from((0, 0)));
        assert_eq!(plan.secondary_location("HDMI-A-1"), Point::from((2560, 0)));
    }

    #[test]
    fn output_layout_size_includes_primary_location() {
        let plan = output_locations_for_layout(
            geometry_with_size("eDP-1", 100, 100),
            vec![geometry_with_size("HDMI-A-1", 100, 100)],
            &layout(&[("eDP-1", 100, 0), ("HDMI-A-1", 0, 0)]),
        );

        assert_eq!(plan.primary, Point::from((100, 0)));
        assert_eq!(plan.secondary_location("HDMI-A-1"), Point::from((0, 0)));
    }

    #[test]
    fn minimum_output_width_uses_smallest_monitor() {
        let width = minimum_output_width([
            Rectangle::new((0, 0).into(), (2560, 1440).into()),
            Rectangle::new((2560, 0).into(), (1366, 768).into()),
        ]);

        assert_eq!(width, Some(1366));
    }

    #[test]
    fn output_layout_places_unconfigured_outputs_to_the_right() {
        let plan = output_locations_for_layout(
            geometry("eDP-1"),
            vec![geometry("HDMI-A-1"), geometry("DP-1")],
            &layout(&[("primary", 0, 0)]),
        );

        assert_eq!(plan.primary, Point::from((0, 0)));
        assert_eq!(plan.secondary_location("DP-1"), Point::from((100, 0)));
        assert_eq!(plan.secondary_location("HDMI-A-1"), Point::from((200, 0)));
    }
}
