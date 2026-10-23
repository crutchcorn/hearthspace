//! The bar's application icons and bounded, content-sized DBusMenu popup.
use super::{
    popup::{self, PopupKind},
    state::{ShellOutputInstance, ShellState},
    tray_button::{self, Target},
    tray_state::{self, TrayState},
};
use crate::ui::appearance::{ThemeStyle as _, Tone};
use hearthspace_common::{TRAY_MENU_APP_ID, tray_menu_app_id_for_output};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView, WindowView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, label},
};

#[cfg(test)]
pub(super) fn strip(
    output: &ShellOutputInstance,
    state: &TrayState,
    reserved_width: f64,
) -> Option<impl WidgetView<ShellState> + use<>> {
    tray_state::visible(&state.snapshot).next()?;
    let icons = tray_state::visible(&state.snapshot)
        .take(tray_state::capacity(output.tray.bar_width - reserved_width))
        .map(|item| tray_button::icon(output.key(), item))
        .collect::<Vec<_>>();
    Some(flex_row(icons).gap(4.0.px()))
}

pub(super) fn frame_strip(
    output: &ShellOutputInstance,
    state: &ShellState,
    vertical: bool,
    chain: &super::frame_layout::focus::Chain,
    scroll_in_parent: bool,
) -> impl WidgetView<ShellState> + use<> {
    let icons = tray_state::strip_visible(state, output.key())
        .enumerate()
        .map(|(index, item)| {
            let mut icon = tray_button::icon(output.key(), item);
            icon.appearance.scroll_in_parent = scroll_in_parent;
            super::frame_layout::focus::item(icon, chain, index).boxed()
        })
        .collect::<Vec<_>>();
    flex_row(icons)
        .direction(if vertical {
            xilem::masonry::kurbo::Axis::Vertical
        } else {
            xilem::masonry::kurbo::Axis::Horizontal
        })
        .gap(4.0.px())
}

pub(super) fn window(output: &ShellOutputInstance, state: &TrayState) -> WindowView<ShellState> {
    let app_id = output
        .output_name
        .as_deref()
        .map(tray_menu_app_id_for_output)
        .unwrap_or_else(|| TRAY_MENU_APP_ID.to_string());
    let key = output.key().to_string();
    let size = popup::size(output, PopupKind::Tray, 340.0, 160.0);
    popup::window(
        output.tray.window_id,
        "Hearthspace Application Tray",
        app_id,
        size,
        panel(output, state),
        move |state| tray_state::dismiss(state, &key),
    )
}

pub(super) fn panel(
    output: &ShellOutputInstance,
    state: &TrayState,
) -> impl WidgetView<ShellState> + use<> {
    let item = output
        .tray
        .item
        .as_ref()
        .and_then(|key| tray_state::visible(&state.snapshot).find(|item| &item.key == key));
    let title = item
        .map_or("Application menu", |item| item.title.as_str())
        .to_string();
    let nodes = tray_state::menu_items(&state.snapshot, &output.tray);
    let mut rows: Vec<Box<AnyWidgetView<ShellState>>> = Vec::new();
    let error = output
        .tray
        .error
        .as_deref()
        .or_else(|| item.and_then(|item| item.menu_error.as_deref()))
        .or(state.snapshot.error.as_deref());
    if let Some(error) = error {
        rows.push(
            label(error.to_string())
                .line_break_mode(LineBreaking::WordWrap)
                .theme_color(Tone::Muted)
                .boxed(),
        );
    }
    if let Some(item) = item {
        for node in nodes.iter().filter(|node| node.visible) {
            rows.push(
                tray_button::menu(
                    output.key(),
                    &item.key,
                    output.tray.menu_path.as_deref().unwrap_or_default(),
                    node,
                    output.tray.selected == Some(node.id),
                )
                .boxed(),
            );
        }
        if nodes.is_empty() {
            rows.push(components::muted_text("No menu actions available").boxed());
        }
    }
    let back = (!output.tray.path.is_empty())
        .then(|| tray_button::text(output.key(), "Back".into(), Target::Back, false));
    popup::panel_with_header(
        output,
        PopupKind::Tray,
        flex_row((
            label(title)
                .text_size(15.0)
                .line_break_mode(LineBreaking::Clip)
                .flex(1.0),
            back,
            tray_button::text(
                output.key(),
                "Close".into(),
                Target::Close,
                item.is_some() && output.tray.selected.is_none(),
            ),
        ))
        .gap(4.0.px()),
        flex_col(rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(4.0.px()),
    )
}
