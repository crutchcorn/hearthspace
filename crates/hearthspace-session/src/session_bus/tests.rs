//! Disposable D-Bus integration; no ambient desktop or process environment.
use super::*;
use std::{collections::BTreeMap, time::Duration};

mod audio;
mod deadlines;
mod display;
mod fixture;
mod manager;
mod publication;
mod retirement;

use fixture::{Bus, pairs};

#[test]
fn shared_payload_uses_canonical_runtime_absolute_display_and_only_app_inputs() {
    let bus = Bus::start();
    let paths = bus.paths("session");
    let mut session = bus.session(&paths);
    session.vars.extend(pairs(&[
        ("RUST_LOG", "trace"),
        ("ARBITRARY_TEST_VALUE", "must-not-escape"),
        ("XDG_CONFIG_HOME", "/synthetic/ordinary/config"),
        ("GSETTINGS_BACKEND", "memory"),
        ("AT_SPI_BUS_ADDRESS", "unix:path=/synthetic/stale-atspi"),
        ("WAYLAND_SOCKET", "99"),
        ("DBUS_SESSION_BUS_PID", "12345"),
        ("DISPLAY", ":91"),
    ]));
    let before = session.vars.clone();
    let profile = pairs(&[
        ("XDG_CONFIG_HOME", "/synthetic/isolated/config"),
        ("GSETTINGS_BACKEND", "keyfile"),
    ]);
    let values = environment::values(&paths, &session, false, &profile).unwrap();
    assert_eq!(
        values["XDG_RUNTIME_DIR"],
        paths.host_runtime_dir.to_str().unwrap()
    );
    assert_eq!(
        values["WAYLAND_DISPLAY"],
        paths.session_dir.join("wayland-test").to_str().unwrap()
    );
    assert_eq!(values["DBUS_SESSION_BUS_ADDRESS"], bus.address);
    assert_eq!(values["DISPLAY"], "");
    for key in [
        "RUST_LOG",
        "ARBITRARY_TEST_VALUE",
        "XDG_CONFIG_HOME",
        "GSETTINGS_BACKEND",
        "AT_SPI_BUS_ADDRESS",
        "WAYLAND_SOCKET",
        "DBUS_SESSION_BUS_PID",
    ] {
        assert!(!values.contains_key(key), "shared payload contains {key}");
    }
    for key in [
        "HEARTHSPACE_SETTINGS_SOCKET",
        "HEARTHSPACE_SESSION_SOCKET",
        "HEARTHSPACE_COMMAND_SOCKET",
        "HEARTHSPACE_SESSION_DIR",
        "HEARTHSPACE_HOST_XDG_RUNTIME_DIR",
        "HEARTHSPACE_SESSION_BUS_ADDRESS",
        "HEARTHSPACE_APP_SCOPE_UNIT",
        "HEARTHSPACE_DEV",
        "HEARTHSPACE_REPO_ROOT",
        "PATH",
        "XDG_DATA_DIRS",
        "XDG_CONFIG_DIRS",
        "GTK_USE_PORTAL",
    ] {
        let expected = before.iter().find(|(name, _)| name == key).unwrap();
        assert_eq!(&values[key], &expected.1, "missing app input {key}");
    }
    assert_eq!(session.vars, before);
}

#[test]
fn private_payload_uses_explicit_profile_without_reading_ambient_profile_settings() {
    let bus = Bus::start();
    let paths = bus.paths("private");
    let mut session = bus.session(&paths);
    session.vars.extend(pairs(&[
        ("XDG_CONFIG_HOME", "/synthetic/ordinary/config"),
        ("AT_SPI_BUS_ADDRESS", "unix:path=/synthetic/stale-atspi"),
        ("WAYLAND_SOCKET", "99"),
    ]));
    let before = session.vars.clone();
    let profile = pairs(&[
        ("XDG_CONFIG_HOME", "/synthetic/isolated/config"),
        ("XDG_CACHE_HOME", "/synthetic/isolated/cache"),
        ("XDG_DATA_HOME", "/synthetic/isolated/data"),
        ("XDG_STATE_HOME", "/synthetic/isolated/state"),
        ("GSETTINGS_BACKEND", "keyfile"),
    ]);
    let private = environment::values(&paths, &session, true, &profile).unwrap();
    let shared = environment::values(&paths, &session, false, &profile).unwrap();
    for (key, value) in &profile {
        assert_eq!(&private[key], value);
        assert!(!shared.contains_key(key));
    }
    assert_eq!(
        private["XDG_RUNTIME_DIR"],
        paths.session_dir.to_str().unwrap()
    );
    assert_eq!(private["WAYLAND_DISPLAY"], "wayland-test");
    assert_eq!(private["DISPLAY"], "");
    assert!(!private.contains_key("AT_SPI_BUS_ADDRESS"));
    assert!(!private.contains_key("WAYLAND_SOCKET"));
    assert_eq!(session.vars, before);
}
