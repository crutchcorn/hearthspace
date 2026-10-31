//! Real catalog/dock controls route to the generation-owning compositor worker.
use super::*;
use crate::ui::{dock, launcher, test_support::Ui};
use masonry::core::{
    PointerButton, TextEvent,
    keyboard::{Key, NamedKey},
};
use xilem::{WidgetView, view::sized_box};

#[path = "app_context_sequence_tests.rs"]
mod sequences;

fn fixture(directory: &Path) -> (ShellState, tokio::sync::mpsc::Receiver<Request>) {
    std::fs::create_dir(directory.join("applications")).unwrap();
    std::fs::write(
        directory.join("applications/editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=true\n",
    )
    .unwrap();
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.catalog = AppCatalog::load_from_data_dirs(vec![directory.to_owned()]);
    state.favorites.store = FavoriteStore::in_memory();
    state.favorites.store.add("editor.desktop").unwrap();
    state.outputs[0].show_search = true;
    state.frame_layout.popup_epoch = 4;
    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    state.desktop.requests = Some(sender);
    (state, receiver)
}

fn apps(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(launcher::launcher_view(
        state,
        &state.outputs[0],
        launcher::results(state, &state.outputs[0]),
        None,
    ))
}

fn strip(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(dock::view(state, &state.outputs[0], 160.0))
}

#[test]
fn catalog_context_key_waits_for_release_and_does_not_launch() {
    let directory = tempfile::tempdir().unwrap();
    let (state, mut requests) = fixture(directory.path());
    let mut ui = Ui::new(state, apps, (650, 560));
    let editor = ui.button("Open: Editor");
    ui.harness.focus_on(Some(editor));
    ui.dispatch(&apps);
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ContextMenu)));
    ui.dispatch(&apps);
    assert!(requests.try_recv().is_err());
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::ContextMenu)));
    ui.dispatch(&apps);
    let Request::AppContext(request) = requests.try_recv().unwrap() else {
        panic!("missing app menu");
    };
    assert_eq!(request.target.app_id, "editor.desktop");
    assert_eq!(
        request.target.source_app_id,
        ui.state.outputs[0].launcher_app_id()
    );
    assert_eq!(request.target.output_name.as_deref(), Some("eDP-1"));
    assert!(request.anchor.width() > 0.0 && request.anchor.height() > 0.0);
    assert!(request.current());
    assert_eq!(request.popup_epoch, 4);
    assert_eq!(request.intent, crate::ui::app_context::Intent::Ordinary);
    assert!(ui.state.outputs[0].show_search);
    assert!(ui.state.outputs[0].launch_pending.is_none());
    assert!(requests.try_recv().is_err());
    // Losing the source window invalidates queued work, even if it later regains focus.
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(false));
    ui.dispatch(&apps);
    assert!(!request.current());
    ui.state.dismiss_panels();
    crate::ui::app_context::finished(&mut ui.state, request, Err("source closed".into()));
    assert!(!ui.state.outputs[0].show_search);
}

#[test]
fn dock_secondary_click_uses_measured_app_anchor_after_release() {
    let directory = tempfile::tempdir().unwrap();
    let (mut state, mut requests) = fixture(directory.path());
    state.outputs[0].show_search = false;
    let mut ui = Ui::new(state, strip, (160, 64));
    let editor = ui.button("Open Editor, pinned");
    ui.harness.mouse_move_to(editor);
    ui.harness
        .mouse_button_press(Some(PointerButton::Secondary));
    ui.dispatch(&strip);
    assert!(requests.try_recv().is_err());
    ui.harness
        .mouse_button_release(Some(PointerButton::Secondary));
    ui.dispatch(&strip);
    let Request::AppContext(request) = requests.try_recv().unwrap() else {
        panic!("missing app menu");
    };
    assert_eq!(request.target.app_id, "editor.desktop");
    assert_eq!(
        request.target.source_app_id,
        hearthspace_common::frame_layout::frame_widget_app_id(Some("eDP-1"), "apps")
    );
    assert!(request.current());
    assert_eq!(request.intent, crate::ui::app_context::Intent::Ordinary);
    assert!(ui.state.outputs[0].launch_pending.is_none());
    assert!(!ui.state.outputs[0].show_search);
    assert!(requests.try_recv().is_err());
    crate::ui::app_context::finished(&mut ui.state, request, Err("source unavailable".into()));
    assert!(
        ui.state.outputs[0]
            .launch_error
            .as_deref()
            .unwrap()
            .contains("source unavailable")
    );
}

#[test]
fn context_key_release_after_source_replacement_cannot_open_another_apps_menu() {
    let directory = tempfile::tempdir().unwrap();
    let (state, mut requests) = fixture(directory.path());
    let mut ui = Ui::new(state, apps, (650, 560));
    let editor = ui.button("Open: Editor");
    ui.harness.focus_on(Some(editor));
    ui.dispatch(&apps);
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ContextMenu)));
    ui.dispatch(&apps);
    std::fs::remove_file(directory.path().join("applications/editor.desktop")).unwrap();
    std::fs::write(
        directory.path().join("applications/viewer.desktop"),
        "[Desktop Entry]\nType=Application\nName=Viewer\nExec=true\n",
    )
    .unwrap();
    ui.state.catalog = AppCatalog::load_from_data_dirs(vec![directory.path().to_owned()]);
    ui.rebuild(&apps);
    ui.dispatch(&apps);
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::ContextMenu)));
    ui.dispatch(&apps);
    assert!(requests.try_recv().is_err());
    ui.button("Open: Viewer");
}

#[test]
fn touch_hold_opens_app_menu_only_after_release_and_cancel_discards_it() {
    use masonry::core::{PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType};
    let directory = tempfile::tempdir().unwrap();
    let (state, mut requests) = fixture(directory.path());
    let mut ui = Ui::new(state, strip, (160, 64));
    let editor = ui.button("Open Editor, pinned");
    let center = ui
        .harness
        .get_widget_with_id(editor)
        .ctx()
        .bounding_box()
        .center();
    let event = PointerButtonEvent {
        pointer: PointerInfo {
            pointer_type: PointerType::Touch,
            pointer_id: Some(masonry::core::pointer::PointerId::PRIMARY),
            persistent_device_id: None,
        },
        state: PointerState {
            position: (center.x, center.y).into(),
            ..Default::default()
        },
        button: None,
    };
    ui.harness
        .process_pointer_event(PointerEvent::Down(event.clone()));
    ui.harness.animate_ms(0);
    ui.harness.animate_ms(600);
    ui.dispatch(&strip);
    assert!(requests.try_recv().is_err());
    ui.harness
        .process_pointer_event(PointerEvent::Up(event.clone()));
    ui.dispatch(&strip);
    assert!(matches!(
        requests.try_recv().unwrap(),
        Request::AppContext(_)
    ));
    assert!(ui.state.outputs[0].launch_pending.is_none());
    ui.harness
        .process_pointer_event(PointerEvent::Down(event.clone()));
    ui.harness
        .process_pointer_event(PointerEvent::Cancel(event.pointer));
    ui.harness.animate_ms(0);
    ui.harness.animate_ms(600);
    ui.harness
        .process_pointer_event(PointerEvent::Up(event.clone()));
    ui.dispatch(&strip);
    assert!(requests.try_recv().is_err());
    assert!(ui.state.outputs[0].launch_pending.is_none());

    // A touch scroll is never promoted to either a click or a context request.
    let moved = PointerButtonEvent {
        pointer: event.pointer,
        state: PointerState {
            position: (center.x, center.y).into(),
            ..Default::default()
        },
        button: None,
    };
    ui.harness
        .process_pointer_event(PointerEvent::Down(moved.clone()));
    ui.harness
        .process_pointer_event(PointerEvent::Move(masonry::core::PointerUpdate {
            pointer: moved.pointer,
            current: PointerState {
                position: (center.x + 20.0, center.y).into(),
                ..Default::default()
            },
            coalesced: vec![],
            predicted: vec![],
        }));
    ui.harness.animate_ms(0);
    ui.harness.animate_ms(600);
    ui.harness.process_pointer_event(PointerEvent::Up(moved));
    ui.dispatch(&strip);
    assert!(requests.try_recv().is_err());
    assert!(ui.state.outputs[0].launch_pending.is_none());
}

#[test]
fn delayed_rejection_after_popup_epoch_changed_does_not_reopen_apps_list() {
    let directory = tempfile::tempdir().unwrap();
    let (state, mut requests) = fixture(directory.path());
    let mut ui = Ui::new(state, strip, (160, 64));
    let editor = ui.button("Open Editor, pinned");
    ui.harness.mouse_move_to(editor);
    ui.harness
        .mouse_button_press(Some(PointerButton::Secondary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Secondary));
    ui.dispatch(&strip);
    let Request::AppContext(request) = requests.try_recv().unwrap() else {
        panic!("missing app menu");
    };
    ui.state.frame_layout.popup_epoch = 5;
    ui.state.outputs[0].show_search = false;
    crate::ui::app_context::finished(&mut ui.state, request, Err("expired popup epoch".into()));
    assert!(!ui.state.outputs[0].show_search);
    assert!(ui.state.outputs[0].launch_error.is_none());
}

#[test]
fn dock_accessibility_context_requests_explicit_intent_with_original_lifetime() {
    use masonry::accesskit::{Action, ActionRequest, TreeId};
    let directory = tempfile::tempdir().unwrap();
    let (mut state, mut requests) = fixture(directory.path());
    state.outputs[0].show_search = false;
    let mut ui = Ui::new(state, strip, (160, 64));
    let editor = ui.button("Open Editor, pinned");
    // An assistive action may target the visible dock while another app owns
    // Wayland focus. Masonry focus alone cannot grant compositor admission.
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(false));
    ui.harness.process_access_event(ActionRequest {
        action: Action::ShowContextMenu,
        target_tree: TreeId::ROOT,
        target_node: editor.to_raw().into(),
        data: None,
    });
    ui.dispatch(&strip);
    let Request::AppContext(mut request) = requests.try_recv().unwrap() else {
        panic!("missing accessibility app menu");
    };
    assert_eq!(
        request.intent,
        crate::ui::app_context::Intent::Accessibility
    );
    assert_eq!(request.popup_epoch, 4);
    assert_eq!(request.target.app_id, "editor.desktop");
    assert_eq!(
        request.target.source_app_id,
        hearthspace_common::frame_layout::frame_widget_app_id(Some("eDP-1"), "apps")
    );
    assert_eq!(
        request.anchor,
        ui.harness.get_widget_with_id(editor).ctx().bounding_box()
    );
    assert!(request.current());
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
    assert!(ui.state.outputs[0].launch_pending.is_none());
    assert!(!ui.state.outputs[0].show_search);
    assert!(requests.try_recv().is_err());
    // Accessibility must neither renew the deadline nor survive source teardown.
    request.deadline = std::time::Instant::now();
    assert!(
        request
            .perform(Path::new("/unused"))
            .unwrap_err()
            .contains("expired")
    );
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(true));
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(false));
    ui.dispatch(&strip);
    assert!(!request.alive.load(std::sync::atomic::Ordering::Acquire));
    crate::ui::app_context::finished(&mut ui.state, request, Err("source closed".into()));
    assert!(!ui.state.outputs[0].show_search);
}

#[test]
fn unsupported_catalog_target_reports_menu_error_without_removing_launch_entry() {
    use masonry::accesskit::{Action, ActionRequest, TreeId};
    let directory = tempfile::tempdir().unwrap();
    let (mut state, mut requests) = fixture(directory.path());
    let mut nested = directory.path().join("applications");
    for _ in 0..5 {
        nested.push("x".repeat(220));
    }
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        nested.join("oversized.desktop"),
        "[Desktop Entry]\nType=Application\nName=Oversized\nExec=true\n",
    )
    .unwrap();
    state.catalog = AppCatalog::load_from_data_dirs(vec![directory.path().to_owned()]);
    let mut ui = Ui::new(state, apps, (650, 560));
    let oversized = ui.button("Open: Oversized");
    ui.harness.process_access_event(ActionRequest {
        action: Action::ShowContextMenu,
        target_tree: TreeId::ROOT,
        target_node: oversized.to_raw().into(),
        data: None,
    });
    ui.dispatch(&apps);
    assert!(
        requests.try_recv().is_err(),
        "unsupported menu must not reach the worker"
    );
    let error = ui.state.outputs[0].launch_error.as_deref().unwrap();
    assert!(
        error.contains("identifier") && error.contains("still launch"),
        "{error}"
    );
    assert!(ui.state.outputs[0].show_search);
    assert_eq!(ui.state.catalog.apps().len(), 2);
    assert!(
        ui.state
            .catalog
            .apps()
            .iter()
            .any(|app| app.id.len() > 1020)
    );
    ui.button("Open: Oversized");
    ui.button("Open: Editor");
}
