//! The ScreenCast backend is always a session-owned child, never a borrowed service.
use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::cli::RunMode;

pub(super) const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.hearthspace_wlr";
pub(super) const DESCRIPTOR: &str = "[portal]\n\
    DBusName=org.freedesktop.impl.portal.desktop.hearthspace_wlr\n\
    Interfaces=org.freedesktop.impl.portal.ScreenCast;\n\
    UseIn=Hearthspace\n";
const BACKENDS: &[&str] = &[
    "/usr/local/libexec/xdg-desktop-portal-wlr",
    "/usr/libexec/xdg-desktop-portal-wlr",
];
const PICKER: &str = "hearthspace-share-picker";

pub(super) fn backend() -> io::Result<PathBuf> {
    discover(BACKENDS.iter().map(Path::new))
}

fn discover<'a>(paths: impl IntoIterator<Item = &'a Path>) -> io::Result<PathBuf> {
    paths
        .into_iter()
        .find(|path| executable(path))
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Screen sharing requires xdg-desktop-portal-wlr >= 0.8.4 with Hearthspace \
                 --bus-name/--no-restore support at \
                 /usr/local/libexec/xdg-desktop-portal-wlr or /usr/libexec/xdg-desktop-portal-wlr",
            )
        })
}

pub(super) fn picker(mode: RunMode, binary_dir: Option<&Path>) -> io::Result<PathBuf> {
    match mode {
        RunMode::DevCargo => Ok(crate::session_env::repo_root()
            .join("scripts/runtime")
            .join(PICKER)),
        RunMode::InstalledBinaries => {
            let executable = env::current_exe()?;
            Ok(binary_dir
                .unwrap_or_else(|| executable.parent().expect("executable directory"))
                .join(PICKER))
        }
    }
}

fn executable(path: &Path) -> bool {
    path.is_absolute()
        && fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

pub(super) fn spec(
    paths: &SessionPaths,
    session: &SessionEnv,
    backend: &Path,
    picker: &Path,
) -> Result<ProcessSpec, Box<dyn std::error::Error>> {
    for key in ["WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"] {
        if !session
            .vars
            .iter()
            .any(|(name, value)| name == key && !value.is_empty())
        {
            return Err(format!("ScreenCast backend requires explicit {key}").into());
        }
    }
    if !executable(backend) {
        return Err(format!(
            "ScreenCast backend is missing or not executable: {}",
            backend.display()
        )
        .into());
    }
    // Canonicalize the release/dev helper before quoting it for xdpw's shell.
    let picker = picker.canonicalize().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "cannot find screen sharing picker {}: {error}",
                picker.display()
            ),
        )
    })?;
    if !executable(&picker) {
        return Err(format!(
            "screen sharing picker is not executable: {}",
            picker.display()
        )
        .into());
    }
    let picker = picker
        .to_str()
        .ok_or("screen sharing picker path is not UTF-8")?;
    if picker.contains(['\n', '\r', '\0']) {
        return Err("screen sharing picker path contains an invalid config delimiter".into());
    }
    // inih parses whitespace followed by ';' as a comment even inside shell
    // quotes. Reject semicolons before writing a command it could truncate.
    if picker.contains(';') {
        return Err(
            "screen sharing picker path contains ';', which is unsupported in the backend INI configuration; use a release or checkout path without semicolons"
                .into(),
        );
    }
    let config = paths.session_dir.join("screencast.conf");
    let quoted = format!("'{}'", picker.replace('\'', "'\\''"));
    fs::write(
        &config,
        format!("[screencast]\nmax_fps=30\nchooser_type=dmenu\nchooser_cmd={quoted}\n"),
    )?;
    let mut spec = startup::service_spec(
        paths,
        session,
        "portal-wlr",
        backend
            .to_str()
            .ok_or("ScreenCast backend path is not UTF-8")?,
    )?;
    spec.args = vec![
        "-c".into(),
        config
            .to_str()
            .ok_or("ScreenCast config path is not UTF-8")?
            .into(),
        format!("--bus-name={BUS_NAME}"),
        "--no-restore".into(),
    ];
    // This controls new persistence, not incoming restore_data. The required
    // --no-restore option prevents restored selections from bypassing the chooser.
    spec.envs.push(("XDPW_PERSIST_MODE".into(), "none".into()));
    spec.envs.push(("GTK_USE_PORTAL".into(), "0".into()));
    Ok(spec)
}

pub(super) fn foreign_owner(pid: u32) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "cannot start ScreenCast backend: {BUS_NAME} is already owned by foreign process {pid}; refusing to replace or adopt it"
        ),
    )
}

pub(super) fn require_unowned(
    bus: &str,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    if let Some(pid) = owner_pid(bus, BUS_NAME, deadline, cancelled)? {
        return Err(foreign_owner(pid));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
