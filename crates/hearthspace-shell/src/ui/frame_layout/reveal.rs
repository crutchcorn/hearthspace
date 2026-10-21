//! Named, non-destructive native overlays expose live pinned windows.
use crate::ui::state::ShellState;
use hearthspace_common::frame_layout::{FrameEdge, pinned_window_reveal_app_id};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{flex_row, indexed_stack, label, sized_box, transformed},
};

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    state
        .frame_layout
        .snapshot
        .pinned_windows
        .iter()
        .filter_map(|pinned| {
            let window = *state
                .frame_layout
                .windows
                .get(&format!("reveal:{}", pinned.id))?;
            let rect = pinned.rect;
            let vertical = matches!(pinned.edge, Some(FrameEdge::Left | FrameEdge::Right));
            let width = rect.width.max(1.0);
            let height = rect.height.max(1.0);
            let id = pinned.id;
            let caption = flex_row((
                icon(Icon::AppsGrid, 18.0),
                label(pinned.title.clone())
                    .text_size(12.0)
                    .line_break_mode(LineBreaking::Clip),
            ))
            .gap(10.0.px());
            let caption: Box<xilem::AnyWidgetView<ShellState>> = if vertical {
                sized_box(
                    transformed(sized_box(caption).width(height.px()).height(40.0.px()))
                        .rotate(std::f64::consts::FRAC_PI_2)
                        .translate((40.0, 0.0)),
                )
                .width(40.0.px())
                .height(height.px())
                .boxed()
            } else {
                sized_box(caption).height(40.0.px()).boxed()
            };
            let control = components::button_surface(
                components::semantic_button(
                    caption,
                    format!("Open pinned {}", pinned.title),
                    None,
                    move |state: &mut ShellState| {
                        super::submit(state, format!("frame-reveal {id}"))
                    },
                )
                .width(width.px())
                .height(height.px())
                .padding(0.0.px())
                .corner_radius(15.0.px()),
                components::ButtonSurface::Bare,
                false,
            );
            let target = hearthspace_common::frame_layout::LayoutTarget::new(
                hearthspace_common::frame_layout::TargetKind::Window,
                id.to_string(),
            );
            let chain = state
                .frame_layout
                .chains
                .get(&target.key())
                .cloned()
                .unwrap_or_default();
            let request = state
                .frame_layout
                .snapshot
                .focus_request
                .clone()
                .filter(|request| request.target == target);
            let content = crate::ui::canvas_search::material::shell(
                super::focus::item(control, &chain, 0),
                state.appearance.palette,
            );
            let available = !pinned.expanded && !state.frame_layout.snapshot.editing;
            let view = indexed_stack((
                sized_box(label("")).width(width.px()).height(height.px()),
                content,
            ))
            .active(usize::from(available));
            let view = super::focus::boundary(view, chain, pinned.output.clone(), target, request);
            Some(crate::ui::popup::window(
                window,
                "Pinned window",
                pinned_window_reveal_app_id(id),
                LogicalSize::new(width, height),
                view,
                |_| {},
            ))
        })
        .collect()
}
