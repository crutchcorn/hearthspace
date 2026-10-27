//! Pointer/touch return focus without a ring; keyboard return remains visible.
use super::{
    navigation_tests::{bounds, key, touch},
    tests::{state, view},
};
use crate::{state::SettingsApp, test_support::Ui};
use masonry::core::{WidgetId, keyboard::NamedKey};
use xilem::WidgetView;

pub(super) fn has_header_ring<V: WidgetView<SettingsApp>>(ui: &mut Ui<V>, toggle: WidgetId) -> bool
where
    V::Widget: Sized,
{
    let rect = bounds(ui, toggle);
    let [r, g, b, _] = ui.state.palette().focus_ring.to_rgba8().to_u8_array();
    ui.harness.render().enumerate_pixels().any(|(x, y, pixel)| {
        y < 49 && !rect.contains((f64::from(x), f64::from(y))) && pixel.0[..3] == [r, g, b]
    })
}

#[test]
fn pointer_category_selection_restores_native_focus_without_a_keyboard_ring() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (480, 490));
    let toggle = ui.button("Settings categories");
    ui.harness.mouse_click_on(toggle, None);
    ui.dispatch(&view);
    ui.harness.mouse_click_on(ui.button("System"), None);
    ui.dispatch(&view);
    ui.harness.mouse_move((479.0, 489.0));
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert!(!ui.state.navigation.open);
    assert!(
        !has_header_ring(&mut ui, toggle),
        "pointer selection must match :focus-visible"
    );

    // Subsequent native Tab/Shift+Tab is still visibly focused: the pointer
    // restoration override cannot leak into later keyboard interaction.
    ui.harness.press_tab_key(false);
    ui.harness.press_tab_key(true);
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert!(
        has_header_ring(&mut ui, toggle),
        "Tab must still show the keyboard ring"
    );

    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    ui.harness.focus_on(Some(ui.button("System")));
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert!(
        has_header_ring(&mut ui, toggle),
        "keyboard category selection restores a visible ring"
    );
}

#[test]
fn touch_selection_is_pointer_origin_and_escape_is_keyboard_origin() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (480, 490));
    let toggle = ui.button("Settings categories");
    let point = bounds(&ui, toggle).center();
    for kind in ["down", "up"] {
        ui.harness
            .process_pointer_event(touch(kind, point.x, point.y));
    }
    ui.dispatch(&view);
    assert!(ui.state.navigation.open);
    let point = bounds(&ui, ui.button("System")).center();
    for kind in ["down", "up"] {
        ui.harness
            .process_pointer_event(touch(kind, point.x, point.y));
    }
    ui.dispatch(&view);
    assert!(!ui.state.navigation.open);
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert!(
        !has_header_ring(&mut ui, toggle),
        "touch button=None must not be mistaken for keyboard"
    );

    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    ui.harness.focus_on(Some(ui.button("Background")));
    key(&mut ui, NamedKey::Escape);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert!(
        has_header_ring(&mut ui, toggle),
        "Escape restores visible keyboard focus"
    );
}
