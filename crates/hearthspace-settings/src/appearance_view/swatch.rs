//! Swatch shadows need paint overflow; Masonry Canvas clips to its layout box.
use crate::state::SettingsApp;
use hearthspace_ui_components::settings_material;
use xilem::{
    Color, Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, Widget,
        },
        imaging::Painter,
        kurbo::{Axis, BezPath, Shape, Size, Vec2},
        layout::{LenReq, Length},
    },
};

pub(super) struct Swatch(pub Color);
impl ViewMarker for Swatch {}
impl View<SettingsApp, (), ViewCtx> for Swatch {
    type Element = Pod<SwatchWidget>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut SettingsApp) -> (Self::Element, ()) {
        (ctx.create_pod(SwatchWidget(self.0)), ())
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

pub(crate) struct SwatchWidget(Color);
impl Widget for SwatchWidget {
    type Action = NoAction;
    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        _: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        Length::px(29.0)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {
        ctx.set_paint_insets(16.0);
    }
    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let rect = ctx.border_box();
        let radius = rect.width().min(rect.height()) / 2.0;
        let shape = rect.to_rounded_rect(radius);
        settings_material::shadow(
            painter,
            rect,
            radius,
            self.0.with_alpha(0.24),
            (0.0, 3.0),
            12.0,
            0.0,
        );
        painter.fill(shape, self.0).draw();
        let inset = shape + Vec2::new(0.0, 1.0);
        let highlight = BezPath::from_iter(
            rect.path_elements(0.1)
                .chain(inset.to_path(0.1).reverse_subpaths()),
        );
        painter.with_fill_clip(shape, |painter| {
            painter
                .fill(highlight, Color::from_rgba8(255, 255, 255, 0x59))
                .draw();
        });
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
