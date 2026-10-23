use super::*;
use crate::desktop_items::test_support::*;
use std::{
    collections::HashMap,
    fs,
    os::unix::ffi::OsStringExt,
    sync::{Arc, Mutex},
};

const APP: &str = "org.hearthspace.TestFilePlayer";
const CHILD: &str =
    "desktop_items::file_launch::activation_tests::activate_in_isolated_environment";

struct Player(Arc<Mutex<Vec<Vec<String>>>>);

#[zbus::interface(name = "org.freedesktop.Application")]
impl Player {
    fn open(&self, uris: Vec<String>, _platform_data: HashMap<String, zbus::zvariant::OwnedValue>) {
        self.0.lock().unwrap().push(uris);
    }
}

#[tokio::test]
async fn gio_activates_an_existing_application_on_the_private_bus_with_file_uris() {
    let fixture = Fixture::new();
    let bus = Bus::start(fixture.path());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let _player = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name(APP)
        .unwrap()
        .serve_at("/org/hearthspace/TestFilePlayer", Player(requests.clone()))
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
    let mut command = fixture.child(CHILD);
    command.env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address);
    tokio::task::spawn_blocking(move || assert_success(&mut command))
        .await
        .unwrap();
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
    let command = gio_command(&app.desktop_file, &[path], &[], "wayland-private", &[]).unwrap();
    run_opener(command).unwrap();
}
