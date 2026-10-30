use super::*;

#[test]
fn canvas_tools_round_trip_explicit_commands_and_lowercase_status() {
    for (tool, name) in [
        (CanvasTool::Select, "select"),
        (CanvasTool::Pan, "pan"),
        (CanvasTool::Search, "search"),
        (CanvasTool::Group, "group"),
        (CanvasTool::Layout, "layout"),
    ] {
        let command = ShellCommand::CanvasTool(tool);
        assert_eq!(command.wire_name(), format!("canvas-tool {name}"));
        assert_eq!(ShellCommand::parse(&command.wire_name()), Some(command));
        assert_eq!(serde_json::to_value(tool).unwrap(), name);
        assert_eq!(
            serde_json::from_value::<CanvasTool>(name.into()).unwrap(),
            tool
        );
        assert_eq!(
            ShellCommand::parse(&format!("canvas-tool {name} extra")),
            None
        );
    }
    for invalid in [
        "canvas-tool",
        "canvas-tool hand",
        "canvas-tool Pan",
        "canvas-tool map",
    ] {
        assert_eq!(ShellCommand::parse(invalid), None);
    }
    assert!(serde_json::from_str::<CanvasTool>("\"Pan\"").is_err());
}

#[test]
fn window_commands_round_trip_without_ignoring_extra_arguments() {
    for command in [
        ShellCommand::LockSession,
        ShellCommand::ListApps,
        ShellCommand::ListWindows,
        ShellCommand::GroupSelectionToggle,
        ShellCommand::GroupClear,
        ShellCommand::GroupGather,
        ShellCommand::GroupStatus,
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
    for command in [
        ShellCommand::Spawn(SpawnTarget::A11yTest),
        ShellCommand::Spawn(SpawnTarget::Foot),
        ShellCommand::PanLeft,
        ShellCommand::PanRight,
        ShellCommand::PanUp,
        ShellCommand::PanDown,
        ShellCommand::ZoomIn,
        ShellCommand::ZoomOut,
        ShellCommand::ReloadSettings,
        ShellCommand::LogAccessibilityTree,
    ] {
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
fn search_settings_command_preserves_output_and_rejects_malformed_arguments() {
    for output_name in [
        None,
        Some("HDMI-A-1".into()),
        Some("Virtual output 2".into()),
    ] {
        let command = ShellCommand::OpenSearchSettings { output_name };
        let wire = command.wire_name();
        assert_eq!(ShellCommand::parse(&wire), Some(command));
        assert_eq!(ShellCommand::parse(&format!("{wire} extra")), None);
    }
    for invalid in [
        "open-search-settings 1",
        "open-search-settings gg",
        "open-search-settings ff",
        "open-search-settings --page search",
    ] {
        assert_eq!(ShellCommand::parse(invalid), None);
    }
}

#[test]
fn output_zoom_round_trips_operations_and_encoded_output_names() {
    for output_name in ["eDP-1", "Virtual output 二", "Display\nwith\ttabs"] {
        for operation in [
            ZoomOperation::In,
            ZoomOperation::Out,
            ZoomOperation::To(1.0),
        ] {
            let command = ShellCommand::ZoomOnOutput {
                output_name: output_name.into(),
                operation,
            };
            let wire = command.wire_name();
            assert!(!wire.contains(['\n', '\t']));
            assert_eq!(ShellCommand::parse(&wire), Some(command));
            assert_eq!(ShellCommand::parse(&format!("{wire} extra")), None);
        }
    }
    assert_eq!(
        ShellCommand::ZoomOnOutput {
            output_name: "eDP-1".into(),
            operation: ZoomOperation::To(1.0),
        }
        .wire_name(),
        "zoom-on-output 6544502d31 to 1"
    );
    assert_eq!(ShellCommand::ZoomIn.wire_name(), "zoom-in");
    assert_eq!(ShellCommand::ZoomOut.wire_name(), "zoom-out");
    assert_eq!(ShellCommand::ZoomTo(1.0).wire_name(), "zoom-to 1");
}

#[test]
fn output_zoom_rejects_missing_or_malformed_arguments() {
    for invalid in [
        "zoom-on-output",
        "zoom-on-output 6544502d31",
        "zoom-on-output in",
        "zoom-on-output 1 in",
        "zoom-on-output gg out",
        "zoom-on-output ff to 1",
        "zoom-on-output 6544502d31 reset",
        "zoom-on-output 6544502d31 to",
        "zoom-on-output 6544502d31 to NaN",
        "zoom-on-output 6544502d31 to inf",
        "zoom-on-output 6544502d31 in 1",
    ] {
        assert_eq!(ShellCommand::parse(invalid), None, "accepted {invalid}");
    }
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
