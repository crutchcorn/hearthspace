use super::*;
use crate::{
    ai::AiSettings,
    state::SettingsCategory,
    test_support::{Ui, ids},
};
use xilem::{
    WidgetView,
    masonry::{
        accesskit::{Role, Toggled},
        core::{PointerButton, TextEvent, keyboard::Key},
    },
};

fn state() -> (tempfile::TempDir, SettingsApp) {
    let (dir, favorites) = crate::favorites::tests::fixture();
    (
        dir,
        SettingsApp {
            running: true,
            appearance_theme: Default::default(),
            navigation: Default::default(),
            frame_settings: Default::default(),
            activation_listener: None,
            ai: AiSettings::default(),
            favorites,
            settings: hearthspace_ipc::Settings::default(),
            live_outputs: vec![],
            selected_category: SettingsCategory::Ai,
            selected_monitor_id: None,
            background_preview: Default::default(),
            background_picker_request_sender: None,
            background_picker_pending: false,
            status: String::new(),
        },
    )
}

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    toggle_row(
        "Enable AI features",
        Some("A description of the setting."),
        state.settings.test,
        state.background_picker_pending,
        |state, on| {
            state.settings.test = on;
            state.status.push('x');
        },
    )
}

#[test]
fn clicking_the_left_label_toggles_once_and_disabled_rows_do_nothing() {
    let (_dir, mut state) = state();
    state.settings.test = false;
    let mut ui = Ui::new(state, view, (380, 68));
    let root = ui.harness.root_id();
    let roles: Vec<_> = ids(&ui.harness)
        .into_iter()
        .filter_map(|id| ui.harness.access_node(id).map(|node| node.role()))
        .collect();
    assert_eq!(
        roles.iter().filter(|role| **role == Role::Switch).count(),
        1
    );
    assert!(!roles.contains(&Role::Button));
    assert_eq!(
        ui.harness
            .access_node(root)
            .unwrap()
            .description()
            .as_deref(),
        Some("A description of the setting.")
    );

    ui.harness.mouse_move((24.0, 22.0));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert!(ui.state.settings.test);
    assert_eq!(
        ui.state.status, "x",
        "the label and switch must share one action"
    );
    assert_eq!(
        ui.harness.access_node(root).unwrap().toggled(),
        Some(Toggled::True)
    );

    ui.state.background_picker_pending = true;
    ui.rebuild(&view);
    for x in [24.0, 348.0] {
        ui.harness.mouse_move((x, 34.0));
        ui.harness.mouse_button_press(Some(PointerButton::Primary));
        ui.harness
            .mouse_button_release(Some(PointerButton::Primary));
    }
    ui.activate(root);
    ui.dispatch(&view);
    assert_eq!(ui.state.status, "x");
    assert!(ui.harness.access_node(root).unwrap().is_disabled());
}

#[test]
fn whole_row_hover_is_visible_and_keyboard_focus_remains_on_one_switch() {
    let (_dir, mut state) = state();
    state.settings.test = false;
    let mut ui = Ui::new(state, view, (380, 68));
    let root = ui.harness.root_id();
    ui.harness.mouse_move((-10.0, -10.0));
    let idle = ui.harness.render();
    ui.harness.mouse_move((24.0, 22.0));
    let hovered = ui.harness.render();
    assert_ne!(idle.get_pixel(180, 10), hovered.get_pixel(180, 10));
    ui.harness.mouse_move((-10.0, -10.0));
    ui.harness.press_tab_key(false);
    assert_eq!(ui.harness.focused_widget_id(), Some(root));
    let focused = ui.harness.render();
    let ring = components::FOCUS_RING.to_rgba8().to_u8_array();
    assert!(
        idle.pixels()
            .zip(focused.pixels())
            .any(|(before, after)| { before != after && after.0[..3] == ring[..3] }),
        "keyboard focus must still draw the switch's orange ring"
    );
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    assert_eq!(ui.state.status, "x");
    assert!(ui.state.settings.test);
    ui.harness.press_tab_key(false);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(root),
        "there is only one focus stop"
    );
}

#[test]
fn ai_page_label_click_starts_one_install_and_disables_the_whole_row() {
    fn ai_view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
        xilem::view::sized_box(crate::ai_view::view(state))
    }
    let (_dir, mut state) = state();
    let (sender, mut requests) = xilem::tokio::sync::mpsc::unbounded_channel();
    state.ai.sender = Some(sender);
    state.ai.receive(crate::ai::Response::Finished {
        operation: crate::ai::Operation::Refresh,
        snapshot: Some(hearthspace_ai::ModelStatus::default()),
        error: None,
    });
    let mut ui = Ui::new(state, ai_view, (440, 420));
    let switch = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Switch && node.label().as_deref() == Some("Enable AI features")
            })
        })
        .unwrap();
    let rect = ui.harness.get_widget_with_id(switch).ctx().bounding_box();
    ui.harness.mouse_move((rect.x0 + 24.0, rect.y0 + 34.0));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&ai_view);
    assert_eq!(requests.try_recv().unwrap(), crate::ai::Operation::Install);
    assert!(requests.try_recv().is_err());
    assert!(ui.harness.access_node(switch).unwrap().is_disabled());
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&ai_view);
    assert!(
        requests.try_recv().is_err(),
        "the busy row must not queue another operation"
    );
}
