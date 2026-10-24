//! Ordinary clients share the selected connection; compositor records stay private.
use std::{
    io,
    path::{Path, PathBuf},
};

mod audio;
pub use audio::resolved as audio;

pub const APP_RUNTIME: &str = "HEARTHSPACE_APP_XDG_RUNTIME_DIR";
pub const APP_DISPLAY: &str = "HEARTHSPACE_APP_WAYLAND_DISPLAY";

/// Explicit isolation retains the private runtime and relative socket. Shared
/// native activation and direct children use the same host runtime/absolute socket.
pub fn connection(
    host: &Path,
    session: &Path,
    display: &str,
    private: bool,
) -> io::Result<(String, String)> {
    if !host.is_absolute() || !session.is_absolute() {
        return Err(io::Error::other("application runtimes must be absolute"));
    }
    let runtime = if private { session } else { host };
    let socket = if private {
        PathBuf::from(display)
    } else {
        session.join(display)
    };
    Ok((utf8(runtime)?, utf8(&socket)?))
}

fn utf8(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("application connection path is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_native_and_private_harness_keep_their_distinct_runtime_contracts() {
        let host = Path::new("/run/user/1000");
        let session = Path::new("/run/user/1000/hearthspace/session-1");
        assert_eq!(
            connection(host, session, "wayland-9", false).unwrap(),
            (
                "/run/user/1000".into(),
                "/run/user/1000/hearthspace/session-1/wayland-9".into()
            )
        );
        assert_eq!(
            connection(host, session, "wayland-9", true).unwrap(),
            (
                "/run/user/1000/hearthspace/session-1".into(),
                "wayland-9".into()
            )
        );
        assert_eq!(
            connection(host, session, "/private/custom-wayland", false)
                .unwrap()
                .1,
            "/private/custom-wayland"
        );
        assert!(connection(Path::new("relative"), session, "wayland-9", false).is_err());
    }
}
