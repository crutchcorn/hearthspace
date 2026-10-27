use super::*;
use clap::Parser;

fn specification(flags: &[&str], vnc: bool) -> ProcessSpec {
    let args = Args::parse_from(std::iter::once("session").chain(flags.iter().copied()));
    let root = PathBuf::from("/unused/session-specification");
    let paths = SessionPaths {
        host_runtime_dir: root.clone(),
        session_dir: root.clone(),
        applications_dir: root.join("share/applications"),
        settings_socket: root.join("settings.sock"),
        session_socket: root.join("session.sock"),
        shell_command_socket: root.join("commands.sock"),
    };
    let supervisor = Supervisor::new(
        RunMode::InstalledBinaries,
        paths,
        SessionEnv {
            wayland_display: "wayland-test".into(),
            vars: Vec::new(),
        },
        Arc::new(AtomicBool::new(false)),
        &args,
        vnc.then(|| crate::wayvnc::Configuration {
            program: "/usr/bin/wayvnc".into(),
            port: 5900,
        }),
    );
    supervisor.compositor_spec(CompositorBackend::NativeTty)
}

#[test]
fn desktop_capture_does_not_enable_virtual_input_on_either_bus() {
    for flags in [
        vec!["--backend", "native"],
        vec!["--backend", "native", "--private-session-bus"],
    ] {
        let spec = specification(&flags, false);
        assert!(spec.args.iter().any(|arg| arg == "--screen-cast"));
        assert!(!spec.args.iter().any(|arg| arg == "--remote-desktop"));
    }
}

#[test]
fn disabled_desktop_services_and_nested_sessions_do_not_enable_capture() {
    for flags in [
        vec!["--backend", "native", "--no-desktop-services"],
        vec!["--backend", "nested"],
    ] {
        let spec = specification(&flags, false);
        assert!(!spec.args.iter().any(|arg| arg == "--screen-cast"));
        assert!(!spec.args.iter().any(|arg| arg == "--remote-desktop"));
    }
}

#[test]
fn wayvnc_keeps_its_separate_remote_input_opt_in() {
    let spec = specification(&["--backend", "native", "--no-desktop-services"], true);
    assert!(!spec.args.iter().any(|arg| arg == "--screen-cast"));
    assert!(spec.args.iter().any(|arg| arg == "--remote-desktop"));
}
