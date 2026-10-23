use super::*;
use crate::desktop_items::test_support::*;
use std::{
    os::unix::{ffi::OsStringExt, fs::symlink},
    path::Path,
    sync::{Arc, Mutex},
};

const CHILD: &str = "desktop_items::actions::properties::tests::properties_in_isolated_environment";
type Requests = Arc<Mutex<Vec<(Vec<String>, String)>>>;

struct FileManager {
    requests: Requests,
    reject: bool,
}

#[zbus::interface(name = "org.freedesktop.FileManager1")]
impl FileManager {
    fn show_item_properties(&self, uris: Vec<String>, startup_id: String) -> zbus::fdo::Result<()> {
        if self.reject {
            return Err(zbus::fdo::Error::Failed(
                "Fixture manager rejected the request".into(),
            ));
        }
        self.requests.lock().unwrap().push((uris, startup_id));
        Ok(())
    }
}

fn selection(root: &Path) -> Vec<DesktopItem> {
    vec![
        DesktopItem {
            id: "file:note".into(),
            title: "note".into(),
            kind: DesktopItemKind::File {
                path: root.join(std::ffi::OsString::from_vec(b"note # ' \xff.txt".to_vec())),
            },
        },
        DesktopItem {
            id: "file:folder".into(),
            title: "Projects".into(),
            kind: DesktopItemKind::Folder {
                path: root.join("Projects"),
            },
        },
        DesktopItem {
            id: "file:link".into(),
            title: "shortcut".into(),
            kind: DesktopItemKind::File {
                path: root.join("shortcut"),
            },
        },
        DesktopItem {
            id: "app:player.desktop".into(),
            title: "Player".into(),
            kind: DesktopItemKind::App {
                app_id: "player.desktop".into(),
                icon: None,
            },
        },
    ]
}

fn fixture() -> Fixture {
    let fixture = Fixture::new();
    let items = selection(fixture.path());
    let DesktopItemKind::File { path } = &items[0].kind else {
        unreachable!()
    };
    fs::write(path, "notes").unwrap();
    fs::create_dir(fixture.path().join("Projects")).unwrap();
    symlink("missing target", fixture.path().join("shortcut")).unwrap();
    fs::write(
        fixture.path().join("share/applications/player.desktop"),
        "[Desktop Entry]\nType=Application\nName=Player\nExec=/bin/true\n",
    )
    .unwrap();
    fixture
}

async fn manager(bus: &Bus, requests: Requests, reject: bool) -> zbus::Connection {
    zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name(FILE_MANAGER)
        .unwrap()
        .serve_at(FILE_MANAGER_PATH, FileManager { requests, reject })
        .unwrap()
        .build()
        .await
        .unwrap()
}

async fn invoke(fixture: &Fixture, bus: &Bus, error: Option<&str>) {
    let mut command = fixture.child(CHILD);
    command.env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address);
    if let Some(error) = error {
        command.env("HEARTHSPACE_TEST_PROPERTIES_ERROR", error);
    }
    tokio::task::spawn_blocking(move || assert_success(&mut command))
        .await
        .unwrap();
}

#[tokio::test]
async fn properties_send_selected_file_folder_link_and_app_uris_to_the_private_file_manager() {
    let fixture = fixture();
    let bus = Bus::start(fixture.path());
    let requests = Requests::default();
    let _manager = manager(&bus, requests.clone(), false).await;
    invoke(&fixture, &bus, None).await;
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let (uris, startup_id) = &requests[0];
    let paths: Vec<_> = uris
        .iter()
        .map(|uri| hearthspace_common::desktop_files::local_path(uri).unwrap())
        .collect();
    assert_eq!(
        paths,
        vec![
            fixture
                .path()
                .join(std::ffi::OsString::from_vec(b"note # ' \xff.txt".to_vec())),
            fixture.path().join("Projects"),
            fixture.path().join("shortcut"),
            fixture.path().join("share/applications/player.desktop"),
        ]
    );
    assert!(startup_id.is_empty());
}

#[tokio::test]
async fn properties_return_file_manager_rejection_to_the_desktop() {
    let fixture = fixture();
    let bus = Bus::start(fixture.path());
    let _manager = manager(&bus, Requests::default(), true).await;
    invoke(&fixture, &bus, Some("Fixture manager rejected the request")).await;
}

#[tokio::test]
async fn properties_report_a_missing_service_instead_of_using_the_host_file_manager() {
    let fixture = fixture();
    let bus = Bus::start(fixture.path());
    invoke(
        &fixture,
        &bus,
        Some("Could not open properties in the file manager"),
    )
    .await;
}

#[test]
fn properties_in_isolated_environment() {
    let Some(root) = child_root() else { return };
    let result = properties(&selection(&root));
    if let Ok(expected) = std::env::var("HEARTHSPACE_TEST_PROPERTIES_ERROR") {
        let error = result.unwrap_err();
        assert!(error.contains(&expected), "{error}");
    } else {
        result.unwrap();
    }
}

#[test]
fn properties_require_a_selection_and_an_explicit_private_bus() {
    assert!(
        show_properties(&[], None)
            .unwrap_err()
            .contains("Select an item")
    );
    let fixture = fixture();
    assert!(
        show_properties(&selection(fixture.path())[..1], None)
            .unwrap_err()
            .contains("desktop session bus is unavailable")
    );
}
