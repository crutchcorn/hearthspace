//! Searchable controls open the same native dropdowns as their corner buttons.
use crate::ui::{quick_panel::Kind, state::ShellState};
use hearthspace_common::canvas_search_result_app_id;
use hearthspace_ipc::command::CanvasTool;
use hearthspace_ui_components::icons::{Icon, icon};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use xilem::{
    WidgetView, WindowId, WindowView,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, parley::style::FontWeight, properties::LineBreaking},
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, button, flex_col, flex_row, indexed_stack, label, sized_box,
    },
};

#[path = "control_catalog.rs"]
mod catalog;
#[path = "control_details.rs"]
mod details;
use catalog::{ENTRIES, Entry};

pub(super) fn registration() -> Vec<Value> {
    ENTRIES.iter().map(|entry| json!({"id":entry.id,"title":entry.title,
        "search_text":format!("{} {} {}", entry.title, entry.keywords, if entry.id.starts_with("tool:") {"tool"} else {"widget system control"})})).collect()
}

pub(super) fn window_ids() -> BTreeMap<String, WindowId> {
    ENTRIES
        .iter()
        .map(|entry| (entry.id.to_owned(), WindowId::next()))
        .collect()
}

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    let search = &state.canvas_search;
    ENTRIES
        .iter()
        .map(|entry| {
            crate::ui::popup::window(
                search.result_windows[entry.id],
                entry.title,
                canvas_search_result_app_id(0, entry.id),
                LogicalSize::new(240.0, 72.0),
                view(state, entry),
                |_| {},
            )
        })
        .collect()
}

fn view(state: &ShellState, entry: &'static Entry) -> impl WidgetView<ShellState> + use<> {
    let selected = state.canvas_search.snapshot.selected_id.as_deref() == Some(entry.id);
    let available = state.canvas_search.snapshot.active
        && state
            .canvas_search
            .snapshot
            .results
            .iter()
            .any(|result| result.id == entry.id);
    let (glyph, detail) = detail(state, entry);
    let palette = state.appearance.palette;
    let card = super::material::card(
        button(
            flex_row((
                sized_box(
                    icon(
                        glyph,
                        if entry.id == "control:battery" {
                            28.0
                        } else {
                            26.0
                        },
                    )
                    .color(palette.text),
                )
                .width(30.0.px()),
                flex_col((
                    label(entry.title)
                        .text_size(14.0)
                        .weight(FontWeight::SEMI_BOLD)
                        .color(palette.text)
                        .line_break_mode(LineBreaking::Clip),
                    details::view(state, entry.id, detail),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(5.0.px())
                .flex(1.0),
            ))
            .gap(14.0.px()),
            move |state: &mut ShellState| super::activate(state, Some(entry.id)),
        )
        .disabled(!available)
        .padding(16.0.px())
        .width(240.0.px())
        .height(72.0.px())
        .corner_radius(20.0.px()),
        selected,
        palette,
    )
    .hovered(state.canvas_search.snapshot.hovered_id.as_deref() == Some(entry.id));
    // Stash the interactive subtree, rather than unmapping its native window:
    // renderers stay warm without exporting invisible search buttons to AT.
    indexed_stack((
        sized_box(label("")).width(240.0.px()).height(72.0.px()),
        card,
    ))
    .active(usize::from(available))
}

fn detail(state: &ShellState, entry: &Entry) -> (Icon, Option<String>) {
    let mut glyph = entry.icon;
    let text =
        match entry.id {
            "control:notifications" => {
                let count = state
                    .notifications
                    .iter()
                    .filter(|notification| notification.active)
                    .count();
                glyph = if count > 0 {
                    Icon::BellUnread
                } else {
                    Icon::Bell
                };
                Some(if count > 0 {
                    format!("{count} unread")
                } else {
                    "All clear".into()
                })
            }
            "control:volume" => Some(match &state.controls.snapshot.audio {
                Ok(audio) if audio.muted => {
                    glyph = Icon::VolumeMuted;
                    "Muted".into()
                }
                Ok(_) => state
                    .controls
                    .volume()
                    .map_or("Unavailable".into(), |volume| format!("{volume}%")),
                Err(_) => "Unavailable".into(),
            }),
            "control:brightness" => Some(
                state
                    .controls
                    .brightness()
                    .map_or("Unavailable".into(), |value| format!("{value}%")),
            ),
            "control:battery" => Some(state.battery.as_ref().map_or(
                "No battery".into(),
                |battery| {
                    glyph = Icon::Battery {
                        percentage: battery.percentage,
                        charging: battery.is_charging(),
                    };
                    format!("{}%", battery.percentage)
                },
            )),
            "control:clock" => Some(state.clock.clone()),
            "control:wifi" => Some("Network controls".into()),
            id if tool(id)
                .is_some_and(|tool| state.frame_layout.snapshot.canvas_tool == Some(tool)) =>
            {
                Some("Selected".into())
            }
            _ => None,
        };
    (glyph, text)
}

/// Called only after the compositor redeems its one-use action token.
pub(super) fn activate(state: &mut ShellState, id: &str, output: Option<&str>) {
    let key = state
        .outputs
        .iter()
        .find(|candidate| candidate.output_name.as_deref() == output)
        .or_else(|| state.outputs.first())
        .map(|output| output.key().to_owned());
    let Some(key) = key else {
        return;
    };
    if id == "tool:map" {
        // Keyboard invocations can arrive while Overview is already open.
        // Its toggle owns dismissal and must see the current popup state.
        state.toggle_map(&key);
        return;
    }
    state.dismiss_panels();
    if let Some(tool) = tool(id) {
        crate::ui::frame_layout::select_tool(state, tool);
        return;
    }
    match id {
        "control:notifications" => state.toggle_notifications(&key),
        "control:apps" => state.toggle_search(&key),
        "control:power" => state.toggle_power(&key),
        "control:wifi" => state.toggle_quick_panel(&key, Kind::Wifi),
        "control:volume" => state.toggle_quick_panel(&key, Kind::Sound),
        "control:brightness" => state.toggle_quick_panel(&key, Kind::Brightness),
        "control:battery" => state.toggle_quick_panel(&key, Kind::Battery),
        "control:clock" => state.toggle_quick_panel(&key, Kind::Clock),
        // Closing search reveals the existing canvas toolbar in its corner.
        "control:canvas" => {}
        _ => {}
    }
}

fn tool(id: &str) -> Option<CanvasTool> {
    match id {
        "tool:cursor" => Some(CanvasTool::Select),
        "tool:pan" => Some(CanvasTool::Pan),
        "tool:search" => Some(CanvasTool::Search),
        "tool:freeform" => Some(CanvasTool::Group),
        _ => None,
    }
}

#[cfg(test)]
#[path = "controls_tests.rs"]
mod tests;
