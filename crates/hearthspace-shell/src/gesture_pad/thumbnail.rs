//! A passive reminder of a favorite's saved shape.

use hearthspace_ui_components as components;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, Widget,
        },
        imaging::Painter,
        kurbo::{Axis, BezPath, Cap, Join, Point, Rect, Size, Stroke},
        layout::{AsUnit, LenReq, Length},
    },
    view::sized_box,
};

pub fn thumbnail<State: 'static>(points: Vec<[f64; 2]>) -> impl WidgetView<State> + use<State> {
    thumbnail_sized(points, 64.0, 40.0)
}

/// Fit the same saved shape into another compact preview without stretching it.
pub fn thumbnail_sized<State: 'static>(
    points: Vec<[f64; 2]>,
    width: f64,
    height: f64,
) -> impl WidgetView<State> + use<State> {
    sized_box(ThumbnailView(points))
        .fixed_width(width.max(0.0).px())
        .fixed_height(height.max(0.0).px())
}

struct ThumbnailView(Vec<[f64; 2]>);

impl ViewMarker for ThumbnailView {}

impl<State: 'static> View<State, (), ViewCtx> for ThumbnailView {
    type Element = Pod<Thumbnail>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
        (
            ctx.create_pod(Thumbnail {
                points: self.0.clone(),
                size: Size::ZERO,
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        old: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.0 != old.0 {
            element.widget.points.clone_from(&self.0);
            element.ctx.request_paint_only();
        }
    }

    fn teardown(&self, _: &mut (), _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        _: &mut (),
        _: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        _: &mut State,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

struct Thumbnail {
    points: Vec<[f64; 2]>,
    size: Size,
}

fn fit(points: &[[f64; 2]], size: Size) -> Vec<Point> {
    if points.len() > 4096
        || size.width < 10.0
        || size.height < 10.0
        || points
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || value.abs() > 1e6)
    {
        return vec![];
    }
    let Some(first) = points.first() else {
        return vec![];
    };
    let mut bounds = Rect::new(first[0], first[1], first[0], first[1]);
    for point in points {
        bounds = bounds.union_pt(Point::new(point[0], point[1]));
    }
    let scale = ((size.width - 10.0) / bounds.width().max(1e-9))
        .min((size.height - 10.0) / bounds.height().max(1e-9));
    let source_center = bounds.center();
    let center = size.to_rect().center();
    points
        .iter()
        .map(|point| center + (Point::new(point[0], point[1]) - source_center) * scale)
        .collect()
}

impl Widget for Thumbnail {
    type Action = NoAction;

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        Length::const_px(if axis == Axis::Horizontal { 64.0 } else { 40.0 })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        painter
            .fill(
                self.size.to_rect().to_rounded_rect(5.0),
                components::SURFACE_SUBTLE,
            )
            .draw();
        let mut path = BezPath::new();
        for (index, point) in fit(&self.points, self.size).into_iter().enumerate() {
            if index == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        painter
            .stroke(
                path,
                &Stroke::new(2.0)
                    .with_caps(Cap::Round)
                    .with_join(Join::Round),
                components::ACCENT,
            )
            .draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label("Assigned gesture");
        node.set_description("The saved shape for this favorite app.");
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::testing::TestHarness;

    #[test]
    fn fits_shape_centered_with_padding_and_unchanged_aspect() {
        let points = [[-0.5, -0.25], [0.5, -0.25], [0.5, 0.25]];
        let fitted = fit(&points, Size::new(64.0, 40.0));
        assert_eq!(
            fitted,
            [
                Point::new(5.0, 6.5),
                Point::new(59.0, 6.5),
                Point::new(59.0, 33.5)
            ]
        );
        let shifted = points.map(|point| [point[0] + 0.1, point[1] - 0.1]);
        for (a, b) in fitted.iter().zip(fit(&shifted, Size::new(64.0, 40.0))) {
            assert!(a.distance(b) < 1e-6);
        }
        assert!(fit(&[[f64::NAN, 0.0]], Size::new(64.0, 40.0)).is_empty());
    }

    #[test]
    fn thumbnail_is_visible_named_and_passive() {
        let mut harness = TestHarness::create_with_size(
            components::hearthspace_light_properties(),
            Thumbnail {
                points: vec![[-0.5, -0.5], [0.0, 0.5], [0.5, -0.5]],
                size: Size::ZERO,
            }
            .prepare(),
            (64, 40),
        );
        let image = harness.render();
        assert!(
            image
                .pixels()
                .any(|pixel| u16::from(pixel[0]) > u16::from(pixel[2]) + 30)
        );
        let node = harness.access_node(harness.root_id()).unwrap();
        assert_eq!(node.label().as_deref(), Some("Assigned gesture"));
        assert_eq!(node.role(), Role::Image);
        harness.mouse_move((32.0, 20.0));
        harness.mouse_button_press(Some(masonry::core::pointer::PointerButton::Primary));
        harness.mouse_button_release(Some(masonry::core::pointer::PointerButton::Primary));
        assert!(harness.focused_widget_id().is_none());
        assert!(harness.pop_action_erased().is_none());
    }
}
