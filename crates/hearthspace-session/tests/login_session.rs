//! Failed display-manager bootstrap must clean its own runtime before returning.
use std::{fs, process::Command};

#[test]
fn missing_login_guard_cleans_prepared_runtime_without_starting_desktop() {
    let root =
        std::env::temp_dir().join(format!("hearthspace-login-failure-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let runtime = root.join("session");
    let output = Command::new(env!("CARGO_BIN_EXE_hearthspace-session"))
        .args([
            "--installed",
            "--backend",
            "native",
            "--login-session",
            "--no-desktop-services",
        ])
        .arg("--binary-dir")
        .arg(&root)
        .arg("--session-dir")
        .arg(&runtime)
        .env("XDG_RUNTIME_DIR", &root)
        .env_remove("HEARTHSPACE_HOST_XDG_RUNTIME_DIR")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("DISPLAY")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing installed login cleanup helper"),
        "{stderr}"
    );
    assert!(
        !runtime.exists(),
        "failed startup left private runtime behind"
    );
    fs::remove_dir(root).unwrap();
}

#[test]
fn login_entry_rejects_an_existing_desktop_before_creating_runtime() {
    let runtime =
        std::env::temp_dir().join(format!("hearthspace-reject-login-{}", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_hearthspace-session"))
        .args(["--installed", "--backend", "native", "--login-session"])
        .arg("--session-dir")
        .arg(&runtime)
        .env("DISPLAY", ":123")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not inside an existing desktop"));
    assert!(!runtime.exists());
}
