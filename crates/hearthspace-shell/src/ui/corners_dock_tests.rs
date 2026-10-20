use super::*;
use crate::ui::{test_support::Ui, window_catalog::RunningWindow};
use masonry::core::{TextEvent, WindowEvent};

fn apps(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(apps_view(state, &state.outputs[0]))
}

#[test]
fn closing_an_app_keeps_apps_list_under_a_stationary_pointer_before_surface_resize() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.catalog = Default::default();
    state.favorites.tracks_installed_apps = false;
    state.windows = vec![
        RunningWindow {
            id: 1,
            app_id: "foot".into(),
            title: "Terminal".into(),
            focused: false,
        },
        RunningWindow {
            id: 2,
            app_id: "dev.hearthspace.Settings".into(),
            title: "Hearthspace Settings".into(),
            focused: true,
        },
    ];
    let mut ui = Ui::new(state, apps, (150, 54));
    let button = ui.button("Apps List");
    let before = ui
        .harness
        .access_node(button)
        .unwrap()
        .bounding_box()
        .unwrap();
    ui.harness.mouse_move_to(button);

    // The catalog changes before the compositor's smaller configure arrives.
    // A click still targets the same screen position in the old 150px surface.
    ui.state.windows.retain(|window| window.id != 2);
    ui.rebuild(&apps);
    ui.dispatch(&apps);
    assert_eq!(apps_width(&ui.state), 102.0);
    assert_eq!(ui.button("Apps List"), button);
    assert_eq!(
        ui.harness.access_node(button).unwrap().bounding_box(),
        Some(before),
        "removing a running app must preserve the rightmost pointer target until resize"
    );
    ui.harness.mouse_button_press(None);
    ui.harness.mouse_button_release(None);
    ui.dispatch(&apps);
    assert!(ui.state.outputs[0].show_search);

    // Closing and reopening at the same location still works after deactivation.
    ui.state.dismiss_search("eDP-1");
    ui.rebuild(&apps);
    ui.dispatch(&apps);
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(false));
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(true));
    ui.harness.mouse_button_press(None);
    ui.harness.mouse_button_release(None);
    ui.dispatch(&apps);
    assert!(ui.state.outputs[0].show_search);

    ui.harness
        .process_window_event(WindowEvent::Resize((102, 54).into()));
    ui.dispatch(&apps);
    let after = ui
        .harness
        .access_node(button)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert_eq!(150.0 - before.x1, 102.0 - after.x1);
    assert_eq!(before.width(), after.width());

    ui.state.windows.clear();
    ui.rebuild(&apps);
    ui.dispatch(&apps);
    assert_eq!(
        apps_width(&ui.state),
        54.0,
        "empty dock must not reserve a gap"
    );
    assert_eq!(
        ui.harness.access_node(button).unwrap().bounding_box(),
        Some(after)
    );
    ui.harness
        .process_window_event(WindowEvent::Resize((54, 54).into()));
    ui.dispatch(&apps);
    let empty = ui
        .harness
        .access_node(button)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert_eq!(150.0 - before.x1, 54.0 - empty.x1);
}
