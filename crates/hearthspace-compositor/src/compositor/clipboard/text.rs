//! Explicit agent clipboard ownership; transfers never block the compositor loop.
use std::{
    io,
    os::fd::OwnedFd,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use smithay::{
    reexports::rustix::{
        event::{PollFd, PollFlags, Timespec, poll},
        fs::{OFlags, fcntl_getfl, fcntl_setfl},
        io::{Errno, write},
    },
    wayland::selection::data_device::set_data_device_selection,
};

use super::{App, SelectionData};
use crate::compositor::shell_integration::ControlReply;

const MAX_TEXT_BYTES: usize = 16 * 1024;
const MIME_TYPES: [&str; 2] = ["text/plain;charset=utf-8", "text/plain"];
static TRANSFERS: AtomicUsize = AtomicUsize::new(0);

impl App {
    pub(in crate::compositor) fn set_clipboard_text(&mut self, hex: &str) -> ControlReply {
        let bytes = match decode_text(hex) {
            Ok(bytes) => Arc::<[u8]>::from(bytes),
            Err(message) => return ControlReply::Err(message.into()),
        };
        let mime_types = MIME_TYPES.iter().map(|mime| (*mime).to_owned()).collect();
        set_data_device_selection(
            &self.display_handle,
            &self._seat,
            mime_types,
            SelectionData::Text(bytes),
        );
        #[cfg(feature = "xwayland")]
        if let Some(xwm) = self.xwm.as_mut().filter(|_| self.xdisplay.is_some())
            && let Err(error) = xwm.new_selection(
                smithay::wayland::selection::SelectionTarget::Clipboard,
                Some(MIME_TYPES.iter().map(|mime| (*mime).to_owned()).collect()),
            )
        {
            return ControlReply::Err(format!(
                "clipboard set, but XWayland advertisement failed: {error}"
            ));
        }
        ControlReply::Ok
    }

    #[cfg(feature = "xwayland")]
    pub(in crate::compositor) fn send_selection_text(
        &self,
        bytes: Arc<[u8]>,
        mime: &str,
        fd: OwnedFd,
    ) {
        send_text(bytes, mime, fd);
    }
}

fn decode_text(hex: &str) -> Result<Vec<u8>, &'static str> {
    if hex.len() > MAX_TEXT_BYTES * 2 || !hex.len().is_multiple_of(2) {
        return Err("clipboard text must be at most 16384 UTF-8 bytes encoded as hex");
    }
    let nibble = |byte| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    let bytes = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Some(nibble(pair[0])? << 4 | nibble(pair[1])?))
        .collect::<Option<Vec<_>>>()
        .ok_or("clipboard text is not hex")?;
    if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
        return Err("clipboard text must be valid UTF-8 without NUL");
    }
    Ok(bytes)
}

pub(super) fn send_text(bytes: Arc<[u8]>, mime: &str, fd: OwnedFd) {
    if !MIME_TYPES.contains(&mime) {
        return;
    }
    // Bound stalled readers as well as each payload. Dropping the FD reports
    // cancellation through the ordinary selection protocol.
    if TRANSFERS
        .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < 16).then_some(count + 1)
        })
        .is_err()
    {
        tracing::warn!("clipboard transfer capacity reached");
        return;
    }
    struct Transfer;
    impl Drop for Transfer {
        fn drop(&mut self) {
            TRANSFERS.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let transfer = Transfer;
    if let Err(error) = std::thread::Builder::new()
        .name("clipboard-transfer".into())
        .spawn(move || {
            let _transfer = transfer;
            if let Err(error) = write_text(fd, &bytes, Duration::from_secs(5)) {
                tracing::debug!(%error, "clipboard text transfer ended early");
            }
        })
    {
        tracing::warn!(%error, "failed to start clipboard transfer");
    }
}

fn write_text(fd: OwnedFd, bytes: &[u8], timeout: Duration) -> io::Result<()> {
    fcntl_setfl(&fd, fcntl_getfl(&fd)? | OFlags::NONBLOCK)?;
    let deadline = Instant::now() + timeout;
    let mut remaining = bytes;
    while !remaining.is_empty() {
        let wait = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "clipboard reader stalled"))?;
        match write(&fd, remaining) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => remaining = &remaining[count..],
            Err(Errno::INTR) => continue,
            Err(Errno::AGAIN) => {
                let duration = Timespec::try_from(wait).map_err(io::Error::other)?;
                match poll(&mut [PollFd::new(&fd, PollFlags::OUT)], Some(&duration)) {
                    Ok(_) | Err(Errno::INTR) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, os::unix::net::UnixStream};

    #[test]
    fn validates_entire_utf8_payload_before_setting_selection() {
        assert_eq!(
            decode_text("436166c3a920f09fa680").unwrap(),
            "Café 🦀".as_bytes()
        );
        assert_eq!(decode_text("").unwrap(), b"");
        for invalid in ["0", "zz", "ff", "00"] {
            assert!(decode_text(invalid).is_err());
        }
        assert!(decode_text(&"61".repeat(MAX_TEXT_BYTES + 1)).is_err());
    }

    #[test]
    fn partial_writes_deliver_exact_bytes_and_close_pipe() {
        let (mut reader, writer) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let bytes = vec![0x5a; 1024 * 1024];
        let expected = bytes.clone();
        let worker =
            std::thread::spawn(move || write_text(writer.into(), &bytes, Duration::from_secs(2)));
        let mut actual = Vec::new();
        reader.read_to_end(&mut actual).unwrap();
        worker.join().unwrap().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn stalled_reader_has_absolute_deadline() {
        let (_reader, writer) = UnixStream::pair().unwrap();
        let error = write_text(
            writer.into(),
            &vec![0x5a; 1024 * 1024],
            Duration::from_millis(20),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
}
