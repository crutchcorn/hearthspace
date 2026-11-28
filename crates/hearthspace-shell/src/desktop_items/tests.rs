use super::*;
use hearthspace_desktop::app_catalog::AppCatalog;
use std::{
    ffi::OsString,
    os::unix::{ffi::OsStringExt, fs::symlink},
};

fn catalog(directory: &Path) -> AppCatalog {
    let applications = directory.join("applications");
    fs::create_dir_all(&applications).unwrap();
    for (id, name) in [("editor.desktop", "Editor"), ("files.desktop", "Files")] {
        fs::write(
            applications.join(id),
            format!(
                "[Desktop Entry]\nType=Application\nName={name}\nExec=/bin/true\nIcon={name}\n"
            ),
        )
        .unwrap();
    }
    AppCatalog::load_from_data_dirs(vec![directory.into()])
}

#[test]
fn installed_apps_stay_in_catalog_without_creating_desktop_shortcuts() {
    let fixture = tempfile::tempdir().unwrap();
    let catalog = catalog(fixture.path());
    let desktop = fixture.path().join("Desktop");
    fs::create_dir(&desktop).unwrap();
    fs::create_dir(desktop.join("Projects")).unwrap();
    fs::write(desktop.join("Notes.txt"), "notes").unwrap();
    fs::write(desktop.join(".private"), "hidden").unwrap();
    let initial = discover(Some(&desktop)).unwrap();
    assert_eq!(
        initial
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>(),
        ["Projects", "Notes.txt"]
    );
    assert!(matches!(initial[0].kind, DesktopItemKind::Folder { .. }));
    assert!(matches!(initial[1].kind, DesktopItemKind::File { .. }));
    fs::remove_file(desktop.join("Notes.txt")).unwrap();
    let updated = discover(Some(&desktop)).unwrap();
    assert_eq!(updated, initial[..1]);
    assert!(discover(None).unwrap().is_empty());
    assert!(catalog.app_by_id("editor.desktop").is_some());
    assert!(catalog.app_by_id("files.desktop").is_some());
}

#[test]
fn desktop_objects_preserve_byte_identity_and_do_not_read_file_contents() {
    let fixture = tempfile::tempdir().unwrap();
    let directory = fixture.path();
    let first = directory.join(OsString::from_vec(vec![b'a', 0xff]));
    let second = directory.join(OsString::from_vec(vec![b'a', 0xfe]));
    fs::File::create(&first)
        .unwrap()
        .set_len(10 * 1024 * 1024 * 1024)
        .unwrap();
    fs::write(&second, []).unwrap();
    let items = discover(Some(directory)).unwrap();
    assert_eq!(items.len(), 2);
    assert_ne!(items[0].id, items[1].id);
    assert!(
        items
            .iter()
            .all(|item| matches!(item.kind, DesktopItemKind::File { .. }))
    );
    let uri = hearthspace_common::desktop_files::file_uri(&first).unwrap();
    assert_eq!(
        hearthspace_common::desktop_files::local_path(&uri).unwrap(),
        first
    );
}

#[test]
fn removed_desktop_folder_is_empty_and_does_not_create_files() {
    let fixture = tempfile::tempdir().unwrap();
    let missing = fixture.path().join("missing");
    assert_eq!(discover(Some(&missing)).unwrap(), discover(None).unwrap());
    assert!(!missing.exists());
    assert!(open_path(Path::new("/unused/socket"), &missing, None).is_err());
}

#[test]
fn linked_folders_are_items_without_recursively_enumerating_their_children() {
    let fixture = tempfile::tempdir().unwrap();
    let desktop = fixture.path().join("Desktop");
    let projects = fixture.path().join("Projects");
    fs::create_dir(&desktop).unwrap();
    fs::create_dir(&projects).unwrap();
    fs::write(projects.join("secret.txt"), "not a desktop item").unwrap();
    symlink(&projects, desktop.join("Projects")).unwrap();
    symlink(fixture.path().join("missing"), desktop.join("Broken")).unwrap();
    let items = discover(Some(&desktop)).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Projects");
    assert!(matches!(items[0].kind, DesktopItemKind::Folder { .. }));
}
