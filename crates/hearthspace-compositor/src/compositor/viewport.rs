use std::time::Instant;

use smithay::{
    backend::input::InputTime,
    utils::{Logical, Point},
};
use tracing::{debug, trace};

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

impl ViewportAnimation {
    fn sample(&self, now: Instant) -> (CanvasPoint, f64, bool) {
        let progress = (now.saturating_duration_since(self.started_at).as_secs_f64()
            / VIEWPORT_ANIMATION_DURATION.as_secs_f64())
        .clamp(0.0, 1.0);
        let eased = ease_out_cubic(progress);
        (
            interpolate_canvas_point(self.from_offset, self.to_offset, eased),
            interpolate_f64(self.from_scale, self.to_scale, eased),
            progress >= 1.0,
        )
    }
}

impl App {
    pub(super) fn start_viewport_animation(&mut self, to_offset: CanvasPoint, to_scale: f64) {
        // Explicit shell/scroll navigation supersedes an in-progress pinch.
        self.interrupt_pinch_zoom();
        self.start_owned_viewport_animation(to_offset, to_scale);
    }

    /// Animate within an interaction that already owns navigation, such as a
    /// held window switcher. Retarget from the currently displayed position,
    /// without cancelling the gesture that requested the new destination.
    pub(super) fn start_owned_viewport_animation(&mut self, to_offset: CanvasPoint, to_scale: f64) {
        self.advance_viewport_animation();
        if self.viewport_offset == to_offset
            && (self.viewport_scale - to_scale).abs() < f64::EPSILON
        {
            trace!(?to_offset, to_scale, "skipping no-op viewport animation");
            self.viewport_animation = None;
            return;
        }

        debug!(
            from_offset = ?self.viewport_offset,
            from_scale = self.viewport_scale,
            ?to_offset,
            to_scale,
            "starting viewport animation"
        );
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

        let previous = (self.viewport_offset, self.viewport_scale);
        let (offset, scale, finished) = animation.sample(Instant::now());
        self.viewport_offset = offset;
        self.viewport_scale = scale;

        if finished {
            let animation = self.viewport_animation.take().unwrap();
            self.viewport_offset = animation.to_offset;
            self.viewport_scale = animation.to_scale;
            debug!(offset = ?self.viewport_offset, scale = self.viewport_scale, "viewport animation completed");
        } else {
            self.request_redraw();
        }
        if previous != (self.viewport_offset, self.viewport_scale) {
            // A stationary pointer must track the surface moving beneath it,
            // including after Alt is released before the final animation frame.
            let raw = self.raw_pointer_location;
            self.apply_pointer_motion(self.pointer_location, InputTime::now());
            self.raw_pointer_location = raw;
            self.request_redraw();
        }
    }

    pub(super) fn pan_viewport_by(&mut self, x: i32, y: i32) {
        debug!(x, y, "panning viewport");
        self.start_viewport_animation(
            CanvasPoint {
                x: self.viewport_offset.x + x,
                y: self.viewport_offset.y + y,
            },
            self.viewport_scale,
        );
    }

    pub(super) fn horizontal_pan_step(&self) -> i32 {
        (f64::from(self.primary_output_logical_rect().size.w) / 2.0 / self.viewport_scale).round()
            as i32
    }

    pub(super) fn vertical_pan_step(&self) -> i32 {
        (f64::from(self.primary_output_logical_rect().size.h) / 2.0 / self.viewport_scale).round()
            as i32
    }

    pub(super) fn canvas_to_screen(&self, point: Point<f64, Logical>) -> Point<f64, Logical> {
        transform_canvas_to_screen(point, self.viewport_offset, self.viewport_scale)
    }

    pub(super) fn screen_to_canvas(&self, point: Point<f64, Logical>) -> Point<f64, Logical> {
        transform_screen_to_canvas(point, self.viewport_offset, self.viewport_scale)
    }

    pub(super) fn animate_zoom_around_viewport_center(&mut self, multiplier: f64) {
        debug!(multiplier, "zooming viewport around center");
        let primary_output = self.primary_output_logical_rect();
        let center_screen = Point::<f64, Logical>::from((
            f64::from(primary_output.loc.x) + f64::from(primary_output.size.w) / 2.0,
            f64::from(primary_output.loc.y) + f64::from(primary_output.size.h) / 2.0,
        ));
        let (_, target_scale) = zoom_around_screen_point(
            self.viewport_offset,
            self.viewport_scale,
            center_screen,
            multiplier,
        );
        if target_scale != self.viewport_scale {
            self.restore_maximized_for_canvas_zoom();
        }
        let (viewport_offset, viewport_scale) = zoom_around_screen_point(
            self.viewport_offset,
            self.viewport_scale,
            center_screen,
            multiplier,
        );
        self.start_viewport_animation(viewport_offset, viewport_scale);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{CanvasPoint, VIEWPORT_ANIMATION_DURATION, ViewportAnimation};

    #[test]
    fn quick_pan_preserves_scale_and_reaches_its_exact_target() {
        let started_at = Instant::now();
        let animation = ViewportAnimation {
            from_offset: CanvasPoint { x: -100, y: 200 },
            to_offset: CanvasPoint { x: 1500, y: -600 },
            from_scale: 0.75,
            to_scale: 0.75,
            started_at,
        };
        assert_eq!(
            animation.sample(started_at),
            (animation.from_offset, 0.75, false)
        );
        let (halfway, scale, finished) =
            animation.sample(started_at + VIEWPORT_ANIMATION_DURATION / 2);
        assert_eq!(halfway, CanvasPoint { x: 1300, y: -500 });
        assert_eq!(scale, 0.75);
        assert!(!finished);
        assert_eq!(
            animation.sample(started_at + VIEWPORT_ANIMATION_DURATION),
            (animation.to_offset, 0.75, true)
        );
    }

    #[test]
    fn rapid_retarget_starts_at_the_interpolated_position() {
        let started_at = Instant::now();
        let first = ViewportAnimation {
            from_offset: CanvasPoint { x: 0, y: 0 },
            to_offset: CanvasPoint { x: 2000, y: 1000 },
            from_scale: 1.0,
            to_scale: 1.0,
            started_at,
        };
        let now = started_at + Duration::from_millis(45);
        let (offset, scale, _) = first.sample(now);
        let next = ViewportAnimation {
            from_offset: offset,
            from_scale: scale,
            to_offset: CanvasPoint { x: -1000, y: 500 },
            to_scale: scale,
            started_at: now,
        };
        assert_eq!(next.sample(now), (offset, scale, false));
        assert_ne!(offset, first.from_offset);
        assert_ne!(offset, first.to_offset);
        assert_eq!(
            next.sample(now + VIEWPORT_ANIMATION_DURATION),
            (next.to_offset, scale, true)
        );
    }
}
