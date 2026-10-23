//! Native children retain semantics; this wrapper paints the reference glass.
use hearthspace_ui_components::Palette;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::properties::{Background, BorderColor},
};

#[path = "material_widget.rs"]
mod widget;
use widget::PanelWidget;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Field,
    Card,
}

pub(super) struct Panel<V> {
    inner: V,
    selected: bool,
    hovered: bool,
    kind: Kind,
    palette: Palette,
}

pub(super) fn panel<V>(inner: V, selected: bool, palette: Palette) -> Panel<V> {
    Panel {
        inner,
        selected,
        hovered: false,
        kind: Kind::Field,
        palette,
    }
}

pub(super) fn card<V>(inner: V, selected: bool, palette: Palette) -> Panel<V> {
    Panel {
        inner,
        selected,
        hovered: false,
        kind: Kind::Card,
        palette,
    }
}

impl<V> Panel<V> {
    pub(super) fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }
}

const CONTENT: ViewId = ViewId::new(0);
impl<V> ViewMarker for Panel<V> {}
impl<State: 'static, V: WidgetView<State>> View<State, (), ViewCtx> for Panel<V> {
    type Element = Pod<PanelWidget>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (mut child, state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        child.new_widget = child.new_widget.with_props((
            Background::Color(xilem::Color::TRANSPARENT),
            BorderColor {
                color: xilem::Color::TRANSPARENT,
            },
        ));
        (
            ctx.create_pod(PanelWidget::new(
                child.new_widget,
                self.kind,
                self.selected,
                self.hovered,
                self.palette,
            )),
            state,
        )
    }

    fn rebuild(
        &self,
        previous: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &previous.inner,
                view_state,
                ctx,
                PanelWidget::child_mut(&mut element).downcast(),
                state,
            )
        });
        if self.kind != previous.kind
            || self.selected != previous.selected
            || self.hovered != previous.hovered
            || self.palette != previous.palette
        {
            element.widget.kind = self.kind;
            element.widget.selected = self.selected;
            element.widget.hovered = self.hovered;
            element.widget.palette = self.palette;
            element.ctx.request_paint_only();
        }
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
                PanelWidget::child_mut(&mut element).downcast(),
            )
        });
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CONTENT) => self.inner.message(
                view_state,
                message,
                PanelWidget::child_mut(&mut element).downcast(),
                state,
            ),
            _ => MessageResult::Stale,
        }
    }
}
