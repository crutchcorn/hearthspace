//! Inspector extent shared by native content and compositor glass during resize.
use super::{EXPANDABLE_WIDGETS, LayoutTarget, TargetKind};

pub fn editor_height(
    target: Option<&LayoutTarget>,
    grouped: bool,
    widget_kind: Option<&str>,
) -> f64 {
    match target {
        None => 54.0,
        Some(target) if target.kind == TargetKind::Widget && widget_kind == Some("clock") => 452.0,
        Some(target)
            if grouped
                || (target.kind == TargetKind::Widget
                    && widget_kind.is_some_and(|kind| EXPANDABLE_WIDGETS.contains(&kind))) =>
        {
            156.0
        }
        Some(_) => 103.0,
    }
}
