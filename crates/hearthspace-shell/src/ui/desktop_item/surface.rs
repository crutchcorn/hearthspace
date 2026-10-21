//! Preserve paint room without widening the interactive/accessibility tile.
use super::{DesktopTile, HEIGHT, ITEM_HEIGHT, ITEM_WIDTH, MARGIN, WIDTH};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ActionCtx, ChildrenIds, ErasedAction, LayoutCtx, MeasureCtx, NewWidget,
        NoAction, PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, Widget, WidgetId, WidgetMut,
        WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{AsUnit, LenReq, Length},
};

#[derive(Debug)]
pub(super) struct NativeFocus(pub bool);

pub(crate) struct TileSurface {
    child: WidgetPod<DesktopTile>,
}

impl TileSurface {
    pub(super) fn new(child: NewWidget<DesktopTile>) -> Self {
        Self {
            child: child.to_pod(),
        }
    }

    pub(super) fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, DesktopTile> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for TileSurface {
    type Action = NoAction;

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if let Some(NativeFocus(focused)) = action.downcast_ref::<NativeFocus>() {
            if *focused {
                ctx.set_focus(source);
            } else if ctx.has_focus_target() {
                ctx.resign_focus();
            }
            ctx.set_handled();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        if axis == Axis::Horizontal {
            WIDTH.px()
        } else {
            HEIGHT.px()
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {
        ctx.run_layout(&mut self.child, Size::new(ITEM_WIDTH, ITEM_HEIGHT));
        ctx.place_child(&mut self.child, Point::new(MARGIN, MARGIN));
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn accepts_pointer_interaction(&self) -> bool {
        false
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
