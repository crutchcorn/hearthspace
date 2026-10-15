use super::*;
use gio::prelude::AppInfoExt;
use std::{
    env,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = env::temp_dir().join(format!(
            "hearthspace-entry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("share/applications")).unwrap();
        Self(path)
    }

    fn paths(&self) -> SessionPaths {
        SessionPaths {
            host_runtime_dir: self.0.clone(),
            session_dir: self.0.clone(),
            applications_dir: self.0.join("share/applications"),
            settings_socket: self.0.join("settings.sock"),
            session_socket: self.0.join("session.sock"),
            shell_command_socket: self.0.join("command.sock"),
        }
    }

    fn entry(&self) -> PathBuf {
        self.paths()
            .applications_dir
            .join("dev.hearthspace.Settings.desktop")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn prebuilt_settings_entry_launches_the_selected_binary_with_literal_special_characters() {
    let fixture = Fixture::new();
    let binary_dir = fixture
        .0
        .join("selected build \"quoted\" \\ $literal `literal`");
    fs::create_dir(&binary_dir).unwrap();
    let executable = binary_dir.join("hearthspace-settings");
    let marker = fixture.0.join("launched.txt");
    fs::write(
        &executable,
        format!(
            "#!/usr/bin/python3\nfrom pathlib import Path\nPath({:?}).write_text(__file__)\n",
            marker.to_str().unwrap()
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    generate_desktop_entries(
        &fixture.paths(),
        RunMode::InstalledBinaries,
        Some(&binary_dir),
    )
    .unwrap();
    let entry = fs::read_to_string(fixture.entry()).unwrap();
    assert!(entry.contains("Name=Hearthspace Settings\n"));
    assert!(!entry.contains("dev-run-app"));
    assert!(!entry.contains("X-Hearthspace-Dev"));
    // GLib parses the desktop value, then Gio parses and launches the Exec.
    // This tests both escaping layers with an owned non-GUI executable.
    let deadline = Instant::now() + Duration::from_secs(5);
    let values = gio::glib::KeyFile::new();
    values
        .load_from_file(fixture.entry(), gio::glib::KeyFileFlags::NONE)
        .unwrap();
    let command = values.string("Desktop Entry", "Exec").unwrap();
    let app = gio::AppInfo::create_from_commandline(
        command.as_str(),
        None,
        gio::AppInfoCreateFlags::NONE,
    )
    .unwrap();
    app.launch(&[], None::<&gio::AppLaunchContext>).unwrap();
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        fs::read_to_string(marker).unwrap(),
        executable.to_str().unwrap()
    );
}

#[test]
fn development_entries_keep_the_cargo_wrapper_and_storybook() {
    let fixture = Fixture::new();
    generate_desktop_entries(&fixture.paths(), RunMode::DevCargo, None).unwrap();
    let entry = fs::read_to_string(fixture.entry()).unwrap();
    assert!(entry.contains("scripts/dev-run-app"));
    assert!(entry.contains("X-Hearthspace-Dev=true\n"));
    assert!(
        fixture
            .paths()
            .applications_dir
            .join("dev.hearthspace.UIStorybook.desktop")
            .is_file()
    );
}

#[test]
fn path_based_installed_sessions_keep_their_normal_binary_lookup() {
    let fixture = Fixture::new();
    generate_desktop_entries(&fixture.paths(), RunMode::InstalledBinaries, None).unwrap();
    assert!(
        fs::read_to_string(fixture.entry())
            .unwrap()
            .contains("Exec=\"hearthspace-settings\"\n")
    );
}

#[test]
fn control_characters_cannot_inject_desktop_entry_fields() {
    for path in [
        "/tmp/bin\nHidden=true",
        "/tmp/bin\rExec=other",
        "/tmp/bin\tapp",
    ] {
        assert!(escape_exec_path(Path::new(path)).is_err());
    }
}

#[test]
fn executable_percent_signs_are_literal_not_desktop_field_codes() {
    assert_eq!(
        escape_exec_path(Path::new("/build/%f%U%%/hearthspace-settings")).unwrap(),
        "/build/%%f%%U%%%%/hearthspace-settings"
    );
}
