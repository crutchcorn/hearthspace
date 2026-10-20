use super::{button::SettingsSelectButton, option::SelectOption};
use hearthspace_ui_components as components;
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, ComposeCtx, EventCtx, Layer, LayoutCtx, MeasureCtx, NoAction,
        PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Widget, WidgetId,
        WidgetMut, WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Rect, Size},
    layout::{AsUnit, LenReq, Length},
};

const WINDOW_MARGIN: f64 = 8.0;
const MENU_PADDING: f64 = 4.0;
const ROW_HEIGHT: f64 = 34.0;

pub(super) struct SettingsSelectLayer {
    creator: WidgetId,
    anchor: Rect,
    menu: WidgetPod<SelectMenu>,
    option_ids: Vec<WidgetId>,
    menu_bounds: Rect,
}

impl SettingsSelectLayer {
    pub fn new(
        creator: WidgetId,
        anchor: Rect,
        name: String,
        labels: &[String],
        selected: usize,
    ) -> Self {
        let options: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                WidgetPod::new(SelectOption::new(
                    creator,
                    index,
                    label.clone(),
                    selected == index,
                ))
            })
            .collect();
        let option_ids = options.iter().map(WidgetPod::id).collect();
        Self {
            creator,
            anchor,
            menu: WidgetPod::new(SelectMenu { name, options }),
            option_ids,
            menu_bounds: Rect::ZERO,
        }
    }

    pub fn option_ids(&self) -> Vec<WidgetId> {
        self.option_ids.clone()
    }

    pub fn set_anchor(this: &mut WidgetMut<'_, Self>, anchor: Rect) {
        if this.widget.anchor != anchor {
            this.widget.anchor = anchor;
            this.ctx.request_layout();
        }
    }

    pub fn highlight(this: &mut WidgetMut<'_, Self>, active: usize) {
        let mut menu = this.ctx.get_mut(&mut this.widget.menu);
        for (index, option) in menu.widget.options.iter_mut().enumerate() {
            let mut option = menu.ctx.get_mut(option);
            option.widget.highlighted = index == active;
            option.ctx.request_paint_only();
        }
    }
}

impl Widget for SettingsSelectLayer {
    type Action = NoAction;
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Capture hooks precede Masonry's normal pointer-down focus clearing.
        // Restore the owner here too, after that clearing, including outside
        // clicks which dismiss this full-window layer.
        if matches!(event, PointerEvent::Down(_)) {
            ctx.set_focus(self.creator);
            ctx.set_handled();
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
        self.menu_bounds = popup_bounds(self.anchor, size, self.option_ids.len());
        ctx.run_layout(&mut self.menu, self.menu_bounds.size());
        ctx.place_child(&mut self.menu, self.menu_bounds.origin());
    }
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        // A resize can move the opener without changing its own size. Masonry
        // updates that transform without necessarily calling its compose hook.
        // Read its final window bounds after this full-window layer composes.
        ctx.mutate_later(self.creator, |mut creator| {
            SettingsSelectButton::update_anchor(&mut creator.downcast());
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
    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }
}

impl Layer for SettingsSelectLayer {
    fn capture_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let dismiss = match event {
            PointerEvent::Down(event) => !self
                .menu_bounds
                .contains(ctx.local_position(event.state.position)),
            PointerEvent::Cancel(_) => true,
            _ => false,
        };
        if dismiss {
            ctx.set_focus(self.creator);
            ctx.mutate_later(self.creator, |mut creator| {
                SettingsSelectButton::finish(&mut creator.downcast(), None);
            });
            ctx.set_handled();
        }
    }
}

fn popup_bounds(anchor: Rect, window: Size, count: usize) -> Rect {
    let margin = WINDOW_MARGIN
        .min(window.width / 4.0)
        .min(window.height / 4.0);
    let width = anchor
        .width()
        .max(200.0)
        .min((window.width - margin * 2.0).max(0.0));
    let height = (ROW_HEIGHT * count as f64 + MENU_PADDING * 2.0)
        .min((window.height - margin * 2.0).max(0.0));
    let x = (anchor.x1 - width).clamp(margin, (window.width - margin - width).max(margin));
    let below = anchor.y1 + 4.0;
    let y = if below + height <= window.height - margin {
        below
    } else {
        anchor.y0 - height - 4.0
    };
    let y = y.clamp(margin, (window.height - margin - height).max(margin));
    Rect::from_origin_size((x, y), (width, height))
}

struct SelectMenu {
    name: String,
    options: Vec<WidgetPod<SelectOption>>,
}

impl Widget for SelectMenu {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for option in &mut self.options {
            ctx.register_child(option);
        }
    }
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Horizontal => 200.0.px(),
            Axis::Vertical => (ROW_HEIGHT * self.options.len() as f64 + MENU_PADDING * 2.0).px(),
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        let padding = MENU_PADDING.min(size.width / 4.0).min(size.height / 4.0);
        ctx.set_paint_insets(32.0);
        let row = (size.height - padding * 2.0) / self.options.len() as f64;
        for (index, option) in self.options.iter_mut().enumerate() {
            ctx.run_layout(
                option,
                Size::new((size.width - padding * 2.0).max(0.0), row),
            );
            ctx.place_child(option, Point::new(padding, padding + row * index as f64));
        }
    }
    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        components::settings_material::paint_surface(
            painter,
            ctx.border_box(),
            components::RADIUS_CONTROL,
            components::SettingsSurface::Raised,
        );
    }
    fn accessibility_role(&self) -> Role {
        Role::ListBox
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(format!("{} options", self.name));
    }
    fn children_ids(&self) -> ChildrenIds {
        self.options.iter().map(WidgetPod::id).collect()
    }
}
