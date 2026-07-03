use smithay::{
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    utils::{Logical, Physical, Point, Size, Transform},
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
        self.primary.set_location(plan.primary);
        let mut next_secondary = Vec::with_capacity(descriptors.len());

        for descriptor in descriptors {
            let location = plan.secondary_location(&descriptor.name);
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
        self.primary.set_location(plan.primary);
        for output in &mut self.secondary {
            output.set_location(plan.secondary_location(&output.name));
        }
    }

    fn logical_size(&self) -> Size<i32, Logical> {
        let primary_size = self.primary.logical_size();
        let mut width = primary_size.w;
        let mut height = primary_size.h;
        for output in &self.secondary {
            let size = output.logical_size();
            width = width.max(output.location.x + size.w);
            height = height.max(output.location.y + size.h);
        }
        Size::from((width.max(1), height.max(1)))
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
        self.size.to_logical(self.scale)
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

    fn set_location(&mut self, location: Point<i32, Logical>) {
        if self.location == location {
            return;
        }

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
        self.monitors
            .iter()
            .find(|monitor| monitor.id == output_name || monitor.name == output_name)
            .or_else(|| self.monitors.iter().find(|monitor| monitor.id == role_id))
            .map(|monitor| Point::from((monitor.x, monitor.y)))
    }
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
        self.size.to_logical(self.scale)
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
    secondary: Vec<(String, Point<i32, Logical>)>,
}

impl OutputLayoutPlan {
    fn secondary_location(&self, name: &str) -> Point<i32, Logical> {
        self.secondary
            .iter()
            .find(|(secondary_name, _)| secondary_name == name)
            .map(|(_, location)| *location)
            .unwrap_or((0, 0).into())
    }
}

fn output_locations_for_layout(
    primary: OutputGeometry,
    mut secondary: Vec<OutputGeometry>,
    layout: &OutputLayout,
) -> OutputLayoutPlan {
    secondary.sort_by(|a, b| a.name.cmp(&b.name));

    let primary_location = layout
        .location_for(&primary.name, "primary")
        .unwrap_or((0, 0).into());
    let mut placed = vec![(
        primary.name.clone(),
        primary.logical_size(),
        primary_location,
    )];
    let mut unplaced = Vec::new();

    for (index, output) in secondary.into_iter().enumerate() {
        let role_id = secondary_role_id(index);
        if let Some(location) = layout.location_for(&output.name, &role_id) {
            let size = output.logical_size();
            placed.push((output.name, size, location));
        } else {
            unplaced.push(output);
        }
    }

    let fallback_y = primary_location.y;
    let mut next_x = placed
        .iter()
        .map(|(_, size, location)| location.x + size.w)
        .max()
        .unwrap_or(0);
    for output in unplaced {
        let size = output.logical_size();
        let location = Point::from((next_x, fallback_y));
        next_x += size.w;
        placed.push((output.name, size, location));
    }

    let min_x = placed
        .iter()
        .map(|(_, _, location)| location.x)
        .min()
        .unwrap_or(0);
    let min_y = placed
        .iter()
        .map(|(_, _, location)| location.y)
        .min()
        .unwrap_or(0);

    let mut primary_plan = Point::from((0, 0));
    let mut secondary_plan = Vec::new();
    for (name, _, location) in placed {
        let normalized = Point::from((location.x - min_x, location.y - min_y));
        if name == primary.name {
            primary_plan = normalized;
        } else {
            secondary_plan.push((name, normalized));
        }
    }

    OutputLayoutPlan {
        primary: primary_plan,
        secondary: secondary_plan,
    }
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
            });
        }

        self.outputs
            .secondary
            .iter()
            .find(|output| output.name == name)
            .map(|output| OutputRenderView {
                name: output.name.clone(),
                location: output.location,
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
        descriptor: OutputDescriptor,
    ) {
        if self.outputs.primary.name != descriptor.name {
            tracing::info!(
                wayland_name = %self.outputs.primary.name,
                kms_name = %descriptor.name,
                "retargeting primary KMS output while keeping the existing Wayland output global"
            );
        }
        self.outputs.primary.update(descriptor, (0, 0).into());
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

#[cfg(all(test, feature = "udev"))]
mod tests {
    use smithay::{
        output::{PhysicalProperties, Subpixel},
        utils::{Physical, Point, Size},
    };

    use super::{
        OutputDescriptor, OutputGeometry, OutputLayout, output_locations_for_layout,
        secondary_descriptors_for_layout,
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

    fn layout(monitors: &[(&str, i32, i32)]) -> OutputLayout {
        OutputLayout::from_monitors(
            monitors
                .iter()
                .map(|(id, x, y)| hearthspace_ipc::MonitorConfig {
                    id: (*id).into(),
                    name: (*id).into(),
                    width: 100,
                    height: 100,
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
