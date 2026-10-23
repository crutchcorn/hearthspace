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
            keyboard::{Key, NamedKey},
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length},
        properties::Dimensions,
        widgets::TextAction,
    },
};

use super::state::ShellState;

pub(super) type FocusTarget = Arc<Mutex<Option<WidgetId>>>;

#[path = "search_focus_record.rs"]
mod record;
pub(super) use record::Record;
#[path = "search_focus_navigation.rs"]
mod navigation;

pub(super) fn focus_on_activation<V>(
    inner: V,
    target: FocusTarget,
    output_key: String,
) -> FocusOnActivation<V> {
    FocusOnActivation {
        inner,
        target,
        output_key,
        dismiss: ShellState::dismiss_search,
        back: None,
        cancel_editor: true,
        navigation: Vec::new(),
    }
}

pub(super) struct FocusOnActivation<V> {
    inner: V,
    target: FocusTarget,
    output_key: String,
    dismiss: fn(&mut ShellState, &str),
    back: Option<fn(&mut ShellState, &str)>,
    cancel_editor: bool,
    navigation: Vec<FocusTarget>,
}

impl<V> FocusOnActivation<V> {
    pub(super) fn with_back(mut self, back: Option<fn(&mut ShellState, &str)>) -> Self {
        self.back = back;
        self
    }

    pub(super) fn with_arrow_navigation(mut self, targets: Vec<FocusTarget>) -> Self {
        self.navigation = targets;
        self
    }

    /// Dialogs focus a button, so Escape calls their dismissal directly.
    pub(super) fn dismiss_directly(mut self) -> Self {
        self.cancel_editor = false;
        self
    }

    pub(super) fn with_dismiss(mut self, dismiss: fn(&mut ShellState, &str)) -> Self {
        self.dismiss = dismiss;
        self
    }
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
            cancel_editor: self.cancel_editor,
            back: self.back.is_some(),
            navigation: self.navigation.clone(),
        };
        let pod = Pod::new_with_props(widget, Dimensions::MAX);
        ctx.record_action_source(pod.new_widget.id());
        (pod, view_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) {
        let previous_editor = *element.widget.target.lock().unwrap();
        let restore_editor = !Arc::ptr_eq(&element.widget.target, &self.target)
            || previous_editor.is_none()
            || element.ctx.focus_target_id() == previous_editor;
        element.widget.target = self.target.clone();
        element.widget.cancel_editor = self.cancel_editor;
        element.widget.back = self.back.is_some();
        element.widget.navigation.clone_from(&self.navigation);
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &prev.inner,
                view_state,
                ctx,
                SearchFocus::child_mut(&mut element).downcast(),
                state,
            )
        });
        let editor = *self.target.lock().unwrap();
        if restore_editor && editor.is_some() && previous_editor != editor {
            let WidgetMut { widget, ctx, .. } = &mut element;
            ctx.mutate_child_later(&mut widget.inner, |mut child| {
                child
                    .ctx
                    .submit_untyped_action(Box::new(InitializeSearchFocus));
            });
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
                SearchFocus::child_mut(&mut element).downcast(),
            )
        });
        ctx.teardown_action_source(element);
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
            None if message.take_message::<DismissLauncher>().is_some() => {
                (self.dismiss)(state, &self.output_key);
                MessageResult::Action(())
            }
            None if message.take_message::<GoBack>().is_some() => {
                if let Some(back) = self.back {
                    back(state, &self.output_key);
                }
                MessageResult::Action(())
            }
            _ => MessageResult::Stale,
        }
    }
}

pub(super) struct SearchFocus {
    inner: WidgetPod<dyn Widget>,
    target: FocusTarget,
    cancel_editor: bool,
    back: bool,
    navigation: Vec<FocusTarget>,
}

#[derive(Debug)]
struct InitializeSearchFocus;
#[derive(Debug)]
struct DismissLauncher;
#[derive(Debug)]
struct GoBack;

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
            // newly opened search also receives keyboard and activation events.
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
        if navigation::handle(ctx, event, &self.navigation, self.back) {
            return;
        }
        if matches!(event, TextEvent::WindowFocusChange(true))
            && let Some(target) = *self.target.lock().unwrap()
        {
            ctx.set_focus(target);
        }
        if let TextEvent::Keyboard(event) = event
            && event.state.is_down()
            && event.key == Key::Named(NamedKey::Escape)
        {
            // Escape must also dismiss search when a result or close button has
            // keyboard focus. Reuse the search view's existing cancel callback.
            if self.cancel_editor
                && let Some(target) = *self.target.lock().unwrap()
            {
                ctx.mutate_later(target, |mut search| {
                    search
                        .ctx
                        .submit_action::<TextAction>(TextAction::Cancelled);
                });
                ctx.set_focus(target);
            } else {
                ctx.submit_untyped_action(Box::new(DismissLauncher));
            }
            ctx.set_handled();
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
        widgets::{Button, Flex, TextArea, TextInput},
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
            cancel_editor: true,
            back: false,
            navigation: Vec::new(),
        }
        .prepare()
        .with_props(Dimensions::MAX);
        let mut harness = TestHarness::create(default_property_set(), widget);
        // Dynamically created Xilem windows have no focus fallback.
        assert_eq!(harness.focused_widget_id(), Some(search_id));
        harness.process_text_event(TextEvent::WindowFocusChange(true));
        assert_eq!(harness.focused_widget_id(), Some(search_id));

        while harness.pop_action_erased().is_some() {}
        harness.focus_on(Some(button_id));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
        assert!(matches!(
            harness.pop_action::<TextAction>(),
            Some((TextAction::Cancelled, id)) if id == search_id
        ));
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
