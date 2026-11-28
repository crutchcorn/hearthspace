//! Reusable native content is hosted in one persistent surface per layout part.
use super::{focus, widget_key, widgets};
use crate::ui::{popup, state::ShellState};
use hearthspace_common::frame_layout::{ObjectPlacement, frame_widget_app_id, widget_title};
use xilem::{
    WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::{kurbo::Axis, layout::AsUnit},
    style::Style,
    view::{indexed_stack, label, sized_box},
};

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    let mut windows = Vec::new();
    for layout in &state.frame_layout.snapshot.outputs {
        let Some(output) = state
            .outputs
            .iter()
            .find(|output| output.key() == layout.output)
        else {
            continue;
        };
        for instance in layout.layout.widgets.instances() {
            let identity = instance.id().to_string();
            let id = instance.kind().key();
            let key = widget_key(&layout.output, &identity);
            let Some(window) = state.frame_layout.windows.get(&key).copied() else {
                continue;
            };
            let Some(chain) = state.frame_layout.chains.get(&key) else {
                continue;
            };
            let target = hearthspace_common::frame_layout::LayoutTarget::widget(&identity);
            let part = layout.widgets.iter().find(|part| part.id == identity);
            let grouped =
                layout.layout.group_for(&target).is_some() || part.is_some_and(|part| part.merged);
            let vertical = part.is_some_and(|part| part.orientation == "vertical");
            let expanded = layout.layout.is_expanded(&identity);
            let available = state.frame_layout.visible
                && layout.layout.placement(&target) != ObjectPlacement::Hidden
                && (!instance.preferences().hide_when_unavailable
                    || ((id != "battery" || state.battery.is_some())
                        && (id != "tray"
                            || crate::ui::tray_state::visible(&state.tray.snapshot)
                                .next()
                                .is_some())));
            let mut size = widgets::size(
                state,
                id,
                vertical,
                expanded,
                grouped,
                layout.layout.widget_clock_date_format(&identity),
                layout.compact,
            );
            if id == "apps"
                && let Some(part) = part.filter(|part| part.merged)
            {
                if vertical {
                    size.1 = part.rect.height.max(44.0);
                } else {
                    size.0 = part.rect.width.max(44.0);
                }
            }
            if id == "tray" && !grouped {
                size = super::tray::size(layout, vertical, size);
            }
            let content = widgets::content(
                state,
                output,
                &identity,
                vertical,
                expanded,
                layout.layout.widget_clock_date_format(&identity),
                chain,
            );
            let content = sized_box(content).padding(if grouped { 0.0 } else { 5.0 }.px());
            let content = super::material(content, !grouped, state).boxed();
            let request = state
                .frame_layout
                .snapshot
                .focus_request
                .clone()
                .filter(|request| request.output == layout.output && request.target == target);
            let view = focus::boundary(
                indexed_stack((
                    sized_box(label("")).width(size.0.px()).height(size.1.px()),
                    content,
                ))
                .active(usize::from(available)),
                chain.clone(),
                layout.output.clone(),
                target,
                request,
            );
            windows.push(popup::window(
                window,
                widget_title(id),
                frame_widget_app_id(output.output_name.as_deref(), &identity),
                LogicalSize::new(size.0, size.1),
                view,
                |_| {},
            ));
        }
    }
    windows
}

pub(super) fn axis(vertical: bool) -> Axis {
    if vertical {
        Axis::Vertical
    } else {
        Axis::Horizontal
    }
}
