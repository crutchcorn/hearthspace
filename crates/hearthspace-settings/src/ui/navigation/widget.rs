use super::{HEADER_HEIGHT, MENU_HEIGHT, NavigationMenu};
use hearthspace_ui_components::{self as components, settings_material::RestoreFocus};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
        NewWidget, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent,
        Widget, WidgetId, WidgetMut, WidgetPod,
        keyboard::{Key, NamedKey},
    },
    imaging::Painter,
    kurbo::{Axis, Point, Rect, Size},
    layout::{LenReq, Length},
};

#[derive(Debug)]
pub(crate) struct Close;

pub(crate) struct NavigationLayout {
    page: WidgetPod<dyn Widget>,
    header: WidgetPod<dyn Widget>,
    pub(super) navigation: WidgetPod<NavigationMenu>,
    compact: bool,
    open: bool,
    header_control: Option<WidgetId>,
    current_category: Option<WidgetId>,
    category_controls: Vec<WidgetId>,
    menu_bounds: Rect,
    keyboard: bool,
}
impl NavigationLayout {
    pub(super) fn new(
        page: NewWidget<impl Widget + ?Sized>,
        header: NewWidget<impl Widget + ?Sized>,
        navigation: NewWidget<NavigationMenu>,
        compact: bool,
        open: bool,
    ) -> Self {
        Self {
            page: page.erased().to_pod(),
            header: header.erased().to_pod(),
            navigation: navigation.to_pod(),
            compact,
            open,
            header_control: None,
            current_category: None,
            category_controls: Vec::new(),
            menu_bounds: Rect::ZERO,
            keyboard: true,
        }
    }
    pub(super) fn page_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.page)
    }
    pub(super) fn header_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.header)
    }
    pub(super) fn with_navigation<R>(
        this: &mut WidgetMut<'_, Self>,
        f: impl FnOnce(WidgetMut<'_, dyn Widget>) -> R,
    ) -> R {
        let mut menu = this.ctx.get_mut(&mut this.widget.navigation);
        f(NavigationMenu::child_mut(&mut menu))
    }
    pub(super) fn configure(this: &mut WidgetMut<'_, Self>, compact: bool, open: bool) {
        if this.widget.compact == compact && this.widget.open == open {
            return;
        }
        let header_focused = this
            .ctx
            .get_mut(&mut this.widget.header)
            .ctx
            .has_focus_target();
        let nav_focused = this
            .ctx
            .get_mut(&mut this.widget.navigation)
            .ctx
            .has_focus_target();
        let restore_header = compact && ((!open && this.widget.open) || (!open && nav_focused));
        let restore_nav = !compact && header_focused;
        this.widget.compact = compact;
        this.widget.open = open;
        this.ctx.set_stashed(&mut this.widget.header, !compact);
        this.ctx
            .set_stashed(&mut this.widget.navigation, compact && !open);
        {
            let mut menu = this.ctx.get_mut(&mut this.widget.navigation);
            menu.widget.compact = compact;
            menu.ctx.request_layout();
            menu.ctx.request_paint_only();
        }
        this.ctx.children_changed();
        this.ctx.request_layout();
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
        let target = if restore_header {
            this.widget.header_control
        } else if restore_nav {
            this.widget.current_category
        } else {
            None
        };
        if let Some(target) = target {
            let keyboard = this.widget.keyboard;
            this.ctx.mutate_later(target, move |mut control| {
                control
                    .ctx
                    .submit_untyped_action(Box::new(RestoreFocus { keyboard }));
            });
        }
    }
}
impl Widget for NavigationLayout {
    type Action = Close;
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if matches!(event, PointerEvent::Down(_)) {
            self.keyboard = false;
        }
        if self.compact
            && self.open
            && let PointerEvent::Down(event) = event
            && self
                .menu_bounds
                .contains(ctx.local_position(event.state.position))
            && !self.category_controls.contains(&ctx.target())
        {
            // Browser clicks on nonfocusable navigation padding keep the
            // current control focused. Preserve that here so Escape still has
            // a target, without overriding category-button focus or page hits.
            if let Some(target) = ctx.focus_target_id().or(self.header_control) {
                ctx.set_focus(target);
            }
        }
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if matches!(event, TextEvent::Keyboard(_)) {
            self.keyboard = true;
        }
        if self.compact
            && self.open
            && let TextEvent::Keyboard(key) = event
            && key.key == Key::Named(NamedKey::Escape)
            && key.state.is_down()
        {
            ctx.submit_action::<Close>(Close);
            ctx.set_handled();
        }
    }
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if let Some(identity) = action.downcast_ref::<crate::page_components::NavigationControl>() {
            if identity.controls == Some(self.navigation.id()) {
                self.header_control = Some(source);
            } else if let Some(current) = identity.current_page {
                if !self.category_controls.contains(&source) {
                    self.category_controls.push(source);
                }
                if current {
                    self.current_category = Some(source);
                } else if self.current_category == Some(source) {
                    self.current_category = None;
                }
            }
            ctx.set_handled();
        } else if let Some(input) = action.downcast_ref::<crate::page_components::NavigationInput>()
        {
            self.keyboard = input.keyboard;
            ctx.set_handled();
        } else if action.is::<RestoreFocus>() {
            ctx.set_focus(source);
            ctx.set_handled();
        }
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.page);
        ctx.register_child(&mut self.header);
        ctx.register_child(&mut self.navigation);
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        ctx.context_size().length(axis).unwrap_or(Length::ZERO)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.set_stashed(&mut self.header, !self.compact);
        ctx.set_stashed(&mut self.navigation, self.compact && !self.open);
        let page_origin = if self.compact {
            Point::new(0.0, HEADER_HEIGHT)
        } else {
            Point::new(160.0, 0.0)
        };
        ctx.run_layout(
            &mut self.page,
            Size::new(
                (size.width - page_origin.x).max(0.0),
                (size.height - page_origin.y).max(0.0),
            ),
        );
        ctx.place_child(&mut self.page, page_origin);
        if self.compact {
            ctx.run_layout(&mut self.header, Size::new(size.width, HEADER_HEIGHT));
            ctx.place_child(&mut self.header, Point::ORIGIN);
        }
        if !self.compact || self.open {
            let nav_size = if self.compact {
                Size::new(
                    size.width,
                    MENU_HEIGHT.min((size.height - HEADER_HEIGHT).max(0.0)),
                )
            } else {
                Size::new(160.0, size.height)
            };
            let origin = if self.compact {
                Point::new(0.0, HEADER_HEIGHT)
            } else {
                Point::ORIGIN
            };
            self.menu_bounds = Rect::from_origin_size(origin, nav_size);
            ctx.run_layout(&mut self.navigation, nav_size);
            ctx.place_child(&mut self.navigation, origin);
        }
    }
    fn property_changed(
        &mut self,
        ctx: &mut xilem::masonry::core::UpdateCtx<'_>,
        property_type: std::any::TypeId,
    ) {
        components::Palette::prop_changed(ctx, property_type);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<components::Palette>(ctx.property_cache());
        if self.compact {
            painter
                .fill(
                    Rect::new(0.0, 0.0, ctx.border_box().width(), HEADER_HEIGHT),
                    palette.surface_subtle,
                )
                .draw();
            painter
                .fill(
                    Rect::new(
                        0.0,
                        HEADER_HEIGHT - 1.0,
                        ctx.border_box().width(),
                        HEADER_HEIGHT,
                    ),
                    palette.border,
                )
                .draw();
        }
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        // Reading order remains logical even while the navigation paints above
        // the page. Stashed descendants remain hidden to assistive technology.
        if !self.compact {
            node.set_children([self.navigation.id().into(), self.page.id().into()]);
        } else if self.open {
            node.set_children([
                self.header.id().into(),
                self.navigation.id().into(),
                self.page.id().into(),
            ]);
        } else {
            node.set_children([self.header.id().into(), self.page.id().into()]);
        }
    }
    fn children_ids(&self) -> ChildrenIds {
        if !self.compact {
            ChildrenIds::from_slice(&[self.navigation.id(), self.page.id(), self.header.id()])
        } else if self.open {
            ChildrenIds::from_slice(&[self.page.id(), self.header.id(), self.navigation.id()])
        } else {
            ChildrenIds::from_slice(&[self.header.id(), self.page.id(), self.navigation.id()])
        }
    }
}
