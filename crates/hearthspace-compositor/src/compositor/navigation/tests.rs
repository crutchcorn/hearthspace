use super::*;

#[test]
fn cycling_reaches_every_window_and_wraps_in_both_directions() {
    let ids = [4, 11, 18];
    assert_eq!(cycle_target(&ids, Some(4), false), Some(11));
    assert_eq!(cycle_target(&ids, Some(11), false), Some(18));
    assert_eq!(cycle_target(&ids, Some(18), false), Some(4));
    assert_eq!(cycle_target(&ids, Some(4), true), Some(18));
    assert_eq!(cycle_target(&ids, Some(18), true), Some(11));
    assert_eq!(cycle_target(&ids, None, false), Some(4));
    assert_eq!(cycle_target(&ids, Some(99), true), Some(18));
    assert_eq!(cycle_target(&[], None, false), None);
    assert_eq!(cycle_target(&[4], Some(4), true), Some(4));
}

#[test]
fn off_canvas_window_is_centered_on_a_nonzero_output_at_current_zoom() {
    let canvas = Rectangle::new((4000, -2000).into(), (600, 400).into());
    let output = Rectangle::new((1280, 80).into(), (1280, 720).into());
    let offset = reveal_offset(canvas, output, 0.5);
    let screen = crate::geometry::canvas_to_screen(canvas.loc.to_f64(), offset, 0.5);
    assert_eq!(screen, Point::from((1770.0, 340.0)));
}

#[test]
fn oversized_window_keeps_its_top_left_controls_reachable() {
    let canvas = Rectangle::new((-3000, 5000).into(), (2000, 1500).into());
    let output = usable_output(Rectangle::from_size((1280, 800).into()));
    let offset = reveal_offset(canvas, output, 2.0);
    let screen = crate::geometry::canvas_to_screen(canvas.loc.to_f64(), offset, 2.0);
    assert_eq!(screen, output.loc.to_f64());
}
