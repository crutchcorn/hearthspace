//! Use the same output-local allocation for dock controls and tray suppression.
use super::{ShellState, widget_key};
use hearthspace_common::frame_layout::{LayoutTarget, ObjectPlacement};

#[cfg(test)]
#[path = "dock_tests.rs"]
mod tests;

pub(in crate::ui) fn menu_width(state: &ShellState, output: &str) -> Option<f64> {
    if !state.frame_layout.visible
        || state.frame_layout.snapshot.editing
        || state.frame_layout.snapshot.confirmation.is_some()
        || !state
            .frame_layout
            .windows
            .contains_key(&widget_key(output, "apps"))
    {
        return None;
    }
    width(state, output)
}

pub(in crate::ui) fn width(state: &ShellState, output: &str) -> Option<f64> {
    let layout = state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find(|layout| layout.output == output)?;
    let target = LayoutTarget::widget("apps");
    if layout.layout.placement(&target) == ObjectPlacement::Hidden {
        return None;
    }
    let part = layout.widgets.iter().find(|part| part.id == "apps")?;
    if part.rect.width <= 0.0 || part.rect.height <= 0.0 {
        return None;
    }
    let grouped = part.merged || layout.layout.group_for(&target).is_some();
    let extent = if part.orientation == "vertical" {
        part.rect.height
    } else {
        part.rect.width
    };
    let padding = if grouped { 0.0 } else { 10.0 };
    Some(
        (extent - padding - 48.0)
            .max(0.0)
            .min(crate::ui::dock::desired_width(state)),
    )
}
