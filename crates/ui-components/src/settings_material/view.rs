use super::{Material, widget::MaterialWidget};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::properties::{Background, BorderColor},
};

pub struct MaterialView<V> {
    pub(super) inner: V,
    pub(super) material: Material,
    pub(super) disabled: bool,
}

impl<V> MaterialView<V> {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

const CONTENT: ViewId = ViewId::new(0);
impl<V> ViewMarker for MaterialView<V> {}
impl<State: 'static, Action: 'static, V: WidgetView<State, Action>> View<State, Action, ViewCtx>
    for MaterialView<V>
{
    type Element = Pod<MaterialWidget>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (mut child, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        if matches!(self.material, Material::Button(..)) {
            child.new_widget = child.new_widget.with_props((
                Background::Color(xilem::Color::TRANSPARENT),
                BorderColor {
                    color: xilem::Color::TRANSPARENT,
                },
            ));
        }
        let disabled = child.new_widget.options.disabled;
        let mut element = ctx.create_pod(MaterialWidget::new(
            child.new_widget,
            self.material,
            disabled,
        ));
        element.new_widget.options.disabled = self.disabled;
        (element, view_state)
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
                MaterialWidget::child_mut(&mut element).downcast(),
                state,
            );
        });
        if matches!(self.material, Material::Button(..)) {
            let mut child = MaterialWidget::child_mut(&mut element);
            child.insert_prop(Background::Color(xilem::Color::TRANSPARENT));
            child.insert_prop(BorderColor {
                color: xilem::Color::TRANSPARENT,
            });
        }
        element.widget.material = self.material;
        if self.disabled != previous.disabled {
            element.ctx.set_disabled(self.disabled);
        }
        element.ctx.request_paint_only();
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
                MaterialWidget::child_mut(&mut element).downcast(),
            );
        });
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_first() {
            Some(CONTENT) => self.inner.message(
                view_state,
                message,
                MaterialWidget::child_mut(&mut element).downcast(),
                state,
            ),
            _ => MessageResult::Stale,
        }
    }
}
