//! Settings materials, independent of the shell's translucent contrast theme.
use xilem::{Color, WidgetView};

mod focus;
mod paint;
mod view;
mod widget;
pub use focus::FocusIndicator;
pub use paint::{dot_pattern, edge_light, glass_edge, paint_button, paint_surface, shadow};
pub use view::MaterialView;

/// Reference muted ink for Settings; shell glass keeps its stronger ink.
pub const MUTED: Color = Color::from_rgb8(0x66, 0x73, 0x73);
/// Decorative strong orange. Use the shared ACCENT_STRONG for small text.
pub const ACCENT_DECORATIVE: Color = Color::from_rgb8(0xc7, 0x5a, 0x2a);
pub const ACCENT_BRIGHT: Color = Color::from_rgb8(0xf1, 0x9c, 0x77);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsButton {
    Bare,
    Control,
    Outline,
    Card,
    Primary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsSurface {
    Control,
    Subtle,
    Raised,
}

#[derive(Clone, Copy, Default)]
pub struct Interaction {
    pub hovered: bool,
    pub active: bool,
    pub focused: bool,
    pub disabled: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Material {
    Button(SettingsButton, bool),
    Surface(SettingsSurface),
}

impl Material {
    pub(crate) fn radius(self) -> f64 {
        match self {
            Self::Button(SettingsButton::Card, _) | Self::Surface(_) => crate::RADIUS_TILE,
            Self::Button(_, _) => crate::RADIUS_CONTROL,
        }
    }
}

/// Finish the native button's sizing and padding before wrapping it. Apply
/// dynamic `.disabled(...)` to the returned wrapper so its paint shares state.
/// The native child retains focus, activation, and its accessibility identity.
pub fn settings_button<State: 'static, Action: 'static, V: WidgetView<State, Action>>(
    inner: V,
    kind: SettingsButton,
    selected: bool,
) -> MaterialView<V> {
    MaterialView {
        inner,
        material: Material::Button(kind, selected),
        disabled: false,
    }
}

/// Paint a card without adding interactive semantics or changing its content.
pub fn settings_surface<State: 'static, Action: 'static, V: WidgetView<State, Action>>(
    inner: V,
    kind: SettingsSurface,
) -> MaterialView<V> {
    MaterialView {
        inner,
        material: Material::Surface(kind),
        disabled: false,
    }
}

#[cfg(test)]
mod tests;
