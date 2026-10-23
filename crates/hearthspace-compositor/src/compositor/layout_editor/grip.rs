//! A toolbar's grabbed point follows its measured row or column, including external grips.
use super::{Point, Rect};

#[derive(Clone, Copy, Debug)]
pub(super) struct ToolbarGrip {
    along: f64,
    across: f64,
}

impl ToolbarGrip {
    pub(super) fn at(bounds: Rect, anchor: Point, vertical: bool) -> Self {
        let x = (anchor.x - bounds.x) / bounds.width.max(1.);
        let y = (anchor.y - bounds.y) / bounds.height.max(1.);
        if vertical {
            Self {
                along: y,
                across: 1. - x,
            }
        } else {
            Self {
                along: x,
                across: y,
            }
        }
    }

    pub(super) fn pivot(self, width: f64, height: f64, vertical: bool) -> Point {
        if vertical {
            Point {
                x: (1. - self.across) * width,
                y: self.along * height,
            }
        } else {
            Point {
                x: self.along * width,
                y: self.across * height,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exterior_group_grip_stays_at_the_anchor_after_measured_column_reflow() {
        let row = Rect {
            x: 100.,
            y: 50.,
            width: 224.,
            height: 54.,
        };
        let anchor = Point { x: 212., y: 122. };
        let grip = ToolbarGrip::at(row, anchor, false);
        // The clock remains horizontal, so this column is not a transpose of the row.
        let pivot = grip.pivot(80., 198., true);
        assert!((pivot.x + 80. / 3.).abs() < 0.000_001);
        assert_eq!(pivot.y, 99.);
        let column = Rect {
            x: anchor.x - pivot.x,
            y: anchor.y - pivot.y,
            width: 80.,
            height: 198.,
        };
        let restored = ToolbarGrip::at(column, anchor, true).pivot(224., 54., false);
        assert!((restored.x - 112.).abs() < 0.000_001);
        assert!((restored.y - 72.).abs() < 0.000_001);
    }

    #[test]
    fn expanded_text_reflow_keeps_the_same_grab_fraction_without_clamping() {
        let grip = ToolbarGrip::at(
            Rect {
                x: 0.,
                y: 0.,
                width: 54.,
                height: 200.,
            },
            Point { x: -18., y: 50. },
            true,
        );
        let pivot = grip.pivot(110., 342., true);
        assert!((pivot.x + 110. / 3.).abs() < 0.000_001);
        assert_eq!(pivot.y, 85.5);
    }
}
