use std::{
    fs,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::Path,
};

use super::{BindSettingsSocket, bind_settings_socket_at, handle_connection_with_config_path};

#[test]
fn get_returns_default_settings_when_config_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");

    let response = round_trip("get\n", &config_path);

    assert!(response.starts_with(
            "ok test false background-mode repeat-with-canvas background-image-path default monitor primary 1920 1080 1 0 0 Primary"
        ));
    assert!(response.contains("monitor secondary 1920 1080 1 1920 0 Secondary"));
    assert!(!config_path.exists());
}

#[test]
fn get_reads_existing_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    fs::write(
        &config_path,
        "test = true\n\
             background_mode = contains\n\
             background_image_path = /tmp/wallpaper.png\n\
             \n\
             [[monitor]]\n\
             id = dock\n\
             name = Dock Display\n\
             width = 2560\n\
             height = 1440\n\
             scale = 2\n\
             x = -2560\n\
             y = 120\n",
    )
    .unwrap();

    let response = round_trip("get\n", &config_path);

    assert_eq!(
        response,
        "ok test true background-mode contains background-image-path %2Ftmp%2Fwallpaper.png monitor dock 2560 1440 2 -2560 120 Dock%20Display color-mode light accent-color orange\n"
    );
}

#[test]
fn appearance_persists_across_requests_and_rejects_invalid_changes() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    round_trip("set background-mode cover\n", &config);
    for mode in hearthspace_ipc::ColorMode::ALL {
        round_trip(&format!("set color-mode {mode}\n"), &config);
        for accent in hearthspace_ipc::AccentColor::ALL {
            let response = round_trip(&format!("set accent-color {accent}\n"), &config);
            assert!(response.contains(&format!("color-mode {mode} accent-color {accent}")));
            assert!(response.contains("background-mode cover"));
            assert_eq!(response, round_trip("get\n", &config));
            let contents = fs::read_to_string(&config).unwrap();
            assert!(contents.contains(&format!("color_mode = \"{mode}\"\n")));
            assert!(contents.contains(&format!("accent_color = \"{accent}\"\n")));
        }
    }
    let original = fs::read(&config).unwrap();
    for invalid in ["set color-mode auto\n", "set accent-color pink\n"] {
        assert!(round_trip(invalid, &config).starts_with("err "));
        assert_eq!(fs::read(&config).unwrap(), original);
    }
}

#[test]
fn get_reads_legacy_background_moves_with_canvas_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    fs::write(
        &config_path,
        "test = true\n\
             background_moves_with_canvas = false\n",
    )
    .unwrap();

    let response = round_trip("get\n", &config_path);

    assert!(response.starts_with(
            "ok test true background-mode cover background-image-path default monitor primary 1920 1080 1 0 0 Primary"
        ));
}

#[test]
fn set_test_writes_config_and_returns_saved_settings() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");

    let response = round_trip("set test true\n", &config_path);

    assert!(response.starts_with(
            "ok test true background-mode repeat-with-canvas background-image-path default monitor primary 1920 1080 1 0 0 Primary"
        ));
    let contents = fs::read_to_string(config_path).unwrap();
    assert!(contents.contains("version = 1\n"));
    assert!(contents.contains("test = true\n"));
    assert!(contents.contains("background_mode = \"repeat-with-canvas\"\n"));
    assert!(!contents.contains("background_image_path ="));
    assert!(contents.contains("[[monitor]]\nid = \"primary\"\n"));
    assert!(contents.contains("scale = 1\n"));
}

#[test]
fn set_background_mode_writes_config_and_returns_saved_settings() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");

    let response = round_trip("set background-mode cover\n", &config_path);

    assert!(response.starts_with(
            "ok test false background-mode cover background-image-path default monitor primary 1920 1080 1 0 0 Primary"
        ));
    let contents = fs::read_to_string(config_path).unwrap();
    assert!(contents.contains("background_mode = \"cover\"\n"));
}

#[test]
fn set_background_image_path_writes_config_and_returns_saved_settings() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");

    let response = round_trip(
        "set background-image-path %2Ftmp%2Fwallpaper.png\n",
        &config_path,
    );

    assert!(response.starts_with(
            "ok test false background-mode repeat-with-canvas background-image-path %2Ftmp%2Fwallpaper.png monitor primary 1920 1080 1 0 0 Primary"
        ));
    let contents = fs::read_to_string(config_path).unwrap();
    assert!(contents.contains("background_image_path = \"/tmp/wallpaper.png\"\n"));
}

#[test]
fn set_background_image_path_default_clears_saved_path() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");

    let response = round_trip("set background-image-path default\n", &config_path);

    assert!(response.starts_with(
            "ok test false background-mode repeat-with-canvas background-image-path default monitor primary 1920 1080 1 0 0 Primary"
        ));
    let contents = fs::read_to_string(config_path).unwrap();
    assert!(!contents.contains("background_image_path ="));
}

#[test]
fn set_monitor_position_writes_config_and_returns_layout() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");

    let response = round_trip("set monitor secondary -1600 90\n", &config_path);

    assert!(response.contains("monitor secondary 1920 1080 1 -1600 90 Secondary"));
    let contents = fs::read_to_string(config_path).unwrap();
    assert!(contents.contains("id = \"secondary\"\n"));
    assert!(contents.contains("x = -1600\n"));
    assert!(contents.contains("y = 90\n"));
}

#[test]
fn set_monitor_scale_writes_config_and_returns_layout() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");

    let response = round_trip("set monitor-scale secondary 2\n", &config_path);

    assert!(response.contains("monitor secondary 1920 1080 2 1920 0 Secondary"));
    let contents = fs::read_to_string(config_path).unwrap();
    assert!(contents.contains("id = \"secondary\"\n"));
    assert!(contents.contains("scale = 2\n"));
}

#[test]
fn set_monitor_position_rejects_unknown_monitor() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");

    let response = round_trip("set monitor missing 10 20\n", &config_path);

    assert_eq!(response, "err unknown monitor id: missing\n");
    assert!(!config_path.exists());
}

#[test]
fn invalid_request_returns_error_without_writing_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("hearthspace/config.toml");

    let response = round_trip("set nope true\n", &config_path);

    assert!(response.starts_with("err unknown request:"));
    assert!(!config_path.exists());
}

#[test]
fn bind_exits_cleanly_when_listener_is_already_active() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("settings.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

    let result = bind_settings_socket_at(socket_path.clone()).unwrap();

    assert!(matches!(result, BindSettingsSocket::AlreadyRunning(path) if path == socket_path));
}

#[test]
fn bind_removes_stale_socket_file() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("settings.sock");
    std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

    let result = bind_settings_socket_at(socket_path).unwrap();

    assert!(matches!(result, BindSettingsSocket::Listening(_)));
}

#[test]
fn bind_keeps_a_live_socket_with_a_full_backlog() {
    use nix::sys::socket::{Backlog, listen};
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("settings.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    listen(&listener, Backlog::new(0).unwrap()).unwrap();
    let _queued = UnixStream::connect(&socket_path).unwrap();
    assert!(matches!(
        bind_settings_socket_at(socket_path.clone()).unwrap(),
        BindSettingsSocket::AlreadyRunning(path) if path == socket_path
    ));
    assert!(socket_path.exists());
}

pub(super) fn round_trip(request: &str, config_path: &Path) -> String {
    let (server, mut client) = UnixStream::pair().unwrap();
    client.write_all(request.as_bytes()).unwrap();
    client.shutdown(Shutdown::Write).unwrap();

    handle_connection_with_config_path(server, config_path).unwrap();

    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    response
}
