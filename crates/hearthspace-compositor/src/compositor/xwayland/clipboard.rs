use std::os::fd::OwnedFd;

use smithay::{
    wayland::selection::{
        SelectionTarget,
        data_device::{
            SelectionRequestError, clear_data_device_selection,
            current_data_device_selection_userdata, request_data_device_client_selection,
            set_data_device_selection,
        },
    },
    xwayland::xwm::XwmId,
};
use tracing::warn;

use crate::compositor::{App, ManagedSurface, clipboard::ClipboardSource};

impl App {
    pub(super) fn xwm_has_keyboard_focus(&self, xwm: XwmId) -> bool {
        if self.xdisplay.is_none() || !self.xwm.as_ref().is_some_and(|current| current.id() == xwm)
        {
            return false;
        }
        let Some(focus) = self.keyboard.current_focus() else {
            return false;
        };
        self.windows.iter().any(|window| {
            matches!(&window.surface, ManagedSurface::X11(surface)
                if surface.xwm_id() == Some(xwm) && surface.wl_surface().as_ref() == Some(focus.surface()))
        })
    }

    pub(super) fn send_wayland_clipboard_to_x11(
        &mut self,
        xwm: XwmId,
        target: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
    ) {
        // Recheck focus when the actual transfer begins, not just when XWM
        // validates the request. No application payload passes through here.
        if target != SelectionTarget::Clipboard || !self.xwm_has_keyboard_focus(xwm) {
            return;
        }
        match request_data_device_client_selection(&self._seat, mime_type, fd) {
            Ok(()) => {}
            Err(SelectionRequestError::NoSelection) => {
                // Smithay clears a dead Wayland source without calling
                // SelectionHandler::new_selection. Release its X11 proxy too.
                if let Some(xwm) = self.xwm.as_mut()
                    && let Err(error) = xwm.new_selection(target, None)
                {
                    warn!(%error, "failed to clear expired Wayland clipboard in XWayland");
                }
            }
            Err(error) => warn!(%error, "failed to transfer Wayland clipboard to X11"),
        }
    }

    pub(super) fn set_x11_clipboard(
        &mut self,
        xwm: XwmId,
        target: SelectionTarget,
        mime_types: Vec<String>,
    ) {
        if target == SelectionTarget::Clipboard
            && self.xdisplay.is_some()
            && self.xwm.as_ref().is_some_and(|current| current.id() == xwm)
        {
            set_data_device_selection(
                &self.display_handle,
                &self._seat,
                mime_types,
                ClipboardSource::new(xwm),
            );
        }
    }

    pub(super) fn clear_x11_clipboard(&mut self, xwm: XwmId) {
        // A late X11 clear must not erase a more recent native Wayland copy.
        let owned = current_data_device_selection_userdata(&self._seat)
            .is_some_and(|source| source.xwm == xwm);
        if owned {
            clear_data_device_selection(&self.display_handle, &self._seat);
        }
    }

    pub(super) fn xwayland_disconnected(&mut self, xwm: XwmId) {
        if self.xdisplay.is_none() || !self.xwm.as_ref().is_some_and(|current| current.id() == xwm)
        {
            return;
        }
        self.clear_x11_clipboard(xwm);
        if self.xwm_has_keyboard_focus(xwm) {
            self.clear_keyboard_focus();
        }
        // The nested dispatcher drops the XWM only after its current batch.
        // Dropping it here lets a pending clipboard pipe callback reach None.
        self.xdisplay = None;
        let windows = self
            .windows
            .iter()
            .filter_map(|window| match &window.surface {
                ManagedSurface::X11(surface) if surface.xwm_id() == Some(xwm) => {
                    Some(surface.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for window in windows {
            self.unmap_x11_window(&window);
        }
        warn!(?xwm, "XWayland disconnected");
    }
}
