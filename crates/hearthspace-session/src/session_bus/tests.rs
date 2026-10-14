use super::*;
use std::ffi::OsStr;

#[test]
fn native_bootstrap_is_skipped_for_the_harness_nested_mode_and_dry_runs() {
    assert!(should_bootstrap(true, false, false));
    assert!(!should_bootstrap(true, true, false));
    assert!(!should_bootstrap(false, false, false));
    assert!(!should_bootstrap(true, false, true));
}

#[test]
fn bootstrap_gives_the_bus_its_private_runtime_before_starting_it() {
    let command = bootstrap_command(
        Path::new("/bin/hearthspace-session"),
        vec!["--installed".into()],
        Path::new("/run/user/1000"),
        Path::new("/run/user/1000/private"),
        false,
        false,
        "wayland-99",
    );
    let args = command.get_args().collect::<Vec<_>>();
    assert_eq!(command.get_program(), "dbus-run-session");
    assert!(args.contains(&OsStr::new("--private-session-bus")));
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--session-dir", "/run/user/1000/private"])
    );
    assert!(args.windows(2).any(|pair| pair == ["--backend", "native"]));
    let variables = command
        .get_envs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        variables[OsStr::new("XDG_RUNTIME_DIR")],
        Some(OsStr::new("/run/user/1000/private"))
    );
    assert_eq!(
        variables[OsStr::new("WAYLAND_DISPLAY")],
        Some(OsStr::new("wayland-99"))
    );
    for key in [
        "DISPLAY",
        "WAYLAND_SOCKET",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        assert_eq!(variables[OsStr::new(key)], None);
    }
}

#[test]
fn activation_updates_only_the_private_bus_with_session_values() {
    let command = activation_command(
        &SessionEnv {
            wayland_display: "wayland-99".into(),
            vars: vec![
                ("XDG_RUNTIME_DIR".into(), "/private/runtime".into()),
                ("WAYLAND_DISPLAY".into(), "wayland-99".into()),
                (
                    "HEARTHSPACE_SESSION_BUS_ADDRESS".into(),
                    "unix:path=/private/bus".into(),
                ),
            ],
        },
        &[],
    );
    let args = command.get_args().collect::<Vec<_>>();
    assert!(!args.contains(&OsStr::new("--systemd")));
    assert!(!args.contains(&OsStr::new("--all")));
    assert!(args.contains(&OsStr::new("WAYLAND_DISPLAY")));
    let variables = command
        .get_envs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        variables[OsStr::new("DBUS_SESSION_BUS_ADDRESS")],
        Some(OsStr::new("unix:path=/private/bus"))
    );
    assert_eq!(variables[OsStr::new("DISPLAY")], Some(OsStr::new("")));
    assert!(!variables.contains_key(OsStr::new("WAYLAND_SOCKET")));
}

#[test]
fn activation_profile_does_not_mutate_authoritative_session_environment() {
    let session = SessionEnv {
        wayland_display: "wayland-99".into(),
        vars: vec![("XDG_CONFIG_HOME".into(), "/ordinary/config".into())],
    };
    let profile = vec![
        ("XDG_CONFIG_HOME".into(), "/private/config".into()),
        ("GSETTINGS_BACKEND".into(), "keyfile".into()),
    ];
    let command = activation_command(&session, &profile);
    let variables = command
        .get_envs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        variables[OsStr::new("XDG_CONFIG_HOME")],
        Some(OsStr::new("/private/config"))
    );
    assert_eq!(
        variables[OsStr::new("GSETTINGS_BACKEND")],
        Some(OsStr::new("keyfile"))
    );
    assert_eq!(session.vars[0].1, "/ordinary/config");
}
