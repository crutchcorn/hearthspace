//! Exercise the vendored launch API through the workspace dependency graph.
#![cfg(feature = "xwayland")]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use calloop::EventLoop;
use smithay::xwayland::{XWayland, XWaylandEvent};
use wayland_server::Display;

fn executable(root: &Path, name: &str, source: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, source).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

fn wait_event(xwayland: XWayland) -> XWaylandEvent {
    let mut events: EventLoop<Option<XWaylandEvent>> = EventLoop::try_new().unwrap();
    events
        .handle()
        .insert_source(xwayland, |event, _, result| *result = Some(event))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut result = None;
    while result.is_none() && Instant::now() < deadline {
        events
            .dispatch(
                Some(deadline.saturating_duration_since(Instant::now())),
                &mut result,
            )
            .unwrap();
    }
    result.expect("bounded Xwayland event delivery")
}

#[test]
fn absolute_executable_preserves_arguments_environment_and_inherited_descriptors() {
    let root = tempfile::tempdir().unwrap();
    let selected = executable(
        root.path(),
        "selected-Xwayland",
        r#"#!/bin/sh
{
    printf 'selected=%s\n' "$0"
    printf 'token=%s\n' "$FIXTURE_TOKEN"
    printf 'home=%s\n' "${HOME-unset}"
    printf 'arg=%s\n' "$@"
    test -e "/proc/self/fd/$WAYLAND_SOCKET" || exit 81
    printf 'wayland-fd=open\n'
} > "$CAPTURE"
while test "$#" -gt 0; do
    case "$1" in
        -wm|-listenfd) shift; test -e "/proc/self/fd/$1" || exit 82 ;;
        -displayfd) shift; displayfd="$1" ;;
    esac
    shift
done
printf 'descriptors=open\n' >> "$CAPTURE"
printf '17\n' > "/proc/self/fd/$displayfd"
"#,
    );
    let forbidden = root.path().join("fallback-executed");
    executable(
        root.path(),
        "Xwayland",
        &format!("#!/bin/sh\n: > '{}'\n", forbidden.display()),
    );
    let capture = root.path().join("capture");
    let display: Display<()> = Display::new().unwrap();
    let (xwayland, _client) = XWayland::spawn_with_executable(
        &display.handle(),
        &selected,
        None,
        [
            ("PATH", root.path().as_os_str()),
            ("CAPTURE", capture.as_os_str()),
            ("FIXTURE_TOKEN", "chosen".as_ref()),
        ],
        ["-surface-input-targets"],
        false,
        Stdio::null(),
        Stdio::inherit(),
        |_| (),
    )
    .unwrap();
    assert!(matches!(wait_event(xwayland), XWaylandEvent::Ready { .. }));
    let output = fs::read_to_string(capture).unwrap();
    assert!(
        output.contains(&format!("selected={}\n", selected.display())),
        "{output}"
    );
    assert_eq!(output.matches("arg=-rootless\n").count(), 1, "{output}");
    assert_eq!(
        output.matches("arg=-surface-input-targets\n").count(),
        1,
        "{output}"
    );
    assert!(output.contains("token=chosen\n"), "{output}");
    assert!(output.contains("home=unset\n"), "{output}");
    assert!(output.contains("wayland-fd=open\n"), "{output}");
    assert!(output.contains("descriptors=open\n"), "{output}");
    assert!(!forbidden.exists());
}

#[test]
fn missing_selected_executable_never_falls_back_to_path() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("fallback-executed");
    executable(
        root.path(),
        "Xwayland",
        &format!("#!/bin/sh\n: > '{}'\n", marker.display()),
    );
    let display: Display<()> = Display::new().unwrap();
    let error = XWayland::spawn_with_executable(
        &display.handle(),
        root.path().join("missing"),
        None,
        [("PATH", root.path())],
        ["-surface-input-targets"],
        false,
        Stdio::null(),
        Stdio::null(),
        |_| (),
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(!marker.exists());
}

#[test]
fn relative_executable_is_rejected_without_path_search() {
    let display: Display<()> = Display::new().unwrap();
    let error = XWayland::spawn_with_executable(
        &display.handle(),
        "Xwayland",
        None,
        std::iter::empty::<(String, String)>(),
        ["-surface-input-targets"],
        false,
        Stdio::null(),
        Stdio::null(),
        |_| (),
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}
