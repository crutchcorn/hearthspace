//! Keyboard focus is an ordinary Wayland surface, including Satellite clients.
use std::borrow::Cow;

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

use super::App;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum KeyboardFocusTarget {
    Wayland(WlSurface),
}

impl KeyboardFocusTarget {
    pub(super) fn surface(&self) -> &WlSurface {
        match self {
            Self::Wayland(surface) => surface,
        }
    }

    fn target(&self) -> &dyn KeyboardTarget<App> {
        match self {
            Self::Wayland(surface) => surface,
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
        // Pointer hits may name a dropdown/content subsurface. Keyboard focus
        // belongs to its surface-tree root; entering a child makes toolkits
        // treat the owning window as unfocused and dismiss its controls.
        let mut surface = surface;
        while let Some(parent) = get_parent(&surface) {
            surface = parent;
        }
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
        }
    }
}

impl KeyboardTarget<App> for KeyboardFocusTarget {
    fn enter(&self, seat: &Seat<App>, data: &mut App, keys: Vec<KeysymHandle<'_>>, serial: Serial) {
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
