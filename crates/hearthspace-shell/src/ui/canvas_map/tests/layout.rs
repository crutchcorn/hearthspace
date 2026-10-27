use hearthspace_common::{AccentColor, Appearance, ColorMode};
use hearthspace_ui_components::{Palette, icons::IconWidget};
use masonry::{
    accesskit::{Live, Rect},
    core::WindowEvent,
};

use super::*;

fn bounds(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>, id: WidgetId) -> Rect {
    ui.harness.access_node(id).unwrap().bounding_box().unwrap()
}

fn image(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>) -> WidgetId {
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Image
                    && node
                        .label()
                        .is_some_and(|name| name.starts_with("Canvas minimap:"))
            })
        })
        .expect("live minimap image")
}

#[test]
fn overview_header_map_and_footer_follow_the_panel_geometry() {
    let ui = Ui::new(state(0.85), panel_view, (352, 312));
    let title = named(&ui, "Overview");
    let title_node = ui.harness.access_node(title).unwrap();
    assert_eq!(title_node.role(), Role::Heading);
    assert_eq!(title_node.level(), Some(2));
    let title = bounds(&ui, title);
    assert_eq!((title.x0, title.y0, title.height()), (21.0, 21.0, 28.0));
    let status = named(&ui, "Current canvas zoom");
    let status_node = ui.harness.access_node(status).unwrap();
    assert_eq!(status_node.role(), Role::Status);
    assert_eq!(status_node.live(), Live::Polite);
    let percentage = bounds(&ui, status);
    assert_eq!(percentage.x1, 331.0);
    assert_eq!((percentage.y0 + percentage.y1) / 2.0, 35.0);
    assert!(title.x1 + 15.0 <= percentage.x0);

    let map = bounds(&ui, image(&ui));
    assert_eq!(map, Rect::new(21.0, 67.0, 331.0, 235.0));
    assert_eq!(map.y0 - title.y1, 18.0);
    let reset = bounds(&ui, ui.button(RESET));
    let out = bounds(&ui, ui.button("Zoom out"));
    let zoom_in = bounds(&ui, ui.button("Zoom in"));
    assert_eq!(reset, Rect::new(195.0, 247.0, 239.0, 291.0));
    assert_eq!(out, Rect::new(241.0, 247.0, 285.0, 291.0));
    assert_eq!(zoom_in, Rect::new(287.0, 247.0, 331.0, 291.0));
    assert_eq!(reset.y0 - map.y1, 12.0);
    assert_eq!(312.0 - zoom_in.y1, 21.0);
    assert_eq!(
        popup::size(&ui.state.outputs[0], PopupKind::Map, 352.0, 312.0).height,
        312.0
    );

    for name in [RESET, "Zoom out", "Zoom in"] {
        let button = ui.button(name);
        let target = bounds(&ui, button);
        let mut pending = vec![button];
        let mut glyphs = Vec::new();
        while let Some(id) = pending.pop() {
            let widget = ui.harness.get_widget_with_id(id);
            if widget.downcast::<IconWidget>().is_some() {
                glyphs.push(
                    widget
                        .ctx()
                        .window_transform()
                        .transform_rect_bbox(widget.ctx().border_box()),
                );
            }
            pending.extend(widget.children().iter().map(|child| child.id()));
        }
        assert_eq!(glyphs.len(), 1, "{name} has one glyph");
        assert_eq!(glyphs[0].size(), masonry::kurbo::Size::new(18.0, 18.0));
        assert_eq!(glyphs[0].center().x, (target.x0 + target.x1) / 2.0);
        assert_eq!(glyphs[0].center().y, (target.y0 + target.y1) / 2.0);
    }
}

#[test]
fn narrow_width_preserves_footer_targets_and_header_order() {
    let mut ui = Ui::new(state(1.25), panel_view, (352, 312));
    ui.harness
        .process_window_event(WindowEvent::Resize((252, 312).into()));
    ui.dispatch(&panel_view);
    let map = bounds(&ui, image(&ui));
    assert_eq!((map.x0, map.x1, map.height()), (21.0, 231.0, 168.0));
    let title = bounds(&ui, named(&ui, "Overview"));
    let percentage = bounds(&ui, named(&ui, "Current canvas zoom"));
    assert!(title.x1 <= percentage.x0);
    assert_eq!(percentage.x1, map.x1);
    let mut previous_right = map.x0;
    for name in [RESET, "Zoom out", "Zoom in"] {
        let button = bounds(&ui, ui.button(name));
        assert_eq!((button.width(), button.height()), (44.0, 44.0));
        assert!(button.x0 >= previous_right && button.x1 <= map.x1);
        assert_eq!(button.y0, 247.0);
        previous_right = button.x1;
    }
    assert_eq!(previous_right, map.x1);
}

#[test]
fn live_dark_mode_and_accent_repaint_the_map_without_replacing_focused_control() {
    let mut ui = Ui::new(state(0.85), panel_view, (352, 312));
    let focused = ui.button("Zoom in");
    ui.harness.focus_on(Some(focused));
    let target = bounds(&ui, focused);
    let map_id = image(&ui);
    let map = bounds(&ui, map_id);
    let before = ui.harness.render();
    ui.state.appearance.update(Palette::new(Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Blue,
    }));
    ui.rebuild(&panel_view);
    ui.dispatch(&panel_view);
    let after = ui.harness.render();
    assert_eq!(ui.harness.focused_widget_id(), Some(focused));
    assert_eq!(ui.button("Zoom in"), focused);
    assert_eq!(bounds(&ui, focused), target);
    assert_eq!(image(&ui), map_id);
    assert!(has_text(&ui, "85%"));
    assert!(ui.state.outputs[0].show_map);
    // Compare only actual map pixels, so a changed percentage cannot mask a
    // custom painter that failed to invalidate after a palette change.
    assert!((map.y0 as u32..map.y1 as u32).any(|y| {
        (map.x0 as u32..map.x1 as u32).any(|x| before.get_pixel(x, y) != after.get_pixel(x, y))
    }));
    ui.state.canvas = Some(snapshot(1.25));
    ui.rebuild(&panel_view);
    ui.dispatch(&panel_view);
    assert!(has_text(&ui, "125%"));
    assert_eq!(ui.harness.focused_widget_id(), Some(focused));
    assert_eq!(bounds(&ui, focused), target);
}

#[test]
fn map_keyboard_focus_ring_survives_the_composed_scroll_viewport() {
    let mut ui = Ui::new(state(1.0), panel_view, (352, 312));
    let map = image(&ui);
    ui.harness.focus_on(Some(ui.button("Zoom out")));
    let before = ui.harness.render();
    ui.harness.press_tab_key(true);
    ui.dispatch(&panel_view);
    assert_eq!(ui.harness.focused_widget_id(), Some(map));
    let focused = ui.harness.render();
    // Check all four sides outside the map, where a content-width Portal used
    // to clip the left/right ring even though standalone widget tests passed.
    for (x, y) in [(17, 151), (334, 151), (176, 63), (176, 238)] {
        assert_ne!(
            before.get_pixel(x, y),
            focused.get_pixel(x, y),
            "focus ring at {x},{y}"
        );
    }
    assert_eq!(bounds(&ui, map), Rect::new(21.0, 67.0, 331.0, 235.0));
}
