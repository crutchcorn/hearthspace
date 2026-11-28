//! Tests choose controls by capability and send the actual instance identity.
use crate::ui::state::ShellState;
use hearthspace_common::frame_layout::{FrameLayout, LayoutTarget};

pub(in crate::ui) fn id(state: &ShellState, output: &str, kind: &str) -> String {
    let layout = &state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find(|item| item.output == output)
        .expect("fixture output")
        .layout;
    layout.widget_id(kind).expect("fixture widget")
}

pub(in crate::ui) fn target(state: &ShellState, output: &str, kind: &str) -> LayoutTarget {
    LayoutTarget::widget(id(state, output, kind))
}

pub(in crate::ui) fn key(state: &ShellState, output: &str, kind: &str) -> String {
    super::widget_key(output, &id(state, output, kind))
}

pub(in crate::ui) fn layout_target(layout: &FrameLayout, kind: &str) -> LayoutTarget {
    LayoutTarget::widget(layout.widget_id(kind).expect("fixture widget"))
}
