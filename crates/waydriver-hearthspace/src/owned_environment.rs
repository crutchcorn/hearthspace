//! An owned compositor must not inherit another desktop's sockets or profiles.
use std::path::Path;
use tokio::process::Command;

pub(super) fn configure(command: &mut Command, runtime: &Path, socket: &Path, display: &str) {
    command
        .env("XDG_RUNTIME_DIR", runtime)
        .env("XDG_CONFIG_HOME", runtime.join("config"))
        .env("XDG_DATA_HOME", runtime.join("data"))
        .env("XDG_STATE_HOME", runtime.join("state"))
        .env("XDG_CACHE_HOME", runtime.join("cache"))
        .env("WAYLAND_DISPLAY", display)
        .env("HEARTHSPACE_WAYLAND_DISPLAY", display)
        .env("HEARTHSPACE_COMMAND_SOCKET", socket)
        .env("HEARTHSPACE_SETTINGS_SOCKET", runtime.join("settings.sock"))
        .env("HEARTHSPACE_LAYOUT_PATH", runtime.join("layout.json"))
        .env_remove("HEARTHSPACE_HOST_XDG_RUNTIME_DIR")
        .env_remove("WAYLAND_SOCKET");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_child_uses_its_owned_sockets_even_with_foreign_or_empty_overrides() {
        let runtime = tempfile::tempdir().unwrap();
        let socket = runtime.path().join("control.sock");
        let mut command = Command::new("/usr/bin/python3");
        command
            .env_clear()
            .env("HEARTHSPACE_COMMAND_SOCKET", "")
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", "/foreign/runtime")
            .env("HEARTHSPACE_SETTINGS_SOCKET", "/foreign/settings.sock")
            .env("HEARTHSPACE_LAYOUT_PATH", "/foreign/layout.json")
            .env("WAYLAND_SOCKET", "42")
            .env("HEARTHSPACE_WAYLAND_DISPLAY", "foreign-display");
        configure(&mut command, runtime.path(), &socket, "wayland-99");
        let keys = [
            "HEARTHSPACE_COMMAND_SOCKET",
            "HEARTHSPACE_SETTINGS_SOCKET",
            "HEARTHSPACE_LAYOUT_PATH",
            "HEARTHSPACE_HOST_XDG_RUNTIME_DIR",
            "WAYLAND_SOCKET",
            "HEARTHSPACE_WAYLAND_DISPLAY",
            "WAYLAND_DISPLAY",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_STATE_HOME",
        ];
        let output = command
            .args([
                "-c",
                "import os,json,sys;print(json.dumps({k:os.getenv(k) for k in sys.argv[1:]}))",
            ])
            .args(keys)
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        let env: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(env["HEARTHSPACE_COMMAND_SOCKET"], socket.to_str().unwrap());
        assert_eq!(
            env["HEARTHSPACE_SETTINGS_SOCKET"],
            runtime.path().join("settings.sock").to_str().unwrap()
        );
        assert_eq!(
            env["HEARTHSPACE_LAYOUT_PATH"],
            runtime.path().join("layout.json").to_str().unwrap()
        );
        assert!(env["HEARTHSPACE_HOST_XDG_RUNTIME_DIR"].is_null());
        assert!(env["WAYLAND_SOCKET"].is_null());
        assert_eq!(env["HEARTHSPACE_WAYLAND_DISPLAY"], "wayland-99");
        assert_eq!(env["WAYLAND_DISPLAY"], "wayland-99");
        for key in ["CONFIG", "DATA", "CACHE", "STATE"] {
            assert_eq!(
                env[format!("XDG_{key}_HOME")],
                runtime.path().join(key.to_lowercase()).to_str().unwrap()
            );
        }
    }
}
