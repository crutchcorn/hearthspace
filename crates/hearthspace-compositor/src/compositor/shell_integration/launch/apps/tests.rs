//! Isolated child environment exercises preparation without launching user apps.
use super::*;
use std::{fs, path::PathBuf, process::Command};
const CHILD: &str =
    "compositor::shell_integration::launch::apps::tests::prepare_in_isolated_environment";

#[test]
fn launcher_preparation_preserves_terminal_settings_and_activation_only_entries() {
    let root = tempfile::tempdir().unwrap();
    for directory in [
        "home",
        "data/applications",
        "config",
        "runtime",
        "session",
        "bin",
        "cwd with spaces",
    ] {
        fs::create_dir_all(root.path().join(directory)).unwrap();
    }
    let write = |id: &str, contents: &str| {
        fs::write(root.path().join("data/applications").join(id), contents).unwrap()
    };
    write(
        "org.example.Activate.desktop",
        "[Desktop Entry]\nType=Application\nName=Activate\nDBusActivatable=true\n",
    );
    write(
        "org.example.Fallback.desktop",
        "[Desktop Entry]\nType=Application\nName=Fallback\nDBusActivatable=true\nExec=/usr/bin/true --nested-fallback\n",
    );
    write(
        "terminal.desktop",
        "[Desktop Entry]\nType=Application\nName=Terminal\nExec=/usr/bin/true --terminal\nCategories=TerminalEmulator;\nTerminalArgExec=--run\n",
    );
    write(
        "console.desktop",
        &format!(
            "[Desktop Entry]\nType=Application\nName=Console\nExec=/bin/echo \"literal $value\"\nTerminal=true\nPath={}\n",
            root.path().join("cwd with spaces").display()
        ),
    );
    write(
        SETTINGS,
        "[Desktop Entry]\nType=Application\nName=Settings\nExec=/synthetic/settings --session\n",
    );
    write(
        "snap.desktop",
        "[Desktop Entry]\nType=Application\nName=Snap\nExec=/snap/bin/firefox\nX-SnapInstanceName=firefox\n",
    );
    fs::write(
        root.path().join("config/xdg-terminals.list"),
        "terminal.desktop\n",
    )
    .unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CHILD, "--nocapture"])
        .env_clear()
        .env("HEARTHSPACE_TEST_LAUNCH_ROOT", root.path())
        .env("HOME", root.path().join("home"))
        .env("XDG_DATA_HOME", root.path().join("data"))
        .env("XDG_DATA_DIRS", root.path().join("empty"))
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("XDG_CONFIG_DIRS", root.path().join("empty"))
        .env("XDG_RUNTIME_DIR", root.path().join("session"))
        .env(
            hearthspace_common::SESSION_DIR_ENV,
            root.path().join("session"),
        )
        .env(
            hearthspace_common::HOST_RUNTIME_DIR_ENV,
            root.path().join("runtime"),
        )
        .env(
            "HEARTHSPACE_SESSION_BUS_ADDRESS",
            "unix:path=/synthetic/selected-bus",
        )
        .env(
            "HEARTHSPACE_APP_SCOPE_UNIT",
            "hearthspace-apps-test.service",
        )
        .env("PATH", root.path().join("bin"));
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let output = command
        .env_remove("HEARTHSPACE_SESSION_BUS_ADDRESS")
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/synthetic/borrowed-parent",
        )
        .env("HEARTHSPACE_TEST_NESTED_ROUTING", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn prepare_in_isolated_environment() {
    let Some(root) = std::env::var_os("HEARTHSPACE_TEST_LAUNCH_ROOT").map(PathBuf::from) else {
        return;
    };
    let args = |prepared: &Prepared| {
        prepared
            .execution
            .command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    };
    if std::env::var_os("HEARTHSPACE_TEST_NESTED_ROUTING").is_some() {
        assert!(
            prepare("org.example.Activate.desktop".into(), Vec::new()).is_err(),
            "activation-only entry cannot target the borrowed parent desktop"
        );
        let fallback = prepare("org.example.Fallback.desktop".into(), Vec::new()).unwrap();
        assert_eq!(fallback.execution.command.get_program(), "/usr/bin/true");
        assert_eq!(args(&fallback), ["--nested-fallback"]);
        assert!(!args(&fallback).contains(&file_launch::NATIVE_GIO_FLAG.into()));
        super::super::files::tests::assert_nested_routing(&root);
        return;
    }
    let activate = prepare("org.example.Activate.desktop".into(), Vec::new()).unwrap();
    assert!(args(&activate).contains(&file_launch::NATIVE_GIO_FLAG.into()));
    assert_eq!(
        args(&activate).last().unwrap(),
        root.join("data/applications/org.example.Activate.desktop")
            .to_str()
            .unwrap()
    );
    let console = prepare("console.desktop".into(), Vec::new()).unwrap();
    assert_eq!(console.execution.command.get_program(), "/usr/bin/true");
    assert_eq!(
        args(&console),
        ["--terminal", "--run", "/bin/echo", "literal $value"]
    );
    assert_eq!(
        console.execution.command.get_current_dir(),
        Some(root.join("cwd with spaces").as_path())
    );
    let settings = prepare(SETTINGS.into(), vec!["--page".into(), "gestures".into()]).unwrap();
    assert_eq!(
        settings.execution.command.get_program(),
        "/synthetic/settings"
    );
    assert_eq!(args(&settings), ["--session", "--page", "gestures"]);
    let snap = prepare("snap.desktop".into(), Vec::new()).unwrap();
    assert!(args(&snap).contains(&file_launch::SCOPED_GIO_FLAG.into()));
    assert!(
        args(&snap)
            .iter()
            .any(|arg| arg.starts_with("--unit=snap.firefox.firefox-"))
    );
    assert_eq!(
        fs::read_dir(root.join("session/app-scopes"))
            .unwrap()
            .count(),
        1
    );
    assert!(!root.join("runtime/app-scopes").exists());
}
