//! The same native status backends, launcher, and canvas actions in movable parts.
use super::{focus, parts::axis};
use crate::ui::{
    quick_panel::Kind,
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_common::frame_layout::ClockDateFormat;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::AsUnit,
        parley::style::FontWeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{button, flex_row, label},
};

pub(super) fn clock(format: ClockDateFormat) -> String {
    use chrono::Local;
    let now = Local::now();
    let date = match format {
        ClockDateFormat::None => return now.format("%H:%M").to_string(),
        ClockDateFormat::Weekday => "%a",
        ClockDateFormat::Short => "%b %-d",
        ClockDateFormat::Numeric => "%m/%d/%Y",
        ClockDateFormat::Long => "%B %-d, %Y",
    };
    format!("{} · {}", now.format(date), now.format("%H:%M"))
}

pub(super) fn size(
    state: &ShellState,
    id: &str,
    vertical: bool,
    expanded: bool,
    grouped: bool,
    format: ClockDateFormat,
    compact: bool,
) -> (f64, f64) {
    let width = match id {
        "tools" if compact => 44.0,
        "tools" => 236.0,
        "apps" => {
            44.0 + if state.windows.is_empty() {
                0.0
            } else {
                4.0 + crate::ui::dock::desired_width(state)
            }
        }
        "tray" => {
            38.0 * (crate::ui::tray_state::visible(&state.tray.snapshot)
                .count()
                .min(8)
                + 1) as f64
                - 4.0
        }
        "notifications" if expanded => {
            super::text_metrics::width(&notification_text(state), 11.0) + 47.0
        }
        "clock" if expanded => (super::text_metrics::width(&clock(format), 14.0) + 24.0).max(70.0),
        "wifi" if expanded => (super::text_metrics::width("Wi-Fi", 11.0) + 43.0).max(44.0),
        "volume" | "brightness" | "battery" if expanded => {
            let percentage = match id {
                "volume" => state.controls.volume(),
                "brightness" => state.controls.brightness(),
                _ => state.battery.as_ref().map(|battery| battery.percentage),
            };
            percentage.map_or(44.0, |value| {
                super::text_metrics::width(&format!("{value}%"), 11.0)
                    + if id == "battery" { 44.0 } else { 43.0 }
            })
        }
        _ => 44.0,
    };
    let padding = if grouped { 0.0 } else { 10.0 };
    if vertical && matches!(id, "tools" | "apps" | "tray") {
        (44.0 + padding, width + padding)
    } else {
        (width + padding, 44.0 + padding)
    }
}

pub(super) fn content(
    state: &ShellState,
    output: &ShellOutputInstance,
    id: &str,
    vertical: bool,
    expanded: bool,
    format: ClockDateFormat,
    chain: &focus::Chain,
) -> Box<AnyWidgetView<ShellState>> {
    if id == "tools" {
        return if super::tool_menu::compact(state, output.key()) {
            super::tool_menu::trigger(state, output, chain).boxed()
        } else {
            tools(state, output, vertical, chain).boxed()
        };
    }
    if id == "apps" {
        return apps(state, output, vertical, chain).boxed();
    }
    if id == "tray" {
        return crate::ui::tray_view::frame_strip(output, &state.tray, vertical, chain).boxed();
    }
    let count = state
        .notifications
        .iter()
        .filter(|item| item.active)
        .count();
    let (glyph, name, detail, selected, kind) = match id {
        "notifications" => (
            if count > 0 {
                Icon::BellUnread
            } else {
                Icon::Bell
            },
            "Notifications".into(),
            Some(notification_text(state)),
            output.show_notifications,
            None,
        ),
        "clock" => (
            Icon::Clock,
            "Date and time".into(),
            Some(clock(format)),
            output.show_controls && output.quick_panel == Kind::Clock,
            Some(Kind::Clock),
        ),
        "wifi" => (
            Icon::Wifi,
            "Wi-Fi settings".into(),
            Some("Wi-Fi".into()),
            output.show_controls && output.quick_panel == Kind::Wifi,
            Some(Kind::Wifi),
        ),
        "volume" => (
            if state
                .controls
                .snapshot
                .audio
                .as_ref()
                .is_ok_and(|audio| audio.muted)
            {
                Icon::VolumeMuted
            } else {
                Icon::Volume
            },
            "Sound settings".into(),
            state.controls.volume().map(|v| format!("{v}%")),
            output.show_controls && output.quick_panel == Kind::Sound,
            Some(Kind::Sound),
        ),
        "brightness" => (
            Icon::Sun,
            "Brightness settings".into(),
            state.controls.brightness().map(|v| format!("{v}%")),
            output.show_controls && output.quick_panel == Kind::Brightness,
            Some(Kind::Brightness),
        ),
        "battery" => {
            let battery = state.battery.as_ref();
            (
                Icon::Battery {
                    percentage: battery.map_or(0, |b| b.percentage),
                    charging: battery.is_some_and(|b| b.is_charging()),
                },
                battery.map_or("Battery".into(), |b| b.accessible_label()),
                battery.map(|b| format!("{}%", b.percentage)),
                output.show_controls && output.quick_panel == Kind::Battery,
                Some(Kind::Battery),
            )
        }
        _ => (Icon::Power, "Power".into(), None, output.show_power, None),
    };
    let id = id.to_owned();
    let key = output.key().to_owned();
    let width = size(state, &id, vertical, expanded, true, format, false).0;
    let show_icon = id != "clock" || !expanded;
    let caption = expanded.then_some(detail).flatten();
    let contents = flex_row((
        show_icon
            .then(|| icon(glyph, if id == "battery" { 19.0 } else { 18.0 }).label(name.clone())),
        caption.map(|text| {
            label(text)
                .font("Liberation Sans")
                .weight(FontWeight::MEDIUM)
                .text_size(if id == "clock" { 14.0 } else { 11.0 })
                .line_break_mode(LineBreaking::Clip)
        }),
    ))
    .gap(if id == "notifications" { 9.0 } else { 5.0 }.px());
    let control = components::button_surface(
        components::semantic_button(
            contents,
            name,
            Some(selected),
            move |state: &mut ShellState| {
                if let Some(kind) = kind {
                    state.toggle_quick_panel(&key, kind)
                } else if id == "notifications" {
                    state.toggle_notifications(&key)
                } else {
                    state.toggle_power(&key)
                }
            },
        )
        .padding(if show_icon {
            Padding::all(10.0.px())
        } else {
            Padding::from_vh(0.0.px(), 12.0.px())
        })
        .width(width.px())
        .height(44.0.px())
        .corner_radius(15.0.px()),
        components::ButtonSurface::Bare,
        selected,
    );
    focus::item(crate::ui::corner_button::focus(control), chain, 0).boxed()
}

fn tools(
    state: &ShellState,
    output: &ShellOutputInstance,
    vertical: bool,
    chain: &focus::Chain,
) -> impl WidgetView<ShellState> + use<> {
    let children: Vec<Box<AnyWidgetView<ShellState>>> = vec![
        focus::item(crate::ui::desktop::selection_button(state), chain, 0).boxed(),
        focus::item(crate::ui::canvas_search::button(state), chain, 1).boxed(),
        focus::item(
            crate::ui::window_group::button(&state.window_group),
            chain,
            2,
        )
        .boxed(),
        focus::item(layout_button(state), chain, 3).boxed(),
        focus::item(crate::ui::canvas_map::map_button(output), chain, 4).boxed(),
    ];
    flex_row(children).direction(axis(vertical)).gap(4.0.px())
}

pub(super) fn layout_button(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    components::button_surface(
        button(
            icon(Icon::FrameLayout, 20.0).label("Change layout"),
            |state: &mut ShellState| {
                state.dismiss_panels();
                super::submit(state, "layout-tool".into());
            },
        )
        .padding(11.0.px())
        .width(44.0.px())
        .height(44.0.px())
        .corner_radius(15.0.px()),
        components::ButtonSurface::Bare,
        state.frame_layout.snapshot.editing,
    )
}

fn apps(
    state: &ShellState,
    output: &ShellOutputInstance,
    vertical: bool,
    chain: &focus::Chain,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_owned();
    let desired = crate::ui::dock::desired_width(state);
    let dock_width = state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find(|layout| layout.output == output.key())
        .and_then(|layout| {
            layout
                .widgets
                .iter()
                .find(|part| part.id == "apps" && part.merged)
        })
        .map_or(desired, |part| {
            (part.rect.width - 48.0).max(1.0).min(desired)
        });
    flex_row((
        (!state.windows.is_empty())
            .then(|| crate::ui::dock::frame_view(state, output, dock_width, vertical, chain)),
        focus::item(
            crate::ui::icons::icon_button(
                crate::ui::icons::Icon::Apps,
                "Apps List",
                output.show_search,
                move |state| state.toggle_search(&key),
            ),
            chain,
            100,
        ),
    ))
    .direction(axis(vertical))
    .main_axis_alignment(xilem::view::MainAxisAlignment::End)
    .gap(4.0.px())
}

fn notification_text(state: &ShellState) -> String {
    let count = state
        .notifications
        .iter()
        .filter(|item| item.active)
        .count();
    if count == 0 {
        "All clear".into()
    } else {
        format!("{count} new")
    }
}
