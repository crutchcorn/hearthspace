//! Small status dropdowns share the shell's existing popup lifecycle.
use super::{
    control_slider::ControlSlider,
    control_state::Action,
    icons::{Icon, glyph, icon_button},
    popup::{self, PopupKind},
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_common::{CONTROL_CENTER_APP_ID, control_center_app_id_for_output};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView, WindowView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, FlexSpacer, button, flex_col, flex_row, label, sized_box},
};

#[path = "quick_calendar.rs"]
mod calendar;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Kind {
    #[default]
    Controls,
    Wifi,
    Sound,
    Brightness,
    Battery,
    Clock,
}

impl Kind {
    fn title(self) -> &'static str {
        match self {
            Self::Controls => "Control center",
            Self::Wifi => "Wi-Fi",
            Self::Sound => "Sound",
            Self::Brightness => "Brightness",
            Self::Battery => "Battery",
            Self::Clock => "Date and time",
        }
    }
}

pub(super) fn button_view(
    output: &ShellOutputInstance,
    kind: Kind,
    icon: Icon,
    name: &str,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    icon_button(
        icon,
        name,
        output.show_controls && output.quick_panel == kind,
        move |state| state.toggle_quick_panel(&key, kind),
    )
}

pub(super) fn clock_button(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let text = state.window_group.hint().unwrap_or_else(|| {
        state
            .clock
            .split_whitespace()
            .last()
            .unwrap_or(&state.clock)
            .to_string()
    });
    components::button_surface(
        button(
            flex_row((
                components::icons::icon(components::icons::Icon::Clock, 16.0)
                    .label("Date and time"),
                label(text)
                    .text_size(14.0)
                    .color(components::TEXT)
                    .line_break_mode(LineBreaking::Clip)
                    .flex(1.0),
            ))
            .gap(7.0.px()),
            move |state: &mut ShellState| state.toggle_quick_panel(&key, Kind::Clock),
        )
        .padding(10.0.px())
        .width(88.0.px())
        .height(44.0.px())
        .corner_radius(14.0.px()),
        components::ButtonSurface::Raised,
        output.show_controls && output.quick_panel == Kind::Clock,
    )
}

/// Keep every status dropdown reachable when the output needs a compact tray.
pub(super) fn shortcuts(output: &ShellOutputInstance) -> impl WidgetView<ShellState> + use<> {
    flex_row((
        button_view(output, Kind::Wifi, Icon::Wifi, "Wi-Fi settings"),
        button_view(
            output,
            Kind::Sound,
            Icon::Volume { muted: false },
            "Sound settings",
        ),
        button_view(
            output,
            Kind::Battery,
            Icon::Battery {
                percentage: 100,
                charging: false,
            },
            "Battery status",
        ),
        {
            let key = output.key().to_string();
            components::button_surface(
                button(
                    components::icons::icon(components::icons::Icon::Clock, 20.0)
                        .label("Date and time"),
                    move |state: &mut ShellState| state.toggle_quick_panel(&key, Kind::Clock),
                )
                .padding(11.0.px()),
                components::ButtonSurface::Bare,
                false,
            )
        },
    ))
    .gap(4.0.px())
}

pub(super) fn window(state: &ShellState, output: &ShellOutputInstance) -> WindowView<ShellState> {
    if output.quick_panel == Kind::Controls {
        return super::control_center::window(output, &state.controls);
    }
    let key = output.key().to_string();
    popup::window(
        output.controls_window_id,
        &format!("Hearthspace {}", output.quick_panel.title()),
        output
            .output_name
            .as_deref()
            .map(control_center_app_id_for_output)
            .unwrap_or_else(|| CONTROL_CENTER_APP_ID.into()),
        popup::size(output, PopupKind::Controls, 320.0, 200.0),
        panel(state, output),
        move |state| state.dismiss_controls(&key),
    )
}

pub(super) fn panel(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> Box<AnyWidgetView<ShellState>> {
    let content = match output.quick_panel {
        Kind::Controls => return super::control_center::panel(output, &state.controls).boxed(),
        Kind::Wifi => wifi().boxed(),
        Kind::Sound => sound(state, output).boxed(),
        Kind::Brightness => brightness(state, output).boxed(),
        Kind::Battery => battery(state).boxed(),
        Kind::Clock => calendar::view().boxed(),
    };
    popup::panel_named(
        output,
        PopupKind::Controls,
        output.quick_panel.title(),
        content,
    )
    .boxed()
}

fn heading(title: &'static str, detail: String) -> impl WidgetView<ShellState> + use<> {
    flex_row((
        label(title).text_size(16.0).color(components::TEXT),
        FlexSpacer::Flex(1.0),
        label(detail).text_size(30.0).color(components::TEXT),
    ))
    .gap(12.0.px())
}

fn wifi() -> impl WidgetView<ShellState> + use<> {
    flex_col((
        heading("Wi-Fi", String::new()),
        components::icons::icon(components::icons::Icon::Wifi, 24.0).color(components::TEXT_MUTED),
        label("Network controls").text_size(20.0),
        label("Use your network tray application to manage connections.")
            .text_size(13.0)
            .color(components::TEXT_MUTED)
            .line_break_mode(LineBreaking::WordWrap),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(16.0.px())
}

fn sound(state: &ShellState, output: &ShellOutputInstance) -> impl WidgetView<ShellState> + use<> {
    let audio = state.controls.snapshot.audio.as_ref().ok();
    let muted = audio.is_some_and(|audio| audio.muted);
    let value = state.controls.volume();
    let key = output.key().to_string();
    let more_key = key.clone();
    flex_col((
        heading(
            "Sound",
            value.map_or("—".into(), |value| format!("{value}%")),
        ),
        flex_row((
            components::button_surface(
                button(
                    glyph(
                        Icon::Volume { muted },
                        if muted { "Unmute" } else { "Mute" }.into(),
                    ),
                    move |state: &mut ShellState| {
                        super::control_task::submit(state, &key, Action::ToggleMute, None);
                    },
                )
                .disabled(audio.is_none() || state.controls.sender.is_none())
                .padding(10.0.px()),
                components::ButtonSurface::Bare,
                muted,
            ),
            sized_box(ControlSlider {
                name: "Volume",
                output: output.key().into(),
                minimum: 0,
                value: value.unwrap_or(0),
                enabled: value.is_some() && state.controls.sender.is_some(),
            })
            .height(28.0.px())
            .flex(1.0),
            glyph(Icon::Volume { muted: false }, "Sound output".into()),
        ))
        .gap(8.0.px()),
        label(if muted {
            "Muted".to_string()
        } else {
            audio
                .map(|audio| audio.output.clone())
                .unwrap_or_else(|| "No audio output available".into())
        })
        .text_size(13.0)
        .color(components::TEXT_MUTED)
        .line_break_mode(LineBreaking::WordWrap),
        components::compact_action_button(
            "Brightness and sound controls",
            move |state: &mut ShellState| {
                state.toggle_controls(&more_key);
            },
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(16.0.px())
}

fn brightness(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    let value = state.controls.brightness();
    let available = value.is_some() && state.controls.sender.is_some();
    let reason = state.controls.brightness_error.clone().or_else(|| {
        match &state.controls.snapshot.brightness {
            Err(error) => Some(error.clone()),
            Ok(None) => Some("This display has no adjustable backlight.".into()),
            Ok(Some(_)) if state.controls.sender.is_none() => {
                Some("Brightness controls are unavailable.".into())
            }
            Ok(Some(_)) => None,
        }
    });
    let more_key = output.key().to_string();
    flex_col((
        heading(
            "Brightness",
            value.map_or("—".into(), |value| format!("{value}%")),
        ),
        flex_row((
            components::icons::icon(components::icons::Icon::Sun, 16.0)
                .color(components::TEXT_MUTED),
            sized_box(ControlSlider {
                name: "Brightness",
                output: output.key().into(),
                minimum: 1,
                value: value.unwrap_or(1),
                enabled: available,
            })
            .height(28.0.px())
            .flex(1.0),
            components::icons::icon(components::icons::Icon::Sun, 22.0).color(components::TEXT),
        ))
        .gap(12.0.px()),
        label(reason.unwrap_or_else(|| "Adjust the display backlight.".into()))
            .text_size(13.0)
            .color(components::TEXT_MUTED)
            .line_break_mode(LineBreaking::WordWrap),
        components::compact_action_button(
            "Brightness and sound controls",
            move |state: &mut ShellState| state.toggle_controls(&more_key),
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(16.0.px())
}

fn battery(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    let battery = state.battery.as_ref();
    let percentage = battery.map_or(0.0, |battery| f64::from(battery.percentage));
    let meter = flex_row((
        sized_box(label(""))
            .height(8.0.px())
            .background_color(components::ACCENT)
            .corner_radius(4.0.px())
            .flex(percentage.max(0.001)),
        sized_box(label(""))
            .height(8.0.px())
            .background_color(components::BORDER)
            .corner_radius(4.0.px())
            .flex((100.0 - percentage).max(0.001)),
    ))
    .gap(0.0.px());
    flex_col((
        heading(
            "Battery",
            battery.map_or("—".into(), |battery| format!("{}%", battery.percentage)),
        ),
        meter,
        components::muted_text(battery.map_or("No battery detected".into(), |battery| {
            battery.state_label().to_string()
        })),
        super::battery_view::detail(battery),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(20.0.px())
}

#[cfg(test)]
#[path = "quick_panel_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "quick_brightness_tests.rs"]
mod brightness_tests;
