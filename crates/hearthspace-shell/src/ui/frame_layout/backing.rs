//! One glass body per group, with a separate inward-facing editing grip.
use crate::ui::state::ShellState;
use hearthspace_common::frame_layout::{
    LayoutTarget, TargetKind, frame_group_app_id, frame_group_grip_app_id,
};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::layout::AsUnit,
    style::Style,
    view::{button, label, sized_box},
};

fn native_size(rect: super::Rect, scale: f64) -> LogicalSize<f64> {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    // Status bounds are projected screen coordinates. Native buffers remain
    // in canvas units because the compositor applies the viewport scale.
    LogicalSize::new(
        (rect.width / scale).max(1.0),
        (rect.height / scale).max(1.0),
    )
}

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    let mut windows = Vec::new();
    for output in &state.frame_layout.snapshot.outputs {
        for group in &output.groups {
            let key = format!("group:{}:{}", output.output, group.id);
            let Some(id) = state.frame_layout.windows.get(&key).copied() else {
                continue;
            };
            if group.rect.width <= 0.0 || group.rect.height <= 0.0 {
                continue;
            }
            let body_size = native_size(group.rect, group.scale);
            let body = sized_box(label(""))
                .fixed_width(body_size.width.px())
                .fixed_height(body_size.height.px());
            let body = if group.material {
                crate::ui::canvas_search::material::shell(body, state.appearance.palette)
                    .hovered(group.hovered || group.focused)
                    .boxed()
            } else {
                body.boxed()
            };
            windows.push(crate::ui::popup::window(
                id,
                "Frame group",
                frame_group_app_id(Some(&output.output), &group.id),
                body_size,
                body,
                |_| {},
            ));
            if !state.frame_layout.snapshot.editing || group.backing_only {
                continue;
            }
            let Some(id) = state
                .frame_layout
                .windows
                .get(&format!("grip:{}:{}", output.output, group.id))
                .copied()
            else {
                continue;
            };
            let target = LayoutTarget::new(TargetKind::Group, group.id.clone());
            let output_name = output.output.clone();
            let grip_size = native_size(group.grip, group.scale);
            let content = components::button_surface(
                button(
                    icon(Icon::Grip, 14.0).label("Move group"),
                    move |state: &mut ShellState| {
                        super::command_on_output(
                            state,
                            output_name.clone(),
                            hearthspace_ipc::frame_layout::Action::Select {
                                target: target.clone(),
                            },
                        )
                    },
                )
                .padding(4.0.px())
                .width(grip_size.width.px())
                .height(grip_size.height.px())
                .corner_radius(10.0.px()),
                components::ButtonSurface::Raised,
                false,
            );
            windows.push(crate::ui::popup::window(
                id,
                "Move group",
                frame_group_grip_app_id(Some(&output.output), &group.id),
                grip_size,
                content,
                |_| {},
            ));
        }
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_group_and_grip_keep_native_canvas_dimensions_at_any_zoom() {
        for scale in [0.5, 1.0, 2.0] {
            let group: super::super::Group = serde_json::from_value(serde_json::json!({
                "id": "system", "scale": scale,
                "rect": {"width": 54.0 * scale, "height": 302.0 * scale},
                "grip": {"width": 24.0 * scale, "height": 28.0 * scale}
            }))
            .unwrap();
            assert_eq!(
                native_size(group.rect, group.scale),
                LogicalSize::new(54.0, 302.0)
            );
            assert_eq!(
                native_size(group.grip, group.scale),
                LogicalSize::new(24.0, 28.0)
            );
        }
        let legacy: super::super::Group = serde_json::from_value(serde_json::json!({
            "id": "system", "rect": {"width": 302.0, "height": 54.0}
        }))
        .unwrap();
        assert_eq!(legacy.scale, 1.0);
        assert_eq!(
            native_size(legacy.rect, legacy.scale),
            LogicalSize::new(302.0, 54.0)
        );
    }
}
