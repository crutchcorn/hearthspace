//! Boundary regressions exercise the real line protocol, handler, and persistence.
use std::fs;

use hearthspace_ipc::{BackgroundMode, MonitorConfig, Settings, display_layout_request};

use crate::{load_settings_from_path, tests::round_trip};

fn third_monitor() -> MonitorConfig {
    MonitorConfig {
        id: "DP-3".into(),
        name: "Third #3 \"虹\"".into(),
        width: 2560,
        height: 1440,
        scale: 2,
        x: 3840,
        y: 80,
    }
}

#[test]
fn first_layout_save_upserts_a_third_monitor_with_current_sizes_and_names() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    let mut monitors = Settings::default().monitors;
    monitors[0].width = 1600;
    monitors[0].name = "Built-in display".into();
    monitors.push(third_monitor());

    let response = round_trip(&display_layout_request(&monitors).unwrap(), &config);
    assert!(response.starts_with("ok "), "{response}");
    assert_eq!(load_settings_from_path(config).unwrap().monitors, monitors);
}

#[test]
fn display_transaction_keeps_disconnected_preferences_and_other_domains() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    round_trip("set test true\n", &config);
    round_trip("set color-mode dark\n", &config);
    round_trip("set background-mode cover\n", &config);
    round_trip("set background-image-path %2Ftmp%2Fa%23b.png\n", &config);
    let mut expected = load_settings_from_path(&config).unwrap();
    let third = third_monitor();
    expected.monitors.push(third.clone());

    assert!(round_trip(&display_layout_request(&[third]).unwrap(), &config).starts_with("ok "));
    assert_eq!(load_settings_from_path(config).unwrap(), expected);
}

#[test]
fn partly_invalid_layout_rejects_every_change_without_migrating_legacy_file() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    let legacy = "test = true\nbackground_mode = contains\n";
    fs::write(&config, legacy).unwrap();
    let mut monitors = Settings::default().monitors;
    monitors[0].x = -3000;
    monitors[1].scale = 0;
    // Bypass the client validator to exercise the daemon's trust boundary.
    let payload = serde_json::to_string(&monitors).unwrap();
    let response = round_trip(
        &format!("set display-layout {{\"version\":1,\"monitors\":{payload}}}\n"),
        &config,
    );
    assert!(response.starts_with("err "), "{response}");
    assert_eq!(fs::read_to_string(config).unwrap(), legacy);
}

#[test]
fn successful_update_migrates_legacy_once_and_preserves_quoted_strings_on_next_update() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    fs::write(
        &config,
        "test = true\nbackground_image_path = /tmp/虹 #1.png\nbackground_mode = cover\n",
    )
    .unwrap();
    let before = load_settings_from_path(&config).unwrap();
    assert!(round_trip("get\n", &config).starts_with("ok "));
    assert!(!fs::read_to_string(&config).unwrap().contains("version ="));
    assert!(round_trip("set test false\n", &config).starts_with("ok "));
    assert!(fs::read_to_string(&config).unwrap().contains("version = 1"));
    assert!(round_trip("set test true\n", &config).starts_with("ok "));
    assert_eq!(load_settings_from_path(config).unwrap(), before);
}

#[test]
fn malformed_file_is_never_silently_replaced_by_an_update() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    let contents = "version = 1\nbackground_image_path = \"broken\n";
    fs::write(&config, contents).unwrap();
    assert!(round_trip("get\n", &config).starts_with("err "));
    assert!(round_trip("set background-mode cover\n", &config).starts_with("err "));
    assert_eq!(fs::read_to_string(&config).unwrap(), contents);
}

#[test]
fn path_with_quotes_hash_unicode_and_newline_survives_two_ipc_requests() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    let request = "set background-image-path %2Ftmp%2F%22%E8%99%B9%22%20%23%0A.png\n";
    assert!(round_trip(request, &config).starts_with("ok "));
    assert!(round_trip("set background-mode contains\n", &config).starts_with("ok "));
    let settings = load_settings_from_path(&config).unwrap();
    assert_eq!(
        settings.background_image_path.as_deref(),
        Some("/tmp/\"虹\" #\n.png")
    );
    assert_eq!(settings.background_mode, BackgroundMode::Contains);
    assert!(round_trip("get\n", &config).contains("%2Ftmp%2F%22%E8%99%B9%22%20%23%0A.png"));
}
