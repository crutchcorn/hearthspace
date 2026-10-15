//! Short-lived feedback surfaces. The compositor keeps them noninteractive.
use super::{
    control_state::Device,
    icons::{Icon, glyph},
    popup,
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_common::{CONTROL_OSD_APP_ID, control_osd_app_id_for_output};
use hearthspace_ui_components as components;
use std::time::{Duration, Instant};
use xilem::{
    WidgetView, WindowView,
    core::{MessageProxy, NoElement, View},
    dpi::LogicalSize,
    masonry::{imaging::Painter, kurbo::Rect, layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, canvas, flex_col, flex_row, label, sized_box, task_raw},
};

const LIFETIME: Duration = Duration::from_millis(1600);

#[derive(Clone, Debug)]
pub(super) struct Osd {
    pub device: Device,
    pub value: u8,
    pub muted: bool,
    pub error: Option<String>,
    pub expires: Instant,
}

pub(super) fn show(state: &mut ShellState, key: &str, device: Device, error: Option<String>) {
    let value = match device {
        Device::Brightness => state
            .controls
            .snapshot
            .brightness
            .as_ref()
            .ok()
            .and_then(|reading| reading.as_ref())
            .map_or(0, |reading| reading.percentage),
        Device::Volume => state
            .controls
            .snapshot
            .audio
            .as_ref()
            .map_or(0, |reading| reading.volume),
    };
    let muted = device == Device::Volume
        && state
            .controls
            .snapshot
            .audio
            .as_ref()
            .is_ok_and(|reading| reading.muted);
    for output in &mut state.outputs {
        output.osd = if output.key() == key {
            Some(Osd {
                device,
                value,
                muted,
                error: error.clone(),
                expires: Instant::now() + LIFETIME,
            })
        } else {
            None
        };
    }
}

pub(super) fn window(output: &ShellOutputInstance, osd: &Osd) -> WindowView<ShellState> {
    let app_id = output
        .output_name
        .as_deref()
        .map(control_osd_app_id_for_output)
        .unwrap_or_else(|| CONTROL_OSD_APP_ID.into());
    let key = output.key().to_string();
    popup::window(
        output.osd_window_id,
        "Hearthspace On-screen Display",
        app_id,
        LogicalSize::new(240.0, if osd.error.is_some() { 110.0 } else { 76.0 }),
        panel(osd),
        move |state| {
            if let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
                output.osd = None;
            }
        },
    )
}

pub(super) fn panel(osd: &Osd) -> impl WidgetView<ShellState> + use<> {
    let (name, icon) = match osd.device {
        Device::Brightness => ("Brightness", Icon::Brightness),
        Device::Volume => ("Volume", Icon::Volume { muted: osd.muted }),
    };
    let text = if osd.error.is_some() {
        format!("{name} unavailable")
    } else if osd.muted {
        "Volume muted".into()
    } else {
        format!("{name} {}%", osd.value)
    };
    sized_box(
        flex_col((
            flex_row((glyph(icon, name.into()), label(text).text_size(15.0))).gap(10.0.px()),
            (osd.error.is_none()).then(|| {
                let fraction = if osd.muted {
                    0.0
                } else {
                    f64::from(osd.value.min(100)) / 100.0
                };
                sized_box(canvas(move |_: &mut ShellState, _, scene, size| {
                    let mut painter = Painter::new(scene);
                    painter
                        .fill(
                            Rect::new(0.0, 0.0, size.width, size.height).to_rounded_rect(4.0),
                            components::BORDER,
                        )
                        .draw();
                    if fraction > 0.0 {
                        painter
                            .fill(
                                Rect::new(0.0, 0.0, size.width * fraction, size.height)
                                    .to_rounded_rect(4.0),
                                components::ACCENT,
                            )
                            .draw();
                    }
                }))
                .height(8.0.px())
            }),
            osd.error.clone().map(|error| {
                label(error)
                    .text_size(12.0)
                    .color(components::TEXT_MUTED)
                    .line_break_mode(LineBreaking::WordWrap)
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(10.0.px()),
    )
    .padding(16.0.px())
    .background_color(components::SURFACE)
    .border(components::BORDER, 1.0.px())
    .corner_radius(12.0.px())
}

pub(super) fn expire(state: &mut ShellState, now: Instant) {
    for output in &mut state.outputs {
        if output.osd.as_ref().is_some_and(|osd| now >= osd.expires) {
            output.osd = None;
        }
    }
}

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Instant>, _: &mut ShellState| async move {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if proxy.message(Instant::now()).is_err() {
                    break;
                }
            }
        },
        expire,
    )
}
