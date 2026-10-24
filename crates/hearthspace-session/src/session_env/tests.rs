use super::*;

#[test]
fn session_env_uses_private_runtime_dir() {
    let paths = SessionPaths {
        host_runtime_dir: PathBuf::from("/tmp/hearthspace-test"),
        session_dir: PathBuf::from("/tmp/hearthspace-test/session-1"),
        applications_dir: PathBuf::from("/tmp/hearthspace-test/session-1/share/applications"),
        settings_socket: PathBuf::from("/tmp/hearthspace-test/session-1/settings.sock"),
        session_socket: PathBuf::from("/tmp/hearthspace-test/session-1/session.sock"),
        shell_command_socket: PathBuf::from("/tmp/hearthspace-test/session-1/shell-command.sock"),
    };
    let args = Args {
        backend: Some(crate::cli::Backend::Nested),
        session_dir: None,
        binary_dir: None,
        compositor_binary: None,
        private_session_bus: false,
        startup_timeout: 120,
        dev: false,
        vnc: false,
        vnc_port: 5900,
        installed: true,
        login_session: false,
        wayland_display: Some("hearthspace-test".to_string()),
        watch_apps: false,
        no_watch_apps: false,
        no_shell_restart: false,
        no_settingsd_restart: false,
        no_desktop_services: false,
        log_level: "info".to_string(),
        print_env: false,
        dry_run: false,
    };

    let env = create_session_env(&args, RunMode::InstalledBinaries, &paths).unwrap();

    assert!(env.vars.iter().any(|(key, value)| {
        key == "XDG_RUNTIME_DIR" && value == "/tmp/hearthspace-test/session-1"
    }));
}

#[test]
fn session_env_uses_stable_default_wayland_display() {
    let paths = SessionPaths {
        host_runtime_dir: PathBuf::from("/tmp/hearthspace-test"),
        session_dir: PathBuf::from("/tmp/hearthspace-test/session-1"),
        applications_dir: PathBuf::from("/tmp/hearthspace-test/session-1/share/applications"),
        settings_socket: PathBuf::from("/tmp/hearthspace-test/session-1/settings.sock"),
        session_socket: PathBuf::from("/tmp/hearthspace-test/session-1/session.sock"),
        shell_command_socket: PathBuf::from("/tmp/hearthspace-test/session-1/shell-command.sock"),
    };
    let args = Args {
        backend: Some(crate::cli::Backend::Nested),
        session_dir: None,
        binary_dir: None,
        compositor_binary: None,
        private_session_bus: false,
        startup_timeout: 120,
        dev: false,
        vnc: false,
        vnc_port: 5900,
        installed: true,
        login_session: false,
        wayland_display: None,
        watch_apps: false,
        no_watch_apps: false,
        no_shell_restart: false,
        no_settingsd_restart: false,
        no_desktop_services: false,
        log_level: "info".to_string(),
        print_env: false,
        dry_run: false,
    };

    let env = create_session_env(&args, RunMode::InstalledBinaries, &paths).unwrap();

    assert_eq!(
        env.wayland_display,
        hearthspace_common::WAYLAND_DISPLAY_NAME
    );
    assert!(env.vars.iter().any(|(key, value)| {
        key == hearthspace_common::HOST_RUNTIME_DIR_ENV && value == "/tmp/hearthspace-test"
    }));
}

mod nested;
