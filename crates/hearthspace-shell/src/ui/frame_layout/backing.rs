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
    WindowView,
    dpi::LogicalSize,
    masonry::layout::AsUnit,
    style::Style,
    view::{button, label, sized_box},
};

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
            windows.push(crate::ui::popup::window(
                id,
                "Frame group",
                frame_group_app_id(Some(&output.output), &group.id),
                LogicalSize::new(group.rect.width, group.rect.height),
                crate::ui::canvas_search::material::shell(
                    sized_box(label(""))
                        .width(group.rect.width.px())
                        .height(group.rect.height.px()),
                    state.appearance.palette,
                ),
                |_| {},
            ));
            if !state.frame_layout.snapshot.editing {
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
            let content = components::button_surface(
                button(
                    icon(Icon::Grip, 14.0).label("Move group"),
                    move |state: &mut ShellState| {
                        super::command(
                            state,
                            "select",
                            serde_json::json!({"output":output_name,"target":target}),
                        )
                    },
                )
                .padding(4.0.px())
                .width(group.grip.width.max(1.0).px())
                .height(group.grip.height.max(1.0).px())
                .corner_radius(10.0.px()),
                components::ButtonSurface::Raised,
                false,
            );
            windows.push(crate::ui::popup::window(
                id,
                "Move group",
                frame_group_grip_app_id(Some(&output.output), &group.id),
                LogicalSize::new(group.grip.width.max(1.0), group.grip.height.max(1.0)),
                content,
                |_| {},
            ));
        }
    }
    windows
}
