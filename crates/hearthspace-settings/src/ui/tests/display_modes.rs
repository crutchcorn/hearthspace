//! Real selectors submit complete timing IDs, and confirmation gates Keep.
use super::*;
use hearthspace_ipc::display_modes::{Mode, Output, Preview, Request, Snapshot};

fn mode(id: &str, w: i32, h: i32, rate: i32) -> Mode {
    Mode {
        id: id.into(),
        width: w,
        height: h,
        refresh_millihertz: rate,
    }
}
fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(crate::display_view::view(state))
}
fn setup() -> (
    tempfile::TempDir,
    SettingsApp,
    xilem::tokio::sync::mpsc::UnboundedReceiver<Request>,
) {
    let (dir, mut state) = super::state();
    let id = state.settings.monitors[0].id.clone();
    state.selected_monitor_id = Some(id.clone());
    let current = mode("fractional", 1920, 1080, 59_940);
    state.display_layout.modes.snapshot = Snapshot {
        available: true,
        outputs: vec![Output {
            id,
            current: current.clone(),
            modes: vec![
                current,
                mode("sixty", 1920, 1080, 60_000),
                mode("small", 1280, 720, 60_000),
            ],
        }],
        ..Default::default()
    };
    let receiver = crate::display_modes::test_channel(&mut state);
    (dir, state, receiver)
}
fn combo<V: WidgetView<SettingsApp, Widget: Sized>>(ui: &Ui<V>, name: &str) -> WidgetId {
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness
                .access_node(*id)
                .is_some_and(|n| n.role() == Role::ComboBox && n.label().as_deref() == Some(name))
        })
        .unwrap()
}
fn key<V: WidgetView<SettingsApp, Widget: Sized>>(
    ui: &mut Ui<V>,
    key: NamedKey,
    build: impl Fn(&SettingsApp) -> V,
) {
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(key)));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(key)));
    ui.dispatch(&build);
}

#[test]
fn native_resolution_selector_submits_supported_timing_for_the_selected_output() {
    let (_dir, state, mut rx) = setup();
    let mut ui = Ui::new(state, view, (680, 800));
    let id = combo(&ui, "Resolution");
    ui.harness.mouse_click_on(id, None);
    ui.dispatch(&view);
    key(&mut ui, NamedKey::ArrowDown, view);
    key(&mut ui, NamedKey::Enter, view);
    let Request::Preview { output, mode } = rx.try_recv().unwrap() else {
        panic!("expected preview")
    };
    assert_eq!(output, ui.state.settings.monitors[0].id);
    assert_eq!(mode, "small");
    assert!(ui.state.display_layout.modes.busy);
    assert!(rx.try_recv().is_err());
}

#[test]
fn refresh_selector_distinguishes_fractional_rate_and_keep_waits_for_presentation() {
    let (_dir, state, mut rx) = setup();
    let mut ui = Ui::new(state, view, (680, 800));
    let id = combo(&ui, "Refresh rate");
    assert_eq!(
        ui.harness.access_node(id).unwrap().value().as_deref(),
        Some("59.940 Hz")
    );
    ui.harness.mouse_click_on(id, None);
    ui.dispatch(&view);
    key(&mut ui, NamedKey::ArrowUp, view);
    key(&mut ui, NamedKey::Enter, view);
    assert!(matches!(rx.try_recv().unwrap(),Request::Preview {mode,..} if mode=="sixty"));
    ui.state.display_layout.modes.busy = false;
    ui.state.display_layout.modes.snapshot.preview = Some(Preview {
        token: 42,
        output: ui.state.settings.monitors[0].id.clone(),
        ready: false,
        seconds_remaining: 5,
    });
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Revert")));
    let keep = ui.button("Keep changes");
    assert!(ui.harness.access_node(keep).unwrap().is_disabled());
    ui.activate(keep);
    ui.dispatch(&view);
    assert!(rx.try_recv().is_err());
    ui.state
        .display_layout
        .modes
        .snapshot
        .preview
        .as_mut()
        .unwrap()
        .ready = true;
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(!ui.harness.access_node(keep).unwrap().is_disabled());
    ui.activate(keep);
    ui.dispatch(&view);
    assert!(matches!(
        rx.try_recv().unwrap(),
        Request::Keep { token: 42 }
    ));
    ui.state.display_layout.modes.busy = false;
    ui.rebuild(&view);
    ui.dispatch(&view);
    ui.activate(ui.button("Revert"));
    ui.dispatch(&view);
    assert!(matches!(
        rx.try_recv().unwrap(),
        Request::Revert { token: 42 }
    ));
}
