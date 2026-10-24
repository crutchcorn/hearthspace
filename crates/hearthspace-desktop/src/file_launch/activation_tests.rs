use super::test_support::*;
use super::*;
use std::{
    collections::HashMap,
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    sync::{Arc, Mutex},
};

const APP: &str = "org.hearthspace.TestFilePlayer";
const CHILD: &str = "file_launch::activation_tests::activate_in_isolated_environment";
const HELPER: &str = "file_launch::activation_tests::native_helper_in_isolated_environment";

struct Player {
    requests: Arc<Mutex<Vec<Vec<String>>>>,
    reject: bool,
}

#[zbus::interface(name = "org.freedesktop.Application")]
impl Player {
    fn open(
        &self,
        uris: Vec<String>,
        _platform_data: HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        self.requests.lock().unwrap().push(uris);
        if self.reject {
            return Err(zbus::fdo::Error::Failed("Cannot open this file".into()));
        }
        Ok(())
    }
}

#[tokio::test]
async fn gio_activates_an_existing_application_on_the_private_bus_with_file_uris() {
    assert_existing_application(false).await;
}

#[tokio::test]
async fn native_helper_reports_an_existing_applications_rejected_open() {
    assert_existing_application(true).await;
}

async fn assert_existing_application(reject: bool) {
    let fixture = Fixture::new();
    let bus = Bus::start(fixture.path());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let _player = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name(APP)
        .unwrap()
        .serve_at(
            "/org/hearthspace/TestFilePlayer",
            Player {
                requests: requests.clone(),
                reject,
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    // An Exec fallback cannot satisfy the observed D-Bus request.
    fs::write(fixture.path().join(format!("share/applications/{APP}.desktop")),
        "[Desktop Entry]\nType=Application\nName=Test File Player\nDBusActivatable=true\nExec=/bin/false %U\n").unwrap();
    let path = fixture
        .path()
        .join(std::ffi::OsString::from_vec(b"song # \xff.flac".to_vec()));
    fs::write(&path, []).unwrap();
    let helper = fixture.path().join("helper");
    fs::write(
        &helper,
        format!(
            r#"#!/bin/sh
export HEARTHSPACE_TEST_HELPER_MODE="$1"
export HEARTHSPACE_TEST_HELPER_DESKTOP="$2"
export HEARTHSPACE_TEST_HELPER_FILE="$3"
exec "$HEARTHSPACE_TEST_EXECUTABLE" --exact {HELPER} --nocapture
"#
        ),
    )
    .unwrap();
    fs::set_permissions(helper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = fixture.child(CHILD);
    command
        .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address)
        .env("HEARTHSPACE_TEST_REJECT_OPEN", reject.to_string());
    tokio::task::spawn_blocking(move || assert_success(&mut command))
        .await
        .unwrap();
    // Completion must follow the Open reply, with exactly one delivery. Waiting
    // for receipt here would hide the premature success of plain `gio launch`.
    assert_eq!(
        *requests.lock().unwrap(),
        vec![vec![file_uri(&path).unwrap()]]
    );
}

#[test]
fn activate_in_isolated_environment() {
    let Some(root) = child_root() else { return };
    let path = root.join(std::ffi::OsString::from_vec(b"song # \xff.flac".to_vec()));
    let app = installed_application(&format!("{APP}.desktop")).unwrap();
    assert!(app.dbus_activatable);
    let mut command = native_gio_command(
        &app.desktop_file,
        &[path],
        "wayland-private",
        &[],
        &root.join("helper"),
    )
    .unwrap();
    command.env(
        "HEARTHSPACE_TEST_EXECUTABLE",
        std::env::current_exe().unwrap(),
    );
    let result = run_opener(command);
    if std::env::var("HEARTHSPACE_TEST_REJECT_OPEN").unwrap() == "true" {
        assert!(result.unwrap_err().contains("application launcher failed"));
    } else {
        result.unwrap();
    }
}

#[test]
fn native_helper_in_isolated_environment() -> Result<(), String> {
    if child_root().is_none() {
        return Ok(());
    }
    assert_eq!(
        std::env::var("HEARTHSPACE_TEST_HELPER_MODE").unwrap(),
        NATIVE_GIO_FLAG
    );
    run_native_gio_helper(
        [
            std::env::var_os("HEARTHSPACE_TEST_HELPER_DESKTOP").unwrap(),
            std::env::var_os("HEARTHSPACE_TEST_HELPER_FILE").unwrap(),
        ]
        .into_iter(),
    )
}
