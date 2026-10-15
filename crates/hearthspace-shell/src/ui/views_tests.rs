use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use masonry::{core::TextEvent, testing::TestHarness};
use xilem::{
    ViewCtx,
    core::{DynMessage, MessageCtx, MessageResult, RawProxy, View, ViewPathTracker},
    masonry::core::keyboard::{Key, NamedKey},
};

use super::{Entry, ShellState, bar_view, launcher_window};
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
fn keyboard_enter_routes_through_rebuilt_bar_and_focuses_matching_window() {
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
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let mut view = bar_view(&state.outputs[0], state.clock.clone(), &[]);
    let (root, mut view_state) = view.build(&mut ctx, &mut state);
    let mut harness = TestHarness::create(
        hearthspace_ui_components::hearthspace_light_properties(),
        root.new_widget,
    );
    for event in "Research"
        .chars()
        .map(|character| TextEvent::key_down(Key::Character(character.to_string())))
        .chain([TextEvent::key_down(Key::Named(NamedKey::Enter))])
    {
        harness.process_text_event(event);
        while let Some((action, widget_id)) = harness.pop_action_erased() {
            let path = ctx.get_id_path(widget_id).unwrap().clone();
            let mut message =
                MessageCtx::new(std::mem::take(ctx.environment()), path, DynMessage(action));
            let result = harness.edit_root_widget(|root| {
                view.message(&mut view_state, &mut message, root, &mut state)
            });
            // Canvas icons request their first draw after layout; those
            // messages must pass through the same view tree as text actions.
            assert!(matches!(
                result,
                MessageResult::Action(()) | MessageResult::RequestRebuild
            ));
            let (environment, _, _) = message.finish();
            *ctx.environment() = environment;
            let next = bar_view(&state.outputs[0], state.clock.clone(), &[]);
            harness.edit_root_widget(|root| {
                next.rebuild(&view, &mut view_state, &mut ctx, root, &mut state);
            });
            view = next;
        }
    }
    server.join().unwrap();
    assert_eq!(received.try_recv().unwrap(), "focus-window 42\n");
    assert!(state.outputs[0].query.is_empty());
    assert!(state.outputs[0].launch_error.is_none());
}

#[test]
fn launcher_creation_options_fit_results_headers_and_errors() {
    // Build the real Xilem WindowView into its Winit creation attributes. A
    // nonresizable Wayland window starts with min=max=initial size, so checking
    // only our desired min/max hints would miss a clipped first presentation.
    for (count, show_windows, launch_error, catalog_error, height) in [
        (0, false, false, false, 52.0),
        (1, false, false, false, 52.0),
        (4, false, false, false, 172.0),
        (4, true, false, false, 212.0),
        (4, false, true, false, 272.0),
        (4, false, false, true, 272.0),
    ] {
        let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
        state.outputs[0].show_windows = show_windows;
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
            &state.outputs[0],
            results,
            catalog_error.then(|| "Catalog unavailable".into()),
        )
        .with_base_color(xilem::Color::BLACK);
        let runtime = xilem::tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
        let (window, _) = view.build(&mut ctx, &mut state);
        let attrs = window.0.attributes;
        let expected = Some(xilem::dpi::LogicalSize::new(360.0, height).into());
        assert!(!attrs.resizable);
        assert_eq!(
            attrs.inner_size, expected,
            "initial size for {count} results"
        );
        assert_eq!(attrs.min_inner_size, expected);
        assert_eq!(attrs.max_inner_size, expected);
    }
}
