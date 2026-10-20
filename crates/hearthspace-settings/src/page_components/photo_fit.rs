//! Photo fitting is a native combobox backed by a separate Masonry layer.
use hearthspace_ipc::BackgroundMode;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{layout::AsUnit, properties::Padding},
    style::Style,
    view::{flex_row, label},
};

use crate::{actions::save_background_mode, state::SettingsApp};

mod button;
mod layer;
mod option;
use button::PhotoFitButton;

pub(super) const MODES: [(BackgroundMode, &str); 4] = [
    (BackgroundMode::RepeatWithCanvas, "Repeat with canvas"),
    (BackgroundMode::Stretch, "Stretch"),
    (BackgroundMode::Contains, "Contain"),
    (BackgroundMode::Cover, "Cover"),
];

pub(crate) fn photo_fit(selected: BackgroundMode, disabled: bool) -> impl WidgetView<SettingsApp> {
    let selected = MODES
        .iter()
        .position(|(mode, _)| *mode == selected)
        .unwrap();
    PhotoFit {
        selected,
        disabled,
        child: flex_row((
            label(MODES[selected].1).text_size(11.0),
            icon(Icon::ChevronDown, 12.0).color(components::TEXT_MUTED),
        ))
        .gap(8.0.px()),
    }
    .height(34.0.px())
    .padding(Padding::from_vh(6.0.px(), 8.0.px()))
}

struct PhotoFit<V> {
    selected: usize,
    disabled: bool,
    child: V,
}

const CONTENT: ViewId = ViewId::new(0);
impl<V> ViewMarker for PhotoFit<V> {}
impl<V: WidgetView<SettingsApp>> View<SettingsApp, (), ViewCtx> for PhotoFit<V> {
    type Element = Pod<PhotoFitButton>;
    type ViewState = V::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(CONTENT, |ctx| self.child.build(ctx, state));
        let element = ctx.with_action_widget(|ctx| {
            let mut element = ctx.create_pod(PhotoFitButton::new(self.selected, child.new_widget));
            element.new_widget.options.disabled = self.disabled;
            element
        });
        (element, view_state)
    }

    fn rebuild(
        &self,
        previous: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) {
        if self.selected != previous.selected || self.disabled {
            PhotoFitButton::dismiss(&mut element);
            element.widget.selected = self.selected;
            element.ctx.request_accessibility_update();
        }
        if self.disabled != previous.disabled {
            element.ctx.set_disabled(self.disabled);
        }
        ctx.with_id(CONTENT, |ctx| {
            self.child.rebuild(
                &previous.child,
                view_state,
                ctx,
                PhotoFitButton::child_mut(&mut element).downcast(),
                state,
            );
        });
        // Other page content may reflow while this control keeps the same size.
        // Refresh an open popup's anchor once the new transforms are composed.
        element.ctx.request_compose();
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        // The attached layer is also removed by Masonry when this widget leaves
        // the tree. Explicit dismissal updates its registration before teardown.
        PhotoFitButton::dismiss(&mut element);
        ctx.with_id(CONTENT, |ctx| {
            self.child.teardown(
                view_state,
                ctx,
                PhotoFitButton::child_mut(&mut element).downcast(),
            );
        });
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CONTENT) => self.child.message(
                view_state,
                message,
                PhotoFitButton::child_mut(&mut element).downcast(),
                state,
            ),
            None => {
                if let Some(mode) = message.take_message::<BackgroundMode>() {
                    if !self.disabled {
                        save_background_mode(state, *mode);
                    }
                    MessageResult::Action(())
                } else {
                    MessageResult::Stale
                }
            }
            _ => MessageResult::Stale,
        }
    }
}
