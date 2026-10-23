//! The compact toolbar exposes the same implemented tools through native rows.
use super::{ShellState, confirm::Record, focus};
use crate::ui::{quick_panel::Kind, state::ShellOutputInstance};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, properties::Padding},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, label, sized_box},
};

const TOOLS: [(&str, Icon, &str); 5] = [
    (
        "Select desktop items",
        Icon::Cursor,
        "desktop-selection-toggle",
    ),
    ("Search canvas", Icon::Search, "canvas-search-tool"),
    ("Select window group", Icon::Lasso, "group-selection-toggle"),
    ("Change layout", Icon::FrameLayout, "layout-tool"),
    ("Canvas map", Icon::Map, ""),
];

fn current(state: &ShellState) -> usize {
    if state.frame_layout.snapshot.editing {
        3
    } else if state.window_group.selecting {
        2
    } else if state.canvas_search.snapshot.mode {
        1
    } else {
        0
    }
}

pub(super) fn compact(state: &ShellState, output: &str) -> bool {
    state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .any(|item| item.output == output && item.compact)
}

pub(super) fn trigger(
    state: &ShellState,
    output: &ShellOutputInstance,
    chain: &focus::Chain,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_owned();
    let selected = output.show_controls && output.quick_panel == Kind::Tools;
    let glyph = TOOLS[current(state)].1;
    focus::item(
        components::button_surface(
            components::semantic_button(
                flex_row((
                    icon(glyph, 20.0),
                    icon(
                        if selected {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronUp
                        },
                        6.0,
                    ),
                ))
                .gap(0.0.px()),
                "Canvas tools",
                Some(selected),
                move |state: &mut ShellState| state.toggle_quick_panel(&key, Kind::Tools),
            )
            .width(44.0.px())
            .height(44.0.px())
            .padding(7.0.px())
            .corner_radius(15.0.px()),
            components::ButtonSurface::Bare,
            selected,
        ),
        chain,
        0,
    )
}

pub(in crate::ui) fn panel(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    let active = current(state);
    let palette = state.appearance.palette;
    let target = state.frame_layout.tool_menu_target.clone();
    let rows: Vec<Box<AnyWidgetView<ShellState>>> = TOOLS
        .into_iter()
        .enumerate()
        .map(|(index, (title, glyph, command))| {
            let key = output.key().to_owned();
            let selected = index == active;
            let button = components::semantic_button(
                flex_row((
                    icon(glyph, 18.0),
                    label(title)
                        .font("Liberation Sans")
                        .text_size(12.0)
                        .flex(1.0),
                    sized_box(icon(Icon::Check, 14.0).color(if selected {
                        palette.accent
                    } else {
                        xilem::Color::TRANSPARENT
                    }))
                    .fixed_width(14.0.px()),
                ))
                .gap(10.0.px()),
                title,
                Some(selected),
                move |state: &mut ShellState| {
                    state.dismiss_panels();
                    if command.is_empty() {
                        state.toggle_map(&key);
                    } else {
                        super::submit(state, command.to_owned());
                    }
                },
            )
            .height(44.0.px())
            .padding(Padding::from_vh(8.0.px(), 10.0.px()))
            .border_width(0.0.px())
            .corner_radius(10.0.px());
            let button = if selected {
                Record {
                    inner: button,
                    target: target.clone(),
                }
                .boxed()
            } else {
                button.boxed()
            };
            crate::ui::canvas_search::material::control(button, selected, palette).boxed()
        })
        .collect();
    crate::ui::search_focus::focus_on_activation(
        crate::ui::canvas_search::material::shell(
            sized_box(crate::ui::popup_group::group(
                "Canvas tools",
                flex_col(rows)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(0.0.px()),
            ))
            .padding(6.0.px())
            .width(260.0.px())
            .height(232.0.px()),
            palette,
        ),
        target,
        output.key().to_owned(),
    )
    .with_dismiss(ShellState::dismiss_controls)
    .dismiss_directly()
}
