//! Desktop-entry discovery and preference saves share the same bounded reader.
use std::{
    fs,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

const MAX_BYTES: u64 = 1024 * 1024;

pub(in crate::app_catalog) fn read_desktop_file(path: &Path) -> io::Result<Option<String>> {
    // Opening a FIFO must not block before we can inspect its file type.
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Application preferences must be a regular desktop file.",
        ));
    }
    let mut contents = String::new();
    file.take(MAX_BYTES + 1).read_to_string(&mut contents)?;
    check_desktop_file_size(&contents)?;
    Ok(Some(contents))
}

pub(in crate::app_catalog) fn check_desktop_file_size(contents: &str) -> io::Result<()> {
    if contents.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The application desktop file is too large.",
        ));
    }
    Ok(())
}
