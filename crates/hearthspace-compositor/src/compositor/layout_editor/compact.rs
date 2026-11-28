//! Responsive bars are a temporary projection of pristine defaults, never saved groups.
use super::{
    App, FrameLayout, LayoutTarget, ObjectPlacement, PlacedTarget, Rect, Target, TargetKind,
};
use hearthspace_common::frame_layout::{FrameEdge, SYSTEM_WIDGETS, starter_placement};

const TOP: &str = "@compact-top";
const BOTTOM: &str = "@compact-bottom";

pub(super) fn backing(id: &str) -> bool {
    matches!(id, TOP | BOTTOM)
}

fn defaults(layout: &FrameLayout, view: Rect) -> (bool, bool) {
    if view.width > 760. {
        return (false, false);
    }
    let original = |id| {
        let Some(instance) = layout.widget_id(id) else {
            return false;
        };
        let target = LayoutTarget::widget(instance);
        layout.group_for(&target).is_none() && layout.placement(&target) == starter_placement(id)
    };
    let system = layout.groups.get("system").is_some_and(|group| {
        group.members.len() == SYSTEM_WIDGETS.len()
            && group.members
                == SYSTEM_WIDGETS
                    .iter()
                    .filter_map(|kind| layout.widget_id(kind))
                    .map(LayoutTarget::widget)
                    .collect::<Vec<_>>()
            && layout.placement(&LayoutTarget::new(TargetKind::Group, "system"))
                == ObjectPlacement::frame(FrameEdge::Top, 1.)
    });
    (
        original("notifications") && system,
        original("tools") && original("apps"),
    )
}

impl App {
    pub(super) fn compact_frame_bars(&self, output: &str) -> (bool, bool) {
        self.layout_editor
            .layouts
            .get(output)
            .zip(self.frame_output_rect(output))
            .map_or((false, false), |(layout, view)| defaults(layout, view))
    }

    pub(super) fn frame_widget_merged(&self, output: &str, id: &str) -> bool {
        let (top, bottom) = self.compact_frame_bars(output);
        let Some(layout) = self.layout_editor.layouts.get(output) else {
            return false;
        };
        let is_primary = |kind| layout.widget_id(kind).as_deref() == Some(id);
        (top && is_primary("notifications"))
            || (bottom && (is_primary("tools") || is_primary("apps")))
    }

    pub(super) fn frame_group_material(&self, output: &str, id: &str) -> bool {
        id != "system" || !self.compact_frame_bars(output).0
    }

    pub(in crate::compositor) fn frame_group_has_material(&self, index: usize) -> bool {
        self.frame_group_identity(index)
            .is_some_and(|(output, id)| self.frame_group_material(&output, &id))
    }

    pub(in crate::compositor) fn frame_control_has_material(&self, index: usize) -> bool {
        let widget = self
            .frame_widget_identity(index)
            .is_some_and(|(output, id)| !self.frame_widget_merged(&output, &id));
        let shortcut = self.desktop_id(index).is_some() && self.layout_target_on_frame(index);
        (widget || shortcut)
            && self
                .frame_target_for_window(index)
                .and_then(|target| self.frame_placed(&target))
                .is_some_and(|p| p.group.is_none())
    }

    pub(super) fn compact_bar_for_window(&self, index: usize) -> Option<(String, String)> {
        let target = self.frame_target_for_window(index)?;
        let (top, bottom) = self.compact_frame_bars(&target.output);
        let placed = self.frame_placed(&target)?;
        let layout = self.layout_editor.layouts.get(&target.output)?;
        let primary = |kind| {
            layout
                .widget_id(kind)
                .is_some_and(|id| target.target == LayoutTarget::widget(id))
        };
        let id = if top
            && (primary("notifications")
                || target.target == LayoutTarget::new(TargetKind::Group, "system")
                || placed.group.as_deref() == Some("system"))
        {
            TOP
        } else if bottom
            && target.target.kind == TargetKind::Widget
            && (primary("tools") || primary("apps"))
        {
            BOTTOM
        } else {
            return None;
        };
        Some((target.output, id.into()))
    }

    pub(in crate::compositor) fn compact_frame_backing(&self, index: usize) -> bool {
        self.frame_group_identity(index)
            .is_some_and(|(_, id)| backing(&id))
    }

    pub(super) fn project_compact_frame(&self, placed: &mut Vec<PlacedTarget>) {
        for (output, layout) in &self.layout_editor.layouts {
            let Some(view) = self.frame_output_rect(output) else {
                continue;
            };
            let (top, bottom) = defaults(layout, view);
            for (enabled, id, edge) in [
                (top, TOP, FrameEdge::Top),
                (bottom, BOTTOM, FrameEdge::Bottom),
            ] {
                if !enabled {
                    continue;
                }
                let rect = Rect {
                    x: view.x + 12.,
                    y: view.y
                        + if edge == FrameEdge::Top {
                            12.
                        } else {
                            view.height - 66.
                        },
                    width: (view.width - 24.).max(1.),
                    height: 54.,
                };
                placed.push(PlacedTarget {
                    target: Target {
                        output: output.clone(),
                        target: LayoutTarget::new(TargetKind::Group, id),
                    },
                    rect,
                    placement: ObjectPlacement::frame(edge, 0.5),
                    group: None,
                    vertical: false,
                });
                let left_id = if top && edge == FrameEdge::Top {
                    "notifications"
                } else {
                    "tools"
                };
                let Some(left_instance) = layout.widget_id(left_id) else {
                    continue;
                };
                let Some(left) = placed.iter_mut().find(|p| {
                    p.target.output == *output
                        && p.target.target == LayoutTarget::widget(&left_instance)
                }) else {
                    continue;
                };
                left.rect.x = rect.x + 5.;
                left.rect.y = rect.y + 5.;
                left.rect.height = 44.;
                if left_id == "tools" {
                    left.rect.width = 44.;
                }
                let remaining_x = left.rect.x + left.rect.width + 4.;
                let remaining_width = (rect.x + rect.width - 5. - remaining_x).max(1.);
                if edge == FrameEdge::Bottom {
                    let apps_instance = layout.widget_id("apps");
                    if let Some(apps) = placed.iter_mut().find(|p| {
                        p.target.output == *output
                            && apps_instance.as_deref() == Some(p.target.target.id.as_str())
                    }) {
                        apps.rect.x = remaining_x;
                        apps.rect.width = remaining_width;
                        apps.rect.y = rect.y + 5.;
                        apps.rect.height = 44.;
                    }
                    continue;
                }
                let group_target = LayoutTarget::new(TargetKind::Group, "system");
                let Some(group) = placed
                    .iter_mut()
                    .find(|p| p.target.output == *output && p.target.target == group_target)
                else {
                    continue;
                };
                let old = group.rect;
                let content_width = placed
                    .iter()
                    .filter(|p| p.target.output == *output && p.group.as_deref() == Some("system"))
                    .map(|p| p.rect.x + p.rect.width - old.x - 5.)
                    .fold(0., f64::max);
                let next = Rect {
                    x: remaining_x,
                    y: rect.y + 5.,
                    width: remaining_width,
                    height: 44.,
                };
                let dx = next.x - old.x - 5. + (next.width - content_width).max(0.);
                for child in placed
                    .iter_mut()
                    .filter(|p| p.target.output == *output && p.group.as_deref() == Some("system"))
                {
                    child.rect.x += dx;
                    child.rect.y = next.y + (44. - child.rect.height) / 2.;
                }
                placed
                    .iter_mut()
                    .find(|p| p.target.output == *output && p.target.target == group_target)
                    .unwrap()
                    .rect = next;
            }
        }
    }
}
