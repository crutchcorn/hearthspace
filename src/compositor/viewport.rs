use std::time::Instant;

use smithay::utils::{Logical, Point};

use crate::{
    config::VIEWPORT_ANIMATION_DURATION,
    geometry::{
        CanvasPoint, canvas_to_screen as transform_canvas_to_screen, ease_out_cubic,
        interpolate_canvas_point, interpolate_f64, screen_to_canvas as transform_screen_to_canvas,
        zoom_around_screen_point,
    },
};

use super::App;

pub(super) struct ViewportAnimation {
    pub(super) from_offset: CanvasPoint,
    pub(super) from_scale: f64,
    pub(super) to_offset: CanvasPoint,
    pub(super) to_scale: f64,
    pub(super) started_at: Instant,
}

impl App {
    pub(super) fn start_viewport_animation(&mut self, to_offset: CanvasPoint, to_scale: f64) {
        if self.viewport_offset == to_offset
            && (self.viewport_scale - to_scale).abs() < f64::EPSILON
        {
            self.viewport_animation = None;
            return;
        }

        self.viewport_animation = Some(ViewportAnimation {
            from_offset: self.viewport_offset,
            from_scale: self.viewport_scale,
            to_offset,
            to_scale,
            started_at: Instant::now(),
        });
        self.request_redraw();
    }

    pub(super) fn advance_viewport_animation(&mut self) {
        let Some(animation) = &self.viewport_animation else {
            return;
        };

        let progress = (animation.started_at.elapsed().as_secs_f64()
            / VIEWPORT_ANIMATION_DURATION.as_secs_f64())
        .clamp(0.0, 1.0);
        let eased = ease_out_cubic(progress);

        self.viewport_offset =
            interpolate_canvas_point(animation.from_offset, animation.to_offset, eased);
        self.viewport_scale = interpolate_f64(animation.from_scale, animation.to_scale, eased);

        if progress >= 1.0 {
            let animation = self.viewport_animation.take().unwrap();
            self.viewport_offset = animation.to_offset;
            self.viewport_scale = animation.to_scale;
        } else {
            self.request_redraw();
        }
    }

    pub(super) fn pan_viewport_by(&mut self, x: i32, y: i32) {
        self.start_viewport_animation(
            CanvasPoint {
                x: self.viewport_offset.x + x,
                y: self.viewport_offset.y + y,
            },
            self.viewport_scale,
        );
    }

    pub(super) fn horizontal_pan_step(&self) -> i32 {
        (f64::from(self.output_size().w) / 2.0 / self.viewport_scale).round() as i32
    }

    pub(super) fn vertical_pan_step(&self) -> i32 {
        (f64::from(self.output_size().h) / 2.0 / self.viewport_scale).round() as i32
    }

    pub(super) fn canvas_to_screen(&self, point: Point<f64, Logical>) -> Point<f64, Logical> {
        transform_canvas_to_screen(point, self.viewport_offset, self.viewport_scale)
    }

    pub(super) fn screen_to_canvas(&self, point: Point<f64, Logical>) -> Point<f64, Logical> {
        transform_screen_to_canvas(point, self.viewport_offset, self.viewport_scale)
    }

    pub(super) fn animate_zoom_around_viewport_center(&mut self, multiplier: f64) {
        let center_screen = Point::<f64, Logical>::from((
            f64::from(self.output_size().w) / 2.0,
            f64::from(self.output_size().h) / 2.0,
        ));
        let (viewport_offset, viewport_scale) = zoom_around_screen_point(
            self.viewport_offset,
            self.viewport_scale,
            center_screen,
            multiplier,
        );
        self.start_viewport_animation(viewport_offset, viewport_scale);
    }
}
