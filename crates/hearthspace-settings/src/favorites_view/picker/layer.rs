use super::{Choice, field::PickerField, option::AppOption};
use hearthspace_ui_components::{self as components, TouchScroll};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ActionCtx, ChildrenIds, ComposeCtx, ErasedAction, EventCtx, Layer, LayoutCtx,
        MeasureCtx, NoAction, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, QueryCtx,
        RegisterCtx, Widget, WidgetId, WidgetMut, WidgetPod, WidgetRef, find_widget_under_pointer,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Rect, Size},
    layout::{AsUnit, LayoutSize, LenReq, Length},
    parley::{FontFamily, FontFamilyName, StyleProperty},
    widgets::{ButtonPress, Label, Portal},
};

pub(super) struct PickerLayer {
    creator: WidgetId,
    editor: WidgetId,
    anchor: Rect,
    menu: WidgetPod<Menu>,
    choices: Vec<(WidgetId, String)>,
    bounds: Rect,
    reveal: Option<usize>,
}
impl PickerLayer {
    pub fn new(
        creator: WidgetId,
        editor: WidgetId,
        anchor: Rect,
        choices: &[Choice],
        selected: &str,
        active: usize,
    ) -> Self {
        let options: Vec<_> = choices
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                WidgetPod::new(AppOption::new(
                    editor,
                    choice,
                    choice.id == selected,
                    index == active,
                ))
            })
            .collect();
        let ids = options
            .iter()
            .zip(choices)
            .map(|(option, choice)| (option.id(), choice.id.clone()))
            .collect();
        let list = AppList {
            options,
            empty: Label::new("No apps found")
                .with_style(StyleProperty::FontSize(12.0))
                .with_style(FontFamily::Single(FontFamilyName::Named(
                    crate::typography::FONT.into(),
                )))
                .prepare()
                .with_class("settings-muted")
                .to_pod(),
        };
        let scroll = TouchScroll::new(
            Portal::new(list.prepare())
                .constrain_horizontal(true)
                .content_must_fill(true)
                .prepare(),
            true,
            false,
        );
        Self {
            creator,
            editor,
            anchor,
            choices: ids,
            bounds: Rect::ZERO,
            reveal: Some(active),
            menu: WidgetPod::new(Menu {
                scroll: WidgetPod::new(scroll),
            }),
        }
    }
    pub fn option_ids(&self) -> Vec<WidgetId> {
        self.choices.iter().map(|(id, _)| *id).collect()
    }
    pub fn menu_id(&self) -> WidgetId {
        self.menu.id()
    }
    pub fn set_anchor(this: &mut WidgetMut<'_, Self>, anchor: Rect) {
        if this.widget.anchor != anchor {
            this.widget.anchor = anchor;
            this.ctx.request_layout();
        }
    }
    pub fn highlight(this: &mut WidgetMut<'_, Self>, active: usize) {
        let mut menu = this.ctx.get_mut(&mut this.widget.menu);
        let mut scroll = menu.ctx.get_mut(&mut menu.widget.scroll);
        let mut portal = TouchScroll::portal_mut(&mut scroll);
        {
            let mut list = Portal::child_mut(&mut portal);
            for (index, option) in list.widget.options.iter_mut().enumerate() {
                let mut option = list.ctx.get_mut(option);
                option.widget.highlighted = index == active;
                option.ctx.request_paint_only();
            }
        }
        Portal::pan_viewport_to(
            &mut portal,
            Rect::new(0.0, active as f64 * 44.0, 1.0, (active + 1) as f64 * 44.0),
        );
    }
}
impl Widget for PickerLayer {
    type Action = NoAction;
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if action.is::<ButtonPress>()
            && let Some((_, app)) = self.choices.iter().find(|(id, _)| *id == source)
        {
            let app = app.clone();
            ctx.set_focus(self.editor);
            ctx.mutate_later(self.creator, move |mut creator| {
                PickerField::finish(&mut creator.downcast(), Some(app))
            });
            ctx.set_handled();
        }
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(event) = event {
            ctx.set_focus(self.editor);
            if !self
                .bounds
                .contains(ctx.local_position(event.state.position))
            {
                ctx.mutate_later(self.creator, |mut creator| {
                    PickerField::finish(&mut creator.downcast(), None)
                });
                ctx.set_handled();
            }
        }
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.menu);
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
        self.bounds = popup_bounds(self.anchor, size, self.choices.len());
        ctx.run_layout(&mut self.menu, self.bounds.size());
        ctx.place_child(&mut self.menu, self.bounds.origin());
        if let Some(active) = self.reveal.take() {
            ctx.mutate_self_later(move |mut layer| Self::highlight(&mut layer.downcast(), active));
        }
    }
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        ctx.mutate_later(self.creator, |mut creator| {
            PickerField::update_anchor(&mut creator.downcast())
        });
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.menu.id()])
    }
    fn find_widget_under_pointer<'c>(
        &'c self,
        ctx: QueryCtx<'c>,
        pos: Point,
    ) -> Option<WidgetRef<'c, dyn Widget>> {
        let point = ctx.to_local(pos);
        if self.anchor.contains(point) && !self.bounds.contains(point) {
            // Keep the field editable while the popup is open. Everywhere else the
            // layer receives outside clicks so dismissal cannot also activate a
            // control underneath it.
            None
        } else {
            find_widget_under_pointer(self, ctx, pos)
        }
    }
    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }
}
impl Layer for PickerLayer {
    fn capture_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if matches!(event, PointerEvent::Cancel(_)) {
            ctx.set_focus(self.editor);
            ctx.mutate_later(self.creator, |mut creator| {
                PickerField::finish(&mut creator.downcast(), None)
            });
            ctx.set_handled();
        }
    }
}

fn popup_bounds(anchor: Rect, window: Size, count: usize) -> Rect {
    let margin = 8.0_f64.min(window.width / 4.0).min(window.height / 4.0);
    let width = anchor.width().min((window.width - margin * 2.0).max(0.0));
    let desired = (count.max(1) as f64 * 44.0 + 8.0).min(272.0);
    let below = (window.height - margin - anchor.y1 - 6.0).max(0.0);
    let above = (anchor.y0 - 6.0 - margin).max(0.0);
    let bottom = below >= desired || below >= above;
    let height = desired.min(if bottom { below } else { above });
    let x = anchor
        .x0
        .clamp(margin, (window.width - margin - width).max(margin));
    let y = if bottom {
        anchor.y1 + 6.0
    } else {
        anchor.y0 - height - 6.0
    };
    Rect::from_origin_size(
        (
            x,
            y.clamp(margin, (window.height - height - margin).max(margin)),
        ),
        (width, height),
    )
}

struct Menu {
    scroll: WidgetPod<TouchScroll<AppList>>,
}
impl Widget for Menu {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.scroll);
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
            &mut self.scroll,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        )
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(
            &mut self.scroll,
            Size::new((size.width - 8.0).max(0.0), (size.height - 8.0).max(0.0)),
        );
        ctx.place_child(&mut self.scroll, Point::new(4.0, 4.0));
        ctx.set_paint_insets(24.0);
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
        components::settings_material::paint_surface_with_palette(
            painter,
            ctx.border_box(),
            14.0,
            components::SettingsSurface::Raised,
            &palette,
        );
    }
    fn accessibility_role(&self) -> Role {
        Role::ListBox
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label("Installed apps");
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.scroll.id()])
    }
}

struct AppList {
    options: Vec<WidgetPod<AppOption>>,
    empty: WidgetPod<Label>,
}
impl Widget for AppList {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for option in &mut self.options {
            ctx.register_child(option);
        }
        ctx.register_child(&mut self.empty);
    }
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        _: Option<Length>,
    ) -> Length {
        match (axis, request) {
            (Axis::Horizontal, LenReq::FitContent(space)) => space,
            (Axis::Horizontal, _) => 90.0.px(),
            _ => (self.options.len().max(1) as f64 * 44.0).px(),
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        for (index, option) in self.options.iter_mut().enumerate() {
            ctx.run_layout(option, Size::new(size.width, 44.0));
            ctx.place_child(option, Point::new(0.0, index as f64 * 44.0));
        }
        ctx.set_stashed(&mut self.empty, !self.options.is_empty());
        if self.options.is_empty() {
            ctx.run_layout(
                &mut self.empty,
                Size::new((size.width - 24.0).max(0.0), 20.0),
            );
            ctx.place_child(&mut self.empty, Point::new(12.0, 12.0));
        }
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        self.options
            .iter()
            .map(WidgetPod::id)
            .chain([self.empty.id()])
            .collect()
    }
}

pub(crate) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: hearthspace_ui_components::Palette,
) {
    properties.insert::<Menu, _>(palette);
}
