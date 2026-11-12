use super::{
    navigation_tests::key,
    tests::{state, view},
};
use crate::{
    frame_settings::{Operation, Snapshot},
    state::SettingsCategory,
    test_support::{Ui, ids},
};
use masonry::{
    accesskit::{Role, Toggled},
    core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    },
};
use xilem::tokio::sync::mpsc::unbounded_channel;

#[test]
fn system_actions_are_named_keyboard_controls_and_keep_settings_open() {
    for (name, expected) in [
        ("Change layout", Operation::OpenEditor),
        ("Restore canvas tools", Operation::RestoreTools),
    ] {
        let (_dir, mut state) = state();
        let (sender, mut requests) = unbounded_channel();
        state.frame_settings.sender = Some(sender);
        state.frame_settings.available = true;
        state.frame_settings.snapshot = Some(Snapshot {
            allow_hide_tools: false,
        });
        state.selected_category = SettingsCategory::System;
        let mut ui = Ui::new(state, view, (680, 490));
        let action = ui.button(name);
        assert!(!ui.harness.access_node(action).unwrap().is_disabled());
        ui.harness.focus_on(Some(action));
        key(&mut ui, NamedKey::Enter);
        ui.dispatch(&view);
        assert_eq!(requests.try_recv().unwrap(), expected);
        assert!(
            ui.state.running,
            "opening the editor keeps Settings available"
        );
        assert_eq!(ui.harness.focused_widget_id(), Some(action));
    }
}

#[test]
fn hide_tools_switch_waits_for_confirmation_and_live_refresh_preserves_focus() {
    let (_dir, mut state) = state();
    let (sender, mut requests) = unbounded_channel();
    state.frame_settings.sender = Some(sender);
    state.frame_settings.available = true;
    state.frame_settings.snapshot = Some(Snapshot {
        allow_hide_tools: false,
    });
    state.selected_category = SettingsCategory::System;
    let mut ui = Ui::new(state, view, (400, 490));
    let switch = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Switch
                    && node.label().as_deref() == Some("Make canvas tools hide-able")
            })
        })
        .unwrap();
    let node = ui.harness.access_node(switch).unwrap();
    assert_eq!(node.label().as_deref(), Some("Make canvas tools hide-able"));
    assert_eq!(
        node.description().as_deref(),
        Some(
            "Removing canvas tools hides their buttons. Press ❖ + K to find and restore them with search."
        )
    );
    ui.harness.focus_on(Some(switch));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    assert_eq!(
        requests.try_recv().unwrap(),
        Operation::AllowHideTools(true)
    );
    assert_eq!(
        ui.harness.access_node(switch).unwrap().toggled(),
        Some(Toggled::False)
    );
    ui.state.frame_settings.snapshot = Some(Snapshot {
        allow_hide_tools: true,
    });
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(switch));
    assert_eq!(
        ui.harness.access_node(switch).unwrap().toggled(),
        Some(Toggled::True)
    );
}

#[test]
fn system_controls_do_not_claim_availability_before_the_compositor_responds() {
    let (_dir, mut state) = state();
    state.selected_category = SettingsCategory::System;
    let mut ui = Ui::new(state, view, (680, 490));
    for name in ["Change layout", "Restore canvas tools"] {
        let button = ui.button(name);
        assert!(ui.harness.access_node(button).unwrap().is_disabled());
        ui.activate(button);
    }
    ui.dispatch(&view);
    assert!(ui.state.frame_settings.snapshot.is_none());
}

#[test]
fn wireless_panels_use_keyboard_actions_and_render_in_settings_palettes() {
    use hearthspace_common::ColorMode;
    use hearthspace_desktop::wireless::{Command, Item, Radio};
    use xilem::view::sized_box;
    for bluetooth in [false, true] {
        for (mode, theme) in [(ColorMode::Light, "light"), (ColorMode::Dark, "dark")] {
            let (_dir, mut state) = state();
            state.settings.appearance.color_mode = mode;
            let (sender, mut commands) = xilem::tokio::sync::mpsc::channel(4);
            state.wireless.sender = Some(sender);
            let radio = Radio {
                enabled: true,
                blocked: false,
                items: vec![Item {
                    id: "private-owner|device".into(),
                    name: "Studio device".into(),
                    detail: "Available nearby".into(),
                    paired: true,
                    ..Default::default()
                }],
            };
            if bluetooth {
                state.wireless.snapshot.bluetooth = Ok(radio)
            } else {
                state.wireless.snapshot.wifi = Ok(radio)
            }
            let build = move |s: &crate::state::SettingsApp| {
                sized_box(crate::wireless::panel(s, bluetooth))
            };
            let mut ui = Ui::new(state, build, (360, 400));
            let button = ui.button("Connect");
            ui.harness.focus_on(Some(button));
            key(&mut ui, NamedKey::Enter);
            ui.dispatch(&build);
            let command = commands.try_recv().unwrap();
            assert!(if bluetooth {
                matches!(command,Command::ConnectBluetooth(id) if id=="private-owner|device")
            } else {
                matches!(command,Command::ConnectWifi(id) if id=="private-owner|device")
            });
            assert!(ui.state.wireless.busy);
            assert!(ui.state.running);
            ui.state.wireless.busy = false;
            ui.rebuild(&build);
            ui.dispatch(&build);
            if let Some(base) = std::env::var_os("HEARTHSPACE_SHELL_EVIDENCE") {
                let dir = std::path::PathBuf::from(base).join("wireless-settings");
                std::fs::create_dir_all(&dir).unwrap();
                ui.harness
                    .render()
                    .save(dir.join(format!(
                        "{}-{theme}.png",
                        if bluetooth { "bluetooth" } else { "wifi" }
                    )))
                    .unwrap();
            }
        }
    }
}
