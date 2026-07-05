use smithay::utils::{Logical, Physical, Point, Rectangle, Size};
use tracing::debug;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::compositor) struct OutputLayout {
    monitors: Vec<hearthspace_ipc::MonitorConfig>,
}

impl OutputLayout {
    pub(in crate::compositor) fn from_monitors(
        monitors: Vec<hearthspace_ipc::MonitorConfig>,
    ) -> Self {
        Self { monitors }
    }

    pub(super) fn location_for(
        &self,
        output_name: &str,
        role_id: &str,
    ) -> Option<Point<i32, Logical>> {
        let use_role_fallback = !default_placeholder_monitors(&self.monitors);
        self.monitor_for(output_name, role_id, use_role_fallback)
            .map(|monitor| Point::from((monitor.x, monitor.y)))
    }

    fn physical_location_for(
        &self,
        output_name: &str,
        role_id: &str,
    ) -> Option<Point<i32, Logical>> {
        let use_role_fallback = !default_placeholder_monitors(&self.monitors);
        self.monitor_for(output_name, role_id, use_role_fallback)
            .map(|monitor| {
                Point::from((
                    physical_axis_position_to_logical(&self.monitors, monitor.x, Axis::X),
                    physical_axis_position_to_logical(&self.monitors, monitor.y, Axis::Y),
                ))
            })
    }

    pub(super) fn scale_for(&self, output_name: &str, role_id: &str) -> Option<i32> {
        let use_role_fallback = !default_placeholder_monitors(&self.monitors);
        self.monitor_for(output_name, role_id, use_role_fallback)
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

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

fn physical_axis_position_to_logical(
    monitors: &[hearthspace_ipc::MonitorConfig],
    coordinate: i32,
    axis: Axis,
) -> i32 {
    let mut intervals = monitors
        .iter()
        .map(|monitor| {
            let start = match axis {
                Axis::X => monitor.x,
                Axis::Y => monitor.y,
            };
            let size = match axis {
                Axis::X => monitor.width,
                Axis::Y => monitor.height,
            }
            .max(1);
            (start, start + size, monitor.scale.max(1))
        })
        .collect::<Vec<_>>();
    intervals.sort_by_key(|(start, _, _)| *start);

    let Some((first_start, _, _)) = intervals.first().copied() else {
        return coordinate;
    };
    let mut previous_end = first_start;
    let mut logical = 0;

    for (start, end, scale) in intervals {
        if coordinate < start {
            return logical + coordinate - previous_end;
        }
        if start > previous_end {
            logical += start - previous_end;
        }
        if coordinate <= end {
            return logical + scaled_offset(coordinate - start, scale);
        }
        logical += scaled_dimension(end - start, scale);
        previous_end = previous_end.max(end);
    }

    logical + coordinate - previous_end
}

fn scaled_offset(offset: i32, scale: i32) -> i32 {
    if offset <= 0 {
        return 0;
    }
    let scale = scale.max(1);
    (offset + scale - 1) / scale
}

fn default_placeholder_monitors(monitors: &[hearthspace_ipc::MonitorConfig]) -> bool {
    monitors == hearthspace_ipc::Settings::default().monitors
}

#[derive(Clone)]
pub(super) struct OutputGeometry {
    pub(super) name: String,
    pub(super) size: Size<i32, Physical>,
    pub(super) scale: i32,
}

impl OutputGeometry {
    fn logical_size(&self) -> Size<i32, Logical> {
        Size::from((
            scaled_dimension(self.size.w, self.scale),
            scaled_dimension(self.size.h, self.scale),
        ))
    }
}

fn scaled_dimension(size: i32, scale: i32) -> i32 {
    let size = size.max(1);
    let scale = scale.max(1);
    ((size + scale - 1) / scale).max(1)
}

pub(super) struct OutputLayoutPlan {
    pub(super) primary: Point<i32, Logical>,
    pub(super) primary_scale: i32,
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
    pub(super) fn secondary_location(&self, name: &str) -> Point<i32, Logical> {
        self.secondary
            .iter()
            .find(|(secondary_name, _, _)| secondary_name == name)
            .map(|(_, location, _)| *location)
            .unwrap_or((0, 0).into())
    }

    pub(super) fn secondary_scale(&self, name: &str) -> Option<i32> {
        self.secondary
            .iter()
            .find(|(secondary_name, _, _)| secondary_name == name)
            .map(|(_, _, scale)| *scale)
    }
}

pub(super) fn output_locations_for_layout(
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

    if !placements_connected(&placed) {
        let physical_placed = output_placements_for_physical_positions(
            fallback_primary.clone(),
            fallback_secondary.clone(),
            layout,
        );
        if !placements_overlap(&physical_placed) && placements_connected(&physical_placed) {
            debug!(
                direct = ?placement_debug(&placed),
                physical = ?placement_debug(&physical_placed),
                "interpreting saved monitor positions as physical coordinates"
            );
            placed = physical_placed;
        }
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

fn output_placements_for_physical_positions(
    primary: OutputGeometry,
    secondary: Vec<OutputGeometry>,
    layout: &OutputLayout,
) -> Vec<OutputPlacement> {
    let primary_location = layout
        .physical_location_for(&primary.name, "primary")
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
        if let Some(location) = layout.physical_location_for(&output.name, &role_id) {
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

    placed
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

fn placements_connected(placements: &[OutputPlacement]) -> bool {
    let Some(first) = placements.first() else {
        return true;
    };
    let mut connected = vec![first.name.clone()];
    let mut changed = true;

    while changed {
        changed = false;
        for placement in placements {
            if connected.contains(&placement.name) {
                continue;
            }
            let rect = Rectangle::new(placement.location, placement.size);
            if placements.iter().any(|other| {
                connected.contains(&other.name)
                    && rectangles_connected(rect, Rectangle::new(other.location, other.size))
            }) {
                connected.push(placement.name.clone());
                changed = true;
            }
        }
    }

    connected.len() == placements.len()
}

fn rectangles_connected(a: Rectangle<i32, Logical>, b: Rectangle<i32, Logical>) -> bool {
    rectangles_overlap(a, b) || rectangles_touch(a, b) || rectangles_touch(b, a)
}

fn rectangles_touch(a: Rectangle<i32, Logical>, b: Rectangle<i32, Logical>) -> bool {
    let horizontal_touch = a.loc.x + a.size.w == b.loc.x
        && a.loc.y < b.loc.y + b.size.h
        && a.loc.y + a.size.h > b.loc.y;
    let vertical_touch = a.loc.y + a.size.h == b.loc.y
        && a.loc.x < b.loc.x + b.size.w
        && a.loc.x + a.size.w > b.loc.x;
    horizontal_touch || vertical_touch
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

/// Native layout policy: selected primary at (0, 0), all other connectors placed
/// horizontally to the right in stable connector-name order.
#[cfg(feature = "udev")]
pub(super) fn secondary_descriptors_for_layout(
    descriptors: Vec<super::OutputDescriptor>,
    primary_name: &str,
) -> Vec<super::OutputDescriptor> {
    let mut secondary = descriptors
        .into_iter()
        .skip(1)
        .filter(|descriptor| descriptor.name != primary_name)
        .collect::<Vec<_>>();
    secondary.sort_by(|a, b| a.name.cmp(&b.name));
    secondary
}

#[cfg(all(test, feature = "udev"))]
pub(super) fn minimum_output_width(
    rects: impl IntoIterator<Item = Rectangle<i32, Logical>>,
) -> Option<i32> {
    rects.into_iter().map(|rect| rect.size.w.max(1)).min()
}

#[cfg(all(test, feature = "udev"))]
mod tests {
    use smithay::{
        output::{PhysicalProperties, Subpixel},
        utils::{Physical, Point, Rectangle, Size},
    };

    use super::{
        OutputGeometry, OutputLayout, minimum_output_width, output_locations_for_layout,
        secondary_descriptors_for_layout,
    };
    use crate::compositor::OutputDescriptor;

    fn descriptor(name: &str) -> OutputDescriptor {
        OutputDescriptor {
            name: name.into(),
            properties: PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "test".into(),
                model: name.into(),
                serial_number: "test".into(),
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
    fn output_layout_keeps_backend_scale_when_monitors_are_placeholders() {
        // Default placeholder monitors ("primary"/"secondary", scale 1) must not
        // override a differently-named backend output's own scale through the
        // "primary" role fallback. Regression: `scale_for` hardcoded role
        // fallback, so `--headless-scale`/native scale was silently reset to 1.
        let primary = OutputGeometry {
            name: "hearthspace-0".into(),
            size: Size::<i32, Physical>::from((1280, 720)),
            scale: 2,
        };

        let plan = output_locations_for_layout(
            primary,
            vec![],
            &OutputLayout::from_monitors(hearthspace_ipc::Settings::default().monitors),
        );

        assert_eq!(plan.primary_scale, 2);
    }

    #[test]
    fn output_layout_places_scaled_outputs_by_logical_size() {
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
        assert_eq!(plan.secondary_location("HDMI-A-1"), Point::from((1280, 0)));
        assert_eq!(plan.secondary_scale("HDMI-A-1"), Some(1));
    }

    #[test]
    fn output_layout_compacts_scaled_monitor_left_of_unscaled_monitor() {
        let plan = output_locations_for_layout(
            geometry_with_size("HDMI-A-1", 1920, 1080),
            vec![geometry_with_size("eDP-1", 1920, 1080)],
            &OutputLayout::from_monitors(vec![
                hearthspace_ipc::MonitorConfig {
                    id: "eDP-1".into(),
                    name: "eDP-1".into(),
                    width: 1920,
                    height: 1080,
                    scale: 2,
                    x: -960,
                    y: 0,
                },
                hearthspace_ipc::MonitorConfig {
                    id: "HDMI-A-1".into(),
                    name: "HDMI-A-1".into(),
                    width: 1920,
                    height: 1080,
                    scale: 1,
                    x: 0,
                    y: 0,
                },
            ]),
        );

        assert_eq!(plan.secondary_location("eDP-1"), Point::from((0, 0)));
        assert_eq!(plan.primary, Point::from((960, 0)));
    }

    #[test]
    fn output_layout_accepts_old_physical_positions_for_scaled_outputs() {
        let plan = output_locations_for_layout(
            geometry_with_size("HDMI-A-1", 1920, 1080),
            vec![geometry_with_size("eDP-1", 1920, 1080)],
            &OutputLayout::from_monitors(vec![
                hearthspace_ipc::MonitorConfig {
                    id: "eDP-1".into(),
                    name: "eDP-1".into(),
                    width: 1920,
                    height: 1080,
                    scale: 2,
                    x: -1920,
                    y: 0,
                },
                hearthspace_ipc::MonitorConfig {
                    id: "HDMI-A-1".into(),
                    name: "HDMI-A-1".into(),
                    width: 1920,
                    height: 1080,
                    scale: 1,
                    x: 0,
                    y: 0,
                },
            ]),
        );

        assert_eq!(plan.secondary_location("eDP-1"), Point::from((0, 0)));
        assert_eq!(plan.primary, Point::from((960, 0)));
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
