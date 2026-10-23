//! Compact progress retains the backend's numeric value and indeterminate state.
use crate::state::SettingsApp;
use hearthspace_ui_components as components;
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

pub(crate) fn progress(value: Option<f64>) -> Progress {
    Progress(
        value
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(0.0, 1.0)),
    )
}

pub(crate) struct Progress(Option<f64>);
impl ViewMarker for Progress {}
impl View<SettingsApp, (), ViewCtx> for Progress {
    type Element = Pod<ProgressWidget>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut SettingsApp) -> (Self::Element, ()) {
        (ctx.create_pod(ProgressWidget(self.0)), ())
    }
    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut SettingsApp,
    ) {
        if self.0 != previous.0 {
            element.widget.0 = self.0;
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
        _: &mut SettingsApp,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

pub(crate) struct ProgressWidget(Option<f64>);
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
        let width = bounds.width() * self.0.unwrap_or(1.0);
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
        node.set_label("AI model progress");
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(1.0);
        if let Some(value) = self.0 {
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
            NewWidget::new(ProgressWidget(Some(0.35))),
            (200, 5),
        );
        let node = determinate.access_node(determinate.root_id()).unwrap();
        assert_eq!(node.role(), Role::ProgressIndicator);
        assert_eq!(node.numeric_value(), Some(0.35));
        let pending = TestHarness::create_with_size(
            components::hearthspace_opaque_properties(),
            NewWidget::new(ProgressWidget(None)),
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
}
