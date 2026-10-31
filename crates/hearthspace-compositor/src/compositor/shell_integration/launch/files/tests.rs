use super::*;
use std::{ffi::OsStr, path::Path};

pub(in crate::compositor::shell_integration::launch) fn assert_gpu_routing(root: &Path) {
    let file = root.join("GPU document.txt");
    std::fs::write(&file, b"gpu").unwrap();
    let preferred = |preference: bool| {
        assert!(preference);
        vec![("DRI_PRIME".into(), "fixture-discrete".into())]
    };
    for id in ["gpu.desktop", "gpu-hidden.desktop", "gpu-snap.desktop"] {
        let request = Request::for_app(std::slice::from_ref(&file), id, None).unwrap();
        let prepared = prepare_with_gpu(request, preferred).unwrap();
        assert!(
            prepared
                .command
                .get_args()
                .any(|arg| arg == file_launch::SCOPED_GIO_FLAG)
        );
        assert_eq!(
            prepared.environment,
            [("DRI_PRIME".into(), "fixture-discrete".into())]
        );
    }
    let request = Request::for_app(&[file], "gpu-only.desktop", None).unwrap();
    let activation = prepare_with_gpu(request, |_| {
        panic!("activation-only hint must remain best effort")
    })
    .unwrap();
    assert!(
        activation
            .command
            .get_args()
            .any(|arg| arg == file_launch::NATIVE_GIO_FLAG)
    );
    assert!(activation.environment.is_empty());
}

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
