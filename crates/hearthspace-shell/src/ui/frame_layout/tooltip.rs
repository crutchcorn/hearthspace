//! Editor hints are passive surfaces; native buttons retain focus and semantics.
use std::time::{Duration, Instant};
use xilem::{
    WindowId, WindowView,
    core::{MessageProxy, NoElement, View},
    dpi::LogicalSize,
    masonry::{layout::AsUnit, properties::Padding},
    style::Style,
    view::{MainAxisAlignment, flex_row, label, resize_observer, sized_box, task_raw},
};

use super::ShellState;
#[path = "tooltip_widget.rs"]
mod widget;
pub(super) use widget::hint;

#[derive(Default)]
pub(super) struct State {
    pub label: Option<&'static str>,
    window: Option<WindowId>,
    width: Option<f64>,
    generation: u64,
    timer: Option<tokio::sync::watch::Sender<Option<(u64, Instant)>>>,
}

pub(super) fn change(state: &mut ShellState, label: &'static str, visible: bool) {
    if !state.frame_layout.snapshot.editing || state.frame_layout.snapshot.confirmation.is_some() {
        return;
    }
    let tooltip = &mut state.frame_layout.tooltip;
    if visible {
        tooltip.generation = tooltip.generation.wrapping_add(1);
        tooltip.label = Some(label);
        tooltip.window.get_or_insert_with(WindowId::next);
        tooltip.width = None;
        if let Some(timer) = &tooltip.timer {
            timer.send_replace(Some((
                tooltip.generation,
                Instant::now() + Duration::from_millis(2500),
            )));
        }
    } else if tooltip.label == Some(label) {
        clear(state);
    }
}

pub(super) fn clear(state: &mut ShellState) {
    let tooltip = &mut state.frame_layout.tooltip;
    tooltip.label = None;
    tooltip.window = None;
    tooltip.width = None;
    if let Some(timer) = &tooltip.timer {
        timer.send_replace(None);
    }
}

pub(super) fn window(state: &ShellState) -> Option<WindowView<ShellState>> {
    let layout = &state.frame_layout;
    if !layout.snapshot.editing || layout.snapshot.confirmation.is_some() {
        return None;
    }
    let text = layout.tooltip.label?;
    let width = layout
        .tooltip
        .width
        .unwrap_or(180.0)
        .min(layout.snapshot.editor_max_width.max(120.0));
    let content = resize_observer(
        move |state: &mut ShellState, size| {
            if state.frame_layout.tooltip.label == Some(text) && size.width.is_finite() {
                state.frame_layout.tooltip.width = Some((size.width + 26.0).ceil());
            }
        },
        label(text)
            .font("Liberation Sans")
            .text_size(11.0)
            .color(state.appearance.palette.text),
    );
    let content = crate::ui::canvas_search::material::tooltip(
        sized_box(flex_row((content,)).main_axis_alignment(MainAxisAlignment::Center))
            .padding(Padding {
                left: 13.0.px(),
                right: 13.0.px(),
                top: 9.0.px(),
                bottom: 9.0.px(),
            })
            .width(width.px())
            .height(32.0.px()),
        state.appearance.palette,
    );
    Some(crate::ui::popup::window(
        layout.tooltip.window?,
        "Layout hint",
        hearthspace_common::frame_layout::layout_tooltip_app_id(
            layout.snapshot.session,
            layout.snapshot.output.as_deref(),
        ),
        LogicalSize::new(width, 32.0),
        content,
        |_| {},
    ))
}

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<u64>, state: &mut ShellState| {
            let (sender, mut receiver) = tokio::sync::watch::channel(None::<(u64, Instant)>);
            state.frame_layout.tooltip.timer = Some(sender);
            async move {
                loop {
                    let pending = *receiver.borrow_and_update();
                    if let Some((generation, deadline)) = pending {
                        tokio::select! {
                            changed = receiver.changed() => if changed.is_err() { break; },
                            _ = tokio::time::sleep_until(deadline.into()) => {
                                if proxy.message(generation).is_err() { break; }
                                if receiver.changed().await.is_err() { break; }
                            }
                        }
                    } else if receiver.changed().await.is_err() {
                        break;
                    }
                }
            }
        },
        |state, generation| {
            if state.frame_layout.tooltip.generation == generation {
                clear(state);
            }
        },
    )
}
