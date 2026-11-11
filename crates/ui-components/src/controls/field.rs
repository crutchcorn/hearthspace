//! Style the native editor's separate placeholder without replacing its editing,
//! selection, IME, focus, or accessibility implementation.
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesMut,
            PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LayoutSize, LenReq, Length},
        parley::{
            FontFamilyName,
            style::{FontFamily, StyleProperty},
        },
        widgets::{Label, TextInput},
    },
};

pub(super) struct FieldView<V>(pub V);
impl<V> ViewMarker for FieldView<V> {}
const CONTENT: ViewId = ViewId::new(0);
impl<State: 'static, V: WidgetView<State, Widget = TextInput>> View<State, (), ViewCtx>
    for FieldView<V>
{
    type Element = Pod<FieldWidget>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(CONTENT, |ctx| self.0.build(ctx, state));
        (
            ctx.create_pod(FieldWidget {
                child: child.new_widget.to_pod(),
            }),
            view_state,
        )
    }
    fn rebuild(
        &self,
        old: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.0
                .rebuild(&old.0, vs, ctx, FieldWidget::child_mut(&mut el), state)
        });
    }
    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.0.teardown(vs, ctx, FieldWidget::child_mut(&mut el))
        });
    }
    fn message(
        &self,
        vs: &mut Self::ViewState,
        msg: &mut MessageCtx,
        mut el: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<()> {
        if msg.take_first() == Some(CONTENT) {
            self.0
                .message(vs, msg, FieldWidget::child_mut(&mut el), state)
        } else {
            MessageResult::Stale
        }
    }
}

pub(super) struct FieldWidget {
    child: WidgetPod<TextInput>,
}
impl FieldWidget {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, TextInput> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}
impl Widget for FieldWidget {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
            ctx.mutate_child_later(&mut self.child, |mut input| {
                let mut placeholder = TextInput::placeholder_mut(&mut input);
                Label::insert_style(&mut placeholder, StyleProperty::FontSize(12.));
                Label::insert_style(
                    &mut placeholder,
                    StyleProperty::FontFamily(FontFamily::Single(FontFamilyName::Named(
                        "Liberation Sans".into(),
                    ))),
                );
            });
        }
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.compute_length(
            &mut self.child,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        )
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
