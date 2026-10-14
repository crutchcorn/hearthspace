//! Focus the launcher search when its shell surface receives keyboard focus.
//!
//! Xilem's text input owns a separate Masonry TextArea. Record its real widget
//! ID and focus it through the normal event API, without synthesizing a click.
use std::sync::{Arc, Mutex};

use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
            NoAction, PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
            UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length},
        properties::Dimensions,
        widgets::TextInput,
    },
};

use super::state::ShellState;

pub(super) type FocusTarget = Arc<Mutex<Option<WidgetId>>>;

pub(super) fn track_search<V>(inner: V, target: FocusTarget) -> TrackedSearch<V> {
    TrackedSearch { inner, target }
}

pub(super) struct TrackedSearch<V> {
    inner: V,
    target: FocusTarget,
}

impl<V> ViewMarker for TrackedSearch<V> {}

impl<V> View<ShellState, (), ViewCtx> for TrackedSearch<V>
where
    V: WidgetView<ShellState, Widget = TextInput>,
{
    type Element = Pod<TextInput>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (element, view_state) = self.inner.build(ctx, state);
        *self.target.lock().unwrap() = Some(element.new_widget.widget.area_pod().id());
        (element, view_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) {
        *self.target.lock().unwrap() = Some(element.widget.area_pod().id());
        self.inner
            .rebuild(&prev.inner, view_state, ctx, element, state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        *self.target.lock().unwrap() = None;
        self.inner.teardown(view_state, ctx, element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        self.inner.message(view_state, message, element, state)
    }
}

pub(super) fn focus_on_activation<V>(inner: V, target: FocusTarget) -> FocusOnActivation<V> {
    FocusOnActivation { inner, target }
}

pub(super) struct FocusOnActivation<V> {
    inner: V,
    target: FocusTarget,
}

const CONTENT: ViewId = ViewId::new(0x53454152);

impl<V> ViewMarker for FocusOnActivation<V> {}

impl<V> View<ShellState, (), ViewCtx> for FocusOnActivation<V>
where
    V: WidgetView<ShellState>,
{
    type Element = Pod<SearchFocus>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        let widget = SearchFocus {
            inner: child.new_widget.erased().to_pod(),
            target: self.target.clone(),
        };
        (Pod::new_with_props(widget, Dimensions::MAX), view_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) {
        element.widget.target = self.target.clone();
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &prev.inner,
                view_state,
                ctx,
                SearchFocus::child_mut(&mut element).downcast(),
                state,
            )
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
                SearchFocus::child_mut(&mut element).downcast(),
            )
        });
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CONTENT) => self.inner.message(
                view_state,
                message,
                SearchFocus::child_mut(&mut element).downcast(),
                state,
            ),
            _ => MessageResult::Stale,
        }
    }
}

pub(super) struct SearchFocus {
    inner: WidgetPod<dyn Widget>,
    target: FocusTarget,
}

#[derive(Debug)]
struct InitializeSearchFocus;

impl SearchFocus {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}

impl Widget for SearchFocus {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
            // Xilem installs a focus fallback only for windows present at startup.
            // Initialize our internal focus through Masonry's action pass so a
            // newly hotplugged bar also receives keyboard and activation events.
            ctx.mutate_child_later(&mut self.inner, |mut child| {
                child
                    .ctx
                    .submit_untyped_action(Box::new(InitializeSearchFocus));
            });
        }
    }

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if source == self.inner.id() && action.is::<InitializeSearchFocus>() {
            if let Some(target) = *self.target.lock().unwrap() {
                ctx.set_focus(target);
            }
            ctx.set_handled();
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut xilem::masonry::core::PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if matches!(event, TextEvent::WindowFocusChange(true))
            && let Some(target) = *self.target.lock().unwrap()
        {
            ctx.set_focus(target);
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.inner, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.inner, size);
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.derive_baselines(&self.inner);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }

    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("SearchFocus", id = id.trace())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::{
        testing::TestHarness,
        theme::default_property_set,
        widgets::{Button, Flex, TextArea},
    };

    #[test]
    fn search_focus_initializes_without_fallback_and_restores_after_button_focus() {
        let input = TextInput::new("");
        let search_id = input.area_pod().id();
        let button = Button::with_text("Pan").prepare();
        let button_id = button.id();
        let contents = Flex::row().with_fixed(input.prepare()).with_fixed(button);
        let widget = SearchFocus {
            inner: contents.prepare().erased().to_pod(),
            target: Arc::new(Mutex::new(Some(search_id))),
        }
        .prepare()
        .with_props(Dimensions::MAX);
        let mut harness = TestHarness::create(default_property_set(), widget);
        // Dynamically created Xilem windows have no focus fallback.
        assert_eq!(harness.focused_widget_id(), Some(search_id));
        harness.process_text_event(TextEvent::WindowFocusChange(true));
        assert_eq!(harness.focused_widget_id(), Some(search_id));
        harness.keyboard_type_chars("foot");
        assert_eq!(
            harness
                .get_widget_with_id(search_id)
                .downcast::<TextArea<true>>()
                .unwrap()
                .text()
                .to_string(),
            "foot"
        );

        harness.focus_on(Some(button_id));
        harness.process_text_event(TextEvent::WindowFocusChange(false));
        harness.process_text_event(TextEvent::WindowFocusChange(true));
        assert_eq!(harness.focused_widget_id(), Some(search_id));
    }
}
