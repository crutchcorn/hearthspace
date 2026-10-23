use super::*;
use crate::ui::window_catalog::RunningWindow;

fn apps(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(widgets::content(
        state,
        &state.outputs[0],
        "apps",
        false,
        false,
        Default::default(),
        &state.frame_layout.chains[&widget_key("test", "apps")],
    ))
}

#[test]
fn compact_running_apps_page_inside_remaining_bar_width_and_keep_launcher_reachable() {
    let mut state = state();
    state.windows = (1..=3)
        .map(|id| RunningWindow {
            id,
            app_id: format!("app{id}"),
            title: format!("App {id}"),
            focused: false,
        })
        .collect();
    state.frame_layout.snapshot.outputs[0].widgets.push(Widget {
        id: "apps".into(),
        merged: true,
        rect: Rect {
            width: 140.0,
            height: 44.0,
            ..Default::default()
        },
        ..Default::default()
    });
    let mut ui = Ui::new(state, apps, (140, 44));
    for id in ids(&ui.harness) {
        if let Some(node) = ui.harness.access_node(id)
            && node.role() == Role::Button
        {
            let bounds = node.bounding_box().unwrap();
            assert!(bounds.x0 >= 0.0 && bounds.x1 <= 140.0);
        }
    }
    ui.button("Switch to app1");
    ui.button("Apps List");
    ui.activate(ui.button("Next apps (page 1 of 3)"));
    ui.dispatch(&apps);
    ui.button("Switch to app2");
    ui.button("Apps List");
}
