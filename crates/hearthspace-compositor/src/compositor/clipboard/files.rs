//! Standard desktop file clipboard formats, with bounded asynchronous reads.
use super::{App, SelectionData, send_bytes};
use crate::compositor::shell_integration::ControlReply;
use hearthspace_common::desktop_files::{COPIED_FILES, URI_LIST};
use smithay::wayland::selection::data_device::{
    clear_data_device_selection, current_data_device_selection_userdata,
    request_data_device_client_selection, set_data_device_selection,
};
use std::{
    os::{fd::OwnedFd, unix::net::UnixStream},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

static GENERATION: AtomicU64 = AtomicU64::new(1);

mod read;

impl App {
    pub(in crate::compositor) fn copy_desktop_files(
        &mut self,
        ids: &[String],
        cut: bool,
    ) -> Result<(), String> {
        let mut files = self.desktop_file_selection(ids, cut)?;
        files.generation = Some(GENERATION.fetch_add(1, Ordering::Relaxed));
        let mime_types: Vec<String> = [COPIED_FILES, URI_LIST].map(String::from).into();
        set_data_device_selection(
            &self.display_handle,
            &self._seat,
            mime_types.clone(),
            SelectionData::Files(Arc::new(files)),
        );
        #[cfg(feature = "xwayland")]
        if let Some(xwm) = self.xwm.as_mut().filter(|_| self.xdisplay.is_some()) {
            xwm.new_selection(
                smithay::wayland::selection::SelectionTarget::Clipboard,
                Some(mime_types),
            )
            .map_err(|error| format!("Could not share file clipboard with XWayland: {error}"))?;
        }
        Ok(())
    }

    pub(in crate::compositor) fn clear_file_clipboard(&mut self, generation: u64) {
        let matches = generation != 0
            && current_data_device_selection_userdata(&self._seat).is_some_and(|current| {
                matches!(&*current, SelectionData::Files(files)
                if files.cut && files.generation == Some(generation))
            });
        if !matches {
            return;
        }
        clear_data_device_selection(&self.display_handle, &self._seat);
        #[cfg(feature = "xwayland")]
        if let Some(xwm) = self.xwm.as_mut().filter(|_| self.xdisplay.is_some()) {
            let _ = xwm.new_selection(
                smithay::wayland::selection::SelectionTarget::Clipboard,
                None,
            );
        }
    }

    pub(in crate::compositor) fn read_file_clipboard(&mut self) -> ControlReply {
        if let Some(SelectionData::Files(files)) =
            current_data_device_selection_userdata(&self._seat).map(|value| value.clone())
        {
            return ControlReply::Payload(
                serde_json::to_vec(&*files).expect("file clipboard serializes"),
            );
        }
        for mime in [COPIED_FILES, URI_LIST] {
            let (reader, writer) = match UnixStream::pair() {
                Ok(pair) => pair,
                Err(error) => {
                    return ControlReply::Err(format!("Could not read file clipboard: {error}"));
                }
            };
            if self.request_file_clipboard(mime, writer.into()).is_ok() {
                return read::start(reader, mime);
            }
        }
        ControlReply::Err("Clipboard does not contain local files.".into())
    }

    fn request_file_clipboard(&mut self, mime: &str, fd: OwnedFd) -> Result<(), String> {
        let owned = current_data_device_selection_userdata(&self._seat).map(|value| value.clone());
        match owned {
            Some(SelectionData::Files(files)) => {
                let bytes = files
                    .encode(mime)
                    .ok_or("unsupported file clipboard format")?;
                send_bytes(bytes.into(), fd);
                Ok(())
            }
            Some(SelectionData::Text(_)) => Err("clipboard contains text".into()),
            #[cfg(feature = "xwayland")]
            Some(SelectionData::X11(source))
                if !source.mime_types.iter().any(|value| value == mime) =>
            {
                Err("clipboard does not offer this file format".into())
            }
            #[cfg(feature = "xwayland")]
            Some(SelectionData::X11(source)) => self
                .xwm
                .as_mut()
                .filter(|xwm| xwm.id() == source.xwm)
                .ok_or("XWayland clipboard owner disappeared".to_string())?
                .send_selection(
                    smithay::wayland::selection::SelectionTarget::Clipboard,
                    mime.into(),
                    fd,
                )
                .map_err(|error| error.to_string()),
            None => request_data_device_client_selection(&self._seat, mime.into(), fd)
                .map_err(|error| error.to_string()),
        }
    }
}
