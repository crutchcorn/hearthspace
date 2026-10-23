//! A maximized window occupies a fixed world rectangle, independent of later panning.
use crate::{compositor::App, config::TITLE_BAR_HEIGHT};
use smithay::utils::{Logical, Rectangle};

impl App {
    pub(super) fn maximized_canvas_bounds(
        &self,
        index: usize,
        output: &str,
    ) -> Rectangle<i32, Logical> {
        let view = self
            .output_logical_rect_for_name(Some(output))
            .unwrap_or_else(|| self.primary_output_logical_rect());
        let screen = maximized_work_area(view);
        let chrome = if self.has_compositor_chrome(index) {
            TITLE_BAR_HEIGHT
        } else {
            0
        };
        Rectangle::new(
            crate::geometry::screen_to_canvas(
                screen.loc.to_f64(),
                self.viewport_offset,
                self.viewport_scale,
            )
            .to_i32_round(),
            (
                (f64::from(screen.size.w) / self.viewport_scale)
                    .round()
                    .max(1.) as i32,
                ((f64::from(screen.size.h) / self.viewport_scale).round() as i32).max(chrome + 1),
            )
                .into(),
        )
    }

    pub(in crate::compositor) fn canvas_maximized_rect(
        &self,
        index: usize,
    ) -> Option<Rectangle<i32, Logical>> {
        let state = &self.windows[index].presentation;
        (state.maximized && !state.output_pinned())
            .then_some(state.maximized_bounds)
            .flatten()
    }
}

fn maximized_work_area(output: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let (side, vertical) = if output.size.w <= 760 {
        (12, 78)
    } else {
        (24, 96)
    };
    let side = side.min((output.size.w - 1).max(0) / 2);
    let vertical = vertical.min((output.size.h - 1).max(0) / 2);
    Rectangle::new(
        (output.loc.x + side, output.loc.y + vertical).into(),
        (
            (output.size.w - side * 2).max(1),
            (output.size.h - vertical * 2).max(1),
        )
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_clearances_follow_output_width_and_origin() {
        assert_eq!(
            maximized_work_area(Rectangle::new((0, 0).into(), (1280, 800).into())),
            Rectangle::new((24, 96).into(), (1232, 608).into())
        );
        assert_eq!(
            maximized_work_area(Rectangle::new((-760, 100).into(), (760, 600).into())),
            Rectangle::new((-748, 178).into(), (736, 444).into())
        );
        assert_eq!(
            maximized_work_area(Rectangle::new((0, 0).into(), (1, 1).into())).size,
            (1, 1).into()
        );
    }
}
