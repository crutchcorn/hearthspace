//! A named, compact switch using Masonry's native input and accessibility behavior.
use hearthspace_ui_components as components;
use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx,
            PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent,
            Update, UpdateCtx, Widget,
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{AsUnit, LenReq, Length},
        widgets::{Switch, SwitchToggled},
    },
};

use super::toggle_row::RowContent;
use crate::state::SettingsApp;

mod paint;

pub(crate) fn toggle<F>(name: &'static str, on: bool, disabled: bool, callback: F) -> Toggle<F>
where
    F: Fn(&mut SettingsApp, bool) + Send + Sync + 'static,
{
    Toggle {
        name,
        on,
        disabled,
        row: false,
        description: None,
        callback,
    }
}

pub(crate) struct Toggle<F> {
    name: &'static str,
    on: bool,
    disabled: bool,
    row: bool,
    description: Option<&'static str>,
    callback: F,
}

impl<F> Toggle<F> {
    pub(super) fn with_row(mut self, description: Option<&'static str>) -> Self {
        self.row = true;
        self.description = description;
        self
    }
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
                focus: components::settings_material::FocusIndicator::default(),
                row: self
                    .row
                    .then(|| RowContent::new(self.name, self.description)),
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
        if self.name != previous.name || self.description != previous.description {
            let xilem::masonry::core::WidgetMut { widget, ctx, .. } = &mut element;
            if let Some(row) = &mut widget.row {
                row.set_text(ctx, self.name, self.description);
            }
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
    focus: components::settings_material::FocusIndicator,
    row: Option<RowContent>,
}

impl Widget for NamedSwitch {
    type Action = SwitchToggled;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.focus.pointer(event);
        self.inner.on_pointer_event(ctx, props, event);
        ctx.request_paint_only();
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.focus.text(event);
        self.inner.on_text_event(ctx, props, event);
        ctx.request_paint_only();
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
        self.focus.update(event);
        self.inner.update(ctx, props, event);
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        if self.row.is_some() && !ctx.is_disabled() {
            CursorIcon::Pointer
        } else {
            self.inner.get_cursor(ctx, pos)
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        if let Some(row) = &mut self.row {
            row.register_children(ctx);
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
        if let Some(row) = &mut self.row {
            return row.measure(ctx, axis, request, cross);
        }
        match axis {
            // Keep the 40×24 track compact while reserving room for its
            // outside focus ring, including in tightly measured rows.
            Axis::Horizontal => 44.0.px(),
            Axis::Vertical => 30.0.px(),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        if let Some(row) = &mut self.row {
            row.layout(ctx, size);
            ctx.set_paint_insets(24.0);
        } else {
            ctx.set_paint_insets(5.0);
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
        let bounds = ctx.content_box();
        let x = if self.row.is_some() {
            RowContent::paint(palette, ctx, painter);
            (bounds.width() - 53.0).max(0.0)
        } else {
            (bounds.width() - 40.0) / 2.0
        };
        let y = (bounds.height() - 24.0) / 2.0;
        paint::paint(
            palette,
            painter,
            x,
            y,
            self.inner.is_on(),
            ctx.is_disabled(),
            ctx.is_focus_target() && self.focus.visible(),
        );
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
        if let Some(row) = &self.row {
            row.accessibility(node);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        self.row
            .as_ref()
            .map_or_else(ChildrenIds::new, RowContent::children_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xilem::masonry::{
        accesskit::Toggled,
        core::{NewWidget, keyboard::Key},
        testing::TestHarness,
    };

    #[test]
    fn switch_focus_is_orange_and_preserves_checked_state_and_keyboard_action() {
        let mut harness = TestHarness::create_with_size(
            components::hearthspace_light_properties(),
            NewWidget::new(NamedSwitch {
                name: "Enable AI",
                inner: Switch::new(true),
                focus: components::settings_material::FocusIndicator::default(),
                row: None,
            }),
            (80, 44),
        );
        let id = harness.root_id();
        let idle = harness.render();
        harness.press_tab_key(false);
        assert_eq!(harness.focused_widget_id(), Some(id));
        let focused = harness.render();
        let [r, g, b, _] = components::FOCUS_RING.to_rgba8().to_u8_array();
        assert!(
            idle.pixels()
                .zip(focused.pixels())
                .any(|(before, after)| { before != after && after.0[..3] == [r, g, b] })
        );
        assert_eq!(
            harness.access_node(id).unwrap().toggled(),
            Some(Toggled::True)
        );
        harness.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert_eq!(
            harness.pop_action::<SwitchToggled>(),
            Some((SwitchToggled(false), id))
        );
    }

    #[test]
    fn off_track_keeps_its_translucent_fill_and_disabled_state_reduces_it() {
        use xilem::masonry::properties::Background;
        let mut harness = TestHarness::create_with_size(
            components::hearthspace_opaque_properties(),
            NewWidget::new(NamedSwitch {
                name: "Enable AI",
                inner: Switch::new(false),
                focus: components::settings_material::FocusIndicator::default(),
                row: None,
            })
            .with_props(Background::Color(components::SURFACE)),
            (80, 44),
        );
        let idle = harness.render();
        // Right side of the off track, clear of the thumb, border and shadow.
        let pixel = idle.get_pixel(52, 22).0;
        assert!(
            pixel[0] > 180 && pixel[1] > 180 && pixel[2] > 180,
            "the25%-alpha off track must not become opaque dark green"
        );
        harness.edit_root_widget(|mut root| root.ctx.set_disabled(true));
        let disabled = harness.render().get_pixel(52, 22).0;
        assert!(
            disabled[0] > pixel[0] && disabled[1] > pixel[1] && disabled[2] > pixel[2],
            "disabled opacity multiplies the existing track alpha"
        );
    }
}
