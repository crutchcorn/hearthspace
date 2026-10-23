//! Editable combobox text: Masonry retains text selection, clipboard and IME.
use std::any::TypeId;
use xilem::masonry::{
    accesskit::{Action, Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, Ime, LayoutCtx, MeasureCtx,
        PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent,
        Update, UpdateCtx, Widget, WidgetId, WidgetPod,
        keyboard::{Key, NamedKey},
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{LenReq, Length},
    parley::{FontFamily, FontFamilyName, StyleProperty},
    widgets::{Label, TextAction, TextArea},
};

#[derive(Debug)]
pub(crate) enum Navigation {
    Focus,
    Step(i32),
    Expand,
    Collapse,
}

pub(crate) struct Editor {
    pub text: TextArea<true>,
    placeholder: WidgetPod<Label>,
    composing: bool,
    pub expanded: bool,
    pub menu: Option<WidgetId>,
    pub active: Option<WidgetId>,
}

fn text_area(text: &str) -> TextArea<true> {
    TextArea::new_editable(text)
        .with_word_wrap(false)
        .with_style(StyleProperty::FontSize(12.0))
        .with_style(FontFamily::Single(FontFamilyName::Named(
            crate::typography::FONT.into(),
        )))
}

impl Editor {
    pub fn new(text: &str) -> Self {
        Self {
            text: text_area(text),
            placeholder: Label::new("Search apps")
                .with_style(StyleProperty::FontSize(12.0))
                .with_style(FontFamily::Single(FontFamilyName::Named(
                    crate::typography::FONT.into(),
                )))
                .accessibility_hidden(true)
                .prepare()
                .with_class("settings-muted")
                .to_pod(),
            composing: false,
            expanded: false,
            menu: None,
            active: None,
        }
    }

    pub fn reset_text(&mut self, text: &str) {
        self.text = text_area(text);
        self.composing = false;
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
        if let TextEvent::Ime(event) = event {
            match event {
                Ime::Preedit(text, _) => self.composing = !text.is_empty(),
                Ime::Commit(_) | Ime::Disabled => self.composing = false,
                _ => {}
            }
        }
        if let TextEvent::Keyboard(event) = event
            && !self.composing
            && !event.modifiers.ctrl()
            && !event.modifiers.alt()
            && !event.modifiers.meta()
            && !event.modifiers.shift()
            && let Key::Named(key @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) = event.key
        {
            if event.state.is_down() {
                ctx.submit_untyped_action(Box::new(Navigation::Step(
                    if key == NamedKey::ArrowDown { 1 } else { -1 },
                )));
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
        match event.action {
            Action::Expand => ctx.submit_untyped_action(Box::new(Navigation::Expand)),
            Action::Collapse => ctx.submit_untyped_action(Box::new(Navigation::Collapse)),
            _ => self.text.on_access_event(ctx, props, event),
        }
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
    }
    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        self.text.property_changed(ctx, property_type);
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.placeholder);
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        props: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        if axis == Axis::Horizontal && matches!(request, LenReq::MinContent) {
            return Length::ZERO;
        }
        self.text.measure(ctx, props, axis, request, cross)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, props: &PropertiesRef<'_>, size: Size) {
        self.text.layout(ctx, props, size);
        ctx.set_stashed(&mut self.placeholder, !self.text.is_empty());
        if self.text.is_empty() {
            ctx.run_layout(&mut self.placeholder, size);
            ctx.place_child(&mut self.placeholder, Point::ORIGIN);
        }
        ctx.set_clip_path(size.to_rect());
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        self.text.paint(ctx, props, painter);
    }
    fn accessibility_role(&self) -> Role {
        Role::EditableComboBox
    }
    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        self.text.accessibility(ctx, props, node);
        // The text layout stays centered for caret/IME geometry; assistive
        // controls expose the full-height input area around that line.
        let text_bounds = ctx.border_box();
        let center_y = text_bounds.center().y;
        node.set_bounds(xilem::masonry::accesskit::Rect::new(
            text_bounds.x0,
            center_y - 22.0,
            text_bounds.x1,
            center_y + 22.0,
        ));
        node.set_label("Open this app");
        node.set_placeholder("Search apps");
        node.set_expanded(self.expanded);
        node.add_action(Action::Expand);
        node.add_action(Action::Collapse);
        if let Some(menu) = self.menu {
            node.set_controls([menu.to_raw().into()]);
        }
        if let Some(active) = self.active {
            node.set_active_descendant(active.to_raw().into());
        }
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
        ChildrenIds::from_slice(&[self.placeholder.id()])
    }
}
