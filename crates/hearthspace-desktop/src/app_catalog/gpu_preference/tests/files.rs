use std::{
    os::unix::fs::symlink,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use super::*;

#[test]
fn internal_parent_alias_cannot_write_another_desktop_ids_entry() {
    let fixture = Fixture::new("b/studio.desktop", ENTRY);
    let directory = fixture.user.join("applications");
    let original = directory.join("b/studio.desktop");
    fs::create_dir_all(original.parent().unwrap()).unwrap();
    fs::write(&original, ENTRY).unwrap();
    symlink("b", directory.join("a")).unwrap();
    // The aliased ID has a different lock, so containment alone is insufficient.
    let other_writer = transaction::lock(&directory, "b-studio.desktop").unwrap();
    assert!(
        fixture
            .save("a-studio.desktop", false)
            .unwrap_err()
            .contains("symlink")
    );
    assert_eq!(fs::read_to_string(&original).unwrap(), ENTRY);
    assert!(!directory.join("a-studio.desktop").exists());
    drop(other_writer);
    let saved = fixture.save("b-studio.desktop", false).unwrap();
    assert_eq!(saved.path, original);
    assert!(!saved.prefers_non_default_gpu);
}

#[test]
fn a_leaf_symlink_is_replaced_without_mutating_the_other_id() {
    let fixture = Fixture::new("b.desktop", ENTRY);
    let directory = fixture.user.join("applications");
    let original = directory.join("b.desktop");
    fs::write(&original, ENTRY).unwrap();
    symlink("b.desktop", directory.join("a.desktop")).unwrap();
    let other_writer = transaction::lock(&directory, "b.desktop").unwrap();
    let saved = fixture.save("a.desktop", false).unwrap();
    assert_eq!(fs::read_to_string(&original).unwrap(), ENTRY);
    assert!(!saved.prefers_non_default_gpu);
    assert!(fs::symlink_metadata(saved.path).unwrap().is_file());
    drop(other_writer);
}

#[test]
fn nested_and_hyphenated_id_collisions_keep_catalog_path_precedence() {
    let fixture = Fixture::new("vendor-tools-studio.desktop", ENTRY);
    let directory = fixture.user.join("applications");
    let paths = [
        "vendor-tools-studio.desktop",
        "vendor-tools/studio.desktop",
        "vendor/tools-studio.desktop",
        "vendor/tools/studio.desktop",
    ];
    for relative in paths {
        let path = directory.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            ENTRY.replace("Name=Studio", &format!("Name={relative}")),
        )
        .unwrap();
    }
    let id = "vendor-tools-studio.desktop";
    let catalog = AppCatalog::load_from_data_dirs(fixture.dirs());
    let expected = catalog.app_by_id(id).unwrap();
    assert_eq!(expected.path, directory.join("vendor/tools/studio.desktop"));
    let saved = fixture.save(id, false).unwrap();
    assert_eq!(saved.path, expected.path);
    assert_eq!(saved.name, expected.name);
    for relative in paths.into_iter().take(3) {
        assert_eq!(
            fs::read_to_string(directory.join(relative)).unwrap(),
            ENTRY.replace("Name=Studio", &format!("Name={relative}")),
        );
    }
}

#[test]
fn public_save_ignores_extensionless_nested_collision_just_like_the_catalog() {
    let fixture = Fixture::new("vendor-.desktop", ENTRY);
    let ignored = fixture.system.join("applications/vendor/.desktop");
    let ignored_contents = ENTRY.replace("Name=Studio", "Name=Ignored nested file");
    fs::create_dir_all(ignored.parent().unwrap()).unwrap();
    fs::write(&ignored, &ignored_contents).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "app_catalog::gpu_preference::tests::files::save_extension_collision_in_isolated_environment",
            "--nocapture",
        ])
        .env_clear()
        .env("HEARTHSPACE_GPU_EXTENSION_TEST", "1")
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
    assert_eq!(fs::read_to_string(ignored).unwrap(), ignored_contents);
    assert_eq!(
        fs::read_to_string(fixture.system.join("applications/vendor-.desktop")).unwrap(),
        ENTRY
    );
}

#[test]
fn save_extension_collision_in_isolated_environment() {
    if env::var_os("HEARTHSPACE_GPU_EXTENSION_TEST").is_none() {
        return;
    }
    let id = "vendor-.desktop";
    let user = PathBuf::from(env::var_os("XDG_DATA_HOME").unwrap());
    let system = PathBuf::from(env::var_os("XDG_DATA_DIRS").unwrap());
    let catalog = AppCatalog::load();
    assert_eq!(catalog.apps().len(), 1);
    let original = catalog.app_by_id(id).unwrap();
    assert_eq!(original.path, system.join("applications/vendor-.desktop"));
    assert_eq!(original.name, "Studio");
    assert!(original.prefers_non_default_gpu);
    let saved = set_app_prefers_non_default_gpu(&original.id, false).unwrap();
    assert_eq!(saved.path, user.join("applications/vendor-.desktop"));
    assert_eq!(saved.name, original.name);
    assert_eq!(saved.id, original.id);
    assert!(!saved.prefers_non_default_gpu);
    assert!(!user.join("applications/vendor/.desktop").exists());
    let reloaded = AppCatalog::load();
    assert_eq!(reloaded.apps(), &[saved]);
    assert!(!reloaded.app_by_id(id).unwrap().prefers_non_default_gpu);
}

#[test]
fn a_hidden_local_entry_still_masks_the_system_entry() {
    let fixture = Fixture::new("studio.desktop", ENTRY);
    let local = fixture.user.join("applications/studio.desktop");
    let hidden = ENTRY.replace("[Desktop Entry]", "[Desktop Entry]\nHidden=true");
    fs::write(&local, &hidden).unwrap();
    assert!(fixture.save("studio.desktop", false).is_err());
    assert_eq!(fs::read_to_string(local).unwrap(), hidden);
}

#[test]
fn new_nested_override_does_not_create_directories_through_a_parent_symlink() {
    let fixture = Fixture::new("vendor/deeper/studio.desktop", ENTRY);
    let directory = fixture.user.join("applications");
    fs::create_dir(directory.join("other")).unwrap();
    symlink("other", directory.join("vendor")).unwrap();
    assert!(
        fixture
            .save("vendor-deeper-studio.desktop", true)
            .unwrap_err()
            .contains("symlink")
    );
    assert!(!directory.join("other/deeper").exists());
    assert!(!directory.join("vendor-deeper-studio.desktop").exists());
    assert_eq!(
        fs::read_to_string(
            fixture
                .system
                .join("applications/vendor/deeper/studio.desktop")
        )
        .unwrap(),
        ENTRY
    );
}

struct BoundedChild(Child);

impl BoundedChild {
    fn finish(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "desktop-file regression child failed: {status}"
                );
                return;
            }
            assert!(
                Instant::now() < deadline,
                "desktop-file reader blocked on a FIFO"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for BoundedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn preference_save_and_catalog_reload_are_bounded_with_special_files() {
    let mut child = BoundedChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "app_catalog::gpu_preference::tests::files::bounded_desktop_reads_child",
                "--nocapture",
            ])
            .env_clear()
            .env("HEARTHSPACE_GPU_DESKTOP_READ_TEST", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    child.finish();
}

#[test]
fn bounded_desktop_reads_child() {
    if env::var_os("HEARTHSPACE_GPU_DESKTOP_READ_TEST").is_none() {
        return;
    }
    let fixture = Fixture::new("vendor/studio.desktop", ENTRY);
    let fifo = fixture.system.join("applications/blocked.desktop");
    nix::unistd::mkfifo(
        &fifo,
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .unwrap();
    symlink(&fifo, fixture.user.join("applications/pipe.desktop")).unwrap();
    let oversized = fixture.user.join("applications/oversized.desktop");
    fs::write(&oversized, format!("{ENTRY}#{}", "x".repeat(1024 * 1024))).unwrap();
    // The rejected local entry must still mask this otherwise valid fallback.
    fs::write(fixture.system.join("applications/oversized.desktop"), ENTRY).unwrap();

    let saved = fixture.save("vendor-studio.desktop", false).unwrap();
    assert!(!saved.prefers_non_default_gpu);
    assert!(
        fixture
            .save("blocked.desktop", true)
            .unwrap_err()
            .contains("regular")
    );
    assert!(
        fixture
            .save("pipe.desktop", true)
            .unwrap_err()
            .contains("regular")
    );
    assert!(
        fixture
            .save("oversized.desktop", true)
            .unwrap_err()
            .contains("too large")
    );
    assert!(fs::symlink_metadata(&oversized).unwrap().len() > 1024 * 1024);
    assert!(
        fs::symlink_metadata(fixture.user.join("applications/pipe.desktop"))
            .unwrap()
            .file_type()
            .is_symlink()
    );

    // Shell save completion reloads the catalog; it must skip the same invalid
    // files instead of hanging after the preference has already been committed.
    let reloaded = AppCatalog::load_from_data_dirs(fixture.dirs());
    assert_eq!(reloaded.apps(), &[saved]);
}
