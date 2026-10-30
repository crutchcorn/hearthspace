use super::*;
use crate::desktop_services::tests::Fixture;

#[test]
fn discovery_prefers_the_explicit_local_install_and_reports_missing_backend() {
    let fixture = Fixture::new("backend-discovery");
    let local = fixture.0.join("local-backend");
    let distro = fixture.0.join("distro-backend");
    for path in [&local, &distro] {
        fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_eq!(
        discover([local.as_path(), distro.as_path()]).unwrap(),
        local
    );
    fs::set_permissions(&local, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        discover([local.as_path(), distro.as_path()]).unwrap(),
        distro
    );
    fs::remove_file(&distro).unwrap();
    let error = discover([local.as_path(), distro.as_path()]).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(error.to_string().contains(">= 0.8.4"));
    assert!(discover([Path::new("relative-backend")]).is_err());
}

#[test]
fn helper_location_stays_with_the_selected_release_or_dev_checkout() {
    let release = Path::new("/opt/hearthspace/releases/test/bin");
    assert_eq!(
        picker(RunMode::InstalledBinaries, Some(release)).unwrap(),
        release.join(PICKER)
    );
    assert_eq!(
        picker(RunMode::InstalledBinaries, None).unwrap(),
        env::current_exe().unwrap().parent().unwrap().join(PICKER)
    );
    assert_eq!(
        picker(RunMode::DevCargo, Some(release)).unwrap(),
        crate::session_env::repo_root()
            .join("scripts/runtime")
            .join(PICKER)
    );
}

#[test]
fn picker_path_with_an_ini_comment_delimiter_preserves_the_existing_configuration() {
    let fixture = Fixture::new("picker-ini-comment");
    let release = fixture.0.join("project ; copy");
    fs::create_dir(&release).unwrap();
    let picker = release.join(PICKER);
    let backend = fixture.0.join("backend");
    for path in [&picker, &backend] {
        fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let paths = SessionPaths {
        host_runtime_dir: fixture.0.clone(),
        session_dir: fixture.0.clone(),
        applications_dir: fixture.0.join("share/applications"),
        settings_socket: fixture.0.join("settings.sock"),
        session_socket: fixture.0.join("session.sock"),
        shell_command_socket: fixture.0.join("command.sock"),
    };
    let session = SessionEnv {
        wayland_display: "wayland-fixture".into(),
        vars: vec![
            ("WAYLAND_DISPLAY".into(), "wayland-fixture".into()),
            ("XDG_RUNTIME_DIR".into(), fixture.0.to_str().unwrap().into()),
            (
                "HEARTHSPACE_SESSION_BUS_ADDRESS".into(),
                format!("unix:path={}", fixture.0.join("bus").display()),
            ),
        ],
    };
    let config = fixture.0.join("screencast.conf");
    fs::write(&config, "previous configuration\n").unwrap();

    let error = spec(&paths, &session, &backend, &picker).unwrap_err();

    assert!(error.to_string().contains("picker path contains ';'"));
    assert!(error.to_string().contains("backend INI configuration"));
    assert_eq!(
        fs::read_to_string(config).unwrap(),
        "previous configuration\n"
    );
}
