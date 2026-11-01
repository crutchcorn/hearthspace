//! Substitute the external GPU inventory boundary; preserve launch preparation.
use super::*;
use std::path::Path;

pub(super) fn write_entries(directory: &Path) {
    for (name, extra) in [
        ("gpu", "DBusActivatable=true\nExec=/bin/true\n"),
        ("gpu-only", "DBusActivatable=true\n"),
        ("gpu-terminal", "Terminal=true\nExec=/bin/true\n"),
        (
            "gpu-snap",
            "DBusActivatable=true\nExec=/usr/bin/env /snap/bin/firefox\nX-SnapInstanceName=firefox\n",
        ),
        (
            "gpu-hidden",
            "NoDisplay=true\nDBusActivatable=true\nExec=/bin/true %F\n",
        ),
    ] {
        fs::write(
            directory.join(format!("{name}.desktop")),
            format!(
                "[Desktop Entry]\nType=Application\nName={name}\nPrefersNonDefaultGPU=true\n{extra}"
            ),
        )
        .unwrap();
    }
}

pub(in crate::compositor::shell_integration::launch) fn selected(
    preferred: bool,
) -> Vec<(String, String)> {
    assert!(
        preferred,
        "the persisted desktop preference must reach the provider"
    );
    vec![("DRI_PRIME".into(), "fixture-discrete".into())]
}

pub(super) fn assert_preparation(root: &Path) {
    let gpu = prepare_with_gpu("gpu.desktop".into(), Vec::new(), selected).unwrap();
    assert!(
        gpu.execution
            .command
            .get_args()
            .any(|arg| arg == file_launch::SCOPED_GIO_FLAG)
    );
    assert!(
        !gpu.execution
            .command
            .get_args()
            .any(|arg| arg == file_launch::NATIVE_GIO_FLAG)
    );
    assert_eq!(
        gpu.environment,
        [("DRI_PRIME".into(), "fixture-discrete".into())]
    );
    let default = prepare_with_gpu("gpu.desktop".into(), Vec::new(), |_| Vec::new()).unwrap();
    assert!(
        default
            .execution
            .command
            .get_args()
            .any(|arg| arg == file_launch::NATIVE_GIO_FLAG)
    );
    let activation = prepare_with_gpu("gpu-only.desktop".into(), Vec::new(), |_| {
        panic!("an activation-only entry must not query per-child GPU variables")
    })
    .unwrap();
    assert!(
        activation
            .execution
            .command
            .get_args()
            .any(|arg| arg == file_launch::NATIVE_GIO_FLAG)
    );
    assert!(activation.environment.is_empty());
    let terminal = prepare_with_gpu("gpu-terminal.desktop".into(), Vec::new(), selected).unwrap();
    assert_eq!(terminal.execution.command.get_program(), "/usr/bin/true");
    assert!(
        terminal
            .execution
            .command
            .get_args()
            .any(|arg| arg == "--run")
    );
    assert!(terminal.execution.command.get_envs().any(|(key, value)| {
        key == "DRI_PRIME" && value == Some(std::ffi::OsStr::new("fixture-discrete"))
    }));
    let snap = prepare_with_gpu("gpu-snap.desktop".into(), Vec::new(), selected).unwrap();
    assert!(
        snap.execution
            .command
            .get_args()
            .any(|arg| arg == file_launch::SCOPED_GIO_FLAG)
    );
    assert!(snap.execution.command.get_args().any(|arg| {
        arg.to_string_lossy()
            .starts_with("--unit=snap.firefox.firefox-")
    }));
    super::super::super::files::tests::assert_gpu_routing(root);
}
