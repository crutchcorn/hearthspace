use super::*;

#[test]
fn fractional_pan_uses_the_original_anchor_without_accumulating_rounding() {
    let drag = Drag {
        pointer: (640., 400.).into(),
        offset: CanvasPoint { x: 5760, y: 3600 },
        scale: 2.,
    };
    for step in 1..=100 {
        let point = (640. + f64::from(step) * 0.2, 400.).into();
        let result = drag.offset_at(point);
        assert_eq!(result.x, (5760. - f64::from(step) * 0.1).round() as i32);
        assert_eq!(result.y, 3600);
    }
}
