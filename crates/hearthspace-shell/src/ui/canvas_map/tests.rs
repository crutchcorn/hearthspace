use masonry::{accesskit::Role, core::WidgetId};
use serde_json::json;
use xilem::view::sized_box;

use super::*;
use crate::ui::{
    minimap::parse_snapshot,
    test_support::{Ui, ids},
};

mod interaction;
mod layout;

const RESET: &str = "Reset canvas zoom to 100%";

fn snapshot(scale: f64) -> CanvasSnapshot {
    parse_snapshot(
        &serde_json::to_vec(&json!({
            "viewport": {"x": -200, "y": 300, "scale": scale},
            "outputs": [{"name":"eDP-1", "logical_x":0, "logical_y":0,
                "logical_width":1280, "logical_height":800}], "windows": []
        }))
        .unwrap(),
    )
    .unwrap()
}

fn state(scale: f64) -> ShellState {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.outputs[0].show_map = true;
    state.canvas = Some(snapshot(scale));
    state
}

fn named(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>, name: &str) -> WidgetId {
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.label().as_deref() == Some(name) || node.value().as_deref() == Some(name)
            })
        })
        .unwrap_or_else(|| panic!("Missing accessible content: {name}"))
}

fn has_text(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>, text: &str) -> bool {
    ids(&ui.harness).into_iter().any(|id| {
        ui.harness
            .access_node(id)
            .is_some_and(|node| node.value().as_deref() == Some(text))
    })
}

fn panel_view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(panel(
        &state.outputs[0],
        state.canvas.as_ref(),
        state.canvas_error.as_deref(),
    ))
}

fn button_view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(map_button(&state.outputs[0]))
}

#[test]
fn map_icon_is_named_and_toggles_after_rebuilding() {
    let state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    let mut ui = Ui::new(state, button_view, (80, 80));
    let id = ui.button("Canvas map");
    ui.activate(id);
    ui.dispatch(&button_view);
    assert!(ui.state.outputs[0].show_map);
    ui.harness.mouse_click_on(ui.button("Canvas map"), None);
    ui.dispatch(&button_view);
    assert!(!ui.state.outputs[0].show_map);
}

#[test]
fn live_minimap_rebuild_updates_accessibility_and_pixels() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    let value = json!({
        "viewport": {"x": 0, "y": 0, "scale": 1},
        "outputs": [{"name":"eDP-1", "logical_x":0, "logical_y":0,
            "logical_width":1280, "logical_height":800}], "windows": []
    });
    state.canvas = Some(parse_snapshot(&serde_json::to_vec(&value).unwrap()).unwrap());
    let mut ui = Ui::new(state, panel_view, (352, 312));
    let before = ui.harness.render();
    let mut changed = value;
    changed["windows"] = json!([{"id": 1, "title": "Remote Notes", "app_id": "editor",
        "focused": true, "canvas_x": -19000, "canvas_y": 12000,
        "content_width": 900, "content_height": 600, "titlebar_height": 32}]);
    ui.state.canvas = Some(parse_snapshot(&serde_json::to_vec(&changed).unwrap()).unwrap());
    ui.rebuild(&panel_view);
    ui.dispatch(&panel_view);
    let after = ui.harness.render();
    assert_ne!(
        before, after,
        "new canvas geometry must update the map image"
    );
    let descriptions: Vec<_> = ids(&ui.harness)
        .into_iter()
        .filter_map(|id| {
            let node = ui.harness.access_node(id)?;
            (node.role() == Role::Image).then(|| node.label()).flatten()
        })
        .collect();
    assert!(
        descriptions
            .iter()
            .any(|text| text.starts_with("Canvas minimap: 1 windows.")
                && text.contains("on eDP-1")
                && text.contains("Remote Notes"))
    );
}

#[test]
fn small_output_keeps_zoom_controls_reachable_by_scrolling() {
    let state = ShellState::new("/missing-map-test-socket".into(), vec!["eDP-1".into()]);
    let mut ui = Ui::new(state, panel_view, (320, 312));
    ui.harness
        .process_window_event(masonry::core::WindowEvent::Resize((320, 192).into()));
    ui.dispatch(&panel_view);
    let zoom = ui.button("Zoom out");
    let before = ui
        .harness
        .access_node(zoom)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(before.y1 > 192.0, "footer initially needs scrolling");
    ui.harness.scroll_into_view(zoom);
    // ScrollIntoView updates layout/compose but the harness publishes the
    // resulting accessibility bounds only on redraw.
    ui.dispatch(&panel_view);
    let visible = ui
        .harness
        .access_node(zoom)
        .unwrap()
        .bounding_box()
        .unwrap();
    // The padding scrolls with Overview's content, leaving painting space for
    // the map's exterior focus ring. The button must stay within the border.
    assert!(visible.y0 >= 1.0 && visible.y1 <= 191.0, "{visible:?}");
    assert_eq!(visible.height(), 44.0);
    // This checks hit testing and clipping, unlike an accessibility action
    // which is intentionally allowed to activate offscreen controls.
    ui.harness.mouse_click_on(zoom, None);
    ui.dispatch(&panel_view);
    assert!(
        ui.state.canvas_error.is_some(),
        "the visible button must attempt its command"
    );
    assert!(
        has_text(&ui, "Map unavailable · retrying…"),
        "a failed map command must retain its unavailable feedback"
    );
    ui.state.canvas_error = None;
    ui.rebuild(&panel_view);
    ui.dispatch(&panel_view);
    assert!(!has_text(&ui, "Map unavailable · retrying…"));
    assert!(!has_text(&ui, "Drag the blue area to move your view"));
    ui.button("Zoom out");
    ui.button("Zoom in");
}
