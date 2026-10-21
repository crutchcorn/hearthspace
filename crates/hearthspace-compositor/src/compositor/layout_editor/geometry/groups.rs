//! Measure and pack native members inside an output-bounded group.
use super::*;
pub(super) struct GroupMeasurement {
    pub size: (f64, f64),
    members: Vec<(Target, (f64, f64))>,
}
impl App {
    pub(super) fn frame_group_measurement(
        &self,
        layout: &FrameLayout,
        target: &Target,
        view: Rect,
        vertical: bool,
    ) -> Option<GroupMeasurement> {
        let group = layout.groups.get(&target.target.id)?;
        let members: Vec<_> = group
            .members
            .iter()
            .filter(|member| {
                member.kind != TargetKind::Widget
                    || self.frame_widget_available(&target.output, &member.id)
            })
            .filter(|member| {
                member.kind != TargetKind::Window
                    || self
                        .target_window_index(&Target {
                            output: target.output.clone(),
                            target: (*member).clone(),
                        })
                        .is_some()
            })
            .map(|member| {
                let target = Target {
                    output: target.output.clone(),
                    target: member.clone(),
                };
                let size = self.frame_target_size(&target, true, vertical, view);
                (target, size)
            })
            .collect();
        if members.is_empty() {
            return None;
        }
        let along = members
            .iter()
            .map(|(_, s)| if vertical { s.1 } else { s.0 })
            .sum::<f64>()
            + members.len().saturating_sub(1) as f64 * 4.
            + 10.;
        let across = members
            .iter()
            .map(|(_, s)| if vertical { s.0 } else { s.1 })
            .fold(44., f64::max)
            + 10.;
        let maximum = if vertical { view.height } else { view.width }
            - if view.width <= 760. { 24. } else { 48. };
        let along = along.min(maximum.max(54.));
        Some(GroupMeasurement {
            size: if vertical {
                (across, along)
            } else {
                (along, across)
            },
            members,
        })
    }

    pub(super) fn place_frame_group(
        &self,
        layout: &FrameLayout,
        target: &Target,
        placement: ObjectPlacement,
        view: Rect,
        vertical: bool,
        result: &mut Vec<PlacedTarget>,
    ) {
        let Some(GroupMeasurement { size, members }) =
            self.frame_group_measurement(layout, target, view, vertical)
        else {
            return;
        };
        let rect = self.standalone_frame_rect(target, placement, size, view);
        result.push(PlacedTarget {
            target: target.clone(),
            rect,
            placement,
            group: None,
            vertical,
        });
        let scale = if placement.is_frame() {
            1.
        } else {
            self.viewport_scale
        };
        let mut offset = 5. * scale;
        for (member, (width, height)) in members {
            let width = width * scale;
            let height = height * scale;
            let child = Rect {
                x: rect.x
                    + if vertical {
                        (rect.width - width) / 2.
                    } else {
                        offset
                    },
                y: rect.y
                    + if vertical {
                        offset
                    } else {
                        (rect.height - height) / 2.
                    },
                width,
                height,
            };
            offset += if vertical { height } else { width } + 4. * scale;
            result.push(PlacedTarget {
                target: member,
                rect: child,
                placement,
                group: Some(target.target.id.clone()),
                vertical,
            });
        }
    }
}
