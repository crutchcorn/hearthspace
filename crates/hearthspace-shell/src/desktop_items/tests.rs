use super::*;
use std::{
    ffi::{OsStr, OsString},
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
fn catalog_shortcuts_remain_complete_when_personal_files_change() {
    let fixture = tempfile::tempdir().unwrap();
    let catalog = catalog(fixture.path());
    let desktop = fixture.path().join("Desktop");
    fs::create_dir(&desktop).unwrap();
    fs::create_dir(desktop.join("Projects")).unwrap();
    fs::write(desktop.join("Notes.txt"), "notes").unwrap();
    fs::write(desktop.join(".private"), "hidden").unwrap();
    let initial = discover(&catalog, Some(&desktop)).unwrap();
    assert_eq!(
        initial
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>(),
        ["Projects", "Notes.txt", "Editor", "Files"]
    );
    assert!(matches!(initial[0].kind, DesktopItemKind::Folder { .. }));
    assert!(matches!(initial[1].kind, DesktopItemKind::File { .. }));
    fs::remove_file(desktop.join("Notes.txt")).unwrap();
    let updated = discover(&catalog, Some(&desktop)).unwrap();
    assert_eq!(&updated[1..], &initial[2..]);
    // Favorites are deliberately absent from discovery: even an empty favorite
    // list cannot remove the permanent installed-app representations.
    assert_eq!(discover(&catalog, None).unwrap(), initial[2..]);
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
    let items = discover(&AppCatalog::default(), Some(directory)).unwrap();
    assert_eq!(items.len(), 2);
    assert_ne!(items[0].id, items[1].id);
    assert!(
        items
            .iter()
            .all(|item| matches!(item.kind, DesktopItemKind::File { .. }))
    );
    let command = open_command(&first, None).unwrap();
    assert_eq!(command.get_args().collect::<Vec<_>>(), [first.as_os_str()]);
}

#[test]
fn removed_desktop_folder_does_not_remove_installed_apps_or_create_files() {
    let fixture = tempfile::tempdir().unwrap();
    let catalog = catalog(fixture.path());
    let missing = fixture.path().join("missing");
    assert_eq!(
        discover(&catalog, Some(&missing)).unwrap(),
        discover(&catalog, None).unwrap()
    );
    assert!(!missing.exists());
    assert!(open_path(&missing, None).is_err());
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
    let items = discover(&AppCatalog::default(), Some(&desktop)).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Projects");
    assert!(matches!(items[0].kind, DesktopItemKind::Folder { .. }));
}

#[test]
fn opening_a_filename_passes_one_literal_argument() {
    let path = Path::new("/tmp/Notes $(touch unwanted); ' quote.txt");
    let command = open_command(path, None).unwrap();
    assert_eq!(command.get_program(), "xdg-open");
    assert_eq!(command.get_args().collect::<Vec<_>>(), [path.as_os_str()]);
    assert!(command.get_envs().any(|(key, _)| key == "WAYLAND_DISPLAY"));
}

#[test]
fn file_opener_removes_an_inherited_display_without_session_xwayland() {
    let command = open_command(Path::new("/tmp/notes.txt"), None).unwrap();
    assert!(
        command
            .get_envs()
            .any(|(key, value)| key == "DISPLAY" && value.is_none())
    );
}

#[test]
fn file_opener_uses_only_the_compositor_acknowledged_xwayland_display() {
    let command = open_command(Path::new("/tmp/notes.txt"), Some(":37")).unwrap();
    assert!(
        command
            .get_envs()
            .any(|(key, value)| { key == "DISPLAY" && value == Some(OsStr::new(":37")) })
    );
}
