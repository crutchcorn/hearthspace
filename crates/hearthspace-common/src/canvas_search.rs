//! Shared canvas-search presentation geometry; never changes stored canvas positions.
mod identity;
mod layout;
pub mod motion;

pub use identity::{
    canvas_search_app_id, canvas_search_from_app_id, canvas_search_result_app_id,
    canvas_search_result_from_app_id,
};
pub use layout::layout;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub matches: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Field {
    /// The 54px input row. The handwriting and empty state extend toward results.
    pub rect: Rect,
    pub results_above: bool,
    pub handwriting_height: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub field: Field,
    pub placements: Vec<Placement>,
}

/// Every whitespace-separated word must occur; order and case are insignificant.
pub fn matches(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| text.contains(word))
}

/// Park a nonmatch beyond its nearest viewport edge, preserving its natural size.
pub fn park(item: Rect, view: Rect) -> Placement {
    let center_x = item.x + item.width / 2.0 - view.x - view.width / 2.0;
    let center_y = item.y + item.height / 2.0 - view.y - view.height / 2.0;
    let horizontal =
        (center_x / view.width.max(1.0)).abs() >= (center_y / view.height.max(1.0)).abs();
    Placement {
        x: if horizontal {
            if center_x < 0.0 {
                view.x - item.width - 48.0
            } else {
                view.x + view.width + 48.0
            }
        } else {
            item.x
        },
        y: if !horizontal {
            if center_y < 0.0 {
                view.y - item.height - 48.0
            } else {
                view.y + view.height + 48.0
            }
        } else {
            item.y
        },
        scale: 1.0,
        matches: false,
    }
}

#[cfg(test)]
mod tests;
