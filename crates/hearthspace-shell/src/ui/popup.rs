//! Shared transparent popup surfaces and measured, bounded content heights.
use hearthspace_ui_components as components;
use xilem::{
    Color, WidgetView, WindowId, WindowOptionsExtLinux, WindowView,
    dpi::LogicalSize,
    masonry::{kurbo::Size, layout::AsUnit},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, portal, resize_observer, sized_box},
};

use super::state::{ShellOutputInstance, ShellState};

const PADDING: f64 = 12.0;
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
    fn group_label(self) -> &'static str {
        match self {
            Self::Search => "Search results",
            Self::Map => "Canvas map controls",
            Self::Notifications => "Notifications",
            Self::Power => "Session controls",
            Self::Tray => "Application tray menu",
            Self::Controls => "Brightness and volume controls",
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
            body + measured.header.map_or(0.0, |header| header + HEADER_GAP)
                + 2.0 * (PADDING + BORDER)
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

fn frame<V: WidgetView<ShellState>>(content: V) -> impl WidgetView<ShellState> + use<V> {
    sized_box(content)
        .padding(PADDING.px())
        .background_color(components::SURFACE)
        .border(components::BORDER, BORDER.px())
        .corner_radius(10.0.px())
}

pub(super) fn panel<V: WidgetView<ShellState>>(
    output: &ShellOutputInstance,
    kind: PopupKind,
    content: V,
) -> impl WidgetView<ShellState> + use<V> {
    // A non-filling portal measures natural content height independently of
    // its current window height, so short content can shrink after a long list.
    frame(
        portal(super::popup_group::group(
            kind.group_label(),
            observed(output, kind, false, content),
        ))
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
        flex_col((
            observed(output, kind, true, header),
            portal(super::popup_group::group(
                kind.group_label(),
                observed(output, kind, false, content),
            ))
            .constrain_horizontal(true)
            .must_fill(false)
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(HEADER_GAP.px()),
    )
}

#[cfg(test)]
mod tests;
