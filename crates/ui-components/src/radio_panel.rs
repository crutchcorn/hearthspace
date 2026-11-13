//! Production radio lists use the same controls and materials as Storybook.
use crate::{
    Palette,
    controls::{self, ButtonVariant},
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::{AsUnit, Dim},
        parley::style::FontWeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, label},
};
mod identity;
use identity::identified;
pub struct Row<A> {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub connected: bool,
    pub known: bool,
    pub secured: bool,
    pub actions: Vec<(String, A)>,
}
pub struct Panel<A> {
    pub title: &'static str,
    pub enabled: Option<bool>,
    pub busy: bool,
    pub blocked: bool,
    pub rows: Vec<Row<A>>,
    pub message: Option<String>,
    pub selected: Option<String>,
}
pub fn view<S: 'static, A: Clone + Send + Sync + 'static>(
    palette: Palette,
    panel: Panel<A>,
    toggle: impl Fn(&mut S, bool) + Send + Sync + 'static,
    scan: impl Fn(&mut S) + Send + Sync + 'static,
    action: impl Fn(&mut S, A) + Clone + Send + Sync + 'static,
    navigate: impl Fn(&mut S, Option<String>) + Clone + Send + Sync + 'static,
) -> impl WidgetView<S> {
    let bluetooth = panel.title == "Bluetooth";
    let disabled = panel.busy || panel.enabled.is_none() || panel.blocked;
    let glyph = if bluetooth {
        Icon::Bluetooth
    } else {
        Icon::Wifi
    };
    let selected = panel
        .rows
        .iter()
        .find(|r| Some(&r.id) == panel.selected.as_ref());
    let body: Box<AnyWidgetView<S>> = if panel.enabled == Some(false) {
        flex_col((
            label(format!("{} is off", panel.title)).text_size(13.),
            label(if bluetooth {
                "Turn on Bluetooth to connect your devices."
            } else {
                "Turn on Wi-Fi to connect to a network."
            })
            .text_size(11.)
            .color(palette.muted)
            .line_break_mode(LineBreaking::WordWrap),
            controls::form_button_with_icon(
                palette,
                Icon::RotateCcw,
                if bluetooth {
                    "Scan for devices"
                } else {
                    "Refresh networks"
                },
                ButtonVariant::Quiet,
                true,
                scan,
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(8.0.px())
        .boxed()
    } else if let Some(row) = selected {
        let back = navigate.clone();
        let buttons = row
            .actions
            .iter()
            .map(|(name, a)| {
                let a = a.clone();
                let action = action.clone();
                controls::form_button(
                    palette,
                    name.clone(),
                    ButtonVariant::Quiet,
                    disabled,
                    move |s: &mut S| action(s, a.clone()),
                )
            })
            .collect::<Vec<_>>();
        identified(
            row.id.clone(),
            flex_col((
                controls::form_button_with_icon(
                    palette,
                    Icon::ArrowLeft,
                    "Back",
                    ButtonVariant::Quiet,
                    false,
                    move |s: &mut S| back(s, None),
                ),
                label(format!("{} details", row.name))
                    .text_size(16.)
                    .weight(FontWeight::MEDIUM)
                    .line_break_mode(LineBreaking::WordWrap),
                label(row.detail.clone())
                    .text_size(12.)
                    .color(palette.muted)
                    .line_break_mode(LineBreaking::WordWrap),
                flex_col(buttons)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(8.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(16.0.px()),
        )
        .boxed()
    } else {
        let mut rows: Vec<Box<AnyWidgetView<S>>> = vec![];
        for (heading, known) in if bluetooth {
            vec![("My devices", true), ("Nearby devices", false)]
        } else {
            vec![("Networks", true)]
        } {
            let first = rows.is_empty();
            rows.push(
                label(heading)
                    .text_size(11.)
                    .color(palette.muted)
                    .padding(Padding {
                        top: if first { 0.0 } else { 16.0 }.px(),
                        bottom: 6.0.px(),
                        left: 0.0.px(),
                        right: 0.0.px(),
                    })
                    .boxed(),
            );
            for row in panel
                .rows
                .iter()
                .filter(|r| !bluetooth || (r.known || r.connected) == known)
            {
                let Some((verb, command)) = row.actions.first() else {
                    continue;
                };
                let command = command.clone();
                let action = action.clone();
                let primary_name = if bluetooth {
                    format!("{verb} {}", row.name)
                } else {
                    format!(
                        "{verb} {} {}",
                        if row.connected { "from" } else { "to" },
                        row.name
                    )
                };
                let id = row.id.clone();
                let details = navigate.clone();
                let content = flex_row((
                    icon(glyph, 19.).color(if row.connected {
                        palette.accent
                    } else {
                        palette.text
                    }),
                    flex_col((
                        label(row.name.clone())
                            .text_size(13.)
                            .weight(FontWeight::MEDIUM)
                            .line_break_mode(LineBreaking::WordWrap)
                            .width(Dim::Stretch),
                        label(row.detail.clone())
                            .text_size(11.)
                            .color(palette.muted)
                            .line_break_mode(LineBreaking::WordWrap)
                            .width(Dim::Stretch),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(3.0.px())
                    .flex(1.),
                    (row.connected || row.secured).then(|| {
                        icon(
                            if row.connected {
                                Icon::Check
                            } else {
                                Icon::Lock
                            },
                            17.,
                        )
                        .color(if row.connected {
                            palette.accent
                        } else {
                            palette.muted
                        })
                    }),
                ))
                .gap(12.0.px());
                let primary =
                    crate::semantic_button(content, primary_name, None, move |s: &mut S| {
                        action(s, command.clone())
                    })
                    .padding(Padding::from_vh(10.0.px(), 8.0.px()))
                    .corner_radius(10.0.px());
                let detail = crate::semantic_button(
                    icon(Icon::ChevronRight, 17.).color(palette.muted),
                    format!("{} details", row.name),
                    None,
                    move |s: &mut S| details(s, Some(id.clone())),
                )
                .width(40.0.px())
                .height(64.0.px())
                .corner_radius(10.0.px());
                rows.push(
                    identified(
                        format!("{}:{verb}", row.id),
                        flex_row((
                            controls::button_frame(
                                primary,
                                ButtonVariant::Quiet,
                                disabled || panel.enabled != Some(true),
                            )
                            .flex(1.),
                            controls::button_frame(detail, ButtonVariant::Quiet, disabled)
                                .width(40.0.px())
                                .height(64.0.px()),
                        ))
                        .cross_axis_alignment(CrossAxisAlignment::Stretch),
                    )
                    .boxed(),
                );
            }
        }
        flex_col((
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(2.0.px()),
            flex_row((controls::form_button_with_icon(
                palette,
                Icon::RotateCcw,
                if bluetooth {
                    "Scan for devices"
                } else {
                    "Refresh networks"
                },
                ButtonVariant::Quiet,
                disabled || panel.enabled != Some(true),
                scan,
            ),)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px())
        .boxed()
    };
    flex_col((
        // A bare title and compact Shared Controls switch, as in the reference.
        flex_row((
            label(panel.title).text_size(15.).flex(1.),
            identified(
                format!("{}:{:?}", panel.title, panel.enabled),
                controls::toggle::toggle(
                    panel.title,
                    panel.enabled.unwrap_or(false),
                    disabled,
                    toggle,
                ),
            )
            .width(40.0.px())
            .height(24.0.px()),
        ))
        .height(44.0.px()),
        panel.message.map(|text| {
            label(text)
                .text_size(12.)
                .color(palette.muted)
                .line_break_mode(LineBreaking::WordWrap)
        }),
        panel.blocked.then(|| {
            label("Wireless is blocked by a hardware switch or airplane mode.")
                .text_size(12.)
                .line_break_mode(LineBreaking::WordWrap)
        }),
        (panel.rows.is_empty() && panel.enabled == Some(true))
            .then(|| label("No devices found. Refresh to search nearby.").text_size(12.)),
        panel.busy.then(|| label("Working…").text_size(12.)),
        body,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(16.0.px())
}
