//! Substitute the systemd manager boundary, retaining both env transitions and GIO.
use super::*;
use hearthspace_shell::desktop_items::file_launch::{gio_command, run_opener};
use std::{
    ffi::OsString,
    os::unix::ffi::OsStringExt,
    time::{Duration, Instant},
};

#[test]
fn snap_scope_launches_gio_with_private_environment_and_unchanged_native_paths() {
    let root = tempfile::tempdir().unwrap();
    let manager = root.path().join("scope-manager");
    fs::write(
        &manager,
        r#"#!/bin/sh
printf '%s\n' "$XDG_RUNTIME_DIR" "$DBUS_SESSION_BUS_ADDRESS" > "$TEST_SCOPE_ROOT/manager-env"
printf '%s\0' "$@" > "$TEST_SCOPE_ROOT/manager-args"
while [ "${1#--}" != "$1" ]; do shift; done
exec "$@"
"#,
    )
    .unwrap();
    fs::set_permissions(&manager, fs::Permissions::from_mode(0o700)).unwrap();
    let recorder = root.path().join("file player");
    fs::write(&recorder, r#"#!/bin/sh
printf '%s\0' "$@" > "$TEST_SCOPE_ROOT/arguments"
printf '%s\n' "$XDG_RUNTIME_DIR" "$DBUS_SESSION_BUS_ADDRESS" "$WAYLAND_DISPLAY" "$DISPLAY" > "$TEST_SCOPE_ROOT/app-env"
touch "$TEST_SCOPE_ROOT/ready"
"#).unwrap();
    fs::set_permissions(&recorder, fs::Permissions::from_mode(0o700)).unwrap();
    let desktop = root.path().join("player.desktop");
    fs::write(&desktop, format!("[Desktop Entry]\nType=Application\nName=Scope Test\nExec=\"{}\" \"--literal=$cash ; #\" %F\n", recorder.display())).unwrap();
    let paths = vec![
        root.path()
            .join(OsString::from_vec(b"01-01 song $ ' ; \xff.flac".to_vec())),
        root.path().join("two % files.flac"),
    ];
    for path in &paths {
        fs::write(path, []).unwrap();
    }
    let identity = vec![
        "env".into(),
        "BAMF_DESKTOP_FILE_HINT=/installed/firefox.desktop".into(),
        "/snap/bin/firefox".into(),
        "%u".into(),
    ];
    let mut prefix = wrap_with(
        "firefox",
        &identity,
        "hearthspace-apps-test.service",
        "/private-runtime",
        "/host-runtime",
        "unix:path=/private-bus",
        "01234567-89ab-cdef-0123-456789abcdef",
    )
    .unwrap();
    let index = prefix
        .iter()
        .position(|arg| arg == "/usr/bin/systemd-run")
        .unwrap();
    prefix[index] = manager.to_str().unwrap().into();
    let mut command = gio_command(
        &desktop,
        &paths,
        &prefix,
        "wayland-owned",
        &[("DISPLAY", ":91")],
    )
    .unwrap();
    command.env("TEST_SCOPE_ROOT", root.path());
    run_opener(command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !root.path().join("ready").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(root.path().join("ready").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("manager-env")).unwrap(),
        "/host-runtime\nunix:path=/host-runtime/bus\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("app-env")).unwrap(),
        "/private-runtime\nunix:path=/private-bus\nwayland-owned\n:91\n"
    );
    let arguments = fs::read(root.path().join("arguments")).unwrap();
    assert_eq!(
        arguments
            .strip_suffix(&[0])
            .unwrap()
            .split(|byte| *byte == 0)
            .collect::<Vec<_>>(),
        [
            b"--literal=$cash ; #".as_slice(),
            paths[0].as_os_str().as_encoded_bytes(),
            paths[1].as_os_str().as_encoded_bytes()
        ]
    );
    let manager_args = fs::read(root.path().join("manager-args")).unwrap();
    let args = manager_args.split(|byte| *byte == 0).collect::<Vec<_>>();
    assert!(args.contains(
        &b"--unit=snap.firefox.firefox-01234567-89ab-cdef-0123-456789abcdef.scope".as_slice()
    ));
    assert!(args.contains(&b"--property=BindsTo=hearthspace-apps-test.service".as_slice()));
    assert!(args.contains(&b"--expand-environment=no".as_slice()));
    assert!(
        args.windows(3)
            .any(|args| args == [b"gio".as_slice(), b"launch".as_slice(), b"--".as_slice()])
    );
}
