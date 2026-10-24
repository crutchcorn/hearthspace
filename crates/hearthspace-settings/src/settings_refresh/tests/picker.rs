//! Deliver real picker worker responses without running the native dialog task.
use super::*;
use crate::state::BackgroundPickerResponse;
use std::sync::Arc;
use xilem::{
    ViewCtx,
    core::{DynMessage, MessageCtx, MessageResult, RawProxy, View, ViewId, ViewPathTracker},
    masonry::testing::{TestHarness, TestHarnessParams},
};

#[derive(Debug)]
struct NoBackgroundMessages;
impl RawProxy for NoBackgroundMessages {
    fn send_message(
        &self,
        _: Arc<[ViewId]>,
        _: xilem::core::SendMessage,
    ) -> Result<(), xilem::core::ProxyError> {
        panic!("the native picker and other background tasks must not run in this regression")
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

fn picker_completion(response: BackgroundPickerResponse, expected_status: &str) {
    let (_directory, mut state) = state();
    state.background_picker_pending = true;
    state.settings.background_image_path = Some("existing.png".into());
    let snapshot = SettingsSnapshot::new(
        5,
        Settings {
            background_image_path: Some("external.png".into()),
            ..Settings::default()
        },
    )
    .unwrap();
    apply_refresh(&mut state, (0, snapshot.clone()));
    assert_eq!(
        state.settings.background_image_path.as_deref(),
        Some("existing.png")
    );

    // Build the real app task tree, but never poll its current-thread runtime.
    // The outer fork's alongside view is the actual picker response handler.
    let runtime = Arc::new(
        xilem::tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let mut ctx = ViewCtx::new(Arc::new(NoBackgroundMessages), runtime);
    let app = sized_box(crate::ui::app_logic(&mut state));
    let (root, mut view_state) = app.build(&mut ctx, &mut state);
    let mut harness = TestHarness::create_with(
        crate::appearance::properties(state.palette()),
        root.new_widget,
        TestHarnessParams::default().with_size((680, 490)),
    );
    let mut message = MessageCtx::new(
        std::mem::take(ctx.environment()),
        vec![ViewId::new(1)],
        DynMessage::new(response),
    );
    let result = harness
        .edit_root_widget(|root| app.message(&mut view_state, &mut message, root, &mut state));
    assert!(matches!(result, MessageResult::Action(())));
    assert!(!state.background_picker_pending);
    assert_eq!(state.status, expected_status);
    // Reads queued before the completion remain stale. The mandatory fresh GET
    // can apply the same committed cursor that was deferred while choosing.
    apply_refresh(&mut state, (0, snapshot.clone()));
    assert_eq!(
        state.settings.background_image_path.as_deref(),
        Some("existing.png")
    );
    apply_refresh(&mut state, (1, snapshot));
    assert_eq!(
        state.settings.background_image_path.as_deref(),
        Some("external.png")
    );
    assert_eq!(state.status, expected_status);
}

#[test]
fn cancelled_picker_refetches_the_external_path_deferred_while_open() {
    picker_completion(
        BackgroundPickerResponse::Cancelled,
        "Background image selection cancelled",
    );
}

#[test]
fn failed_picker_refetches_the_external_path_without_erasing_failure_status() {
    picker_completion(
        BackgroundPickerResponse::Failed("portal unavailable".into()),
        "Background image picker failed: portal unavailable",
    );
}
