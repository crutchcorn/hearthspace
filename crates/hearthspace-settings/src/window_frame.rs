//! The client body completes the compositor's rounded titlebar at its bottom.
use crate::state::SettingsApp;
use hearthspace_ui_components::{self as components, RADIUS_PANEL};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, Widget, WidgetMut, WidgetPod,
        },
        imaging::{ClipRef, Painter},
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length},
    },
};

pub(crate) fn body<V: WidgetView<SettingsApp>>(inner: V, photo: bool) -> BodyView<V> {
    BodyView { inner, photo }
}

pub(crate) struct BodyView<V> {
    inner: V,
    photo: bool,
}

const CONTENT: ViewId = ViewId::new(0);

impl<V> ViewMarker for BodyView<V> {}

impl<V: WidgetView<SettingsApp>> View<SettingsApp, (), ViewCtx> for BodyView<V> {
    type Element = Pod<WindowBody>;
    type ViewState = V::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        let (inner, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        (
            ctx.create_pod(WindowBody {
                inner: inner.new_widget.erased().to_pod(),
                photo: self.photo,
            }),
            view_state,
        )
    }

    fn rebuild(
        &self,
        previous: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) {
        if self.photo != previous.photo {
            element.widget.photo = self.photo;
            element.ctx.request_paint_only();
        }
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &previous.inner,
                view_state,
                ctx,
                WindowBody::child_mut(&mut element).downcast(),
                state,
            );
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.inner.teardown(
                view_state,
                ctx,
                WindowBody::child_mut(&mut element).downcast(),
            );
        });
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CONTENT) => self.inner.message(
                view_state,
                message,
                WindowBody::child_mut(&mut element).downcast(),
                state,
            ),
            _ => MessageResult::Stale,
        }
    }
}

pub(crate) struct WindowBody {
    inner: WidgetPod<dyn Widget>,
    photo: bool,
}

impl WindowBody {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}

impl Widget for WindowBody {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.inner, axis, cross)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.inner, size);
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.derive_baselines(&self.inner);
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        // Paint clipping encloses the sidebar, scrollbars and page background.
        // The square top edge joins the server titlebar without a transparent seam.
        painter.push_clip(ClipRef::fill(ctx.border_box().to_rounded_rect((
            0.0,
            0.0,
            RADIUS_PANEL - 1.0,
            RADIUS_PANEL - 1.0,
        ))));
        painter
            .fill(
                ctx.border_box(),
                components::SURFACE.with_alpha(if self.photo { 0.76 } else { 0.54 }),
            )
            .draw();
        // Continue the reference's window-wide wash below its titlebar.
        let rect = ctx.border_box();
        components::settings_material::edge_light(
            painter,
            xilem::masonry::kurbo::Rect::new(rect.x0, -44.0, rect.x1, rect.y1),
            RADIUS_PANEL,
        );
    }

    fn post_paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        painter.pop_clip();
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }
}
