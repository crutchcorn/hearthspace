//! Native button state styling without local properties masking hover or focus.
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        core::{DefaultProperties, PropertyStack, Selector},
        properties::{Background, BorderColor},
        widgets::Button,
    },
};

const CONTROL: &str = "hearthspace-control";
const SUBTLE: &str = "hearthspace-subtle-control";
const PRIMARY: &str = "hearthspace-primary-control";
const SELECTED: &str = "hearthspace-selected";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonSurface {
    Bare,
    Control,
    Subtle,
    Raised,
    Primary,
}

impl ButtonSurface {
    fn class(self) -> &'static str {
        match self {
            Self::Bare => "hearthspace-bare-control",
            Self::Control => CONTROL,
            Self::Subtle => SUBTLE,
            Self::Raised => crate::TRANSLUCENT_FOCUS_CLASS,
            Self::Primary => PRIMARY,
        }
    }
}

/// Apply after sizing a native button. Do not set a local background or border
/// color: those override Masonry's hover and focus selectors.
pub fn button_surface<V>(inner: V, surface: ButtonSurface, selected: bool) -> ButtonSurfaceView<V> {
    ButtonSurfaceView {
        inner,
        surface,
        selected,
    }
}

pub struct ButtonSurfaceView<V> {
    inner: V,
    surface: ButtonSurface,
    selected: bool,
}

impl<V> ViewMarker for ButtonSurfaceView<V> {}
impl<State: 'static, Action: 'static, V: WidgetView<State, Action>> View<State, Action, ViewCtx>
    for ButtonSurfaceView<V>
{
    type Element = Pod<V::Widget>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (mut element, view_state) = self.inner.build(ctx, state);
        element.new_widget = element.new_widget.with_class(self.surface.class());
        if self.selected {
            element.new_widget = element.new_widget.with_class(SELECTED);
        }
        (element, view_state)
    }

    fn rebuild(
        &self,
        previous: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        self.inner.rebuild(
            &previous.inner,
            view_state,
            ctx,
            element.reborrow_mut(),
            state,
        );
        if self.surface != previous.surface {
            element.ctx.remove_class(previous.surface.class());
            element.ctx.add_class(self.surface.class());
        }
        if self.selected != previous.selected {
            if self.selected {
                element.ctx.add_class(SELECTED);
            } else {
                element.ctx.remove_class(SELECTED);
            }
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        self.inner.teardown(view_state, ctx, element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        self.inner.message(view_state, message, element, state)
    }
}

pub(crate) fn properties(
    properties: &mut DefaultProperties,
    palette: &crate::Palette,
    focus_ring: xilem::Color,
) {
    let mut stack = PropertyStack::new();
    stack.push_layer(
        Selector::classes(&[CONTROL]),
        (
            Background::Color(palette.surface_control),
            BorderColor {
                color: palette.border,
            },
        ),
    );
    stack.push_layer(
        Selector::classes(&[SUBTLE]),
        (
            Background::Color(palette.surface_subtle),
            BorderColor {
                color: palette.border,
            },
        ),
    );
    stack.push_layer(
        Selector::new().with_hovered(true),
        Background::Color(palette.button_hover),
    );
    stack.push_layer(
        Selector::classes(&[crate::TRANSLUCENT_FOCUS_CLASS]).with_hovered(true),
        Background::Color(palette.raised_button_hover),
    );
    stack.push_layer(
        Selector::classes(&[SELECTED]),
        (
            Background::Color(palette.accent_soft),
            BorderColor {
                color: palette.accent_border,
            },
        ),
    );
    stack.push_layer(
        Selector::classes(&[CONTROL, SELECTED]),
        BorderColor {
            color: palette.accent_strong,
        },
    );
    stack.push_layer(
        Selector::new().with_active(true),
        Background::Color(palette.button_active),
    );
    stack.push_layer(
        Selector::classes(&[PRIMARY]),
        (
            Background::Color(palette.accent_strong),
            BorderColor {
                color: palette.accent_strong,
            },
        ),
    );
    stack.push_layer(
        Selector::classes(&[PRIMARY]).with_hovered(true),
        Background::Color(palette.accent_strong.with_alpha(0.9)),
    );
    stack.push_layer(
        Selector::classes(&[PRIMARY]).with_active(true),
        Background::Color(palette.accent_strong),
    );
    stack.push_layer(
        Selector::classes(&[PRIMARY]).with_disabled(true),
        (
            Background::Color(palette.surface_control),
            BorderColor {
                color: palette.border,
            },
        ),
    );
    stack.push_layer(
        Selector::new().with_focused(true),
        BorderColor { color: focus_ring },
    );
    stack.push_layer(
        Selector::classes(&[PRIMARY])
            .with_focused(true)
            .with_disabled(false),
        BorderColor {
            color: palette.focus_ring_on_accent,
        },
    );
    properties.insert_stack::<Button>(stack);
}

#[cfg(test)]
mod tests;
