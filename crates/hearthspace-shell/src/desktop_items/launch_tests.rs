use super::{actions, open_path, test_support::*};
use std::{
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};

const CHILD: &str = "desktop_items::launch_tests::launch_in_isolated_environment";
const MODE: &str = "HEARTHSPACE_TEST_FILE_OPEN_MODE";

fn player_fixture() -> Fixture {
    let fixture = Fixture::new();
    let recorder = fixture.path().join("player with spaces");
    fs::write(&recorder, r#"#!/bin/sh
root="$HEARTHSPACE_TEST_FILE_ACTION_ROOT"
printf '%s\0' "$@" > "$root/arguments"
printf '%s\n' "$WAYLAND_DISPLAY" "${DISPLAY-unset}" "$DBUS_SESSION_BUS_ADDRESS" > "$root/environment"
printf 'ready' > "$root/ready"
i=0
while [ ! -e "$root/release" ] && [ "$i" -lt 300 ]; do
    sleep 0.01
    i=$((i + 1))
done
printf 'done' > "$root/done"
"#).unwrap();
    fs::set_permissions(&recorder, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        fixture.path().join("share/applications/player.desktop"),
        format!("[Desktop Entry]\nType=Application\nName=Fixture Player\nExec=\"{}\" \"--label=two words\" %U\nMimeType=audio/flac;\n", recorder.display()),
    ).unwrap();
    fs::write(
        fixture.path().join("config/mimeapps.list"),
        "[Default Applications]\naudio/flac=player.desktop;\naudio/x-flac=player.desktop;\n",
    )
    .unwrap();
    fixture
}

#[test]
fn default_open_honors_the_flac_association_with_literal_paths_and_private_environment() {
    let fixture = player_fixture();
    assert_success(fixture.child(CHILD).env(MODE, "default"));
}

#[test]
fn open_with_launches_the_selected_desktop_entry_with_multiple_native_paths() {
    let fixture = player_fixture();
    assert_success(fixture.child(CHILD).env(MODE, "selected"));
}

#[test]
fn missing_association_reports_failure_instead_of_succeeding_or_trying_a_browser() {
    let fixture = Fixture::new();
    assert_success(fixture.child(CHILD).env(MODE, "missing"));
}

#[test]
fn launch_in_isolated_environment() {
    let Some(root) = child_root() else { return };
    let mode = std::env::var(MODE).unwrap();
    let file = root.join(std::ffi::OsString::from_vec(
        b"song # ' $(touch unwanted); \xff.flac".to_vec(),
    ));
    fs::write(&file, include_bytes!("artwork/fixtures/silence.flac")).unwrap();
    if mode == "missing" {
        let error = open_path(&file, None).unwrap_err();
        assert!(error.contains("application launcher failed"), "{error}");
        assert!(!root.join("arguments").exists());
        return;
    }
    let mut paths = vec![file];
    if mode == "selected" {
        let second = root.join("another % song.flac");
        fs::write(&second, include_bytes!("artwork/fixtures/silence.flac")).unwrap();
        paths.push(second);
        actions::open_with(&paths, "player.desktop", Some(":91")).unwrap();
    } else {
        open_path(&paths[0], None).unwrap();
    }
    wait_for(&root.join("ready"));
    // The app deliberately stays alive. Finishing the action must not wait for
    // it or its inherited output handles to close.
    assert!(
        !root.join("done").exists(),
        "opening waited for the app to exit"
    );
    let bytes = fs::read(root.join("arguments")).unwrap();
    let arguments: Vec<_> = bytes
        .strip_suffix(&[0])
        .unwrap()
        .split(|b| *b == 0)
        .collect();
    let mut expected = vec![b"--label=two words".as_slice()];
    expected.extend(paths.iter().map(|path| path.as_os_str().as_encoded_bytes()));
    assert_eq!(arguments, expected);
    assert_eq!(
        fs::read_to_string(root.join("environment")).unwrap(),
        format!(
            "wayland-file-action-test\n{}\nunix:path=/nonexistent-private-bus\n",
            if mode == "selected" { ":91" } else { "unset" }
        )
    );
    assert!(!root.join("unwanted").exists());
    fs::write(root.join("release"), []).unwrap();
    wait_for(&root.join("done"));
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(path.exists(), "launcher did not create {}", path.display());
}
