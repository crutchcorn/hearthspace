//! Launcher command bar and the explicit Apps / Draw and grid / list choices.
use super::{AppLayout, ShellOutputInstance, ShellState};
use crate::ui::{favorite_actions, favorite_state::Mode, search_focus::FocusTarget, search_result};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::AsUnit,
        properties::{Dimensions, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, FlexExt, FlexSpacer, flex_col, flex_row, label, sized_box},
};

fn tab(
    output: &ShellOutputInstance,
    draw: bool,
) -> impl WidgetView<ShellState, Widget: Sized> + use<> {
    let key = output.key().to_owned();
    let selected = output.drawing.active() == draw;
    let color = if selected {
        components::ACCENT_STRONG
    } else {
        components::TEXT_MUTED
    };
    search_result::control(
        if draw {
            "Draw to launch"
        } else {
            "Back to apps list"
        },
        Some(selected),
        flex_row((
            icon(if draw { Icon::Pencil } else { Icon::AppsGrid }, 17.0).color(color),
            label(if draw { "Draw" } else { "Apps" })
                .text_size(12.0)
                .color(color),
        ))
        .gap(7.0.px()),
        move |state| {
            if draw {
                favorite_actions::start(state, &key, Mode::Launch);
            } else {
                favorite_actions::cancel(state, &key);
            }
        },
    )
    .height(44.0.px())
    .padding(Padding::from_vh(0.0.px(), 10.0.px()))
    .background_color(if selected {
        components::ACCENT_SUBTLE
    } else {
        xilem::Color::TRANSPARENT
    })
    .corner_radius(10.0.px())
}

fn mode_switch(output: &ShellOutputInstance) -> impl WidgetView<ShellState, Widget: Sized> + use<> {
    sized_box(flex_row((tab(output, false).flex(1.0), tab(output, true).flex(1.0))).gap(2.0.px()))
        .padding(3.0.px())
        .border(components::CHROME_BORDER, 1.0.px())
        .corner_radius(14.0.px())
        .background_color(components::SURFACE_SUBTLE)
}

pub(in crate::ui) fn icon_control<F: Fn(&mut ShellState) + Send + Sync + 'static>(
    glyph: Icon,
    name: &str,
    selected: Option<bool>,
    action: F,
) -> impl WidgetView<ShellState, Widget: Sized> + use<F> {
    search_result::control(
        name,
        selected,
        icon(glyph, 18.0).color(if selected == Some(true) {
            components::ACCENT_STRONG
        } else {
            components::TEXT_MUTED
        }),
        action,
    )
    .dims(Dimensions::fixed(44.0.px(), 44.0.px()))
    .padding(12.0.px())
    .corner_radius(10.0.px())
    .background_color(if selected == Some(true) {
        components::SURFACE_CONTROL
    } else {
        xilem::Color::TRANSPARENT
    })
}

pub(super) fn header(
    output: &ShellOutputInstance,
    target: FocusTarget,
) -> Box<AnyWidgetView<ShellState>> {
    let narrow = output.launcher_width <= 480.0;
    let key = output.key().to_owned();
    let close = icon_control(Icon::X, "Close apps list", None, move |state| {
        state.dismiss_search(&key)
    });
    let leading: Box<AnyWidgetView<ShellState>> = if output.drawing.active() {
        flex_row((
            icon(Icon::Pencil, 23.0).color(components::ACCENT),
            label("Draw to open")
                .text_size(18.0)
                .color(components::TEXT),
        ))
        .gap(12.0.px())
        .height(54.0.px())
        .boxed()
    } else {
        crate::ui::search_input::input(output, target).boxed()
    };
    let content: Box<AnyWidgetView<ShellState>> = if narrow {
        flex_col((
            flex_row((leading.flex(1.0), close)).gap(8.0.px()),
            mode_switch(output),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px())
        .boxed()
    } else {
        flex_row((leading.flex(1.0), mode_switch(output), close))
            .gap(10.0.px())
            .boxed()
    };
    sized_box(content)
        .padding(Padding::from_vh(
            if narrow { 12.0 } else { 14.0 }.px(),
            if narrow { 12.0 } else { 20.0 }.px(),
        ))
        .background_color(components::SURFACE_CONTROL)
        .border(components::BORDER, 1.0.px())
        .boxed()
}

pub(super) fn collection(
    output: &ShellOutputInstance,
    count: usize,
) -> impl WidgetView<ShellState, Widget: Sized> + use<> {
    let searching = !output.query.trim().is_empty();
    let controls: Box<AnyWidgetView<ShellState>> = if searching {
        label(format!("{count} found"))
            .text_size(10.0)
            .color(components::TEXT_MUTED)
            .boxed()
    } else {
        let grid_key = output.key().to_owned();
        let list_key = grid_key.clone();
        flex_row((
            icon_control(
                Icon::AppsGrid,
                "Show app grid",
                Some(output.launcher_layout == AppLayout::Grid),
                move |state| set_layout(state, &grid_key, AppLayout::Grid),
            ),
            icon_control(
                Icon::List,
                "Show app list",
                Some(output.launcher_layout == AppLayout::List),
                move |state| set_layout(state, &list_key, AppLayout::List),
            ),
        ))
        .boxed()
    };
    flex_row((
        label(if searching {
            "Search results"
        } else {
            "Applications"
        })
        .text_size(15.0)
        .color(components::TEXT),
        FlexSpacer::Flex(1.0),
        controls,
    ))
    .height(58.0.px())
}

fn set_layout(state: &mut ShellState, key: &str, layout: AppLayout) {
    if let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
        output.launcher_layout = layout;
    }
}
