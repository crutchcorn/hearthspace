//! Inspector extent shared by native content and compositor glass during resize.
use super::{EXPANDABLE_WIDGETS, LayoutTarget, TargetKind};

pub fn editor_height(target: Option<&LayoutTarget>, grouped: bool) -> f64 {
    match target {
        None => 54.0,
        Some(target) if target.kind == TargetKind::Widget && target.id == "clock" => 452.0,
        Some(target)
            if grouped
                || (target.kind == TargetKind::Widget
                    && EXPANDABLE_WIDGETS.contains(&target.id.as_str())) =>
        {
            156.0
        }
        Some(_) => 103.0,
    }
}
