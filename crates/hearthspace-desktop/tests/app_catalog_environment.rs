use std::{env, fs, process::Command};

use hearthspace_desktop::app_catalog::AppCatalog;

const CHILD_MODE: &str = "HEARTHSPACE_TEST_CATALOG_MODE";
const SETTINGS_ID: &str = "dev.hearthspace.Settings.desktop";
const STORYBOOK_ID: &str = "dev.hearthspace.UIStorybook.desktop";

#[test]
fn installed_catalog_excludes_checkout_apps_and_development_retains_them() {
    let fixture = tempfile::tempdir().unwrap();
    let share = fixture.path().join("release/share");
    let applications = share.join("applications");
    fs::create_dir_all(&applications).unwrap();
    fs::write(
        applications.join(SETTINGS_ID),
        include_str!("../../../data/applications/dev.hearthspace.Settings.desktop"),
    )
    .unwrap();

    // Each child has an isolated environment, so this exercises production
    // discovery without mutating process-global variables alongside other tests.
    for dev in [None, Some("0"), Some("1")] {
        let mode = if dev == Some("1") { "dev" } else { "installed" };
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .args(["--exact", "catalog_in_isolated_environment", "--nocapture"])
            .env_clear()
            .env(CHILD_MODE, mode)
            .env("HOME", fixture.path().join("home"))
            .env("XDG_DATA_HOME", fixture.path().join("home/share"))
            .env("XDG_DATA_DIRS", &share)
            .env("PATH", fixture.path().join("release/bin"))
            .current_dir(fixture.path());
        if let Some(value) = dev {
            command.env(hearthspace_common::DEV_ENV, value);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "catalog with HEARTHSPACE_DEV={dev:?} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn catalog_in_isolated_environment() {
    let Ok(mode) = env::var(CHILD_MODE) else {
        return;
    };
    let catalog = AppCatalog::load();
    let settings = catalog.app_by_id(SETTINGS_ID).unwrap();
    let share = env::var_os("XDG_DATA_DIRS").unwrap();
    assert_eq!(
        settings.path,
        std::path::PathBuf::from(share)
            .join("applications")
            .join(SETTINGS_ID)
    );
    assert_eq!(settings.launch_argv().unwrap(), ["hearthspace-settings"]);

    if mode == "dev" {
        let storybook = catalog.app_by_id(STORYBOOK_ID).unwrap();
        assert_eq!(
            storybook.launch_argv().unwrap(),
            ["hearthspace-ui-storybook"]
        );
        assert_eq!(catalog.apps().len(), 2);
    } else {
        assert!(catalog.app_by_id(STORYBOOK_ID).is_none());
        assert!(catalog.search("storybook", 10).is_empty());
        assert_eq!(catalog.apps().len(), 1);
    }
}
