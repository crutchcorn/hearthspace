use super::*;
use std::{ffi::OsStr, path::Path};

/// Called by the real isolated nested-child fixture after clearing selected bus
/// ownership while retaining its borrowed DBUS_SESSION_BUS_ADDRESS.
pub(in crate::compositor::shell_integration::launch) fn assert_nested_routing(root: &Path) {
    assert!(std::env::var_os("HEARTHSPACE_SESSION_BUS_ADDRESS").is_none());
    assert!(std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some());
    let file = root.join("nested document.txt");
    std::fs::write(&file, b"nested").unwrap();
    for id in [
        "org.example.Activate.desktop",
        "org.example.Fallback.desktop",
    ] {
        let prepared = prepare(
            Request::for_app(std::slice::from_ref(&file), id, Some("nested-output")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            prepared.request.output_name.as_deref(),
            Some("nested-output")
        );
        assert!(
            prepared
                .command
                .get_args()
                .any(|arg| arg == file_launch::NATIVE_GIO_FLAG)
        );
        assert!(
            prepared
                .command
                .get_envs()
                .any(|(key, value)| { key == "DBUS_SESSION_BUS_ADDRESS" && value.is_none() })
        );
        assert_eq!(prepared.command.get_args().last(), Some(file.as_os_str()));
        // The prepared native helper refuses before GIO can use the borrowed
        // broker, including for activation-only entries and entries with Exec.
        let error = file_launch::run_native_gio_helper(
            [
                root.join("data/applications").join(id).into_os_string(),
                file.clone().into_os_string(),
            ]
            .into_iter(),
        )
        .unwrap_err();
        assert!(error.contains("selected desktop session bus is unavailable"));
    }
    let direct = prepare(Request::for_app(&[file], "terminal.desktop", None).unwrap()).unwrap();
    assert_eq!(direct.command.get_program(), OsStr::new("gio"));
    assert!(
        direct
            .command
            .get_envs()
            .any(|(key, value)| { key == "DBUS_SESSION_BUS_ADDRESS" && value.is_none() })
    );
}
