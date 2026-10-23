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
            ui.harness
                .access_node(*id)
                .is_some_and(|node| node.role() == Role::Switch)
        })
        .unwrap();
    let node = ui.harness.access_node(switch).unwrap();
    assert_eq!(node.label().as_deref(), Some("Make canvas tools hide-able"));
    assert_eq!(
        node.description().as_deref(),
        Some(
            "Removing canvas tools hides their buttons. Press / or Ctrl+K to find and restore them with search."
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
