use crate::{Palette, settings_material::FocusIndicator};
use xilem::masonry::{
    accesskit::{Node, Role, Toggled},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
        Widget,
    },
    imaging::Painter,
    kurbo::{Axis, Size},
    layout::{AsUnit, LenReq, Length},
    widgets::{Slider, Switch, SwitchToggled},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Switch,
    Checkbox,
    Slider,
    Meter,
    Progress,
}

pub(super) struct ControlWidget<A = SwitchToggled> {
    action: std::marker::PhantomData<A>,
    pub(super) kind: ControlKind,
    pub(super) name: String,
    pub(super) value: f64,
    switch: Switch,
    slider: Slider,
    pub(super) focus: FocusIndicator,
}
impl<A> ControlWidget<A> {
    pub fn new(kind: ControlKind, name: &str, value: f64) -> Self {
        let value = super::view::normalized(value);
        Self {
            action: std::marker::PhantomData,
            kind,
            name: name.into(),
            value,
            switch: Switch::new(value != 0.),
            slider: Slider::new(0., 100., value).with_step(1.),
            focus: FocusIndicator::default(),
        }
    }
    pub(super) fn set_value(&mut self, kind: ControlKind, name: &str, value: f64) {
        self.kind = kind;
        self.name = name.into();
        self.value = value;
        self.switch = Switch::new(value != 0.);
        self.slider = Slider::new(0., 100., value).with_step(1.);
    }
    fn interactive(&self) -> bool {
        matches!(
            self.kind,
            ControlKind::Checkbox | ControlKind::Switch | ControlKind::Slider
        )
    }
}
impl<A: std::fmt::Debug + Send + 'static> Widget for ControlWidget<A> {
    type Action = A;
    fn accepts_focus(&self) -> bool {
        self.interactive()
    }
    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        e: &PointerEvent,
    ) {
        if ctx.is_disabled() {
            return;
        }
        self.focus.pointer(e);
        match self.kind {
            ControlKind::Switch | ControlKind::Checkbox => {
                self.switch.on_pointer_event(ctx, props, e)
            }
            ControlKind::Slider => self.slider.on_pointer_event(ctx, props, e),
            _ => {}
        }
        ctx.request_paint_only();
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        e: &TextEvent,
    ) {
        if ctx.is_disabled() {
            return;
        }
        self.focus.text(e);
        match self.kind {
            ControlKind::Switch | ControlKind::Checkbox => self.switch.on_text_event(ctx, props, e),
            ControlKind::Slider => self.slider.on_text_event(ctx, props, e),
            _ => {}
        }
        ctx.request_paint_only();
    }
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        e: &AccessEvent,
    ) {
        if ctx.is_disabled() {
            return;
        }
        match self.kind {
            ControlKind::Switch | ControlKind::Checkbox => {
                self.switch.on_access_event(ctx, props, e)
            }
            ControlKind::Slider => self.slider.on_access_event(ctx, props, e),
            _ => {}
        }
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, e: &Update) {
        self.focus.update(e);
        if matches!(e, Update::DisabledChanged(_)) {
            ctx.request_paint_only();
        }
        match self.kind {
            ControlKind::Switch | ControlKind::Checkbox => self.switch.update(ctx, props, e),
            ControlKind::Slider => self.slider.update(ctx, props, e),
            _ => {}
        }
    }
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        r: LenReq,
        _: Option<Length>,
    ) -> Length {
        match (self.kind, axis) {
            (ControlKind::Switch, Axis::Horizontal) => 40.0.px(),
            (ControlKind::Switch, _) => 24.0.px(),
            (ControlKind::Checkbox, _) => 20.0.px(),
            (_, Axis::Horizontal) => match r {
                LenReq::FitContent(v) => v,
                _ => 256.0.px(),
            },
            (ControlKind::Slider, _) => 32.0.px(),
            _ => 8.0.px(),
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {
        ctx.set_paint_insets(16.);
    }
    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, ty: std::any::TypeId) {
        Palette::prop_changed(ctx, ty);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<Palette>(ctx.property_cache());
        super::paint::control(self, ctx, palette, painter);
    }
    fn accessibility_role(&self) -> Role {
        match self.kind {
            ControlKind::Switch => Role::Switch,
            ControlKind::Checkbox => Role::CheckBox,
            ControlKind::Slider => Role::Slider,
            ControlKind::Meter => Role::Meter,
            ControlKind::Progress => Role::ProgressIndicator,
        }
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.name.clone());
        if matches!(self.kind, ControlKind::Switch | ControlKind::Checkbox) {
            node.set_toggled(if self.value != 0. {
                Toggled::True
            } else {
                Toggled::False
            });
            node.add_action(xilem::masonry::accesskit::Action::Click);
        } else {
            node.set_numeric_value(self.value);
            node.set_min_numeric_value(0.);
            node.set_max_numeric_value(100.);
            if self.kind == ControlKind::Slider {
                use xilem::masonry::accesskit::Action;
                node.set_numeric_value_step(1.);
                node.add_action(Action::SetValue);
                node.add_action(Action::Increment);
                node.add_action(Action::Decrement);
            }
        }
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
