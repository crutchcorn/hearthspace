//! Pointer, keyboard and accessibility behavior shared by launcher controls.
use super::{FocusTarget, SelectionStep};
use hearthspace_ui_components as components;
use xilem::masonry::{
    accesskit::{Action, Node, Role, Toggled},
    core::{
        AccessCtx, AccessEvent, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx,
        MeasureCtx, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent,
        Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size, Stroke},
    layout::{LayoutSize, LenReq, Length},
    widgets::ButtonPress,
};

#[path = "search_result_input.rs"]
mod input;
pub(super) use input::Input;

#[derive(Debug)]
pub(super) struct Reveal;
#[derive(Debug)]
pub(super) struct FocusedResult;
pub(super) struct ResultButton {
    pub(super) label: String,
    pub(super) text: WidgetPod<dyn Widget>,
    pub(super) selected: bool,
    pub(super) target: FocusTarget,
    pub(super) columns: usize,
    pub(super) pressed: Option<bool>,
    pub(super) navigation: bool,
    pub(super) raised: bool,
    pub(super) context: Option<crate::ui::app_context::Target>,
    pub(super) input: Input,
}
impl ResultButton {
    pub(super) fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.text)
    }
}
impl Widget for ResultButton {
    type Action = ButtonPress;
    fn property_changed(
        &mut self,
        ctx: &mut xilem::masonry::core::UpdateCtx<'_>,
        property: std::any::TypeId,
    ) {
        components::Palette::prop_changed(ctx, property);
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.text);
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.pointer_input(ctx, event);
    }
    fn on_anim_frame(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, interval: u64) {
        self.touch_frame(ctx, interval);
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.text_input(ctx, event);
    }
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::ShowContextMenu && self.context.is_some() {
            ctx.request_focus();
            self.open_context(ctx, crate::ui::app_context::Intent::Accessibility);
        } else if event.action == Action::Click {
            if let Some(target) = *self.target.lock().unwrap() {
                ctx.set_focus(target);
            }
            ctx.submit_action::<ButtonPress>(ButtonPress { button: None });
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(
            event,
            Update::DisabledChanged(true) | Update::StashedChanged(true)
        ) || (matches!(event, Update::FocusChanged(false)) && !ctx.is_active())
        {
            self.input.cancel();
        }
        if matches!(event, Update::FocusChanged(_) | Update::HoveredChanged(_)) {
            ctx.request_render();
        }
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
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        if self.navigation && axis == Axis::Horizontal {
            // Result copy clips to the available tile/row width. Its complete
            // accessible name must not become a minimum popup width.
            match request {
                LenReq::MinContent => return Length::ZERO,
                LenReq::FitContent(space) => return space,
                LenReq::MaxContent => {}
            }
        }
        ctx.compute_length(
            &mut self.text,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        )
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.text, size);
        ctx.place_child(&mut self.text, Point::ORIGIN);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if self.raised {
            // Masonry paints the border/background from the raised selectors.
            return;
        }
        let palette = *props.get::<components::Palette>(ctx.property_cache());
        let bounds = ctx.border_box().inset(-1.0).to_rounded_rect(10.0);
        if ctx.is_hovered() && !self.selected && self.pressed != Some(true) {
            painter.fill(bounds, palette.button_hover).draw();
        }
        if ctx.is_focus_target() {
            painter
                .stroke(bounds, &Stroke::new(2.0), palette.focus_ring_on_glass)
                .draw();
        }
    }
    fn accessibility_role(&self) -> Role {
        Role::Button
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.label.clone());
        if let Some(pressed) = self.pressed {
            node.set_toggled(if pressed {
                Toggled::True
            } else {
                Toggled::False
            });
        } else if self.navigation {
            node.set_selected(self.selected);
        }
        node.add_action(Action::Click);
        if self.context.is_some() {
            node.add_action(Action::ShowContextMenu);
            node.set_has_popup(xilem::masonry::accesskit::HasPopup::Menu);
            node.set_description(
                "Open the app menu with a secondary click, touch and hold, or Shift+F10.",
            );
        }
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
