//! Identity fallback for bootstrap windows that publish no WM_CLASS.
use smithay::xwayland::X11Surface;

struct ProcessIdentity(Option<String>);

pub(super) fn remember_process(surface: &X11Surface) {
    // Query the X server's client credentials once at mapping, never while
    // painting or polling windows. Do not trust _NET_WM_PID or window titles.
    surface.user_data().insert_if_missing(|| {
        let identity = surface
            .get_client_pid()
            .ok()
            .filter(|pid| *pid != 0)
            .and_then(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).ok())
            .and_then(|path| path.file_name()?.to_str().map(str::to_owned));
        ProcessIdentity(identity)
    });
}

pub(in crate::compositor) fn app_id(surface: &X11Surface) -> Option<String> {
    // Explicit client identity always wins, including properties supplied after
    // the first map. The executable still goes through the shared catalog's
    // unambiguous matching rules before it can select application artwork.
    for identity in [surface.class(), surface.instance()] {
        if !identity.trim().is_empty() {
            return Some(identity);
        }
    }
    surface
        .user_data()
        .get::<ProcessIdentity>()
        .and_then(|identity| identity.0.clone())
}
