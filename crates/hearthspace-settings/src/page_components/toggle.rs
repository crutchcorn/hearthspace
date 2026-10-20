//! A named, compact switch using Masonry's native input and accessibility behavior.
use hearthspace_ui_components as components;
use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
            PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
            Widget,
        },
        imaging::Painter,
        kurbo::{Axis, Circle, Rect, Size, Stroke},
        layout::{AsUnit, LenReq, Length},
        widgets::{Switch, SwitchToggled},
    },
};

use crate::state::SettingsApp;

pub(crate) fn toggle<F>(name: &'static str, on: bool, disabled: bool, callback: F) -> Toggle<F>
where
    F: Fn(&mut SettingsApp, bool) + Send + Sync + 'static,
{
    Toggle {
        name,
        on,
        disabled,
        callback,
    }
}

pub(crate) struct Toggle<F> {
    name: &'static str,
    on: bool,
    disabled: bool,
    callback: F,
}

impl<F> ViewMarker for Toggle<F> {}

impl<F> View<SettingsApp, (), ViewCtx> for Toggle<F>
where
    F: Fn(&mut SettingsApp, bool) + Send + Sync + 'static,
{
    type Element = Pod<NamedSwitch>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut SettingsApp) -> (Self::Element, ()) {
        let element = ctx.with_action_widget(|ctx| {
            let mut element = ctx.create_pod(NamedSwitch {
                name: self.name,
                inner: Switch::new(self.on),
            });
            element.new_widget.options.disabled = self.disabled;
            element
        });
        (element, ())
    }

    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut SettingsApp,
    ) {
        if self.on != previous.on || self.name != previous.name {
            element.widget.inner = Switch::new(self.on);
            element.widget.name = self.name;
            element.ctx.request_render();
        }
        if self.disabled != previous.disabled {
            element.ctx.set_disabled(self.disabled);
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
        if let Some(action) = message.take_message::<SwitchToggled>() {
            if !self.disabled {
                (self.callback)(state, action.0);
            }
            MessageResult::Action(())
        } else {
            MessageResult::Stale
        }
    }
}

pub(crate) struct NamedSwitch {
    name: &'static str,
    inner: Switch,
}

impl Widget for NamedSwitch {
    type Action = SwitchToggled;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.inner.on_pointer_event(ctx, props, event);
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.inner.on_text_event(ctx, props, event);
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        self.inner.on_access_event(ctx, props, event);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        self.inner.update(ctx, props, event);
    }

    fn accepts_focus(&self) -> bool {
        true
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
        match axis {
            Axis::Horizontal => 40.0.px(),
            Axis::Vertical => 24.0.px(),
        }
    }

    fn layout(&mut self, _: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {}

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let bounds = ctx.content_box();
        let x = (bounds.width() - 40.0) / 2.0;
        let y = (bounds.height() - 24.0) / 2.0;
        let track = Rect::new(x + 0.5, y + 0.5, x + 39.5, y + 23.5).to_rounded_rect(12.0);
        let on = self.inner.is_on();
        let alpha = if ctx.is_disabled() { 0.5 } else { 1.0 };
        painter
            .fill(
                track,
                if on {
                    components::ACCENT
                } else {
                    components::CONTROL_TRACK
                }
                .with_alpha(alpha),
            )
            .draw();
        painter
            .stroke(track, &Stroke::new(1.0), components::BORDER)
            .draw();
        painter
            .fill(
                Circle::new((x + if on { 28.0 } else { 12.0 }, y + 12.0), 8.0),
                components::CONTROL_THUMB.with_alpha(alpha),
            )
            .draw();
        if ctx.is_focus_target() {
            painter
                .stroke(
                    Rect::new(x + 1.0, y + 1.0, x + 39.0, y + 23.0).to_rounded_rect(11.0),
                    &Stroke::new(2.0),
                    components::TEXT,
                )
                .draw();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Switch
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        self.inner.accessibility(ctx, props, node);
        node.set_label(self.name);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
