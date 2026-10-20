use crate::{
    ai::{AiSettings, Operation, Response},
    state::{SettingsApp, SettingsCategory},
};
use hearthspace_ai::{DevicePreference, ModelStatus};
use hearthspace_ipc::Settings;
use xilem::{
    WidgetView,
    masonry::{
        accesskit::{Live, Role, Toggled},
        core::{
            TextEvent, WidgetId,
            keyboard::{Key, NamedKey},
        },
    },
    tokio::sync::mpsc::unbounded_channel,
    view::sized_box,
};

use crate::test_support as support;
use support::Ui;

fn state() -> (tempfile::TempDir, SettingsApp) {
    let (dir, favorites) = crate::favorites::tests::fixture();
    (
        dir,
        SettingsApp {
            activation_listener: None,
            ai: AiSettings::default(),
            favorites,
            settings: Settings::default(),
            live_outputs: vec![],
            selected_category: SettingsCategory::Ai,
            selected_monitor_id: None,
            background_picker_request_sender: None,
            background_picker_pending: false,
            status: String::new(),
        },
    )
}

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(super::view(state))
}

#[test]
fn feature_details_expand_with_keyboard_and_report_their_state() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (710, 680));
    let feature = ui.button("On-device AI");
    assert_eq!(
        ui.harness
            .access_node(feature)
            .unwrap()
            .data()
            .is_expanded(),
        Some(false)
    );
    ui.harness.focus_on(Some(feature));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(ui.state.ai.features_open[0]);
    assert_eq!(
        ui.harness
            .access_node(feature)
            .unwrap()
            .data()
            .is_expanded(),
        Some(true)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(feature));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    assert_eq!(
        ui.harness
            .access_node(feature)
            .unwrap()
            .data()
            .is_expanded(),
        Some(false)
    );
}

fn switch<V: WidgetView<SettingsApp, Widget: Sized>>(ui: &Ui<V>) -> WidgetId {
    support::ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Switch && node.label().as_deref() == Some("Enable AI features")
            })
        })
        .expect("named AI switch")
}

fn ready() -> ModelStatus {
    ModelStatus {
        registered: true,
        installed: true,
        downloaded: true,
        warm: true,
        device: DevicePreference::Cpu,
        runtime_error: None,
    }
}

#[test]
fn switch_queues_real_install_and_stays_disabled_during_background_work() {
    let (_dir, mut state) = state();
    let (sender, mut requests) = unbounded_channel();
    state.ai.sender = Some(sender);
    state.ai.receive(Response::Finished {
        operation: Operation::Refresh,
        snapshot: Some(ModelStatus::default()),
        error: None,
    });
    let mut ui = Ui::new(state, view, (440, 420));
    let toggle = switch(&ui);
    ui.harness.focus_on(Some(toggle));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    assert_eq!(requests.try_recv().unwrap(), Operation::Install);
    let node = ui.harness.access_node(toggle).unwrap();
    assert!(node.is_disabled());
    assert_eq!(node.data().toggled(), Some(Toggled::True));
    assert!(support::ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.role() == Role::Status && node.data().live() == Some(Live::Polite)
        })
    }));
    ui.activate(toggle);
    ui.dispatch(&view);
    assert!(
        requests.try_recv().is_err(),
        "busy switch cannot cancel or duplicate work"
    );
    ui.state.select_category(SettingsCategory::Displays);
    assert_eq!(ui.state.ai.pending, Some(Operation::Install));
    assert!(!support::ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            matches!(
                node.label().as_deref(),
                Some(
                    "Local model"
                        | "Processing mode"
                        | "Refresh status"
                        | "Load D1 model"
                        | "Delete D1 model"
                )
            )
        })
    }));
}

#[test]
fn keyboard_off_requires_confirmation_defaults_to_keep_and_restores_switch_focus() {
    let (_dir, mut state) = state();
    let (sender, mut requests) = unbounded_channel();
    state.ai.sender = Some(sender);
    state.ai.receive(Response::Finished {
        operation: Operation::Refresh,
        snapshot: Some(ready()),
        error: None,
    });
    let mut ui = Ui::new(state, view, (440, 580));
    let toggle = switch(&ui);
    ui.harness.focus_on(Some(toggle));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    assert!(
        requests.try_recv().is_err(),
        "opening confirmation must not delete"
    );
    assert_eq!(
        ui.harness.access_node(toggle).unwrap().data().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Keep AI")));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(!ui.state.ai.confirm_off);
    assert!(requests.try_recv().is_err());
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    ui.harness.focus_on(Some(ui.button("Turn off and delete")));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert_eq!(requests.try_recv().unwrap(), Operation::Remove);
    assert!(ui.harness.access_node(toggle).unwrap().is_disabled());
    ui.state.ai.receive(Response::Finished {
        operation: Operation::Remove,
        snapshot: Some(ModelStatus::default()),
        error: None,
    });
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert_eq!(
        ui.harness.access_node(toggle).unwrap().data().toggled(),
        Some(Toggled::False)
    );
    assert_eq!(ui.state.ai.message, "AI features are off.");
}

#[test]
fn partial_install_error_offers_real_retry_and_confirmed_cleanup_within_status() {
    let (_dir, mut state) = state();
    let (sender, mut requests) = unbounded_channel();
    state.ai.sender = Some(sender);
    state.ai.receive(Response::Finished {
        operation: Operation::Install,
        snapshot: Some(ModelStatus {
            downloaded: true,
            runtime_error: Some("Ollama is unavailable".into()),
            ..ModelStatus::default()
        }),
        error: Some("Import failed".into()),
    });
    let mut ui = Ui::new(state, view, (440, 580));
    assert_eq!(
        ui.harness
            .access_node(switch(&ui))
            .unwrap()
            .data()
            .toggled(),
        Some(Toggled::False)
    );
    ui.activate(ui.button("Remove incomplete download"));
    ui.dispatch(&view);
    assert!(requests.try_recv().is_err());
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Keep AI")));
    ui.activate(ui.button("Keep AI"));
    ui.dispatch(&view);
    ui.activate(ui.button("Try again"));
    ui.dispatch(&view);
    assert_eq!(requests.try_recv().unwrap(), Operation::Install);
}

#[test]
fn externally_warmed_model_stays_off_until_user_explicitly_enables_it() {
    let (_dir, mut state) = state();
    let (sender, mut requests) = unbounded_channel();
    state.ai.sender = Some(sender);
    state.ai.receive(Response::Finished {
        operation: Operation::Refresh,
        snapshot: Some(ModelStatus {
            registered: false,
            downloaded: false,
            ..ready()
        }),
        error: None,
    });
    let mut ui = Ui::new(state, view, (440, 420));
    let toggle = switch(&ui);
    assert_eq!(
        ui.harness.access_node(toggle).unwrap().data().toggled(),
        Some(Toggled::False)
    );
    assert_eq!(ui.state.ai.message, "AI features are off.");
    assert!(
        requests.try_recv().is_err(),
        "observing another runtime is not opt-in"
    );
    ui.activate(toggle);
    ui.dispatch(&view);
    assert_eq!(requests.try_recv().unwrap(), Operation::EnableInstalled);
    let mut status = ready();
    status.downloaded = false;
    ui.state.ai.receive(Response::Finished {
        operation: Operation::EnableInstalled,
        snapshot: Some(status),
        error: None,
    });
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(
        ui.harness.access_node(toggle).unwrap().data().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(ui.state.ai.message, "AI features are ready.");
}
