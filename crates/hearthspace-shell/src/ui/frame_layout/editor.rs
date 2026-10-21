//! The reference's compact placement inspector, composed of native controls.
use crate::ui::{
    appearance::{ThemeStyle as _, Tone},
    state::ShellState,
};
use hearthspace_common::frame_layout::{
    EXPANDABLE_WIDGETS, LayoutTarget, ObjectPlacement, TargetKind, layout_editor_app_id,
};
use hearthspace_ui_components::{
    self as components, Palette,
    icons::{Icon, icon},
};
use xilem::{
    WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::{
        layout::AsUnit,
        parley::style::FontWeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, FlexExt, button, flex_col, flex_row, label, portal, sized_box},
};

pub(super) fn window(state: &ShellState) -> Option<WindowView<ShellState>> {
    let layout = &state.frame_layout;
    if !layout.snapshot.editing {
        return None;
    }
    let id = layout.editor?;
    let size = size(state);
    Some(crate::ui::popup::window(
        id,
        "Change layout",
        layout_editor_app_id(layout.snapshot.session, layout.snapshot.output.as_deref()),
        LogicalSize::new(size.0, size.1),
        view(state),
        |state| super::command(state, "done", serde_json::json!({})),
    ))
}

fn selection(
    state: &ShellState,
) -> Option<(
    &LayoutTarget,
    &hearthspace_common::frame_layout::FrameLayout,
)> {
    let selected = state.frame_layout.snapshot.selected.as_ref()?;
    let output = state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find(|output| output.output == selected.output)?;
    Some((&selected.target, &output.layout))
}

fn options(state: &ShellState) -> (bool, bool, bool) {
    selection(state).map_or((false, false, false), |(target, layout)| {
        (
            target.kind == TargetKind::Widget && EXPANDABLE_WIDGETS.contains(&target.id.as_str()),
            layout.group_for(target).is_some(),
            target.kind == TargetKind::Widget && target.id == "clock",
        )
    })
}

fn size(state: &ShellState) -> (f64, f64) {
    let (expand, group, clock) = options(state);
    let height: f64 = if selection(state).is_none() {
        54.0
    } else if clock {
        452.0
    } else if expand || group {
        156.0
    } else {
        103.0
    };
    (
        state
            .frame_layout
            .snapshot
            .editor_max_width
            .clamp(120.0, 260.0),
        height.min(state.frame_layout.snapshot.editor_max_height.max(54.0)),
    )
}

pub(super) fn view(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    let selected = selection(state);
    let palette = state.appearance.palette;
    let (expandable, grouped, is_clock) = options(state);
    let title = if selected.is_none() {
        "Choose an item to place".into()
    } else if !state.frame_layout.snapshot.selected_title.is_empty() {
        state.frame_layout.snapshot.selected_title.clone()
    } else {
        selected
            .map(|(target, _)| {
                hearthspace_common::frame_layout::widget_title(&target.id).to_owned()
            })
            .unwrap_or_default()
    };
    let removable = selected.is_some_and(|(target, layout)| {
        matches!(target.kind, TargetKind::Widget | TargetKind::Group)
            || layout.placement(target).is_frame()
    });
    let header = flex_row((
        label(title)
            .font("Liberation Sans")
            .text_size(13.0)
            .weight(FontWeight::SEMI_BOLD)
            .line_break_mode(LineBreaking::Clip)
            .padding(Padding {
                left: 10.0.px(),
                ..Padding::ZERO
            })
            .flex(1.0),
        removable.then(|| {
            action(
                Icon::Trash2,
                if selected.is_some_and(|(target, _)| {
                    matches!(target.kind, TargetKind::Item | TargetKind::Window)
                }) {
                    "Remove from frame"
                } else {
                    "Remove"
                },
                "remove",
                false,
                palette,
            )
        }),
        action(Icon::Check, "Done changing layout", "done", false, palette),
    ))
    .gap(4.0.px())
    .height(44.0.px());
    let options = (expandable || grouped).then(|| {
        let details = expandable.then(|| {
            let expanded = selected.is_some_and(|(target, layout)| layout.is_expanded(&target.id));
            text_action(
                if expanded { Icon::Square } else { Icon::List },
                if expanded {
                    "Icon only"
                } else {
                    "Show details"
                },
                if expanded {
                    "Shrink to icon"
                } else {
                    "Show details"
                },
                "toggle-expanded",
                palette,
            )
        });
        let group = grouped.then(|| {
            text_action(
                Icon::Ungroup,
                if selected.is_some_and(|(target, _)| target.kind == TargetKind::Group) {
                    "Ungroup"
                } else {
                    "Separate"
                },
                if selected.is_some_and(|(target, _)| target.kind == TargetKind::Group) {
                    "Ungroup items"
                } else {
                    "Remove from group"
                },
                "ungroup",
                palette,
            )
        });
        separated(
            flex_row((details, group)).gap(4.0.px()).height(44.0.px()),
            4.0,
        )
    });
    let placements = selected.map(|(target, layout)| {
        let placement = layout.placement(target);
        separated(
            flex_row((
                placement_button(
                    Icon::Layers,
                    "Canvas",
                    matches!(placement, ObjectPlacement::Canvas { .. }),
                    "canvas",
                    palette,
                )
                .flex(1.0),
                placement_button(
                    Icon::BackgroundLayer,
                    "Background",
                    matches!(placement, ObjectPlacement::Background { .. }),
                    "background",
                    palette,
                )
                .flex(1.0),
                placement_button(
                    Icon::FrameLayout,
                    "Frame",
                    placement.is_frame(),
                    "frame",
                    palette,
                )
                .flex(1.0),
            ))
            .gap(4.0.px()),
            0.0,
        )
    });
    let date = is_clock.then(|| {
        super::editor_clock::view(
            selected.unwrap().1.clock_date_format,
            state.appearance.palette,
        )
    });
    let errors = state.frame_layout.error.clone().map(|error| {
        label(error)
            .font("Liberation Sans")
            .text_size(12.0)
            .theme_color(Tone::Muted)
            .line_break_mode(LineBreaking::WordWrap)
    });
    let content = flex_col((header, options, date, placements, errors))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(0.0.px());
    let size = size(state);
    crate::ui::canvas_search::material::shell(
        sized_box(crate::ui::popup_group::group(
            "Change layout",
            portal(content).constrain_horizontal(true),
        ))
        .padding(5.0.px())
        .width(size.0.px())
        .height(size.1.px()),
        state.appearance.palette,
    )
}

fn separated<V: WidgetView<ShellState>>(
    inner: V,
    bottom: f64,
) -> impl WidgetView<ShellState> + use<V> {
    flex_col((
        sized_box(label(""))
            .height(1.0.px())
            .theme_background(Tone::Border),
        sized_box(inner).padding(Padding {
            top: 4.0.px(),
            bottom: bottom.px(),
            ..Padding::ZERO
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(0.0.px())
}

fn action(
    glyph: Icon,
    name: &'static str,
    action: &'static str,
    selected: bool,
    palette: Palette,
) -> impl WidgetView<ShellState> + use<> {
    super::tooltip::hint(
        crate::ui::canvas_search::material::control(
            button(
                icon(glyph, 19.0).color(palette.text).label(name),
                move |state: &mut ShellState| super::command(state, action, serde_json::json!({})),
            )
            .width(44.0.px())
            .height(44.0.px())
            .padding(10.0.px())
            .corner_radius(15.0.px()),
            selected,
            palette,
        ),
        if action == "done" { "Done" } else { name },
    )
}

fn text_action(
    glyph: Icon,
    name: &'static str,
    accessible_name: &'static str,
    action: &'static str,
    palette: Palette,
) -> impl WidgetView<ShellState> + use<> {
    crate::ui::canvas_search::material::control(
        components::semantic_button(
            flex_row((
                icon(glyph, 18.0).color(palette.text),
                label(name)
                    .font("Liberation Sans")
                    .text_size(12.0)
                    .color(palette.text),
            ))
            .gap(8.0.px()),
            accessible_name,
            None,
            move |state: &mut ShellState| super::command(state, action, serde_json::json!({})),
        )
        .height(44.0.px())
        .padding(10.0.px())
        .corner_radius(15.0.px()),
        false,
        palette,
    )
}

fn placement_button(
    glyph: Icon,
    name: &'static str,
    selected: bool,
    surface: &'static str,
    palette: Palette,
) -> impl WidgetView<ShellState> + use<> {
    super::tooltip::hint(
        crate::ui::canvas_search::material::control(
            components::semantic_button(
                icon(glyph, 19.0).color(if selected {
                    palette.accent
                } else {
                    palette.text
                }),
                name,
                Some(selected),
                move |state: &mut ShellState| {
                    if surface == "frame" {
                        super::command(state, "attach", serde_json::json!({}))
                    } else {
                        super::command(
                            state,
                            "place",
                            serde_json::json!({"placement":{"surface":surface}}),
                        )
                    }
                },
            )
            .height(44.0.px())
            .padding(10.0.px())
            .corner_radius(15.0.px()),
            selected,
            palette,
        ),
        name,
    )
}
