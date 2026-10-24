//! One immutable displayed pose supplies rendering and inverse input transforms.
use smithay::utils::{Logical, Point, Rectangle};

use crate::{
    compositor::{App, ManagedWindowKind},
    config::TITLE_BAR_HEIGHT,
};
use hearthspace_common::canvas_search::Rect;

#[derive(Clone, Copy, Debug)]
pub(in crate::compositor) struct Projection {
    pub origin: Point<f64, Logical>,
    pub bounds: Rectangle<i32, Logical>,
    pub scale: f64,
    pub opacity: f32,
    pub preview: Option<Rect>,
}

impl Projection {
    pub fn screen_to_surface(self, screen: Point<f64, Logical>) -> Point<f64, Logical> {
        (
            (screen.x - self.origin.x) / self.scale,
            (screen.y - self.origin.y) / self.scale,
        )
            .into()
    }

    pub fn surface_to_screen(self, surface: Point<f64, Logical>) -> Point<f64, Logical> {
        (
            self.origin.x + surface.x * self.scale,
            self.origin.y + surface.y * self.scale,
        )
            .into()
    }

    fn with_preview(self, pose: Rect, opacity: f64, result: bool) -> Self {
        let factor = pose.width / f64::from(self.bounds.size.w.max(1));
        Self {
            origin: (
                pose.x + (self.origin.x - f64::from(self.bounds.loc.x)) * factor,
                pose.y + (self.origin.y - f64::from(self.bounds.loc.y)) * factor,
            )
                .into(),
            bounds: rounded_bounds(pose),
            scale: if result {
                pose.width / 240.
            } else {
                self.scale * factor
            },
            opacity: opacity as f32,
            preview: Some(pose),
        }
    }
}

fn rounded_bounds(rect: Rect) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (rect.x.round() as i32, rect.y.round() as i32).into(),
        (rect.width.round() as i32, rect.height.round() as i32).into(),
    )
}

impl App {
    pub(in crate::compositor) fn window_projection(&self, index: usize) -> Projection {
        let base = self.base_window_projection(index);
        self.canvas_preview_pose(index)
            .map_or(base, |(pose, opacity)| {
                base.with_preview(
                    pose,
                    opacity,
                    self.canvas_search_result_identity(index).is_some(),
                )
            })
    }

    pub(super) fn base_window_projection(&self, index: usize) -> Projection {
        #[cfg(feature = "xwayland")]
        if let Some((parent, offset)) = self.x11_popup_anchor(index) {
            let parent = self.window_projection(parent);
            let origin = parent.surface_to_screen(offset.to_f64());
            let inset = self.window_surface_geometry_loc(index);
            let size = self.window_geometry_size(index);
            return Projection {
                origin,
                bounds: Rectangle::new(
                    (origin + inset.to_f64().upscale(parent.scale)).to_i32_round(),
                    size.to_f64().upscale(parent.scale).to_i32_round(),
                ),
                scale: parent.scale,
                opacity: 1.,
                preview: None,
            };
        }
        let window = &self.windows[index];
        let inset = self.window_surface_geometry_loc(index);
        let chrome = if self.has_compositor_chrome(index) {
            TITLE_BAR_HEIGHT
        } else {
            0
        };
        let output_coordinates = self.window_uses_output_coordinates(index);
        let mut scale = match window.kind {
            ManagedWindowKind::Normal if output_coordinates => 1.,
            ManagedWindowKind::Normal | ManagedWindowKind::DesktopItem => self.viewport_scale,
            _ => 1.,
        };
        if !window.presentation.pinned() {
            scale = self.layout_render_scale(index).unwrap_or(scale);
            let target = self.layout_target_rect(index);
            let motion = self.layout_motion_rect(index);
            if let (Some(pose), Some(target)) = (motion, target) {
                scale *= pose.width / target.width.max(1.);
            }
            if let Some(rect) = motion.or(target) {
                let offset = self.canvas_peek_offset(index);
                return Projection {
                    origin: Point::from((
                        rect.x - f64::from(inset.x) * scale,
                        rect.y + f64::from(chrome - inset.y) * scale,
                    )) + offset,
                    bounds: rounded_bounds(Rect {
                        x: rect.x + offset.x,
                        y: rect.y + offset.y,
                        ..rect
                    }),
                    scale,
                    opacity: 1.,
                    preview: None,
                };
            }
        }
        let origin = if let Some(origin) = self.transient_screen_origin(index) {
            origin
        } else if let Some(output) = self.presentation_output_rect(index) {
            let size = self.window_geometry_size(index);
            let center = if window.presentation.fullscreen {
                (
                    f64::from((output.size.w - size.w).max(0)) / 2.,
                    f64::from((output.size.h - size.h).max(0)) / 2.,
                )
            } else {
                (0., 0.)
            };
            (
                f64::from(output.loc.x - inset.x) + center.0,
                f64::from(output.loc.y + chrome - inset.y) + center.1,
            )
                .into()
        } else {
            self.canvas_to_screen(self.content_canvas_origin(index).to_f64())
                + self.canvas_peek_offset(index)
        };
        let bounds = if output_coordinates {
            let size = self.window_geometry_size(index);
            Rectangle::new(
                origin.to_i32_round() + inset - Point::from((0, chrome)),
                (size.w, size.h + chrome).into(),
            )
        } else {
            let canvas = self.window_canvas_rect(index);
            Rectangle::new(
                (self.canvas_to_screen(canvas.loc.to_f64()) + self.canvas_peek_offset(index))
                    .to_i32_round(),
                canvas
                    .size
                    .to_f64()
                    .upscale(self.viewport_scale)
                    .to_i32_round(),
            )
        };
        Projection {
            origin,
            bounds,
            scale,
            opacity: 1.,
            preview: None,
        }
    }
}

#[cfg(test)]
mod tests;
