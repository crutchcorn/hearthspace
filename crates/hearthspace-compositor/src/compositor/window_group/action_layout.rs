//! The gather action stays beside the selected windows and inside its output.
use smithay::utils::{Logical, Rectangle};

pub(super) const ACTION_SIZE: i32 = 44;
const GAP: i64 = 12;

pub(super) fn action_rect(
    bounds: Option<Rectangle<i32, Logical>>,
    output: Rectangle<i32, Logical>,
) -> Rectangle<i32, Logical> {
    let width = ACTION_SIZE.min(output.size.w.max(1));
    let height = ACTION_SIZE.min(output.size.h.max(1));
    let left = i64::from(output.loc.x);
    let top = i64::from(output.loc.y);
    let right = left + i64::from(output.size.w.max(1));
    let bottom = top + i64::from(output.size.h.max(1));
    let (x, y) = bounds.map_or((right - i64::from(width) - GAP, top + GAP), |bounds| {
        let group_left = i64::from(bounds.loc.x);
        let group_right = group_left + i64::from(bounds.size.w);
        let right_side = group_right + GAP;
        let left_side = group_left - GAP - i64::from(width);
        let x = if right_side >= left && right_side + i64::from(width) <= right {
            right_side
        } else if left_side >= left && left_side + i64::from(width) <= right {
            left_side
        } else if right - group_right >= group_left - left {
            right - i64::from(width)
        } else {
            left
        };
        // Center on the visible portion rather than an offscreen group's midpoint.
        let visible_top = i64::from(bounds.loc.y).clamp(top, bottom);
        let visible_bottom =
            (i64::from(bounds.loc.y) + i64::from(bounds.size.h)).clamp(top, bottom);
        (x, (visible_top + visible_bottom - i64::from(height)) / 2)
    });
    Rectangle::new(
        (
            x.clamp(left, right - i64::from(width))
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            y.clamp(top, bottom - i64::from(height))
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        )
            .into(),
        (width, height).into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_prefers_right_then_left_without_covering_group() {
        let output = Rectangle::new((0, 32).into(), (1000, 700).into());
        let group = Rectangle::new((200, 150).into(), (300, 300).into());
        let right = action_rect(Some(group), output);
        assert_eq!(right.loc.x, 512);
        assert_eq!(right.loc.y, 278);
        assert!(!right.overlaps(group));
        let group = Rectangle::new((650, 150).into(), (300, 300).into());
        let left = action_rect(Some(group), output);
        assert_eq!(left.loc.x, 594);
        assert!(!left.overlaps(group));
    }

    #[test]
    fn action_is_visible_on_negative_tiny_and_partially_offscreen_outputs() {
        for output in [
            Rectangle::new((-900, 32).into(), (900, 600).into()),
            Rectangle::new((50, 60).into(), (30, 20).into()),
        ] {
            for group in [
                None,
                Some(Rectangle::new((-1300, -600).into(), (500, 400).into())),
                Some(Rectangle::new(
                    (-10000, -10000).into(),
                    (20000, 20000).into(),
                )),
                Some(Rectangle::new(
                    (i32::MAX - 50, i32::MIN + 50).into(),
                    (500, 400).into(),
                )),
            ] {
                assert!(output.contains_rect(action_rect(group, output)));
            }
        }
    }
}
