//! Optional system battery presentation; machines without a battery use no space.
use crate::battery::BatteryStatus;
use hearthspace_ui_components as components;
use xilem::{
    WidgetView,
    core::{MessageProxy, NoElement, View},
    masonry::layout::AsUnit,
    style::Style,
    view::{flex_row, label, task_raw},
};

use super::{
    icons::{self, Icon},
    state::ShellState,
};

pub(super) fn indicator(
    battery: Option<&BatteryStatus>,
    compact: bool,
) -> Option<impl WidgetView<ShellState> + use<>> {
    let battery = battery?;
    Some(
        flex_row((
            icons::glyph(
                Icon::Battery {
                    percentage: battery.percentage,
                    charging: battery.is_charging(),
                },
                if compact {
                    battery.accessible_label()
                } else {
                    format!("{}: {}", battery.kind.label(), battery.state_label())
                },
            ),
            (!compact).then(|| {
                label(format!("{}%", battery.percentage))
                    .text_size(13.0)
                    .color(components::TEXT)
            }),
        ))
        .gap(4.0.px()),
    )
}

pub(super) fn detail(
    battery: Option<&BatteryStatus>,
) -> Option<impl WidgetView<ShellState> + use<>> {
    battery.map(|battery| {
        label(battery.accessible_label())
            .text_size(13.0)
            .color(components::TEXT_MUTED)
            .line_break_mode(xilem::masonry::properties::LineBreaking::WordWrap)
    })
}

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Option<BatteryStatus>>, _: &mut ShellState| {
            let address = std::env::var("HEARTHSPACE_SESSION_BUS_ADDRESS").ok();
            async move {
                let (sender, mut snapshots) = tokio::sync::mpsc::unbounded_channel();
                let render = async move {
                    while let Some(snapshot) = snapshots.recv().await {
                        if proxy.message(snapshot).is_err() {
                            break;
                        }
                    }
                };
                tokio::select! {
                    () = crate::battery::monitor(sender, address) => {},
                    () = render => {},
                }
            }
        },
        |state: &mut ShellState, battery| state.battery = battery,
    )
}
