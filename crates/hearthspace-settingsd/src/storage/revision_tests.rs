use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn revision_survives_restart_noop_and_external_revision_removal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    fs::write(&path, "color_mode = dark\n").unwrap();
    let legacy = load_snapshot(&path).unwrap();
    assert_eq!(legacy.revision, 0);
    let first = update(&path, |_| Ok(())).unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(first.settings, legacy.settings);
    assert_eq!(update(&path, |_| Ok(())).unwrap(), first);
    let bytes = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        bytes
            .replace("revision = 1\n", "")
            .replace("color_mode = \"dark\"", "color_mode = \"light\""),
    )
    .unwrap();
    let external = load_snapshot(&path).unwrap();
    assert_ne!(external.cursor(), first.cursor());
    let next = update(&path, |settings| {
        settings.test = true;
        Ok(())
    })
    .unwrap();
    assert_eq!(next.revision, 2);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(transaction::lock_path(&path))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn rejected_and_failed_save_never_publish_a_revision() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    let before = update(&path, |_| Ok(())).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(
        update(&path, |settings| {
            settings.monitors[0].scale = 0;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(load_snapshot(&path).unwrap(), before);
    // Force rename failure after a valid load/mutation, through the real store.
    assert!(
        update(&path, |settings| {
            settings.test = true;
            fs::remove_file(&path)?;
            fs::create_dir(&path)
        })
        .is_err()
    );
    fs::remove_dir(&path).unwrap();
    fs::write(&path, bytes).unwrap();
    assert_eq!(load_snapshot(&path).unwrap(), before);
    let next = update(&path, |settings| {
        settings.test = true;
        Ok(())
    })
    .unwrap();
    assert!(next.revision > before.revision);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn competing_lock_is_nonblocking_and_recovers_after_release() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    let lock = transaction::try_lock(&path).unwrap();
    assert_eq!(
        update(&path, |_| Ok(())).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    drop(lock);
    assert_eq!(update(&path, |_| Ok(())).unwrap().revision, 1);
}

#[test]
fn malformed_hand_edit_is_not_replaced_by_a_setter_or_read() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    let bytes = "version = 1\nrevision = \"broken\"\n";
    fs::write(&path, bytes).unwrap();
    assert!(load_snapshot(&path).is_err());
    assert!(
        update(&path, |settings| {
            settings.test = true;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read_to_string(path).unwrap(), bytes);
}
