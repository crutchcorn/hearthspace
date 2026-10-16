//! Small antialiased action icon, including its pending shimmer.
use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Logical, Size, Transform},
};
use tiny_skia::{Color, FillRule, GradientStop, Paint, Path, PathBuilder, Pixmap, Stroke};

pub(super) struct ActionBuffer {
    state: (Size<i32, Logical>, bool, Option<u8>),
    pub(super) buffer: MemoryRenderBuffer,
}

impl ActionBuffer {
    pub(super) fn matches(&self, size: Size<i32, Logical>, hover: bool, phase: Option<u8>) -> bool {
        self.state == (size, hover, phase)
    }

    pub(super) fn new(size: Size<i32, Logical>, hover: bool, phase: Option<u8>) -> Self {
        let pixmap = rasterize(size, hover, phase);
        Self {
            state: (size, hover, phase),
            buffer: MemoryRenderBuffer::from_slice(
                pixmap.data(),
                Fourcc::Abgr8888,
                (size.w, size.h),
                1,
                Transform::Normal,
                None,
            ),
        }
    }
}

fn rasterize(size: Size<i32, Logical>, hover: bool, phase: Option<u8>) -> Pixmap {
    let mut pixels = Pixmap::new(size.w.max(1) as u32, size.h.max(1) as u32)
        .expect("bounded positive gather button size");
    let scale = tiny_skia::Transform::from_scale(size.w as f32 / 44.0, size.h as f32 / 44.0);
    let background = rounded_rectangle(1.0, 1.0, 42.0, 42.0, 10.0);
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        if hover { 27 } else { 13 },
        if hover { 43 } else { 22 },
        if hover { 67 } else { 36 },
        250,
    );
    pixels.fill_path(&background, &paint, FillRule::Winding, scale, None);
    if let Some(phase) = phase {
        let x = f32::from(phase) / 40.0 * 90.0 - 28.0;
        paint.shader = tiny_skia::LinearGradient::new(
            tiny_skia::Point::from_xy(x, 0.0),
            tiny_skia::Point::from_xy(x + 28.0, 10.0),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(117, 171, 255, 0)),
                GradientStop::new(0.5, Color::from_rgba8(117, 171, 255, 90)),
                GradientStop::new(1.0, Color::from_rgba8(117, 171, 255, 0)),
            ],
            tiny_skia::SpreadMode::Pad,
            tiny_skia::Transform::identity(),
        )
        .expect("finite nonempty shimmer gradient");
        pixels.fill_path(&background, &paint, FillRule::Winding, scale, None);
    }
    paint.set_color_rgba8(100, 140, 193, if hover { 230 } else { 155 });
    pixels.stroke_path(
        &background,
        &paint,
        &Stroke {
            width: 1.0,
            ..Stroke::default()
        },
        scale,
        None,
    );
    let rear = rounded_rectangle(11.0, 10.0, 16.0, 16.0, 2.5);
    let front = rounded_rectangle(17.0, 17.0, 16.0, 16.0, 2.5);
    paint.set_color_rgba8(150, 192, 255, 255);
    let stroke = Stroke {
        width: 1.8,
        ..Stroke::default()
    };
    pixels.stroke_path(&rear, &paint, &stroke, scale, None);
    paint.set_color_rgba8(20, 34, 55, 255);
    pixels.fill_path(&front, &paint, FillRule::Winding, scale, None);
    paint.set_color_rgba8(173, 207, 255, 255);
    pixels.stroke_path(&front, &paint, &stroke, scale, None);
    pixels
}

fn rounded_rectangle(x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
    let mut path = PathBuilder::new();
    path.move_to(x + r, y);
    path.line_to(x + w - r, y);
    path.quad_to(x + w, y, x + w, y + r);
    path.line_to(x + w, y + h - r);
    path.quad_to(x + w, y + h, x + w - r, y + h);
    path.line_to(x + r, y + h);
    path.quad_to(x, y + h, x, y + h - r);
    path.line_to(x, y + r);
    path.quad_to(x, y, x + r, y);
    path.close();
    path.finish().expect("finite rounded rectangle")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_is_transparent_outside_round_corners_and_shimmers() {
        let normal = rasterize((44, 44).into(), false, None);
        assert_eq!(normal.pixel(0, 0).unwrap().alpha(), 0);
        assert!(normal.pixel(22, 22).unwrap().alpha() > 240);
        assert_ne!(normal, rasterize((44, 44).into(), true, None));
        assert_ne!(
            rasterize((44, 44).into(), false, Some(10)),
            rasterize((44, 44).into(), false, Some(25))
        );
        for pixel in normal.pixels() {
            assert!(
                pixel.red() <= pixel.alpha()
                    && pixel.green() <= pixel.alpha()
                    && pixel.blue() <= pixel.alpha()
            );
        }
    }
}
