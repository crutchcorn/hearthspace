use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use masonry::{
    accesskit::{Action, ActionRequest, Role, TreeId},
    core::{Widget, WidgetId},
    testing::TestHarness,
};
use serde_json::json;
use xilem::{
    ViewCtx,
    core::{DynMessage, MessageCtx, MessageResult, RawProxy, ViewPathTracker},
};

use super::*;
use crate::ui::minimap::parse_snapshot;

#[derive(Debug)]
struct NoMessages;

impl RawProxy for NoMessages {
    fn send_message(
        &self,
        _path: Arc<[xilem::core::ViewId]>,
        _message: xilem::core::SendMessage,
    ) -> Result<(), xilem::core::ProxyError> {
        panic!("map views must not start background tasks")
    }

    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

struct Ui<V: WidgetView<ShellState>>
where
    V::Widget: Sized,
{
    view: V,
    view_state: V::ViewState,
    state: ShellState,
    ctx: ViewCtx,
    harness: TestHarness<V::Widget>,
}

impl<V: WidgetView<ShellState>> Ui<V>
where
    V::Widget: Sized,
{
    fn new(mut state: ShellState, build: impl Fn(&ShellState) -> V, width: u32) -> Self {
        let runtime = xilem::tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
        let view = build(&state);
        let (root, view_state) = view.build(&mut ctx, &mut state);
        let harness = TestHarness::create_with_size(
            components::hearthspace_light_properties(),
            root.new_widget,
            (width, 336),
        );
        let mut ui = Self {
            view,
            view_state,
            state,
            ctx,
            harness,
        };
        ui.dispatch(&build);
        ui
    }

    fn rebuild(&mut self, build: &impl Fn(&ShellState) -> V) {
        let next = build(&self.state);
        self.harness.edit_root_widget(|root| {
            next.rebuild(
                &self.view,
                &mut self.view_state,
                &mut self.ctx,
                root,
                &mut self.state,
            );
        });
        self.view = next;
    }

    fn dispatch(&mut self, build: &impl Fn(&ShellState) -> V) {
        while let Some((action, id)) = self.harness.pop_action_erased() {
            let path = self.ctx.get_id_path(id).unwrap().clone();
            let mut message = MessageCtx::new(
                std::mem::take(self.ctx.environment()),
                path,
                DynMessage(action),
            );
            let result = self.harness.edit_root_widget(|root| {
                self.view
                    .message(&mut self.view_state, &mut message, root, &mut self.state)
            });
            assert!(matches!(
                result,
                MessageResult::Action(()) | MessageResult::RequestRebuild
            ));
            let (environment, _, _) = message.finish();
            *self.ctx.environment() = environment;
            self.rebuild(build);
        }
        self.harness.redraw();
    }

    fn activate(&mut self, id: WidgetId) {
        self.harness.process_access_event(ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: id.to_raw().into(),
            data: None,
        });
    }

    fn button(&self, name: &str) -> WidgetId {
        ids(&self.harness)
            .into_iter()
            .find(|id| {
                self.harness.access_node(*id).is_some_and(|node| {
                    node.role() == Role::Button && node.label().as_deref() == Some(name)
                })
            })
            .unwrap_or_else(|| panic!("Missing accessible button: {name}"))
    }
}

fn ids<W: Widget>(harness: &TestHarness<W>) -> Vec<WidgetId> {
    let mut result = vec![];
    let mut pending = vec![harness.root_id()];
    while let Some(id) = pending.pop() {
        pending.extend(
            harness
                .get_widget_with_id(id)
                .children()
                .iter()
                .map(|child| child.id()),
        );
        result.push(id);
    }
    result
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
    let mut ui = Ui::new(state, button_view, 80);
    let id = ui.button("Canvas map");
    ui.activate(id);
    ui.dispatch(&button_view);
    assert!(ui.state.outputs[0].show_map);
    ui.harness.mouse_click_on(ui.button("Canvas map"), None);
    ui.dispatch(&button_view);
    assert!(!ui.state.outputs[0].show_map);
}

#[test]
fn popover_icons_have_accessible_names_and_dispatch_zoom_commands() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (sent, received) = mpsc::channel();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut count = 0;
        while Instant::now() < deadline && count < 2 {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = String::new();
                BufReader::new(&stream).read_line(&mut request).unwrap();
                stream.write_all(b"ok\n").unwrap();
                sent.send(request).unwrap();
                count += 1;
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
    });
    let mut state = ShellState::new(socket, vec!["eDP-1".into()]);
    state.outputs[0].show_map = true;
    let mut ui = Ui::new(state, panel_view, 340);
    for label in ["Zoom out", "Zoom in"] {
        if label == "Zoom in" {
            ui.harness.focus_on(Some(ui.button("Zoom out")));
            ui.harness.press_tab_key(false);
            assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Zoom in")));
            ui.harness
                .process_text_event(masonry::core::TextEvent::key_up(
                    masonry::core::keyboard::Key::Named(masonry::core::keyboard::NamedKey::Enter),
                ));
        } else {
            ui.activate(ui.button(label));
        }
        ui.dispatch(&panel_view);
        assert!(ui.state.canvas_error.is_none());
        assert!(ui.state.outputs[0].show_map);
    }
    ui.activate(ui.button("Close canvas map"));
    ui.dispatch(&panel_view);
    assert!(!ui.state.outputs[0].show_map);
    server.join().unwrap();
    assert_eq!(
        received.try_iter().collect::<Vec<_>>(),
        ["zoom-out\n", "zoom-in\n"]
    );
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
    let mut ui = Ui::new(state, panel_view, 340);
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
    let mut ui = Ui::new(state, panel_view, 320);
    ui.harness
        .process_window_event(masonry::core::WindowEvent::Resize((320, 192).into()));
    let zoom = ui.button("Zoom out");
    ui.harness.scroll_into_view(zoom);
    // This checks hit testing and clipping, unlike an accessibility action
    // which is intentionally allowed to activate offscreen controls.
    ui.harness.mouse_click_on(zoom, None);
    ui.dispatch(&panel_view);
    assert!(
        ui.state.canvas_error.is_some(),
        "the visible button must attempt its command"
    );
}
