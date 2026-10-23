use super::*;
use std::{
    ffi::{OsStr, OsString},
    io::{BufRead, BufReader, Write},
    os::unix::{ffi::OsStringExt, net::UnixListener},
};

#[test]
fn gio_receives_literal_native_paths_and_the_compositors_session_environment() {
    let desktop = Path::new("/apps/Player with spaces.desktop");
    let paths = vec![
        PathBuf::from("/owned/--song; $(touch nope).wav"),
        PathBuf::from(OsString::from_vec(b"/owned/song-\xff.wav".to_vec())),
    ];
    let command = gio_command(desktop, &paths, &[], "wayland-test", &[]).unwrap();
    assert_eq!(command.get_program(), "gio");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            OsStr::new("launch"),
            OsStr::new("--"),
            desktop.as_os_str(),
            paths[0].as_os_str(),
            paths[1].as_os_str()
        ]
    );
    let env = command
        .get_envs()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(env.get(OsStr::new("DISPLAY")), Some(&None));
    assert_eq!(env.get(OsStr::new("DBUS_SESSION_BUS_PID")), Some(&None));
    assert_eq!(
        env.get(OsStr::new("DBUS_SESSION_BUS_ADDRESS")),
        Some(&std::env::var_os("HEARTHSPACE_SESSION_BUS_ADDRESS").as_deref())
    );
    assert_eq!(
        env.get(OsStr::new("WAYLAND_DISPLAY")),
        Some(&Some(OsStr::new("wayland-test")))
    );
    let command = gio_command(desktop, &paths, &[], "wayland-test", &[("DISPLAY", ":91")]).unwrap();
    assert!(
        command
            .get_envs()
            .any(|(key, value)| key == "DISPLAY" && value == Some(OsStr::new(":91")))
    );
}

#[test]
fn protocol_preserves_native_paths_and_surfaces_compositor_launch_failures() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture
        .path()
        .join(OsString::from_vec(b"line\nquote\"-\xff.flac".to_vec()));
    std::fs::write(&path, []).unwrap();
    let socket = fixture.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let expected = path.clone();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let request: Request =
            serde_json::from_str(line.strip_prefix("launch-files ").unwrap()).unwrap();
        assert_eq!(request.app_id, "player.desktop");
        assert_eq!(request.paths().unwrap(), [expected]);
        assert_eq!(request.output_name.as_deref(), Some("HDMI-A-1"));
        reader
            .get_mut()
            .write_all(b"err scope preparation failed\n")
            .unwrap();
    });
    let request = Request {
        app_id: "player.desktop".into(),
        uris: vec![file_uri(&path).unwrap()],
        output_name: Some("HDMI-A-1".into()),
    };
    assert_eq!(
        request.send(&socket).unwrap_err(),
        "scope preparation failed"
    );
    server.join().unwrap();
}

#[test]
fn remote_targets_and_uninstalled_desktop_paths_are_rejected() {
    let request = Request {
        app_id: "player.desktop".into(),
        uris: vec!["file://remote/tmp/song".into()],
        output_name: None,
    };
    assert!(request.paths().is_err());
    assert!(installed_application("/tmp/untrusted.desktop").is_err());
    assert!(installed_application("../untrusted.desktop").is_err());
}
