use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use masonry::{accesskit::Role, core::TextEvent};
use xilem::{
    ViewCtx,
    core::{RawProxy, View},
    masonry::core::keyboard::{Key, NamedKey},
};

use super::{Entry, ShellState, launcher, launcher_window};
use crate::ui::window_catalog::RunningWindow;

#[derive(Debug)]
struct NoMessages;

impl RawProxy for NoMessages {
    fn send_message(
        &self,
        _path: Arc<[xilem::core::ViewId]>,
        _message: xilem::core::SendMessage,
    ) -> Result<(), xilem::core::ProxyError> {
        panic!("bar view must not start background tasks")
    }

    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

#[test]
fn search_overlay_focuses_input_and_enter_activates_matching_window() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (requests, received) = mpsc::channel();
    let server = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = String::new();
                BufReader::new(&stream).read_line(&mut request).unwrap();
                stream.write_all(b"ok\n").unwrap();
                requests.send(request).unwrap();
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    let mut state = ShellState::new(socket, vec!["eDP-1".into()]);
    state.catalog = Default::default();
    state.windows = vec![RunningWindow {
        id: 42,
        title: "Research Notes".into(),
        app_id: "editor".into(),
        focused: false,
    }];
    let view = |state: &ShellState| {
        launcher::launcher_view(
            state,
            &state.outputs[0],
            launcher::results(state, &state.outputs[0]),
            None,
        )
    };
    // Use the shared driver fixture so initial resize and icon actions settle
    // before the first key, just as they do when the real popup opens.
    let mut ui = crate::ui::test_support::Ui::new(state, view, (650, 560));
    let input = ui.harness.focused_widget_id().unwrap();
    assert_eq!(
        ui.harness.access_node(input).unwrap().role(),
        Role::TextInput
    );
    for event in "Research"
        .chars()
        .map(|character| TextEvent::key_down(Key::Character(character.to_string())))
        .chain([TextEvent::key_down(Key::Named(NamedKey::Enter))])
    {
        ui.harness.process_text_event(event);
        ui.dispatch(&view);
    }
    server.join().unwrap();
    assert_eq!(received.try_recv().unwrap(), "focus-window 42\n");
    assert!(ui.state.outputs[0].query.is_empty());
    assert!(ui.state.outputs[0].launch_error.is_none());
}

#[test]
fn launcher_creation_options_fit_results_headers_and_errors() {
    // Build the real Xilem WindowView into its Winit creation attributes. A
    // nonresizable Wayland window starts with min=max=initial size, so checking
    // only our desired min/max hints would miss a clipped first presentation.
    for (count, launch_error, catalog_error, height) in [
        (0, false, false, 144.0),
        (1, false, false, 144.0),
        (4, false, false, 264.0),
        (4, true, false, 364.0),
        (4, false, true, 364.0),
    ] {
        let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
        state.outputs[0].show_search = true;
        state.outputs[0].launch_error = launch_error.then(|| "Launch failed".into());
        let results = (0..count)
            .map(|id| {
                Entry::Window(RunningWindow {
                    id,
                    title: format!("Notes {id}"),
                    app_id: "editor".into(),
                    focused: false,
                })
            })
            .collect();
        let view = launcher_window(
            &state,
            &state.outputs[0],
            results,
            catalog_error.then(|| "Catalog unavailable".into()),
        );
        let runtime = xilem::tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
        let (window, _) = view.build(&mut ctx, &mut state);
        assert_eq!(window.0.base_color, xilem::Color::TRANSPARENT);
        let attrs = window.0.attributes;
        let expected = Some(xilem::dpi::LogicalSize::new(650.0, height).into());
        assert!(!attrs.resizable);
        assert!(attrs.transparent);
        assert_eq!(
            attrs.inner_size, expected,
            "initial size for {count} results"
        );
        assert_eq!(attrs.min_inner_size, expected);
        assert_eq!(attrs.max_inner_size, expected);
    }
}
