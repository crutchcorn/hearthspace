use std::os::fd::OwnedFd;

use smithay::{
    wayland::selection::{
        SelectionTarget,
        data_device::{
            SelectionRequestError, clear_data_device_selection,
            current_data_device_selection_userdata, request_data_device_client_selection,
            set_data_device_selection,
        },
        primary_selection::{
            SelectionRequestError as PrimarySelectionRequestError, clear_primary_selection,
            current_primary_selection_userdata, request_primary_client_selection,
            set_primary_selection,
        },
    },
    xwayland::xwm::XwmId,
};
use tracing::warn;

use crate::compositor::{
    App, ManagedSurface,
    clipboard::{ClipboardSource, SelectionData},
};

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
        if !self.xwm_has_keyboard_focus(xwm) {
            return;
        }
        let owned = match target {
            SelectionTarget::Clipboard => current_data_device_selection_userdata(&self._seat),
            SelectionTarget::Primary => current_primary_selection_userdata(&self._seat),
        }
        .map(|source| source.clone());
        match owned {
            Some(SelectionData::Text(text)) => {
                self.send_selection_text(text, &mime_type, fd);
                return;
            }
            Some(SelectionData::Files(files)) => {
                if let Some(bytes) = files.encode(&mime_type) {
                    super::super::clipboard::send_bytes(bytes.into(), fd);
                }
                return;
            }
            _ => {}
        }
        let result = match target {
            SelectionTarget::Clipboard => {
                request_data_device_client_selection(&self._seat, mime_type, fd).map_err(|error| {
                    (
                        matches!(error, SelectionRequestError::NoSelection),
                        error.to_string(),
                    )
                })
            }
            SelectionTarget::Primary => {
                request_primary_client_selection(&self._seat, mime_type, fd).map_err(|error| {
                    (
                        matches!(error, PrimarySelectionRequestError::NoSelection),
                        error.to_string(),
                    )
                })
            }
        };
        match result {
            Ok(()) => {}
            Err((true, _)) => {
                // Smithay clears a dead Wayland source without calling
                // SelectionHandler::new_selection. Release its X11 proxy too.
                if let Some(xwm) = self.xwm.as_mut()
                    && let Err(error) = xwm.new_selection(target, None)
                {
                    warn!(%error, ?target, "failed to clear expired Wayland selection in XWayland");
                }
            }
            Err((_, error)) => {
                warn!(%error, ?target, "failed to transfer Wayland selection to X11")
            }
        }
    }

    pub(super) fn set_x11_clipboard(
        &mut self,
        xwm: XwmId,
        target: SelectionTarget,
        mime_types: Vec<String>,
    ) {
        if self.xdisplay.is_some() && self.xwm.as_ref().is_some_and(|current| current.id() == xwm) {
            let source = ClipboardSource::new(xwm, &mime_types);
            match target {
                SelectionTarget::Clipboard => set_data_device_selection(
                    &self.display_handle,
                    &self._seat,
                    mime_types,
                    SelectionData::X11(source),
                ),
                SelectionTarget::Primary => set_primary_selection(
                    &self.display_handle,
                    &self._seat,
                    mime_types,
                    SelectionData::X11(source),
                ),
            }
        }
    }

    pub(super) fn clear_x11_selection(&mut self, xwm: XwmId, target: SelectionTarget) {
        // A late X11 clear must not erase a more recent native Wayland copy.
        let current = match target {
            SelectionTarget::Clipboard => current_data_device_selection_userdata(&self._seat),
            SelectionTarget::Primary => current_primary_selection_userdata(&self._seat),
        };
        let owned = current.is_some_and(
            |source| matches!(&*source, SelectionData::X11(source) if source.xwm == xwm),
        );
        if owned {
            match target {
                SelectionTarget::Clipboard => {
                    clear_data_device_selection(&self.display_handle, &self._seat)
                }
                SelectionTarget::Primary => {
                    clear_primary_selection(&self.display_handle, &self._seat)
                }
            }
        }
    }

    pub(super) fn xwayland_disconnected(&mut self, xwm: XwmId) {
        if self.xdisplay.is_none() || !self.xwm.as_ref().is_some_and(|current| current.id() == xwm)
        {
            return;
        }
        self.clear_x11_selection(xwm, SelectionTarget::Clipboard);
        self.clear_x11_selection(xwm, SelectionTarget::Primary);
        if self.xwm_has_keyboard_focus(xwm) {
            self.clear_keyboard_focus();
        }
        // The nested dispatcher drops the XWM only after its current batch.
        // Dropping it here lets a pending clipboard pipe callback reach None.
        self.xdisplay = None;
        self.publish_xwayland_display();
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
            self.retire_x11_window(&window, super::RetirementCause::Destroyed);
        }
        warn!(?xwm, "XWayland disconnected");
    }
}
