//! Shell semantics adapted to the shared native SVG geometry.
use crate::ui::appearance::{ThemeStyle as _, Tone};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon as Glyph, icon},
};
use xilem::{WidgetView, masonry::layout::AsUnit, style::Style, view::button};

use super::state::ShellState;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Icon {
    Apps,
    Map,
    SelectGroup,
    SelectDesktop,
    Search,
    #[cfg(test)]
    Wifi,
    #[cfg(test)]
    Power,
    Brightness,
    Volume {
        muted: bool,
    },
    Close,
    #[cfg(test)]
    Battery {
        percentage: u8,
        charging: bool,
    },
}

impl Icon {
    fn glyph(self) -> Glyph {
        match self {
            Self::Apps => Glyph::AppsGrid,
            Self::Map => Glyph::Map,
            Self::SelectGroup => Glyph::Lasso,
            Self::SelectDesktop => Glyph::Cursor,
            Self::Search => Glyph::Search,
            #[cfg(test)]
            Self::Wifi => Glyph::Wifi,
            #[cfg(test)]
            Self::Power => Glyph::Power,
            Self::Brightness => Glyph::Sun,
            Self::Volume { muted: false } => Glyph::Volume,
            Self::Volume { muted: true } => Glyph::VolumeMuted,
            Self::Close => Glyph::X,
            #[cfg(test)]
            Self::Battery {
                percentage,
                charging,
            } => Glyph::Battery {
                percentage,
                charging,
            },
        }
    }

    fn size(self) -> f64 {
        match self {
            Self::Apps => 22.0,
            #[cfg(test)]
            Self::Battery { .. } => 19.0,
            _ => 20.0,
        }
    }
}

pub(super) fn glyph(kind: Icon, name: String) -> impl WidgetView<ShellState> + use<> {
    icon(kind.glyph(), kind.size()).label(name)
}

pub(super) fn icon_button<F: Fn(&mut ShellState) + Send + Sync + 'static>(
    kind: Icon,
    name: &str,
    selected: bool,
    action: F,
) -> impl WidgetView<ShellState> + use<F> {
    components::button_surface(
        button(
            icon(kind.glyph(), kind.size())
                .label(name)
                .theme_color(if selected { Tone::Strong } else { Tone::Text }),
            action,
        )
        .padding(((44.0 - kind.size() - 2.0) / 2.0).px())
        .corner_radius(components::RADIUS_CONTROL.px()),
        components::ButtonSurface::Bare,
        selected,
    )
}
