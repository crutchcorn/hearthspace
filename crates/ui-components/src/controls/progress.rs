//! Compact progress retains the backend's numeric value and indeterminate state.

use crate as components;
use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, Widget,
        },
        imaging::Painter,
        kurbo::{Axis, Rect, Size},
        layout::{AsUnit, LenReq, Length},
        peniko::Gradient,
    },
};

pub fn progress(name: impl Into<String>, value: Option<f64>) -> Progress {
    Progress(
        name.into(),
        value
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(0.0, 1.0)),
    )
}

pub struct Progress(String, Option<f64>);
impl ViewMarker for Progress {}
impl<State: 'static> View<State, (), ViewCtx> for Progress {
    type Element = Pod<ProgressWidget>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
        (ctx.create_pod(ProgressWidget(self.0.clone(), self.1)), ())
    }
    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.0 != previous.0 || self.1 != previous.1 {
            element.widget.0 = self.0.clone();
            element.widget.1 = self.1;
            element.ctx.request_paint_only();
            element.ctx.request_accessibility_update();
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

pub struct ProgressWidget(String, Option<f64>);
impl Widget for ProgressWidget {
    type Action = NoAction;
    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        _: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Vertical => 5.0.px(),
            Axis::Horizontal => match request {
                LenReq::MinContent => Length::ZERO,
                LenReq::MaxContent => 400.0.px(),
                LenReq::FitContent(space) => space,
            },
        }
    }
    fn layout(&mut self, _: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {}
    fn property_changed(
        &mut self,
        ctx: &mut xilem::masonry::core::UpdateCtx<'_>,
        property_type: std::any::TypeId,
    ) {
        components::Palette::prop_changed(ctx, property_type);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<components::Palette>(ctx.property_cache());
        let bounds = ctx.border_box();
        let track = bounds.to_rounded_rect(2.5);
        painter.fill(track, palette.control_track).draw();
        let width = bounds.width() * self.1.unwrap_or(1.0);
        if width > 0.0 {
            let fill =
                Rect::new(bounds.x0, bounds.y0, bounds.x0 + width, bounds.y1).to_rounded_rect(2.5);
            let gradient = Gradient::new_linear((bounds.x0, 0.0), (bounds.x0 + width, 0.0))
                .with_stops([palette.accent, palette.accent_bright]);
            painter.with_fill_clip(track, |painter| {
                painter.fill(fill, &gradient).draw();
            });
        }
    }
    fn accessibility_role(&self) -> Role {
        Role::ProgressIndicator
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.0.clone());
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(1.0);
        if let Some(value) = self.1 {
            node.set_numeric_value(value);
        }
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xilem::masonry::{core::NewWidget, testing::TestHarness};
    #[test]
    fn compact_progress_exposes_actual_fraction_without_inventing_indeterminate_progress() {
        let determinate = TestHarness::create_with_size(
            components::hearthspace_opaque_properties(),
            NewWidget::new(ProgressWidget("Download".into(), Some(0.35))),
            (200, 5),
        );
        let node = determinate.access_node(determinate.root_id()).unwrap();
        assert_eq!(node.role(), Role::ProgressIndicator);
        assert_eq!(node.numeric_value(), Some(0.35));
        let pending = TestHarness::create_with_size(
            components::hearthspace_opaque_properties(),
            NewWidget::new(ProgressWidget("Download".into(), None)),
            (200, 5),
        );
        assert_eq!(
            pending
                .access_node(pending.root_id())
                .unwrap()
                .numeric_value(),
            None
        );
    }
    #[test]
    fn known_progress_can_become_unknown_without_announcing_a_stale_value() {
        let mut h = TestHarness::create_with_size(
            components::hearthspace_opaque_properties(),
            NewWidget::new(ProgressWidget("Download".into(), Some(0.6))),
            (200, 5),
        );
        let id = h.root_id();
        assert_eq!(h.access_node(id).unwrap().numeric_value(), Some(0.6));
        h.edit_root_widget(|mut w| {
            w.widget.1 = None;
            w.ctx.request_paint_only();
            w.ctx.request_accessibility_update();
        });
        h.redraw();
        assert_eq!(h.access_node(id).unwrap().numeric_value(), None);
        assert_eq!(
            h.access_node(id).unwrap().label().as_deref(),
            Some("Download")
        );
        h.press_tab_key(false);
        assert_eq!(h.focused_widget_id(), None);
    }
}
