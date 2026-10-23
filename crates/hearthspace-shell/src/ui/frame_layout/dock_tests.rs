use super::super::{Output, Rect, Snapshot, Widget as Part, apply, widget_key};
use super::*;
use crate::{
    app_catalog::AppCatalog,
    tray::{TrayWorker, channel},
    ui::{
        dock,
        test_support::{Ui, ids},
        tray_state, tray_view,
        window_catalog::RunningWindow,
    },
};
use hearthspace_common::frame_layout::FrameEdge;
use masonry::{
    accesskit::{Action, ActionRequest, Role, TreeId},
    core::{
        PointerButton, TextEvent,
        keyboard::{Key, NamedKey},
    },
};
use xilem::{
    WidgetView,
    view::{flex_col, sized_box},
};

#[path = "dock_focus_tests.rs"]
mod focus_tests;
#[path = "dock_popup_tests.rs"]
mod popup;
#[path = "dock_protocol_tests.rs"]
mod protocol;

fn install(path: &std::path::Path, id: &str, metadata: &str) {
    let dir = path.join("applications");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{id}.desktop")),
        format!("[Desktop Entry]\nType=Application\nName=Steam\nExec=unused\n{metadata}"),
    )
    .unwrap();
}

fn fixture(path: &std::path::Path) -> (ShellState, TrayWorker) {
    install(path, "steam_steam", "X-SnapAppName=steam\nIcon=steam\n");
    let mut state = ShellState::new(
        path.join("command.sock"),
        vec!["eDP-1".into(), "DP-1".into()],
    );
    state.catalog = AppCatalog::load_from_data_dirs(vec![path.to_owned()]);
    state.windows = vec![RunningWindow {
        id: 7,
        app_id: "steam".into(),
        title: "A game library".into(),
        focused: false,
    }];
    let mut item = crate::ui::tray_tests::item(1);
    item.title = "Localized status title".into();
    item.icon.name = Some("steam_tray_mono".into());
    state.tray.snapshot.items = vec![item];
    let (handle, _, worker) = channel(None);
    state.tray.handle = Some(handle);
    apply(
        &mut state,
        Snapshot {
            outputs: ["eDP-1", "DP-1"]
                .into_iter()
                .map(|output| Output {
                    output: output.into(),
                    widgets: vec![Part {
                        id: "apps".into(),
                        rect: Rect {
                            width: 500.0,
                            height: 54.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    }],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
    );
    (state, worker)
}

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    let output = &state.outputs[0];
    sized_box(flex_col((
        dock::frame_view(
            state,
            output,
            width(state, output.key()).unwrap_or(44.0),
            false,
            &Default::default(),
        ),
        tray_view::frame_strip(output, &state.tray, false, &Default::default(), false),
        output
            .tray
            .open
            .then(|| tray_view::panel(output, &state.tray)),
    )))
}

#[test]
fn closed_pin_with_status_menu_launches_on_its_output() {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
        thread,
    };
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    state.windows.clear();
    state.favorites.store.add("steam_steam.desktop").unwrap();
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut command = String::new();
        stream.read_to_string(&mut command).unwrap();
        let launch = crate::ShellCommand::LaunchAppOnOutput {
            app_id: "steam_steam.desktop".into(),
            output_name: "DP-1".into(),
        };
        assert_eq!(command, format!("{}\n", launch.wire_name()));
        stream.write_all(b"ok\n").unwrap();
    });
    let build = |state: &ShellState| {
        sized_box(super::super::widgets::content(
            state,
            &state.outputs[1],
            "apps",
            false,
            false,
            Default::default(),
            &Default::default(),
        ))
    };
    let mut ui = Ui::new(state, &build, (500, 54));
    ui.harness.mouse_click_on(
        ui.button("Open Steam, pinned"),
        Some(PointerButton::Primary),
    );
    ui.dispatch(&build);
    assert!(!ui.state.outputs[1].show_search);
    assert!(!ui.state.outputs[1].tray.open);
    server.join().unwrap();
}

#[test]
fn running_pinned_app_keeps_original_tray_menu_separate_from_dock_focus() {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
        thread,
    };
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    state.favorites.store.add("steam_steam.desktop").unwrap();
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut command = String::new();
        stream.read_to_string(&mut command).unwrap();
        assert_eq!(command, "focus-window 7\n");
        stream.write_all(b"ok\n").unwrap();
    });
    let mut ui = Ui::new(state, view, (500, 300));
    let dock = ui.button("Switch to Steam, pinned");
    let button = ui.button("Localized status title");
    assert_ne!(dock, button);
    assert_eq!(tray_state::visible(&ui.state.tray.snapshot).count(), 1);
    assert!(
        ui.harness
            .access_node(button)
            .unwrap()
            .data()
            .supports_action(Action::ShowContextMenu)
    );
    ui.harness
        .mouse_click_on(button, Some(PointerButton::Secondary));
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].tray.open);
    assert_eq!(
        ui.state.outputs[0].tray.item,
        Some(ui.state.tray.snapshot.items[0].key.clone())
    );
    tray_state::dismiss(&mut ui.state, "eDP-1");
    ui.rebuild(&view);
    ui.harness.focus_on(Some(button));
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ContextMenu)));
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].tray.open);
    tray_state::dismiss(&mut ui.state, "eDP-1");
    ui.rebuild(&view);
    ui.harness.process_access_event(ActionRequest {
        action: Action::ShowContextMenu,
        target_tree: TreeId::ROOT,
        target_node: button.to_raw().into(),
        data: None,
    });
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].tray.open);
    tray_state::dismiss(&mut ui.state, "eDP-1");
    ui.rebuild(&view);
    ui.harness
        .mouse_click_on(dock, Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert!(ui.state.windows[0].focused);
    assert!(!ui.state.outputs[0].tray.open);
    server.join().unwrap();
}

#[test]
fn live_tray_remains_in_system_group_while_app_is_pinned_closed_and_unpinned() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _worker) = fixture(dir.path());
    let layout = &state.frame_layout.snapshot.outputs[0].layout;
    let tray = LayoutTarget::widget("tray");
    assert_eq!(layout.group_for(&tray).unwrap().id, "system");
    assert_eq!(layout.placement(&tray).edge(), Some(FrameEdge::Top));
    assert_eq!(
        layout.placement(&LayoutTarget::widget("apps")).edge(),
        Some(FrameEdge::Bottom)
    );
    let mut ui = Ui::new(state, view, (500, 300));
    let tray = ui.button("Localized status title");
    ui.button("Switch to Steam");
    ui.state.favorites.store.add("steam_steam.desktop").unwrap();
    ui.rebuild(&view);
    ui.dispatch(&view);
    ui.button("Switch to Steam, pinned");
    assert_eq!(ui.button("Localized status title"), tray);

    ui.state.windows.clear();
    ui.rebuild(&view);
    ui.dispatch(&view);
    ui.button("Open Steam, pinned");
    assert_eq!(ui.button("Localized status title"), tray);

    ui.state
        .favorites
        .store
        .remove("steam_steam.desktop")
        .unwrap();
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(dock::desired_width(&ui.state), 0.0);
    assert_eq!(ui.button("Localized status title"), tray);
}

#[test]
fn running_app_keeps_the_tray_surface_available_on_every_output() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    state.favorites.store.add("steam_steam.desktop").unwrap();
    let (sender, updates) = tokio::sync::watch::channel(Vec::new());
    state.frame_layout.availability = Some(sender);
    super::super::reconcile(&state);
    let updates = updates.borrow();
    assert!(updates.contains(&("eDP-1".into(), "tray".into(), true)));
    assert!(updates.contains(&("DP-1".into(), "tray".into(), true)));
    assert_eq!(
        super::super::widgets::size(
            &state,
            "tray",
            false,
            false,
            true,
            Default::default(),
            false
        ),
        (44.0, 44.0)
    );
}
