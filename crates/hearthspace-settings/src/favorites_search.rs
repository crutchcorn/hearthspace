// A named app-search field delegating text, IME and selection to Masonry.
use std::any::TypeId;
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update,
        UpdateCtx, Widget,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{LenReq, Length},
    widgets::{Label, TextAction, TextArea},
};

const PLACEHOLDER: &str = "Find apps";

struct Editor {
    text: TextArea<true>,
    placeholder: Label,
}
impl Editor {
    fn new(text: &str) -> Self {
        Self {
            // Preserve the upstream editor's soft wrapping for long queries.
            // Enter still submits; it never inserts a new line.
            text: TextArea::new_editable(text).with_word_wrap(true),
            placeholder: Label::new(PLACEHOLDER),
        }
    }

    fn reset_text(&mut self, text: &str) {
        // TextArea stores focus in Masonry's context, not in the editor. Keep
        // this widget's identity and placeholder while clearing external text
        // and any composition, matching TextArea::reset_text's semantics.
        self.text = TextArea::new_editable(text).with_word_wrap(true);
    }
}

impl xilem::masonry::core::UsesProperty<xilem::masonry::properties::ContentColor> for Editor {}

impl Widget for Editor {
    type Action = TextAction;
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.text.on_text_event(ctx, props, event);
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.text.on_pointer_event(ctx, props, event);
    }
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        self.text.on_access_event(ctx, props, event);
    }
    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        self.text.on_anim_frame(ctx, props, interval);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        self.text.update(ctx, props, event);
        self.placeholder.update(ctx, props, event);
    }
    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        self.text.property_changed(ctx, property_type);
    }
    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        props: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        self.text.measure(ctx, props, axis, request, cross)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, props: &PropertiesRef<'_>, size: Size) {
        self.text.layout(ctx, props, size);
        self.placeholder.layout(ctx, props, size);
        ctx.set_clip_path(size.to_rect());
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if self.text.is_empty() {
            self.placeholder.paint(ctx, props, painter);
        }
        self.text.paint(ctx, props, painter);
    }
    fn accessibility_role(&self) -> Role {
        Role::TextInput
    }
    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        self.text.accessibility(ctx, props, node);
        node.set_label(PLACEHOLDER);
        node.set_placeholder(PLACEHOLDER);
    }
    fn accepts_focus(&self) -> bool {
        true
    }
    fn accepts_text_input(&self) -> bool {
        true
    }
    fn get_cursor(&self, ctx: &QueryCtx<'_>, point: Point) -> CursorIcon {
        self.text.get_cursor(ctx, point)
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

use crate::state::SettingsApp;
use hearthspace_ui_components as components;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        layout::AsUnit,
        properties::{CaretColor, Padding, SelectionColor},
    },
    style::Style,
};

pub(crate) fn input(query: &str) -> impl WidgetView<SettingsApp> + use<> {
    Input(query.to_owned())
        .height(44.0.px())
        .color(components::TEXT)
        .background_color(components::SURFACE_CONTROL)
        .border(components::BORDER, 1.0.px())
        .corner_radius(10.0.px())
        .padding(Padding::from_vh(6.0.px(), 10.0.px()))
}
struct Input(String);
impl ViewMarker for Input {}
impl View<SettingsApp, (), ViewCtx> for Input {
    type Element = Pod<Editor>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut SettingsApp) -> (Self::Element, ()) {
        (
            ctx.with_action_widget(|ctx| {
                let mut pod = ctx.create_pod(Editor::new(&self.0));
                pod.new_widget = pod.new_widget.with_props((
                    CaretColor {
                        color: components::TEXT,
                    },
                    SelectionColor {
                        color: components::ACCENT_SUBTLE,
                    },
                ));
                pod
            }),
            (),
        )
    }
    fn rebuild(
        &self,
        old: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut SettingsApp,
    ) {
        if self.0 != old.0 && element.widget.text.text() != self.0.as_str() {
            element.widget.reset_text(&self.0);
            element.ctx.request_layout();
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
        state: &mut SettingsApp,
    ) -> MessageResult<()> {
        if message.take_first().is_some() {
            return MessageResult::Stale;
        }
        let Some(action) = message.take_message::<TextAction>() else {
            return MessageResult::Stale;
        };
        match *action {
            TextAction::Changed(query) => state.favorites.query = query,
            TextAction::Cancelled => state.favorites.query.clear(),
            TextAction::Entered(_) => {}
        }
        MessageResult::Action(())
    }
}
