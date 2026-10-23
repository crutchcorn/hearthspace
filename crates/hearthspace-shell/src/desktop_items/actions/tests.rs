use super::*;
use std::os::unix::fs::symlink;

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
