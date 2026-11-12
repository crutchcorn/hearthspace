//! Radio/service agnostic layout made from the Shared Controls catalog.
use crate::{
    Palette,
    controls::{self, ButtonVariant},
};
use xilem::{
    WidgetView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, flex_col, flex_row, label},
};
mod identity;
use identity::identified;
pub struct Row<A> {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub actions: Vec<(String, A)>,
}
pub struct Panel<A> {
    pub title: &'static str,
    pub enabled: Option<bool>,
    pub busy: bool,
    pub blocked: bool,
    pub rows: Vec<Row<A>>,
    pub message: Option<String>,
}
pub fn view<S: 'static, A: Clone + Send + Sync + 'static>(
    palette: Palette,
    panel: Panel<A>,
    toggle: impl Fn(&mut S, bool) + Send + Sync + 'static,
    scan: impl Fn(&mut S) + Send + Sync + 'static,
    action: impl Fn(&mut S, A) + Clone + Send + Sync + 'static,
) -> impl WidgetView<S> {
    let disabled = panel.busy || panel.enabled.is_none() || panel.blocked;
    let empty = panel.rows.is_empty();
    let has_message = panel.message.is_some();
    let rows = panel
        .rows
        .into_iter()
        .map(|row| {
            let key = format!(
                "{}:{:?}",
                row.id,
                row.actions.iter().map(|(name, _)| name).collect::<Vec<_>>()
            );
            let buttons = row
                .actions
                .into_iter()
                .map(|(name, a)| {
                    let action = action.clone();
                    controls::form_button(
                        palette,
                        name,
                        ButtonVariant::Quiet,
                        disabled || panel.enabled != Some(true),
                        move |s: &mut S| action(s, a.clone()),
                    )
                })
                .collect::<Vec<_>>();
            identified(
                key,
                flex_col((
                    flex_col((
                        label(row.name)
                            .text_size(13.0)
                            .line_break_mode(LineBreaking::WordWrap),
                        label(row.detail)
                            .text_size(11.0)
                            .color(palette.muted)
                            .line_break_mode(LineBreaking::WordWrap),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Start),
                    flex_row(buttons).gap(4.0.px()),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(4.0.px()),
            )
        })
        .collect::<Vec<_>>();
    flex_col((
        identified(
            format!("{}:{:?}", panel.title, panel.enabled),
            controls::toggle::toggle(
                panel.title,
                panel.enabled.unwrap_or(false),
                disabled,
                toggle,
            )
            .with_row(None),
        ),
        controls::form_button(
            palette,
            "Refresh devices",
            ButtonVariant::Quiet,
            disabled || panel.enabled != Some(true),
            scan,
        ),
        panel.message.map(|text| {
            label(text)
                .text_size(12.0)
                .color(palette.muted)
                .line_break_mode(LineBreaking::WordWrap)
        }),
        (panel.blocked && !has_message).then(|| {
            label("Wireless is blocked by a hardware switch or airplane mode.")
                .text_size(12.0)
                .line_break_mode(LineBreaking::WordWrap)
        }),
        (empty && panel.enabled == Some(true) && !has_message).then(|| {
            label("No devices found. Refresh to search nearby.")
                .text_size(12.0)
                .line_break_mode(LineBreaking::WordWrap)
        }),
        panel.busy.then(|| label("Working…").text_size(12.0)),
        xilem::view::sized_box(
            crate::touch_portal(flex_col(rows).gap(10.0.px())).constrain_horizontal(true),
        )
        .height(180.0.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(12.0.px())
}
