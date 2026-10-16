//! Rasterize only the visible lasso bounds; cap memory on very large outputs.
use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Logical, Point, Rectangle, Size, Transform},
};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke};

pub(super) struct LassoBuffer {
    points: Vec<Point<f64, Logical>>,
    clip: Rectangle<i32, Logical>,
    pub(super) rect: Rectangle<i32, Logical>,
    pub(super) pixel_size: Size<i32, Logical>,
    pub(super) buffer: MemoryRenderBuffer,
}

impl LassoBuffer {
    pub(super) fn matches(
        &self,
        points: &[Point<f64, Logical>],
        clip: Rectangle<i32, Logical>,
    ) -> bool {
        self.points == points && self.clip == clip
    }

    pub(super) fn new(
        points: &[Point<f64, Logical>],
        clip: Rectangle<i32, Logical>,
    ) -> Option<Self> {
        let first = points.first()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for point in points {
            x0 = x0.min(point.x);
            y0 = y0.min(point.y);
            x1 = x1.max(point.x);
            y1 = y1.max(point.y);
        }
        let bounds = Rectangle::new(
            ((x0.floor() - 2.0) as i32, (y0.floor() - 2.0) as i32).into(),
            (
                (x1.ceil() - x0.floor() + 4.0) as i32,
                (y1.ceil() - y0.floor() + 4.0) as i32,
            )
                .into(),
        );
        let rect = bounds.intersection(clip)?;
        let scale = (4_000_000.0 / (f64::from(rect.size.w) * f64::from(rect.size.h)))
            .sqrt()
            .min(4096.0 / f64::from(rect.size.w))
            .min(4096.0 / f64::from(rect.size.h))
            .min(1.0);
        let pixel_size = Size::from((
            (f64::from(rect.size.w) * scale).ceil() as i32,
            (f64::from(rect.size.h) * scale).ceil() as i32,
        ));
        let pixmap = rasterize(points, rect, pixel_size)?;
        Some(Self {
            points: points.to_vec(),
            clip,
            rect,
            pixel_size,
            buffer: MemoryRenderBuffer::from_slice(
                pixmap.data(),
                Fourcc::Abgr8888,
                (pixel_size.w, pixel_size.h),
                1,
                Transform::Normal,
                None,
            ),
        })
    }
}

fn rasterize(
    points: &[Point<f64, Logical>],
    rect: Rectangle<i32, Logical>,
    size: Size<i32, Logical>,
) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(size.w as u32, size.h as u32)?;
    let sx = size.w as f32 / rect.size.w as f32;
    let sy = size.h as f32 / rect.size.h as f32;
    let mut builder = PathBuilder::new();
    for (index, point) in points.iter().enumerate() {
        let x = ((point.x - f64::from(rect.loc.x)) * f64::from(sx)) as f32;
        let y = ((point.y - f64::from(rect.loc.y)) * f64::from(sy)) as f32;
        if index == 0 {
            builder.move_to(x, y);
        } else {
            builder.line_to(x, y);
        }
    }
    if points.len() >= 3 {
        builder.close();
    }
    let path = builder.finish()?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(117, 171, 255, 27);
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::EvenOdd,
        tiny_skia::Transform::identity(),
        None,
    );
    paint.set_color_rgba8(137, 188, 255, 245);
    pixmap.stroke_path(
        &path,
        &paint,
        &Stroke {
            width: 1.8 * sx.min(sy),
            line_join: tiny_skia::LineJoin::Round,
            line_cap: tiny_skia::LineCap::Round,
            ..Stroke::default()
        },
        tiny_skia::Transform::identity(),
        None,
    );
    Some(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concave_fill_leaves_notch_clear_and_draws_translucent_interior() {
        let points: Vec<_> = [
            (10., 10.),
            (90., 10.),
            (90., 30.),
            (30., 30.),
            (30., 90.),
            (10., 90.),
        ]
        .into_iter()
        .map(Point::from)
        .collect();
        let pixels = rasterize(
            &points,
            Rectangle::from_size((100, 100).into()),
            (100, 100).into(),
        )
        .unwrap();
        assert_eq!(pixels.pixel(60, 60).unwrap().alpha(), 0);
        assert!((1..50).contains(&pixels.pixel(20, 60).unwrap().alpha()));
        assert!(pixels.pixel(10, 60).unwrap().alpha() > 100);
    }

    #[test]
    fn lasso_buffers_clip_and_bound_memory_for_huge_outputs() {
        let points = [
            (-1_000_000., -1_000_000.).into(),
            (1_000_000., -1_000_000.).into(),
            (1_000_000., 1_000_000.).into(),
        ];
        let clip = Rectangle::new((-100, 40).into(), (10000, 10000).into());
        let buffer = LassoBuffer::new(&points, clip).unwrap();
        assert_eq!(buffer.rect, clip);
        assert!(i64::from(buffer.pixel_size.w) * i64::from(buffer.pixel_size.h) <= 4_004_001);
    }
}
