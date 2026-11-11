//! Shared Controls use the web's story identifiers, labels, values and sizes.
use super::{components, state::AppState};
use components::controls::{self, ButtonVariant, ControlKind};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row, sized_box},
};
use xilem_storybook::{Control, Story};

pub(super) fn stories() -> Vec<Story> {
    let mut stories = Vec::new();
    for (family, variants) in [
        (
            "button",
            &[
                "neutral",
                "primary",
                "destructive",
                "quiet",
                "disabled",
                "icon-only",
            ][..],
        ),
        (
            "text-input",
            &["empty", "filled", "password", "invalid", "disabled"][..],
        ),
        ("toggle-switch", &["off", "on", "disabled"][..]),
        ("checkbox", &["unchecked", "checked", "disabled"][..]),
        ("select", &["automatic", "disabled"][..]),
        ("level-bar", &["meter", "progress"][..]),
        ("level-slider", &["adjustable", "disabled"][..]),
    ] {
        for variant in variants {
            let id = format!("shared-controls-{family}--{variant}");
            let mut controls = vec![Control::bool(
                "disabled",
                "Disabled",
                *variant == "disabled",
            )];
            match family {
                "button" => controls.push(Control::text(
                    "text",
                    "Label",
                    match *variant {
                        "neutral" => "Cancel",
                        "destructive" => "Forget device",
                        "quiet" => "Refresh networks",
                        "icon-only" => "Close dialog",
                        _ => "Connect",
                    },
                )),
                "text-input" if matches!(*variant, "password" | "invalid") => {}
                "text-input" => controls.push(Control::text(
                    "value",
                    "Value",
                    if matches!(*variant, "filled" | "disabled") {
                        "Studio network"
                    } else {
                        ""
                    },
                )),
                "toggle-switch" | "checkbox" => controls.push(Control::bool(
                    "checked",
                    "Checked",
                    matches!(*variant, "on" | "checked"),
                )),
                "select" => controls.push(Control::bool("manual", "Manual configuration", false)),
                _ => controls.push(Control::text(
                    "value",
                    "Value (0–100)",
                    if *variant == "meter" {
                        "72"
                    } else if *variant == "progress" {
                        "38"
                    } else {
                        "65"
                    },
                )),
            }
            stories.push(Story::new(
                id,
                format!("{} / {}", title(family), title(variant)),
                if matches!(*variant, "password" | "invalid") && family == "text-input" { "Masked visual sample; the current native editor does not support password masking." } else { "Shared Controls reference preview" },
                controls,
            ));
        }
    }
    stories
}
fn title(s: &str) -> String {
    s.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().unwrap().to_uppercase().to_string() + c.as_str()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn theme_controls(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let p = state.palette();
    flex_row((
        controls::form_button(
            p,
            if state.dark { "Dark" } else { "Light" },
            ButtonVariant::Neutral,
            false,
            |s: &mut AppState| s.dark = !s.dark,
        ),
        controls::select::select(
            p,
            "Accent",
            vec!["Orange", "Rose", "Violet", "Blue", "Teal", "Olive"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            state.accent,
            false,
            |s: &mut AppState, i| s.accent = i,
        ),
    ))
    .gap(12.0.px())
}

pub(super) fn preview(state: &AppState) -> Box<AnyWidgetView<AppState>> {
    let id = state.selected_story().id();
    let disabled = state.selected_bool("disabled").unwrap_or(false);
    let p = state.palette();
    let content: Box<AnyWidgetView<AppState>> = if id.starts_with("shared-controls-button") {
        let variant = if id.ends_with("primary") || id.ends_with("disabled") {
            ButtonVariant::Primary
        } else if id.ends_with("destructive") {
            ButtonVariant::Danger
        } else if id.ends_with("quiet") {
            ButtonVariant::Quiet
        } else {
            ButtonVariant::Neutral
        };
        if id.ends_with("icon-only") {
            components::settings_button(
                components::semantic_button(
                    components::icons::icon(components::icons::Icon::X, 18.),
                    state
                        .selected_text("text")
                        .unwrap_or("Close dialog")
                        .to_string(),
                    None,
                    |s: &mut AppState| s.clicks += 1,
                )
                .height(36.0.px())
                .padding(8.0.px()),
                components::SettingsButton::Bare,
                false,
            )
            .disabled(disabled)
            .boxed()
        } else {
            controls::form_button(
                p,
                state.selected_text("text").unwrap_or("Connect").to_string(),
                variant,
                disabled,
                |s: &mut AppState| s.clicks += 1,
            )
            .boxed()
        }
    } else if id.starts_with("shared-controls-text-input") {
        let password = id.ends_with("password") || id.ends_with("invalid");
        let invalid = id.ends_with("invalid");
        let field: Box<AnyWidgetView<AppState>> = if password {
            // Masonry currently lacks a masked editor. The gallery must not present a
            // plaintext field as password-safe. This is an explicit visual sample.
            sized_box(
                controls::form_label(if invalid {
                    "•••••••"
                } else {
                    "••••••••••"
                })
                .color(p.text),
            )
            .height(42.0.px())
            .padding(14.0.px())
            .border_width(1.0.px())
            .border_color(if invalid {
                xilem::Color::from_rgb8(197, 72, 65)
            } else {
                p.chrome_border
            })
            .corner_radius(10.0.px())
            .background_color(p.surface_control)
            .boxed()
        } else {
            controls::text_field(
                state.selected_text("value").unwrap_or("").into(),
                "Enter a network name",
                invalid,
                disabled,
                |s: &mut AppState, v| s.set_selected_text("value", v),
            )
            .boxed()
        };
        sized_box(
            flex_col((
                controls::form_label(if password { "Password" } else { "Network name" }),
                field,
                invalid.then(|| controls::form_label("Enter the password again.").color(p.muted)),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(7.0.px()),
        )
        .fixed_width(256.0.px())
        .boxed()
    } else if id.starts_with("shared-controls-toggle-switch")
        || id.starts_with("shared-controls-checkbox")
    {
        let switch = id.starts_with("shared-controls-toggle-switch");
        let checked = state.selected_bool("checked").unwrap_or(false);
        let name = if switch {
            "Bluetooth"
        } else {
            "Metered connection"
        };
        flex_row((
            controls::form_label(name).text_size(13.0),
            controls::control(
                if switch {
                    ControlKind::Switch
                } else {
                    ControlKind::Checkbox
                },
                name,
                f64::from(checked),
                disabled,
                |s: &mut AppState, v| s.set_selected_bool("checked", v != 0.),
            ),
        ))
        .main_axis_alignment(MainAxisAlignment::Center)
        .gap(12.0.px())
        .boxed()
    } else if id.starts_with("shared-controls-select") {
        sized_box(
            flex_col((
                controls::form_label("DNS configuration"),
                controls::select::select(
                    p,
                    "DNS configuration",
                    vec!["Automatic".into(), "Manual".into()],
                    usize::from(state.selected_bool("manual").unwrap_or(false)),
                    disabled,
                    |s: &mut AppState, i| s.set_selected_bool("manual", i == 1),
                ),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(7.0.px()),
        )
        .fixed_width(220.0.px())
        .boxed()
    } else {
        let kind = if id.starts_with("shared-controls-level-slider") {
            ControlKind::Slider
        } else if id.ends_with("progress") {
            ControlKind::Progress
        } else {
            ControlKind::Meter
        };
        let value = state
            .selected_text("value")
            .unwrap_or("0")
            .parse()
            .unwrap_or(0.);
        sized_box(controls::control(
            kind,
            match kind {
                ControlKind::Slider => "Sound",
                ControlKind::Progress => "Model download",
                _ => "Battery charge",
            },
            value,
            disabled,
            |s: &mut AppState, v| s.set_selected_text("value", format!("{v:.0}")),
        ))
        .fixed_width(256.0.px())
        .padding(xilem::masonry::properties::Padding::from_vh(
            0.0.px(),
            if kind == ControlKind::Slider {
                10.0
            } else {
                0.0
            }
            .px(),
        ))
        .boxed()
    };
    flex_col((
        sized_box(
            flex_col((content,))
                .main_axis_alignment(MainAxisAlignment::Center)
                .cross_axis_alignment(CrossAxisAlignment::Center),
        )
        .height(260.0.px())
        .padding(24.0.px())
        .background_color(p.app_background),
        controls::form_label(format!("Activations: {}", state.clicks)).color(p.muted),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(8.0.px())
    .boxed()
}
