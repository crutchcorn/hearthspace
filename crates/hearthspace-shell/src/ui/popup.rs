//! Shared transparent popup surfaces and measured, bounded content heights.
use crate::ui::appearance::{ThemeStyle as _, Tone};
use hearthspace_ui_components as components;
use xilem::{
    Color, WidgetView, WindowId, WindowOptionsExtLinux, WindowView,
    dpi::LogicalSize,
    masonry::{kurbo::Size, layout::AsUnit},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, resize_observer, sized_box},
};

use super::state::{ShellOutputInstance, ShellState};

mod rounded;

pub(super) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: components::Palette,
) {
    properties.insert::<rounded::RoundedFrame, _>(palette);
}

const PADDING: f64 = 20.0;
const BORDER: f64 = 1.0;
const HEADER_GAP: f64 = 8.0;

#[derive(Clone, Copy)]
pub(super) enum PopupKind {
    Search,
    Map,
    Notifications,
    Power,
    Tray,
    Controls,
}

impl PopupKind {
    pub(super) fn reset(self, output: &mut ShellOutputInstance) {
        output.popup_measurements[self.index()] = PopupMeasurement::default();
    }
    fn padding(self) -> f64 {
        if matches!(self, Self::Search | Self::Notifications) {
            0.0
        } else {
            PADDING
        }
    }
    fn header_gap(self) -> f64 {
        if matches!(self, Self::Search | Self::Notifications) {
            0.0
        } else {
            HEADER_GAP
        }
    }
    fn group_label(self) -> &'static str {
        match self {
            Self::Search => "Apps and windows",
            Self::Map => "Canvas map controls",
            Self::Notifications => "Notifications",
            Self::Power => "Session controls",
            Self::Tray => "Application tray menu",
            Self::Controls => "Status controls",
        }
    }
    fn index(self) -> usize {
        self as usize
    }
    fn height_cap(self) -> f64 {
        match self {
            Self::Search | Self::Tray => 560.0,
            Self::Notifications => 440.0,
            Self::Map | Self::Power | Self::Controls => 400.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PopupMeasurement {
    header: Option<f64>,
    body: Option<f64>,
    footer: Option<f64>,
}

pub(super) fn size(
    output: &ShellOutputInstance,
    kind: PopupKind,
    width: f64,
    fallback_height: f64,
) -> LogicalSize<f64> {
    let measured = output.popup_measurements[kind.index()];
    let height = measured
        .body
        .map(|body| {
            body + measured
                .header
                .map_or(0.0, |header| header + kind.header_gap())
                + measured.footer.unwrap_or(0.0)
                + 2.0 * (kind.padding() + BORDER)
        })
        .unwrap_or(fallback_height);
    LogicalSize::new(width, height.ceil().clamp(1.0, kind.height_cap()))
}

pub(super) fn window<V, F>(
    id: WindowId,
    title: &str,
    app_id: String,
    size: LogicalSize<f64>,
    content: V,
    on_close: F,
) -> WindowView<ShellState>
where
    V: WidgetView<ShellState>,
    F: Fn(&mut ShellState) + 'static,
{
    xilem::window(id, title, content)
        // Both are required: alpha pixels alone are hidden by an opaque base
        // color; a transparent base alone does not request an alpha surface.
        .with_base_color(Color::TRANSPARENT)
        .with_options(move |options| {
            options
                .with_name(&app_id, &app_id)
                .with_transparent(true)
                .with_decorations(false)
                .with_resizable(false)
                .with_initial_inner_size(size)
                .with_min_inner_size(size)
                .with_max_inner_size(size)
                .on_close(on_close)
        })
}

fn observed<V: WidgetView<ShellState>>(
    output: &ShellOutputInstance,
    kind: PopupKind,
    header: bool,
    content: V,
) -> impl WidgetView<ShellState> + use<V> {
    let key = output.key().to_string();
    resize_observer(
        move |state: &mut ShellState, size: Size| {
            if !size.height.is_finite() {
                return;
            }
            if let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
                let measurement = &mut output.popup_measurements[kind.index()];
                let slot = if header {
                    &mut measurement.header
                } else {
                    &mut measurement.body
                };
                *slot = Some(size.height.ceil());
            }
        },
        content,
    )
}

fn frame<V: WidgetView<ShellState>>(
    kind: PopupKind,
    content: V,
) -> impl WidgetView<ShellState> + use<V> {
    rounded::clip(
        sized_box(content)
            .padding(
                if matches!(kind, PopupKind::Map) {
                    0.0
                } else {
                    kind.padding()
                }
                .px(),
            )
            // A steady tint keeps the text legible over arbitrary client content.
            .theme_background(if matches!(kind, PopupKind::Map) {
                Tone::Transparent
            } else {
                Tone::Chrome
            })
            .theme_border(if matches!(kind, PopupKind::Map) {
                Tone::Transparent
            } else {
                Tone::ChromeBorder
            })
            .border_width(BORDER.px())
            .corner_radius(20.0.px()),
        matches!(kind, PopupKind::Map),
    )
}

pub(super) fn panel<V: WidgetView<ShellState>>(
    output: &ShellOutputInstance,
    kind: PopupKind,
    content: V,
) -> impl WidgetView<ShellState> + use<V> {
    panel_named(output, kind, kind.group_label(), content)
}

pub(super) fn panel_named<V: WidgetView<ShellState>>(
    output: &ShellOutputInstance,
    kind: PopupKind,
    name: &'static str,
    content: V,
) -> impl WidgetView<ShellState> + use<V> {
    // A non-filling portal measures natural content height independently of
    // its current window height, so short content can shrink after a long list.
    frame(
        kind,
        // Overview's map paints an exterior focus ring and glow. Keep its
        // spacing inside the scroll viewport so those sides are not clipped.
        components::touch_portal(
            sized_box(super::popup_group::group(
                name,
                observed(output, kind, false, content),
            ))
            .padding(
                if matches!(kind, PopupKind::Map) {
                    PADDING
                } else {
                    0.0
                }
                .px(),
            ),
        )
        .constrain_horizontal(true)
        .must_fill(false),
    )
}

pub(super) fn panel_with_header<H: WidgetView<ShellState>, V: WidgetView<ShellState>>(
    output: &ShellOutputInstance,
    kind: PopupKind,
    header: H,
    content: V,
) -> impl WidgetView<ShellState> + use<H, V> {
    frame(
        kind,
        flex_col((
            observed(output, kind, true, header),
            components::touch_portal(super::popup_group::group(
                kind.group_label(),
                observed(output, kind, false, content),
            ))
            .constrain_horizontal(true)
            .must_fill(false)
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(kind.header_gap().px()),
    )
}

pub(super) fn launcher_panel<
    H: WidgetView<ShellState>,
    V: WidgetView<ShellState>,
    F: WidgetView<ShellState>,
>(
    output: &ShellOutputInstance,
    header: H,
    content: V,
    footer: Option<F>,
) -> impl WidgetView<ShellState> + use<H, V, F> {
    let kind = PopupKind::Search;
    let key = output.key().to_owned();
    let footer = resize_observer(
        move |state: &mut ShellState, size: Size| {
            if let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
                output.popup_measurements[kind.index()].footer = Some(size.height.ceil());
            }
        },
        flex_col((footer,)),
    );
    frame(
        kind,
        flex_col((
            observed(output, kind, true, header),
            components::touch_portal(
                super::popup_group::group(
                    kind.group_label(),
                    observed(output, kind, false, content),
                )
                .without_accessibility_bounds(),
            )
            .constrain_horizontal(true)
            .must_fill(false)
            .flex(1.0),
            footer,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(0.0.px()),
    )
}

#[cfg(test)]
mod tests;
