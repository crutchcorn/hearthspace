//! First-map placement uses visible client geometry without moving the viewport.
use smithay::utils::{Logical, Point, Rectangle};

use super::super::App;
use super::usable_output;
use crate::geometry::CanvasPoint;

impl App {
    pub(in crate::compositor) fn initial_window_output(
        &self,
        index: usize,
    ) -> Rectangle<i32, Logical> {
        self.windows[index]
            .initial_output_name
            .as_deref()
            .and_then(|name| self.output_logical_rect_for_name(Some(name)))
            .or_else(|| {
                self.output_logical_rects()
                    .into_iter()
                    .find(|output| output.contains(self.pointer_location.to_i32_round()))
            })
            .unwrap_or_else(|| self.primary_output_logical_rect())
    }

    pub(in crate::compositor) fn center_new_window(&mut self, index: usize) {
        if self.transient_parent_index(index).is_some() || self.windows[index].presentation.pinned()
        {
            return;
        }
        let point = centered_position(
            self.window_canvas_rect(index),
            self.windows[index].position,
            usable_output(self.initial_window_output(index)),
            self.viewport_offset,
            self.viewport_scale,
        );
        #[cfg(feature = "xwayland")]
        if let super::super::ManagedSurface::X11(surface) = &self.windows[index].surface
            && let Err(error) = surface.configure(Some(Rectangle::new(
                (point.x, point.y).into(),
                surface.geometry().size,
            )))
        {
            tracing::warn!(%error, "could not center new X11 window");
            return;
        }
        self.windows[index].position = point;
        self.request_redraw();
    }
}

fn centered_position(
    geometry: Rectangle<i32, Logical>,
    position: CanvasPoint,
    output: Rectangle<i32, Logical>,
    viewport: CanvasPoint,
    scale: f64,
) -> CanvasPoint {
    // CSD shadows are outside xdg window geometry. SSD geometry already
    // includes the title bar. Align oversized windows at the usable top-left.
    let origin = Point::<f64, Logical>::from((
        f64::from(output.loc.x)
            + ((f64::from(output.size.w) - f64::from(geometry.size.w) * scale) / 2.0).max(0.0),
        f64::from(output.loc.y)
            + ((f64::from(output.size.h) - f64::from(geometry.size.h) * scale) / 2.0).max(0.0),
    ));
    CanvasPoint {
        x: (f64::from(viewport.x) + origin.x / scale).round() as i32
            - (geometry.loc.x - position.x),
        y: (f64::from(viewport.y) + origin.y / scale).round() as i32
            - (geometry.loc.y - position.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::canvas_to_screen;

    #[test]
    fn csd_window_centers_visible_geometry_in_panned_zoomed_secondary_output() {
        let output = usable_output(Rectangle::new((1280, -200).into(), (1280, 800).into()));
        let viewport = CanvasPoint { x: 2400, y: -1100 };
        let position = CanvasPoint { x: 80, y: 96 };
        let geometry = Rectangle::new((94, 108).into(), (600, 400).into());
        for scale in [0.5, 0.8, 1.0, 2.0] {
            let placed = centered_position(geometry, position, output, viewport, scale);
            let visible = canvas_to_screen(
                (f64::from(placed.x + 14), f64::from(placed.y + 12)).into(),
                viewport,
                scale,
            );
            let expected_x = f64::from(output.loc.x)
                + ((f64::from(output.size.w) - 600.0 * scale) / 2.0).max(0.0);
            let expected_y = f64::from(output.loc.y)
                + ((f64::from(output.size.h) - 400.0 * scale) / 2.0).max(0.0);
            assert!((visible.x - expected_x).abs() <= scale / 2.0);
            assert!((visible.y - expected_y).abs() <= scale / 2.0);
        }
    }

    #[test]
    fn server_titlebar_is_included_and_oversized_controls_stay_reachable() {
        let output = usable_output(Rectangle::from_size((1280, 800).into()));
        let viewport = CanvasPoint { x: -700, y: 900 };
        let position = CanvasPoint { x: 0, y: 0 };
        assert_eq!(
            centered_position(
                Rectangle::from_size((600, 430).into()),
                position,
                output,
                viewport,
                1.0,
            ),
            CanvasPoint { x: -360, y: 1109 }
        );
        assert_eq!(
            centered_position(
                Rectangle::from_size((2000, 1500).into()),
                position,
                output,
                viewport,
                1.0,
            ),
            CanvasPoint { x: -700, y: 948 }
        );
    }
}
