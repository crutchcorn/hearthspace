//! A named result button with explicit selection and automatic scroll reveal.
use super::{
    launcher::{self, Entry},
    search_editor::SelectionStep,
    search_focus::FocusTarget,
    search_selection,
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_ui_components as components;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Action, Node, Role},
        core::{
            AccessCtx, AccessEvent, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx,
            MeasureCtx, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut,
            PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetPod,
            keyboard::{Key, NamedKey},
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{AsUnit, LenReq, Length},
        properties::{LineBreaking, Padding},
        widgets::{ButtonPress, Label},
    },
    style::Style,
};

pub(super) fn row(
    output: &ShellOutputInstance,
    entry: Entry,
    selected: bool,
    target: FocusTarget,
) -> impl WidgetView<ShellState> + use<> {
    ResultView {
        key: output.key().into(),
        output: output.output_name.clone(),
        entry,
        selected,
        target,
    }
    .height(40.0.px())
    .padding(Padding::from_vh(7.0.px(), 10.0.px()))
    .background_color(if selected {
        components::ACCENT_SUBTLE
    } else {
        components::SURFACE
    })
    .corner_radius(6.0.px())
}

struct ResultView {
    key: String,
    output: Option<String>,
    entry: Entry,
    selected: bool,
    target: FocusTarget,
}
impl ViewMarker for ResultView {}
impl View<ShellState, (), ViewCtx> for ResultView {
    type Element = Pod<ResultButton>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        let label = self.entry.label();
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(ResultButton {
                    label: label.clone(),
                    text: Label::new(label)
                        .prepare()
                        .with_props((
                            LineBreaking::Clip,
                            xilem::masonry::properties::ContentColor {
                                color: components::TEXT,
                            },
                        ))
                        .to_pod(),
                    selected: self.selected,
                    target: self.target.clone(),
                })
            }),
            (),
        )
    }
    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) {
        let label = self.entry.label();
        if element.widget.label != label {
            element.widget.label = label.clone();
            let xilem::masonry::core::WidgetMut { widget, ctx, .. } = &mut element;
            Label::set_text(&mut ctx.get_mut(&mut widget.text), label);
            ctx.request_accessibility_update();
        }
        element.widget.target = self.target.clone();
        if previous.selected != self.selected {
            element.widget.selected = self.selected;
            element.ctx.request_accessibility_update();
            if self.selected {
                let xilem::masonry::core::WidgetMut { widget, ctx, .. } = &mut element;
                ctx.mutate_child_later(&mut widget.text, |mut child| {
                    child.ctx.submit_untyped_action(Box::new(Reveal))
                });
            }
        }
    }
    fn teardown(&self, _: &mut (), ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(element);
    }
    fn message(
        &self,
        _: &mut (),
        message: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        if message.take_message::<FocusedResult>().is_some() {
            search_selection::choose(state, &self.key, &self.entry.id());
        } else if let Some(step) = message.take_message::<SelectionStep>() {
            search_selection::step(state, &self.key, step.0);
        } else if let Some(press) = message.take_message::<ButtonPress>() {
            if matches!(press.button, None | Some(PointerButton::Primary)) {
                launcher::select(state, &self.key, self.output.as_deref(), &self.entry);
            }
        } else {
            return MessageResult::Stale;
        }
        MessageResult::Action(())
    }
}

#[derive(Debug)]
struct Reveal;
#[derive(Debug)]
struct FocusedResult;
pub(super) struct ResultButton {
    label: String,
    text: WidgetPod<Label>,
    selected: bool,
    target: FocusTarget,
}
impl Widget for ResultButton {
    type Action = ButtonPress;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.text);
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                ..
            }) => {
                ctx.capture_pointer();
                if let Some(target) = *self.target.lock().unwrap() {
                    ctx.set_focus(target);
                }
            }
            PointerEvent::Up(PointerButtonEvent { button, .. })
                if ctx.is_active() && ctx.is_hovered() =>
            {
                ctx.submit_action::<ButtonPress>(ButtonPress { button: *button });
            }
            _ => {}
        }
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(event) = event {
            if event.state.is_down()
                && !event.modifiers.ctrl()
                && !event.modifiers.alt()
                && !event.modifiers.meta()
                && !event.modifiers.shift()
                && let Key::Named(key @ (NamedKey::ArrowDown | NamedKey::ArrowUp)) = event.key
            {
                if let Some(target) = *self.target.lock().unwrap() {
                    ctx.set_focus(target);
                }
                ctx.submit_untyped_action(Box::new(SelectionStep(if key == NamedKey::ArrowDown {
                    1
                } else {
                    -1
                })));
                ctx.set_handled();
            } else if event.state.is_up()
                && (event.key == Key::Named(NamedKey::Enter)
                    || event.key == Key::Character(" ".into()))
            {
                ctx.submit_action::<ButtonPress>(ButtonPress { button: None });
                ctx.set_handled();
            }
        }
    }
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            ctx.submit_action::<ButtonPress>(ButtonPress { button: None });
        }
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FocusChanged(true)) {
            ctx.mutate_self_later(|mut widget| {
                widget.ctx.submit_untyped_action(Box::new(FocusedResult))
            });
        }
        if self.selected && matches!(event, Update::WidgetAdded) {
            ctx.mutate_child_later(&mut self.text, |mut child| {
                child.ctx.submit_untyped_action(Box::new(Reveal))
            });
        }
    }
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if source == self.text.id() && action.is::<Reveal>() {
            ctx.request_scroll_to_this();
            ctx.set_handled();
        }
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.text, axis, cross)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.text, size);
        ctx.place_child(&mut self.text, Point::ORIGIN);
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::Button
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.label.clone());
        node.set_selected(self.selected);
        node.add_action(Action::Click);
    }
    fn accepts_focus(&self) -> bool {
        true
    }
    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.text.id()])
    }
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("SearchResult", id = id.trace())
    }
}
