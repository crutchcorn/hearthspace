//! Seed a fresh X server's cursor preference before clients learn its display.
use std::{io, path::Path};

use smithay::reexports::x11rb::{
    self,
    connection::Connection,
    protocol::xproto::{AtomEnum, ConnectionExt, PropMode},
    wrapper::ConnectionExt as _,
};

pub(super) fn initialize(
    display_number: u32,
    home: Option<&Path>,
    cursor_size: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    // Creating RESOURCE_MANAGER disables Xlib's ~/.Xdefaults fallback. Some XCB
    // resource loaders also read ~/.Xresources. Preserve either database wholesale,
    // including resources unrelated to cursors, rather than approximating Xrm.
    // Without HOME we cannot establish whether that fallback belongs to the user.
    let Some(home) = home.filter(|path| !path.as_os_str().is_empty()) else {
        return Ok(());
    };
    for name in [".Xdefaults", ".Xresources"] {
        match std::fs::symlink_metadata(home.join(name)) {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }

    // The XWM's connection is private. This short-lived connection targets only
    // the newly ready server, never an inherited host DISPLAY.
    let (connection, screen) = x11rb::connect(Some(&format!(":{display_number}")))?;
    let root = connection.setup().roots[screen].root;
    let existing = connection
        .get_property(false, root, AtomEnum::RESOURCE_MANAGER, AtomEnum::ANY, 0, 0)?
        .reply()?;
    if existing.type_ != x11rb::NONE {
        // Even an empty or unfamiliar property is owned by its existing writer.
        return Ok(());
    }
    let resource = format!("Xcursor.size:\t{cursor_size}\n");
    connection
        .change_property8(
            PropMode::REPLACE,
            root,
            AtomEnum::RESOURCE_MANAGER,
            AtomEnum::STRING,
            resource.as_bytes(),
        )?
        // Finish the server-side write before DISPLAY publication can start an
        // ordinary or activated client. Per-client XCURSOR_SIZE still wins.
        .check()?;
    Ok(())
}
