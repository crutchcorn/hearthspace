//! Installed apps acquire result cards only while searching, never canvas icons.
use crate::ui::{popup, state::ShellState};
use hearthspace_ui_components::icons::{Icon, icon};
use xilem::{
    AnyWidgetView, WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, button, flex_col, flex_row, image, label, sized_box},
};

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    let search = &state.canvas_search;
    search
        .snapshot
        .results
        .iter()
        .filter(|result| result.kind == "app")
        .filter_map(|result| {
            let app_id = result.id.strip_prefix("app:")?;
            let app = state.catalog.app_by_id(app_id)?;
            let window = *search.result_windows.get(&result.id)?;
            let artwork: Box<AnyWidgetView<ShellState>> =
                if let Some(brush) = crate::ui::dock::app_image(state, app) {
                    image(brush).decorative(true).boxed()
                } else {
                    icon(Icon::AppsGrid, 26.0)
                        .color(state.appearance.palette.text)
                        .boxed()
                };
            let id = result.id.clone();
            let contents = flex_row((
                sized_box(artwork).width(30.0.px()).height(30.0.px()),
                flex_col((
                    label(app.name.clone())
                        .text_size(14.0)
                        .color(state.appearance.palette.text)
                        .line_break_mode(LineBreaking::Clip),
                    label("Open application")
                        .text_size(12.0)
                        .color(state.appearance.palette.muted),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(5.0.px())
                .flex(1.0),
            ))
            .gap(14.0.px());
            let content = super::material::card(
                button(contents, move |state: &mut ShellState| {
                    super::activate(state, Some(&id))
                })
                .padding(16.0.px())
                .width(240.0.px())
                .height(72.0.px())
                .corner_radius(20.0.px()),
                result.selected,
                state.appearance.palette,
            )
            .hovered(search.snapshot.hovered_id.as_ref() == Some(&result.id));
            let app_id = hearthspace_common::canvas_search_result_app_id(
                search.snapshot.session,
                &result.id,
            );
            Some(popup::window(
                window,
                &app.name,
                app_id,
                LogicalSize::new(240.0, 72.0),
                content,
                |_| {},
            ))
        })
        .collect()
}
