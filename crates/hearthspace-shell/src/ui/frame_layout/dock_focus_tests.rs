//! Enter through the compositor's frame focus boundary, never by focusing a button directly.
use super::*;
use crate::ui::frame_layout::{FocusRequest, focus, task::Request, widgets};
use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

fn frame_view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    let chain = &state.frame_layout.chains[&widget_key("eDP-1", "apps")];
    sized_box(focus::boundary(
        widgets::content(
            state,
            &state.outputs[0],
            "apps",
            false,
            false,
            Default::default(),
            chain,
        ),
        chain.clone(),
        "eDP-1".into(),
        LayoutTarget::widget("apps"),
        state.frame_layout.snapshot.focus_request.clone(),
    ))
}

fn enter<V: WidgetView<ShellState>>(
    ui: &mut Ui<V>,
    build: &impl Fn(&ShellState) -> V,
    reverse: bool,
) where
    V::Widget: Sized,
{
    let serial = ui
        .state
        .frame_layout
        .snapshot
        .focus_request
        .as_ref()
        .map_or(1, |request| request.serial + 1);
    ui.state.frame_layout.snapshot.focus_request = Some(FocusRequest {
        serial,
        output: "eDP-1".into(),
        target: LayoutTarget::widget("apps"),
        reverse,
    });
    ui.rebuild(build);
    ui.dispatch(build);
}

fn key<V: WidgetView<ShellState>>(ui: &mut Ui<V>, key: NamedKey)
where
    V::Widget: Sized,
{
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(key)));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(key)));
}

fn requests(state: &mut ShellState) -> mpsc::Receiver<Request> {
    let (sender, receiver) = mpsc::channel(64);
    state.frame_layout.requests = Some(sender);
    receiver
}

fn assert_leave(requests: &mut mpsc::Receiver<Request>, reverse: bool) {
    let request = std::iter::from_fn(|| requests.try_recv().ok())
        .find(|request| request.command.starts_with("frame-focus-next "))
        .expect("frame boundary exit");
    let value: serde_json::Value =
        serde_json::from_str(request.command.strip_prefix("frame-focus-next ").unwrap()).unwrap();
    assert_eq!(value["output"], "eDP-1");
    assert_eq!(value["target"]["id"], "apps");
    assert_eq!(value["reverse"], reverse);
}

fn focus_server(state: &ShellState, windows: Vec<u64>) -> thread::JoinHandle<()> {
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    thread::spawn(move || {
        for window in windows {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing focus-window {window}");
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut command = String::new();
            stream.read_to_string(&mut command).unwrap();
            assert_eq!(command, format!("focus-window {window}\n"));
            stream.write_all(b"ok\n").unwrap();
        }
    })
}

fn running_apps(state: &mut ShellState, count: u64, dock_width: f64) {
    state.tray.snapshot.items.clear();
    state.windows = (1..=count)
        .map(|id| RunningWindow {
            id,
            app_id: format!("app{id}"),
            title: format!("App {id}"),
            focused: false,
        })
        .collect();
    state.frame_layout.snapshot.outputs[0].widgets[0].rect.width = dock_width + 58.0;
}

#[test]
fn native_forward_focus_activates_dock_and_shift_tab_exits_its_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    state.tray.snapshot.items.clear();
    let mut requests = requests(&mut state);
    let server = focus_server(&state, vec![7]);
    let mut ui = Ui::new(state, frame_view, (500, 54));
    enter(&mut ui, &frame_view, false);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Switch to Steam"))
    );
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&frame_view);
    assert!(ui.state.windows[0].focused);
    ui.harness.press_tab_key(true);
    ui.dispatch(&frame_view);
    assert_leave(&mut requests, true);
    server.join().unwrap();
}

#[test]
fn native_frame_focus_keeps_dock_activation_independent_of_tray_menu() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    let mut requests = requests(&mut state);
    let server = focus_server(&state, vec![7]);
    let mut ui = Ui::new(state, frame_view, (500, 54));
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(false));
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(true));
    // Native frame traversal publishes a FocusRequest after focusing the surface.
    // WindowFocusChange alone has no widget target in a fresh Masonry window.
    enter(&mut ui, &frame_view, false);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Switch to Steam"))
    );
    key(&mut ui, NamedKey::ContextMenu);
    ui.dispatch(&frame_view);
    assert!(
        std::iter::from_fn(|| requests.try_recv().ok()).all(|request| request.open.is_none()),
        "dock Menu key must not open the separate tray menu"
    );
    assert!(!ui.state.outputs[0].tray.open);
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&frame_view);
    assert!(ui.state.windows[0].focused);
    assert!(!ui.state.outputs[0].tray.open);
    ui.harness.press_tab_key(true);
    ui.dispatch(&frame_view);
    assert_leave(&mut requests, true);
    server.join().unwrap();
}

#[test]
fn partial_last_page_retains_pager_focus_for_repeated_keyboard_activation() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    running_apps(&mut state, 5, 188.0);
    let mut requests = requests(&mut state);
    let mut ui = Ui::new(state, frame_view, (246, 54));
    enter(&mut ui, &frame_view, true);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Apps List")));
    ui.harness.press_tab_key(true);
    ui.dispatch(&frame_view);
    let pager = ui.button("Next apps (page 1 of 2)");
    assert_eq!(ui.harness.focused_widget_id(), Some(pager));
    for page in [2, 1, 2, 1] {
        key(&mut ui, NamedKey::Enter);
        ui.dispatch(&frame_view);
        assert_eq!(ui.state.outputs[0].dock_page, page - 1);
        assert_eq!(ui.button(&format!("Next apps (page {page} of 2)")), pager);
        assert_eq!(ui.harness.focused_widget_id(), Some(pager));
        ui.button(if page == 2 {
            "Switch to app5"
        } else {
            "Switch to app3"
        });
    }
    ui.harness.press_tab_key(false);
    ui.dispatch(&frame_view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Apps List")));
    ui.harness.press_tab_key(false);
    ui.dispatch(&frame_view);
    assert_leave(&mut requests, false);
}

#[test]
fn compact_cycle_retains_actual_button_focus_after_frame_entry_and_repeated_enter() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    running_apps(&mut state, 3, 40.0);
    let mut requests = requests(&mut state);
    let server = focus_server(&state, vec![1, 2, 3]);
    let mut ui = Ui::new(state, frame_view, (98, 54));
    enter(&mut ui, &frame_view, false);
    let cycle = ui.button("Next app: Switch to app1");
    assert_eq!(ui.harness.focused_widget_id(), Some(cycle));
    for id in 1..=3 {
        key(&mut ui, NamedKey::Enter);
        ui.dispatch(&frame_view);
        assert_eq!(ui.harness.focused_widget_id(), Some(cycle));
        assert!(
            ui.state
                .windows
                .iter()
                .any(|window| window.id == id && window.focused)
        );
    }
    enter(&mut ui, &frame_view, true);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Apps List")));
    ui.harness.press_tab_key(true);
    ui.dispatch(&frame_view);
    assert_eq!(ui.harness.focused_widget_id(), Some(cycle));
    ui.harness.press_tab_key(true);
    ui.dispatch(&frame_view);
    assert_leave(&mut requests, true);
    server.join().unwrap();
}
