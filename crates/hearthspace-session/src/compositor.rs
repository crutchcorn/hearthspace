use std::{env, path::PathBuf};

use crate::process::ProcessSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompositorBackend {
    NativeTty,
    NestedWinit,
}

pub(crate) fn select_compositor_backend() -> CompositorBackend {
    if env::var("XDG_SESSION_TYPE").is_ok_and(|session_type| session_type == "tty") {
        return CompositorBackend::NativeTty;
    }

    if host_wayland_display_path().is_some() || env::var_os("DISPLAY").is_some() {
        CompositorBackend::NestedWinit
    } else {
        CompositorBackend::NativeTty
    }
}

pub(crate) fn configure_compositor_environment(spec: &mut ProcessSpec, backend: CompositorBackend) {
    spec.env_removals.extend([
        "WAYLAND_DISPLAY".to_string(),
        "WAYLAND_SOCKET".to_string(),
        "DISPLAY".to_string(),
    ]);

    if backend == CompositorBackend::NestedWinit {
        if let Some(path) = host_wayland_display_path() {
            spec.envs
                .push((hearthspace_common::WAYLAND_DISPLAY_ENV.to_string(), path));
        }
        if let Some(display) = env::var_os("DISPLAY") {
            spec.envs.push((
                "DISPLAY".to_string(),
                display.to_string_lossy().into_owned(),
            ));
        }
    }
}

fn host_wayland_display_path() -> Option<String> {
    let display = env::var_os("WAYLAND_DISPLAY")?;
    let display_path = PathBuf::from(&display);
    let socket_path = if display_path.is_absolute() {
        display_path
    } else {
        PathBuf::from(env::var_os("XDG_RUNTIME_DIR")?).join(display_path)
    };

    socket_path
        .exists()
        .then(|| socket_path.to_string_lossy().into_owned())
}
