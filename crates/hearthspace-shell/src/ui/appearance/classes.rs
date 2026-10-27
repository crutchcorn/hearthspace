//! Palette roles applied through native classes, preserving retained children.
use hearthspace_ui_components::Palette;
use xilem::{
    Color, Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        core::{PropertyStack, Selector},
        properties::{Background, BorderColor, ContentColor},
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum Tone {
    Text,
    Disabled,
    Muted,
    Accent,
    Strong,
    AccentSoft,
    AccentBorder,
    Chrome,
    ChromeBorder,
    Surface,
    Subtle,
    Control,
    Paper,
    Border,
    Danger,
    Transparent,
}
impl Tone {
    const ALL: [Self; 16] = [
        Self::Text,
        Self::Disabled,
        Self::Muted,
        Self::Accent,
        Self::Strong,
        Self::AccentSoft,
        Self::AccentBorder,
        Self::Chrome,
        Self::ChromeBorder,
        Self::Surface,
        Self::Subtle,
        Self::Control,
        Self::Paper,
        Self::Border,
        Self::Danger,
        Self::Transparent,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Text => "shell-text",
            Self::Disabled => "shell-disabled-text",
            Self::Muted => "shell-muted",
            Self::Accent => "shell-accent",
            Self::Strong => "shell-strong",
            Self::AccentSoft => "shell-accent-soft",
            Self::AccentBorder => "shell-accent-border",
            Self::Chrome => "shell-chrome",
            Self::ChromeBorder => "shell-chrome-border",
            Self::Surface => "shell-surface",
            Self::Subtle => "shell-subtle",
            Self::Control => "shell-control",
            Self::Paper => "shell-paper",
            Self::Border => "shell-border",
            Self::Danger => "shell-danger",
            Self::Transparent => "shell-transparent",
        }
    }
    fn color(self, p: &Palette) -> Color {
        match self {
            Self::Text => p.text,
            Self::Disabled => p.text.multiply_alpha(0.35),
            Self::Muted => p.text_muted,
            Self::Accent => p.accent,
            Self::Strong => p.accent_strong,
            Self::AccentSoft => p.accent_soft,
            Self::AccentBorder => p.accent_border,
            Self::Chrome => p.chrome_raised,
            Self::ChromeBorder => p.chrome_border,
            Self::Surface => p.surface,
            Self::Subtle => p.surface_subtle,
            Self::Control => p.surface_control,
            Self::Paper => p.paper,
            Self::Border => p.border,
            Self::Danger => p.danger_subtle,
            Self::Transparent => Color::TRANSPARENT,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Ink,
    Fill,
    Edge,
}
impl Part {
    fn name(self) -> &'static str {
        match self {
            Self::Ink => "shell-ink",
            Self::Fill => "shell-fill",
            Self::Edge => "shell-edge",
        }
    }
}
// Each property uses its own combined class; applying fill and ink to one widget
// must not allow the last selector to overwrite an unrelated property.
fn class(tone: Tone, part: Part) -> String {
    format!("{}-{}", tone.name(), part.name())
}
pub(in crate::ui) fn properties(p: &Palette) -> PropertyStack {
    let mut stack = PropertyStack::new();
    stack.push_layer(
        Selector::classes(&["hearthspace-muted-text"]),
        ContentColor::new(p.text_muted),
    );
    for tone in Tone::ALL {
        let color = tone.color(p);
        stack.push_layer(
            Selector::classes(&[&class(tone, Part::Ink)]),
            ContentColor::new(color),
        );
        stack.push_layer(
            Selector::classes(&[&class(tone, Part::Fill)]),
            Background::Color(color),
        );
        stack.push_layer(
            Selector::classes(&[&class(tone, Part::Edge)]),
            BorderColor { color },
        );
    }
    stack
}
pub(in crate::ui) trait ThemeStyle: Sized {
    fn theme_color(self, tone: Tone) -> ThemeView<Self> {
        ThemeView {
            inner: self,
            tone,
            part: Part::Ink,
        }
    }
    fn theme_background(self, tone: Tone) -> ThemeView<Self> {
        ThemeView {
            inner: self,
            tone,
            part: Part::Fill,
        }
    }
    fn theme_border(self, tone: Tone) -> ThemeView<Self> {
        ThemeView {
            inner: self,
            tone,
            part: Part::Edge,
        }
    }
}
impl<T> ThemeStyle for T {}
pub(in crate::ui) struct ThemeView<V> {
    inner: V,
    tone: Tone,
    part: Part,
}
impl<V> ViewMarker for ThemeView<V> {}
impl<State: 'static, Action: 'static, V: WidgetView<State, Action>> View<State, Action, ViewCtx>
    for ThemeView<V>
{
    type Element = Pod<V::Widget>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (mut element, view_state) = self.inner.build(ctx, state);
        element.new_widget = element.new_widget.with_class(&class(self.tone, self.part));
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
        if self.tone != previous.tone || self.part != previous.part {
            element
                .ctx
                .remove_class(&class(previous.tone, previous.part));
            element.ctx.add_class(&class(self.tone, self.part));
        }
    }
    fn teardown(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        self.inner.teardown(state, ctx, element)
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
