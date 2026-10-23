//! Detached trays scroll inside an output-bounded native surface.
use super::{Output, focus, widgets};
use crate::ui::state::{ShellOutputInstance, ShellState};
use hearthspace_common::frame_layout::{
    FrameEdge, LayoutTarget, ObjectPlacement, Rect, frame_bounds,
};
use xilem::{AnyWidgetView, WidgetView};

pub(super) fn size(output: &Output, vertical: bool, desired: (f64, f64)) -> (f64, f64) {
    if output.rect.width <= 0.0 || output.rect.height <= 0.0 {
        return desired;
    }
    let bounds = frame_bounds(
        ObjectPlacement::frame(
            if vertical {
                FrameEdge::Left
            } else {
                FrameEdge::Top
            },
            0.5,
        ),
        desired.0,
        desired.1,
        Rect {
            width: output.rect.width,
            height: output.rect.height,
            ..Default::default()
        },
        true,
    );
    (bounds.width.max(1.0), bounds.height.max(1.0))
}

pub(super) fn content(
    state: &ShellState,
    output: &ShellOutputInstance,
    vertical: bool,
    chain: &focus::Chain,
) -> Box<AnyWidgetView<ShellState>> {
    let layout = state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find(|layout| layout.output == output.key());
    let grouped = layout.is_some_and(|layout| {
        layout
            .layout
            .group_for(&LayoutTarget::widget("tray"))
            .is_some()
            || layout
                .widgets
                .iter()
                .any(|part| part.id == "tray" && part.merged)
    });
    let desired = widgets::size(
        state,
        "tray",
        vertical,
        false,
        grouped,
        Default::default(),
        false,
    );
    let bounded = layout.map_or(desired, |layout| size(layout, vertical, desired));
    let overflow = !grouped
        && if vertical {
            desired.1 > bounded.1
        } else {
            desired.0 > bounded.0
        };
    let strip = crate::ui::tray_view::frame_strip(output, &state.tray, vertical, chain, overflow);
    if grouped {
        strip.boxed()
    } else {
        hearthspace_ui_components::touch_portal(strip)
            .constrain_horizontal(vertical)
            .constrain_vertical(!vertical)
            .must_fill(true)
            .boxed()
    }
}
