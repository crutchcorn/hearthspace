use super::{
    Artwork, ITEM_HEIGHT, ITEM_WIDTH, TileAction, TileContent, TileState, label::Caption, paint,
    surface::NativeFocus,
};
use hearthspace_ui_components::{
    Palette,
    icons::{Icon, IconGlyph},
};
use xilem::masonry::{
    accesskit::{Action, Node, Role, Toggled},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
        Widget, WidgetMut,
    },
    imaging::Painter,
    kurbo::{Axis, Size},
    layout::{AsUnit, LenReq, Length},
};

pub(crate) struct DesktopTile {
    pub(super) content: TileContent,
    pub(super) state: TileState,
    pub(super) caption: Caption,
    pub(super) glyph: IconGlyph,
}

impl DesktopTile {
    pub(super) fn new(content: TileContent, state: TileState) -> Self {
        let glyph = glyph(&content.artwork, state.photo_background);
        Self {
            content,
            state,
            caption: Caption::default(),
            glyph,
        }
    }

    pub(super) fn set(this: &mut WidgetMut<'_, Self>, content: TileContent, state: TileState) {
        if this.widget.content.name != content.name {
            this.widget.caption.invalidate();
            this.ctx.request_layout();
        }
        if this.widget.content.artwork != content.artwork
            || this.widget.state.photo_background != state.photo_background
        {
            this.widget.glyph = glyph(&content.artwork, state.photo_background);
        }
        if this.widget.state.focused != state.focused {
            this.ctx
                .submit_untyped_action(Box::new(NativeFocus(state.focused)));
        }
        this.widget.content = content;
        this.widget.state = state;
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }
}

fn glyph(artwork: &Artwork, filled: bool) -> IconGlyph {
    let icon = match artwork {
        Artwork::Settings => Icon::AppSettings,
        Artwork::Folder => Icon::Folder,
        Artwork::File => Icon::FileText,
        Artwork::App(_) => Icon::AppsGrid,
    };
    IconGlyph::new(icon, filled)
}

impl Widget for DesktopTile {
    type Action = TileAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if matches!(event, PointerEvent::Down(_)) {
            ctx.request_focus();
        }
        // Selection, dragging, double-click and key activation are compositor-owned.
    }

    fn on_text_event(&mut self, _: &mut EventCtx<'_>, _: &mut PropertiesMut<'_>, _: &TextEvent) {}

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        match event.action {
            Action::Click => ctx.submit_action::<Self::Action>(TileAction::Activate),
            Action::Focus => {
                ctx.request_focus();
                ctx.submit_action::<Self::Action>(TileAction::Focus);
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded if self.state.focused => {
                ctx.submit_untyped_action(Box::new(NativeFocus(true)))
            }
            Update::FontsChanged => {
                self.caption.invalidate();
                ctx.request_layout();
            }
            Update::HoveredChanged(_) | Update::FocusChanged(_) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property: std::any::TypeId) {
        Palette::prop_changed(ctx, property);
    }
    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        if axis == Axis::Horizontal {
            ITEM_WIDTH.px()
        } else {
            ITEM_HEIGHT.px()
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {
        let (font, layout) = ctx.text_contexts();
        self.caption.build(font, layout, &self.content.name);
        ctx.set_paint_insets(6.0);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<Palette>(ctx.property_cache());
        paint::tile(
            painter,
            self,
            &palette,
            ctx.is_hovered() || self.state.hovered,
        );
    }
    fn accessibility_role(&self) -> Role {
        Role::Button
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.content.name.clone());
        node.set_toggled(if self.state.selected {
            Toggled::True
        } else {
            Toggled::False
        });
        node.add_action(Action::Click);
        node.add_action(Action::Focus);
    }
    fn accepts_focus(&self) -> bool {
        true
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
    fn get_debug_text(&self) -> Option<String> {
        Some(self.caption.displayed.clone())
    }
}
