//! Preserve the native keyboard target when switching between Wayland and X11.
use std::borrow::Cow;

#[cfg(feature = "xwayland")]
use smithay::xwayland::X11Surface;
use smithay::{
    backend::input::{InputTime, KeyState},
    desktop::{PopupKind, find_popup_root_surface},
    input::{
        Seat,
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
    },
    utils::{IsAlive, Serial},
    wayland::{compositor::get_parent, seat::WaylandFocus},
};
use wayland_server::protocol::wl_surface::WlSurface;

use super::{App, ManagedSurface};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum KeyboardFocusTarget {
    Wayland(WlSurface),
    #[cfg(feature = "xwayland")]
    X11 {
        x11: Box<X11Surface>,
        // Only mapped X11 windows receive focus. Retain that association so
        // popup-grab pointer conversions stay valid during window teardown.
        surface: WlSurface,
    },
}

impl KeyboardFocusTarget {
    pub(super) fn for_window(window: &ManagedSurface, surface: WlSurface) -> Self {
        match window {
            ManagedSurface::Xdg(_) => Self::Wayland(surface),
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(x11) => Self::X11 {
                x11: x11.clone(),
                surface,
            },
        }
    }

    pub(super) fn surface(&self) -> &WlSurface {
        match self {
            Self::Wayland(surface) => surface,
            #[cfg(feature = "xwayland")]
            Self::X11 { surface, .. } => surface,
        }
    }

    fn target(&self) -> &dyn KeyboardTarget<App> {
        match self {
            Self::Wayland(surface) => surface,
            #[cfg(feature = "xwayland")]
            Self::X11 { x11, .. } => x11.as_ref(),
        }
    }
}

impl IsAlive for KeyboardFocusTarget {
    fn alive(&self) -> bool {
        self.target().alive()
    }
}

impl WaylandFocus for KeyboardFocusTarget {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        Some(Cow::Borrowed(self.surface()))
    }
}

impl From<WlSurface> for KeyboardFocusTarget {
    fn from(surface: WlSurface) -> Self {
        Self::Wayland(surface)
    }
}

impl From<PopupKind> for KeyboardFocusTarget {
    fn from(popup: PopupKind) -> Self {
        Self::Wayland(popup.wl_surface().clone())
    }
}

impl From<KeyboardFocusTarget> for WlSurface {
    fn from(focus: KeyboardFocusTarget) -> Self {
        match focus {
            KeyboardFocusTarget::Wayland(surface) => surface,
            #[cfg(feature = "xwayland")]
            KeyboardFocusTarget::X11 { surface, .. } => surface,
        }
    }
}

impl KeyboardTarget<App> for KeyboardFocusTarget {
    fn enter(&self, seat: &Seat<App>, data: &mut App, keys: Vec<KeysymHandle<'_>>, serial: Serial) {
        // X11Surface performs XSetInputFocus/WM_TAKE_FOCUS in addition to
        // Wayland keyboard delivery. Using only its wl_surface skips that.
        self.target().enter(seat, data, keys, serial);
    }

    fn leave(&self, seat: &Seat<App>, data: &mut App, serial: Serial) {
        self.target().leave(seat, data, serial);
    }

    fn key(
        &self,
        seat: &Seat<App>,
        data: &mut App,
        key: KeysymHandle<'_>,
        state: KeyState,
        serial: Serial,
        time: InputTime,
    ) {
        self.target().key(seat, data, key, state, serial, time);
    }

    fn modifiers(
        &self,
        seat: &Seat<App>,
        data: &mut App,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
        self.target().modifiers(seat, data, modifiers, serial);
    }
}

impl App {
    pub(super) fn clear_keyboard_focus_for_surface(&mut self, root: &WlSurface) {
        let Some(focus) = self.keyboard.current_focus() else {
            return;
        };
        let mut surface = focus.surface().clone();
        loop {
            if &surface == root {
                // A closing toplevel also invalidates its popup's grab.
                self.keyboard.clone().unset_grab(self);
                self.clear_keyboard_focus();
                return;
            }
            if let Some(parent) = get_parent(&surface) {
                surface = parent;
                continue;
            }
            if let Some(popup) = self.popups.find_popup(&surface)
                && let Ok(parent) = find_popup_root_surface(&popup)
                && parent != surface
            {
                surface = parent;
                continue;
            }
            return;
        }
    }
}
