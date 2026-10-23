//! Use Masonry's editor unchanged for text/IME/clipboard, intercepting only the
//! launcher navigation keys before TextArea's vertical caret movement handles them.
use std::any::TypeId;
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update,
        UpdateCtx, Widget, WidgetId,
        keyboard::{Key, NamedKey},
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{LenReq, Length},
    widgets::{Label, TextAction, TextArea},
};

pub(super) const PLACEHOLDER: &str = "Search apps and windows...";
#[derive(Debug)]
pub(super) struct SelectionStep(pub(super) i32);

pub(super) struct SearchEditor {
    pub(super) text: TextArea<true>,
    placeholder: Label,
    composing: bool,
    navigation: bool,
    font_size: f32,
    hint: &'static str,
    accessible_label: &'static str,
}
impl SearchEditor {
    pub(super) fn new(text: &str) -> Self {
        Self::configured(text, "Find an application", PLACEHOLDER, 17.0)
    }

    pub(super) fn configured(
        text: &str,
        hint: &'static str,
        accessible_label: &'static str,
        font_size: f32,
    ) -> Self {
        Self {
            // Preserve the upstream editor's soft wrapping for long queries.
            // Enter still submits; it never inserts a new line.
            text: TextArea::new_editable(text)
                .with_word_wrap(true)
                .with_style(xilem::masonry::parley::StyleProperty::FontSize(font_size)),
            placeholder: Label::new(hint)
                .with_style(xilem::masonry::parley::StyleProperty::FontSize(font_size)),
            composing: false,
            navigation: true,
            font_size,
            hint,
            accessible_label,
        }
    }

    /// Reuse the native editor without launcher-only arrow navigation.
    pub(super) fn text_input(text: &str, label: &'static str) -> Self {
        let mut input = Self::configured(text, label, label, 14.0);
        input.navigation = false;
        input
    }

    pub(super) fn reset_text(&mut self, text: &str) {
        // TextArea stores focus in Masonry's context, not in the editor. Keep
        // this widget's identity and placeholder while clearing external text
        // and any composition, matching TextArea::reset_text's semantics.
        self.text = TextArea::new_editable(text)
            .with_word_wrap(true)
            .with_style(xilem::masonry::parley::StyleProperty::FontSize(
                self.font_size,
            ));
        self.composing = false;
    }
}

impl xilem::masonry::core::UsesProperty<xilem::masonry::properties::ContentColor> for SearchEditor {}

impl Widget for SearchEditor {
    type Action = TextAction;
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Ime(event) = event {
            use xilem::masonry::core::Ime;
            match event {
                Ime::Preedit(text, _) => self.composing = !text.is_empty(),
                Ime::Commit(_) | Ime::Disabled => self.composing = false,
                _ => {}
            }
        }
        if let TextEvent::Keyboard(event) = event
            && self.navigation
            && !self.composing
            && !event.modifiers.ctrl()
            && !event.modifiers.alt()
            && !event.modifiers.meta()
            && !event.modifiers.shift()
            && let Key::Named(key @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) = event.key
        {
            if event.state.is_down() {
                ctx.submit_untyped_action(Box::new(SelectionStep(if key == NamedKey::ArrowDown {
                    1
                } else {
                    -1
                })));
            }
            ctx.set_handled();
            return;
        }
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
        self.placeholder.property_changed(ctx, property_type);
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
        if axis == Axis::Horizontal {
            match request {
                LenReq::MinContent => return Length::ZERO,
                LenReq::FitContent(space) => return space,
                LenReq::MaxContent => {}
            }
        }
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
        if !self.navigation || self.accessible_label == PLACEHOLDER {
            Role::TextInput
        } else {
            Role::SearchInput
        }
    }
    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        self.text.accessibility(ctx, props, node);
        node.set_label(self.accessible_label);
        node.set_placeholder(if self.accessible_label == PLACEHOLDER {
            PLACEHOLDER
        } else {
            self.hint
        });
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
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("SearchEditor", id = id.trace())
    }
}
