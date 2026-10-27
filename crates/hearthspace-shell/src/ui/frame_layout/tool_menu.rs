//! Primary canvas choices share one picker; compact layouts include every tool.
use super::{ShellState, focus};
use crate::ui::{quick_panel::Kind, search_focus::Record, state::ShellOutputInstance};
use hearthspace_ipc::{command::CanvasTool, shortcuts::CanvasAction};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    dpi::LogicalSize,
    masonry::{
        layout::AsUnit,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, label, sized_box},
};

struct Entry {
    title: &'static str,
    glyph: Icon,
    tool: Option<CanvasTool>,
}

const TOOLS: [Entry; 6] = [
    Entry {
        title: "Select desktop items",
        glyph: Icon::Cursor,
        tool: Some(CanvasTool::Select),
    },
    Entry {
        title: "Pan canvas",
        glyph: Icon::Pan,
        tool: Some(CanvasTool::Pan),
    },
    Entry {
        title: "Search canvas",
        glyph: Icon::Search,
        tool: Some(CanvasTool::Search),
    },
    Entry {
        title: "Select window group",
        glyph: Icon::Lasso,
        tool: Some(CanvasTool::Group),
    },
    Entry {
        title: "Change layout",
        glyph: Icon::FrameLayout,
        tool: Some(CanvasTool::Layout),
    },
    Entry {
        title: "Canvas map",
        glyph: Icon::Map,
        tool: None,
    },
];
const ROW_HEIGHT: f64 = 44.0;
const INSET: f64 = 6.0;
const ERROR_HEIGHT: f64 = 64.0;

fn index(tool: CanvasTool) -> usize {
    TOOLS
        .iter()
        .position(|entry| entry.tool == Some(tool))
        .unwrap()
}

pub(in crate::ui) fn primary(state: &ShellState) -> CanvasTool {
    match state.frame_layout.snapshot.canvas_tool {
        Some(tool @ (CanvasTool::Select | CanvasTool::Pan | CanvasTool::Search)) => tool,
        _ => state
            .frame_layout
            .last_primary_tool
            .unwrap_or(CanvasTool::Select),
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

fn entries(state: &ShellState, output: &str) -> &'static [Entry] {
    &TOOLS[..if compact(state, output) {
        TOOLS.len()
    } else {
        3
    }]
}

pub(in crate::ui) fn size(state: &ShellState, output: &ShellOutputInstance) -> LogicalSize<f64> {
    let mut size = LogicalSize::new(
        260.0,
        ROW_HEIGHT * entries(state, output.key()).len() as f64
            + 2.0 * INSET
            + if output.tool_error.is_some() {
                ERROR_HEIGHT
            } else {
                0.0
            },
    );
    if let Some(frame) = state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find(|frame| frame.output == output.key())
    {
        if frame.rect.width > 0.0 {
            size.width = size.width.min((frame.rect.width - 24.0).max(1.0));
        }
        if frame.rect.height > 0.0 {
            size.height = size.height.min((frame.rect.height - 24.0).max(1.0));
        }
    }
    size
}

pub(super) fn trigger(
    state: &ShellState,
    output: &ShellOutputInstance,
    chain: &focus::Chain,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_owned();
    let compact = compact(state, output.key());
    let primary = primary(state);
    let active = state.frame_layout.snapshot.canvas_tool;
    let displayed = if compact {
        active.unwrap_or(primary)
    } else {
        primary
    };
    let entry = &TOOLS[index(displayed)];
    let open = output.show_controls && output.quick_panel == Kind::Tools;
    let name = if compact {
        "Canvas tools".into()
    } else {
        format!("{}, choose canvas tool", entry.title)
    };
    let selected = active == Some(displayed);
    focus::item(
        crate::ui::corner_button::focus(components::button_surface(
            components::semantic_button(
                flex_row((
                    icon(entry.glyph, 20.0),
                    icon(
                        if open {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronUp
                        },
                        6.0,
                    ),
                ))
                .gap(0.0.px()),
                name,
                Some(selected),
                move |state: &mut ShellState| {
                    if !compact && state.frame_layout.snapshot.canvas_tool != Some(primary) {
                        super::select_tool(state, primary);
                    } else {
                        state.toggle_quick_panel(&key, Kind::Tools);
                    }
                },
            )
            .width(44.0.px())
            .height(44.0.px())
            .padding(7.0.px())
            .corner_radius(15.0.px()),
            components::ButtonSurface::Bare,
            selected || open,
        )),
        chain,
        0,
    )
}

pub(in crate::ui) fn button(
    state: &ShellState,
    tool: CanvasTool,
) -> impl WidgetView<ShellState> + use<> {
    let entry = &TOOLS[index(tool)];
    let selected = state.frame_layout.snapshot.canvas_tool == Some(tool);
    crate::ui::corner_button::focus(components::button_surface(
        components::semantic_button(
            icon(entry.glyph, 20.0),
            entry.title,
            Some(selected),
            move |state: &mut ShellState| super::select_tool(state, tool),
        )
        .width(44.0.px())
        .height(44.0.px())
        .padding(11.0.px())
        .corner_radius(15.0.px()),
        components::ButtonSurface::Bare,
        selected,
    ))
}

pub(in crate::ui) fn panel(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    let entries = entries(state, output.key());
    let active = state.frame_layout.snapshot.canvas_tool;
    let focus_index = active
        .map(index)
        .filter(|index| *index < entries.len())
        .unwrap_or_else(|| index(primary(state)));
    let palette = state.appearance.palette;
    let targets = &output.tool_menu_targets[..entries.len()];
    let size = size(state, output);
    let rows: Vec<Box<AnyWidgetView<ShellState>>> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let key = output.key().to_owned();
            let selected = entry.tool.is_some() && entry.tool == active;
            let tool = entry.tool;
            let button = components::semantic_button(
                flex_row((
                    icon(entry.glyph, 18.0),
                    label(entry.title)
                        .font("Liberation Sans")
                        .text_size(12.0)
                        .line_break_mode(LineBreaking::WordWrap)
                        .flex(1.0),
                    label(
                        entry
                            .tool
                            .map_or(CanvasAction::Map, CanvasAction::Tool)
                            .shortcut()
                            .chord(),
                    )
                    .text_size(11.0)
                    .color(palette.muted),
                    sized_box(icon(Icon::Check, 14.0).color(if selected {
                        palette.accent
                    } else {
                        xilem::Color::TRANSPARENT
                    }))
                    .fixed_width(14.0.px()),
                ))
                .gap(10.0.px()),
                entry.title,
                entry.tool.map(|_| selected),
                move |state: &mut ShellState| {
                    if let Some(tool) = tool {
                        super::tool_selection::choose(state, &key, tool);
                    } else {
                        state.dismiss_panels();
                        state.toggle_map(&key);
                    }
                },
            )
            .height(ROW_HEIGHT.px())
            .padding(Padding::from_vh(8.0.px(), 10.0.px()))
            .background_color(xilem::Color::TRANSPARENT)
            .border_width(0.0.px())
            .corner_radius(10.0.px());
            crate::ui::canvas_search::material::control(
                Record {
                    inner: button,
                    target: targets[index].clone(),
                },
                selected,
                palette,
            )
            .boxed()
        })
        .collect();
    let error = output.tool_error.as_ref().map(|error| {
        crate::ui::popup_group::group(
            error.clone(),
            label(error.clone())
                .font("Liberation Sans")
                .text_size(12.0)
                .line_break_mode(LineBreaking::WordWrap)
                .padding(8.0.px()),
        )
        .status()
        .height(ERROR_HEIGHT.px())
    });
    // Handle choice keys before they bubble to the portal's scrolling keys.
    let choices = crate::ui::search_focus::focus_on_activation(
        flex_col(rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(0.0.px()),
        targets[focus_index].clone(),
        output.key().to_owned(),
    )
    .with_arrow_navigation(targets.to_vec())
    .with_dismiss(ShellState::dismiss_controls)
    .dismiss_directly();
    crate::ui::canvas_search::material::shell(
        sized_box(
            flex_col((
                error,
                components::touch_portal(
                    crate::ui::popup_group::group("Canvas tools", choices)
                        .without_accessibility_bounds(),
                )
                .constrain_horizontal(true)
                .must_fill(false)
                .flex(1.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(0.0.px()),
        )
        .padding(INSET.px())
        .width(size.width.px())
        .height(size.height.px()),
        palette,
    )
}
