//! Live property changes update appearance as well as interaction state.
use super::*;

#[test]
fn menu_enabled_changes_update_text_color_and_disabled_accessibility() {
    let mut state = state();
    tray_state::open_menu(&mut state, "eDP-1", &key(1));
    let mut ui = Ui::new(state, panel, (340, 220));
    for enabled in [false, true] {
        ui.state.tray.snapshot.items[0].menu.as_mut().unwrap().items[0].enabled = enabled;
        tray_state::reconcile(&mut ui.state);
        ui.rebuild(&panel);
        ui.dispatch(&panel);
        let row = name(&ui, "First");
        assert_eq!(ui.harness.access_node(row).unwrap().is_disabled(), !enabled);
        let text = ui.harness.get_widget_with_id(row).children()[0].id();
        let color = ui
            .harness
            .get_widget_with_id(text)
            .get_prop::<masonry::properties::ContentColor>()
            .color;
        assert_eq!(
            color,
            if enabled {
                hearthspace_ui_components::TEXT
            } else {
                hearthspace_ui_components::TEXT_MUTED
            }
        );
    }
}
