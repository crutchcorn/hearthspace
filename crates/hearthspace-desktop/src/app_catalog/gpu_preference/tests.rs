use super::*;
use crate::app_catalog::AppCatalog;

mod files;
mod size;

const ENTRY: &str = "# Keep this comment\n[Desktop Entry]\nType=Application\nName=Studio\nName[fr]=Atelier\nExec=studio --literal=%k %U\nIcon=studio\nTerminal=true\nPath=/tmp/project\\sfiles\nMimeType=image/png;\nActions=New;\nX-Custom=unchanged\nX-KDE-RunOnDiscreteGpu=true\n\n[Desktop Action New]\nName=New project\nName[fr]=Nouveau\nExec=studio --new\n";

struct Fixture {
    _root: tempfile::TempDir,
    user: PathBuf,
    system: PathBuf,
}

impl Fixture {
    fn new(relative: &str, contents: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("user");
        let system = root.path().join("system");
        fs::create_dir_all(user.join("applications")).unwrap();
        let source = system.join("applications").join(relative);
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(source, contents).unwrap();
        Self {
            _root: root,
            user,
            system,
        }
    }
    fn dirs(&self) -> Vec<PathBuf> {
        vec![self.user.clone(), self.system.clone()]
    }
    fn save(&self, id: &str, enabled: bool) -> Result<DesktopApp, String> {
        set_in_data_dirs(id, enabled, &self.user, self.dirs())
    }
}

#[test]
fn standard_false_overrides_legacy_true_and_absent_standard_uses_legacy() {
    let fixture = Fixture::new("studio.desktop", ENTRY);
    assert!(
        lookup(&fixture.dirs(), "studio.desktop")
            .unwrap()
            .prefers_non_default_gpu
    );
    let source = fixture.system.join("applications/studio.desktop");
    fs::write(
        &source,
        ENTRY.replace("X-Custom=unchanged", "PrefersNonDefaultGPU=false"),
    )
    .unwrap();
    assert!(
        !lookup(&fixture.dirs(), "studio.desktop")
            .unwrap()
            .prefers_non_default_gpu
    );
}

#[test]
fn atomic_local_override_preserves_actions_translations_and_launch_metadata() {
    let fixture = Fixture::new("vendor/studio.desktop", ENTRY);
    let app = fixture.save("vendor-studio.desktop", false).unwrap();
    assert!(!app.prefers_non_default_gpu);
    assert!(app.terminal);
    assert_eq!(app.working_directory, Some("/tmp/project files".into()));
    assert_eq!(
        app.path,
        fixture.user.join("applications/vendor/studio.desktop")
    );
    assert!(
        !fixture
            .user
            .join("applications/vendor-studio.desktop")
            .exists()
    );
    assert_eq!(
        fs::read_to_string(fixture.system.join("applications/vendor/studio.desktop")).unwrap(),
        ENTRY
    );
    let saved = fs::read_to_string(&app.path).unwrap();
    assert!(saved.contains("# Keep this comment"));
    assert!(!saved.contains(LEGACY_KEY));
    let before = KeyFile::new();
    let after = KeyFile::new();
    let flags = KeyFileFlags::KEEP_COMMENTS | KeyFileFlags::KEEP_TRANSLATIONS;
    before.load_from_data(ENTRY, flags).unwrap();
    after.load_from_data(&saved, flags).unwrap();
    for group in before.groups().iter() {
        for key in before.keys(group).unwrap().iter() {
            if key.as_str() != LEGACY_KEY {
                assert_eq!(
                    before.value(group, key).unwrap(),
                    after.value(group, key).unwrap()
                );
            }
        }
    }
    assert!(!after.boolean(GROUP, KEY).unwrap());
    let again = fixture.save("vendor-studio.desktop", true).unwrap();
    assert!(again.prefers_non_default_gpu);
    assert_eq!(again.path, app.path);
    assert_eq!(
        AppCatalog::load_from_data_dirs(fixture.dirs()).apps().len(),
        1
    );
}

#[test]
fn an_existing_nested_local_entry_keeps_its_path_and_identity() {
    let fixture = Fixture::new("vendor/studio.desktop", ENTRY);
    let local = fixture.user.join("applications/vendor/studio.desktop");
    fs::create_dir_all(local.parent().unwrap()).unwrap();
    fs::write(&local, ENTRY.replace("Name=Studio", "Name=My Studio")).unwrap();
    let app = fixture.save("vendor-studio.desktop", false).unwrap();
    assert_eq!(app.path, local);
    assert_eq!(app.name, "My Studio");
    assert!(
        !fixture
            .user
            .join("applications/vendor-studio.desktop")
            .exists()
    );
}

#[test]
fn locked_save_fails_without_changes_then_retry_rereads_the_latest_entry() {
    let fixture = Fixture::new("studio.desktop", ENTRY);
    let directory = fixture.user.join("applications");
    let lock = transaction::lock(&directory, "studio.desktop").unwrap();
    assert!(
        fixture
            .save("studio.desktop", false)
            .unwrap_err()
            .contains("in progress")
    );
    assert!(!directory.join("studio.desktop").exists());
    fs::write(
        directory.join("studio.desktop"),
        ENTRY.replace("X-Custom=unchanged", "X-Custom=concurrent edit"),
    )
    .unwrap();
    drop(lock);
    let app = fixture.save("studio.desktop", false).unwrap();
    assert!(
        fs::read_to_string(app.path)
            .unwrap()
            .contains("X-Custom=concurrent edit")
    );
}

#[test]
fn competing_setter_rereads_after_the_other_writer_commits() {
    let fixture = Fixture::new("studio.desktop", ENTRY);
    let directory = fixture.user.join("applications");
    let lock = transaction::lock(&directory, "studio.desktop").unwrap();
    let user = fixture.user.clone();
    let dirs = fixture.dirs();
    let (contended, waiting) = std::sync::mpsc::channel();
    let (committed, resume) = std::sync::mpsc::channel();
    let pending = std::thread::spawn(move || {
        transaction::observe_contention(move || {
            contended.send(()).unwrap();
            resume
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        });
        set_in_data_dirs("studio.desktop", false, &user, dirs)
    });
    // The setter has actually tried and failed to acquire our lock. Publish a
    // new higher-priority path while it is paused at that contested boundary.
    waiting
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let local = directory.join("studio.desktop");
    fs::write(
        &local,
        ENTRY
            .replace("Name=Studio", "Name=After lock")
            .replace("Exec=studio --new", "Exec=studio --new --concurrent"),
    )
    .unwrap();
    drop(lock);
    committed.send(()).unwrap();
    let app = pending.join().unwrap().unwrap();
    // A lookup made before acquiring the lock would retain the system path
    // and either lose this action or fail the save's identity revalidation.
    assert_eq!(app.path, local);
    assert_eq!(app.name, "After lock");
    assert!(
        fs::read_to_string(app.path)
            .unwrap()
            .contains("Exec=studio --new --concurrent")
    );
}

#[test]
fn malformed_documents_and_invalid_ids_are_not_replaced() {
    let contents = format!("{ENTRY}\n[Broken section\n");
    let fixture = Fixture::new("studio.desktop", &contents);
    assert!(fixture.save("studio.desktop", true).is_err());
    for id in [
        "../studio.desktop",
        "/studio.desktop",
        "studio",
        "a\0.desktop",
        "a\n.desktop",
    ] {
        assert!(fixture.save(id, true).is_err(), "accepted {id:?}");
    }
    assert_eq!(
        fs::read_to_string(fixture.system.join("applications/studio.desktop")).unwrap(),
        contents
    );
    assert!(!fixture.user.join("applications/studio.desktop").exists());
}

#[test]
fn symlinked_local_subdirectory_cannot_redirect_preference_writes() {
    let fixture = Fixture::new("vendor/studio.desktop", ENTRY);
    std::os::unix::fs::symlink(
        fixture.system.join("applications/vendor"),
        fixture.user.join("applications/vendor"),
    )
    .unwrap();
    assert!(
        fixture
            .save("vendor-studio.desktop", false)
            .unwrap_err()
            .contains("symlink")
    );
    assert_eq!(
        fs::read_to_string(fixture.system.join("applications/vendor/studio.desktop")).unwrap(),
        ENTRY
    );
}

#[test]
fn public_setter_and_file_handlers_observe_the_same_override_without_a_main_loop() {
    let fixture = Fixture::new(
        "vendor/studio.desktop",
        &ENTRY.replace("Exec=studio", "Exec=/bin/true"),
    );
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "app_catalog::gpu_preference::tests::save_in_isolated_environment",
            "--nocapture",
        ])
        .env_clear()
        .env("HEARTHSPACE_GPU_PREFERENCE_TEST", "1")
        .env("XDG_DATA_HOME", &fixture.user)
        .env("XDG_DATA_DIRS", &fixture.system)
        .env("HOME", &fixture.user)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn save_in_isolated_environment() {
    if env::var_os("HEARTHSPACE_GPU_PREFERENCE_TEST").is_none() {
        return;
    }
    let id = "vendor-studio.desktop";
    let original = crate::file_launch::installed_application(id).unwrap();
    assert!(original.prefers_non_default_gpu);
    let updated = set_app_prefers_non_default_gpu(id, false).unwrap();
    let opened = crate::file_launch::installed_application(id).unwrap();
    assert_eq!(opened.desktop_file, updated.path);
    assert!(!opened.prefers_non_default_gpu);
    // A MIME-only entry is still resolved, and an explicit false wins over the
    // legacy true even when a different editor adds the legacy key again.
    let text = fs::read_to_string(&updated.path).unwrap().replace(
        "[Desktop Entry]",
        "[Desktop Entry]\nNoDisplay=true\nX-KDE-RunOnDiscreteGpu=true",
    );
    fs::write(&updated.path, text).unwrap();
    assert!(
        !crate::file_launch::installed_application(id)
            .unwrap()
            .prefers_non_default_gpu
    );
}
