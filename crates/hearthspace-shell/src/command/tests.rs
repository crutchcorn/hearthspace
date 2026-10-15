use super::*;

#[test]
fn window_commands_round_trip_without_ignoring_extra_arguments() {
    for command in [
        ShellCommand::LockSession,
        ShellCommand::ListApps,
        ShellCommand::ListWindows,
        ShellCommand::FocusWindow(17),
        ShellCommand::CloseWindow(17),
        ShellCommand::MoveWindow {
            id: 17,
            x: -300,
            y: 420,
        },
        ShellCommand::ResizeWindow {
            id: 17,
            width: 900,
            height: 700,
        },
        ShellCommand::FocusNextWindow,
        ShellCommand::FocusPreviousWindow,
        ShellCommand::FocusLauncher,
        ShellCommand::AcceptShellEvent(42),
        ShellCommand::ZoomTo(1.25),
        ShellCommand::PanTo { x: -1024, y: 256 },
        ShellCommand::FullscreenWindow {
            id: 17,
            enabled: true,
        },
        ShellCommand::MaximizeWindow {
            id: 17,
            enabled: false,
        },
        ShellCommand::RestoreWindow(17),
    ] {
        let wire = command.wire_name();
        assert_eq!(ShellCommand::parse(&wire), Some(command));
        assert_eq!(ShellCommand::parse(&format!("{wire} unexpected")), None);
    }
    for invalid in [
        "focus-window -1",
        "reset-view",
        "move-window 1 2",
        "resize-window 1 -1 90",
        "pan-to",
        "pan-to 10",
        "pan-to 10.5 20",
        "pan-to NaN 20",
        "pan-to 2147483648 20",
    ] {
        assert_eq!(ShellCommand::parse(invalid), None);
    }
}

#[test]
fn shell_commands_round_trip_wire_names() {
    for command in ShellCommand::STATIC_COMMANDS {
        assert_eq!(ShellCommand::parse(&command.wire_name()), Some(command));
    }
}

#[test]
fn launch_app_parses_desktop_entry_id() {
    assert_eq!(
        ShellCommand::parse("launch-app org.gnome.Calculator.desktop"),
        Some(ShellCommand::LaunchApp(
            "org.gnome.Calculator.desktop".to_string()
        ))
    );
}

#[test]
fn launch_app_on_output_round_trips_output_name() {
    let command = ShellCommand::LaunchAppOnOutput {
        app_id: "org.gnome.Calculator.desktop".to_string(),
        output_name: "HDMI-A-1".to_string(),
    };

    assert_eq!(ShellCommand::parse(&command.wire_name()), Some(command));
}

#[test]
fn waydriver_input_commands_parse() {
    assert_eq!(
        ShellCommand::parse("key-down 30"),
        Some(ShellCommand::KeyDown(30))
    );
    assert_eq!(
        ShellCommand::parse("key-up 0xff"),
        Some(ShellCommand::KeyUp(255))
    );
    assert_eq!(
        ShellCommand::parse("pointer-motion-abs 10.5 20"),
        Some(ShellCommand::PointerMotionAbs { x: 10.5, y: 20.0 })
    );
    assert_eq!(
        ShellCommand::parse("pointer-motion-rel -3 4.25"),
        Some(ShellCommand::PointerMotionRel { dx: -3.0, dy: 4.25 })
    );
    assert_eq!(
        ShellCommand::parse("pointer-button-down 272"),
        Some(ShellCommand::PointerButtonDown(272))
    );
    assert_eq!(
        ShellCommand::parse("axis 0 -120"),
        Some(ShellCommand::Axis {
            horizontal: 0.0,
            vertical: -120.0,
        })
    );
    assert_eq!(
        ShellCommand::parse("screenshot"),
        Some(ShellCommand::Screenshot)
    );
    assert_eq!(ShellCommand::parse("quit"), Some(ShellCommand::Quit));
}

#[test]
fn overview_commands_round_trip_and_reject_malformed_selection() {
    for command in [
        ShellCommand::OverviewOn,
        ShellCommand::OverviewOff,
        ShellCommand::OverviewToggle,
        ShellCommand::OverviewSelect(42),
    ] {
        assert_eq!(ShellCommand::parse(&command.wire_name()), Some(command));
    }
    for invalid in [
        "overview-on extra",
        "overview-select",
        "overview-select 0",
        "overview-select -1",
        "overview-select 42 extra",
    ] {
        assert_eq!(ShellCommand::parse(invalid), None);
    }
}
