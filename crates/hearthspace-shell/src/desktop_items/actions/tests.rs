use super::*;
use std::{ffi::OsStr, os::unix::fs::symlink};

#[test]
fn rename_preserves_existing_files_and_dangling_links() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("notes.txt");
    let occupied = root.path().join("existing.txt");
    let dangling = root.path().join("missing.txt");
    fs::write(&source, b"new notes").unwrap();
    fs::write(&occupied, b"keep me").unwrap();
    symlink("not-created", &dangling).unwrap();

    assert!(rename(&source, "existing.txt").is_err());
    assert!(rename(&source, "missing.txt").is_err());
    assert_eq!(fs::read(&source).unwrap(), b"new notes");
    assert_eq!(fs::read(&occupied).unwrap(), b"keep me");
    assert_eq!(fs::read_link(&dangling).unwrap(), Path::new("not-created"));
}

#[test]
fn rename_changes_the_link_itself_and_accepts_an_unchanged_name() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("original");
    let link = root.path().join("shortcut");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("notes"), b"keep me").unwrap();
    symlink("original", &link).unwrap();

    rename(&link, "shortcut").unwrap();
    rename(&link, "renamed").unwrap();
    assert!(fs::symlink_metadata(&link).is_err());
    assert_eq!(
        fs::read_link(root.path().join("renamed")).unwrap(),
        Path::new("original")
    );
    assert_eq!(fs::read(target.join("notes")).unwrap(), b"keep me");
    assert!(rename(&root.path().join("missing"), "missing").is_err());
}

#[test]
fn folder_creation_and_rename_cannot_escape_or_replace_an_existing_folder() {
    let root = tempfile::tempdir().unwrap();
    new_folder(root.path(), "Projects").unwrap();
    fs::write(root.path().join("Projects/notes"), b"keep me").unwrap();
    assert!(new_folder(root.path(), "Projects").is_err());
    assert!(new_folder(root.path(), "../escaped").is_err());
    assert!(rename(&root.path().join("Projects"), ".").is_err());
    assert_eq!(
        fs::read(root.path().join("Projects/notes")).unwrap(),
        b"keep me"
    );
}

#[test]
fn open_with_passes_literal_native_paths_and_only_the_own_session_environment() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let desktop_file = Path::new("/apps/Player with spaces.desktop");
    let paths = vec![
        PathBuf::from("/owned/--song; $(touch nope).wav"),
        PathBuf::from(OsString::from_vec(b"/owned/song-\xff.wav".to_vec())),
    ];
    let command = open_with_command(desktop_file, &paths, None).unwrap();
    assert_eq!(command.get_program(), OsStr::new("gio"));
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            OsStr::new("launch"),
            OsStr::new("--"),
            desktop_file.as_os_str(),
            paths[0].as_os_str(),
            paths[1].as_os_str(),
        ]
    );
    let environment = command
        .get_envs()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(environment.get(OsStr::new("DISPLAY")), Some(&None));
    assert_eq!(
        environment.get(OsStr::new("DBUS_SESSION_BUS_PID")),
        Some(&None)
    );
    assert_eq!(
        environment.get(OsStr::new("DBUS_SESSION_BUS_ADDRESS")),
        Some(&std::env::var_os("HEARTHSPACE_SESSION_BUS_ADDRESS").as_deref())
    );
    assert_eq!(
        environment.get(OsStr::new("WAYLAND_DISPLAY")),
        Some(&Some(OsStr::new(
            &hearthspace_common::wayland_display_name()
        )))
    );
    let command = open_with_command(desktop_file, &paths, Some(":91")).unwrap();
    assert!(
        command
            .get_envs()
            .any(|(key, value)| key == "DISPLAY" && value == Some(OsStr::new(":91")))
    );
}
