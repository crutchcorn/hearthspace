use super::ControlReply;
use hearthspace_common::desktop_files::{FileClipboard, MAX_FILE_BYTES};
use smithay::reexports::rustix::event::{PollFd, PollFlags, Timespec, poll};
use std::{
    io::{self, Read},
    os::unix::net::UnixStream,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

static READERS: AtomicUsize = AtomicUsize::new(0);
struct Reader;
impl Drop for Reader {
    fn drop(&mut self) {
        READERS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) fn start(reader: UnixStream, mime: &'static str) -> ControlReply {
    if READERS
        .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < 4).then_some(count + 1)
        })
        .is_err()
    {
        return ControlReply::Err("File clipboard is busy. Try again shortly.".into());
    }
    let guard = Reader;
    let (sender, receiver) = mpsc::channel();
    if let Err(error) = std::thread::Builder::new()
        .name("file-clipboard-reader".into())
        .spawn(move || {
            let _guard = guard;
            let result = read(reader).and_then(|bytes| FileClipboard::decode(mime, &bytes));
            let reply = match result {
                Ok(files) => ControlReply::Payload(
                    serde_json::to_vec(&files).expect("file clipboard serializes"),
                ),
                Err(error) => ControlReply::Err(error),
            };
            let _ = sender.send(reply);
        })
    {
        return ControlReply::Err(format!("Could not read file clipboard: {error}"));
    }
    ControlReply::Deferred(receiver)
}

fn read(mut reader: UnixStream) -> Result<Vec<u8>, String> {
    reader
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("File clipboard owner did not respond in time.")?;
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(count) => {
                if bytes.len() + count > MAX_FILE_BYTES {
                    return Err("Clipboard file list is too large.".into());
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                let timeout = Timespec::try_from(remaining).map_err(|error| error.to_string())?;
                poll(&mut [PollFd::new(&reader, PollFlags::IN)], Some(&timeout))
                    .map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}
