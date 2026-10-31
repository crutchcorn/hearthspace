//! App menu requests retain the source control's lifetime and measured anchor.
use super::{ShellState, desktop::task};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use xilem::masonry::kurbo::Rect;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub app_id: String,
    pub output_name: Option<String>,
    pub source_app_id: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Intent {
    #[default]
    Ordinary,
    Accessibility,
}

#[derive(Debug)]
pub(super) struct Request {
    pub target: Target,
    pub anchor: Rect,
    pub alive: Arc<AtomicBool>,
    pub deadline: Instant,
    pub popup_epoch: u64,
    pub intent: Intent,
}

impl Request {
    pub fn current(&self) -> bool {
        self.alive.load(Ordering::Acquire) && Instant::now() < self.deadline
    }

    pub fn perform(&self, socket: &Path) -> Result<(), String> {
        if !self.current() {
            return Err("The app menu request expired. Open the menu again.".into());
        }
        let command = format!(
            "desktop-app-context {}",
            serde_json::json!({
                "app_id": self.target.app_id,
                "output_name": self.target.output_name,
                "source_app_id": self.target.source_app_id,
                "popup_epoch": self.popup_epoch,
                "intent": self.intent,
                "anchor": {
                    "x": self.anchor.x0,
                    "y": self.anchor.y0,
                    "width": self.anchor.width(),
                    "height": self.anchor.height(),
                },
            })
        );
        super::command_client::send_command_with_timeout(
            socket,
            &command,
            self.deadline.saturating_duration_since(Instant::now()),
        )
    }
}

pub(super) fn request(state: &mut ShellState, mut request: Request) {
    if !request.current() {
        return;
    }
    // Capture admission ownership before either reporting a rejection or queueing.
    // The blocking worker must never refresh it.
    request.popup_epoch = state.frame_layout.popup_epoch;
    if let Err(error) = validate_app_id(&request.target.app_id) {
        finished(state, request, Err(error));
        return;
    }
    if state.catalog.app_by_id(&request.target.app_id).is_none() {
        finished(
            state,
            request,
            Err("This application is no longer installed.".into()),
        );
        return;
    }
    task::submit(state, task::Request::AppContext(request));
}

/// Match the compositor registry's desktop identity boundary, not the launch catalog.
pub(super) fn validate_app_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || hearthspace_common::desktop_item_id_from_app_id(
            &hearthspace_common::desktop_item_app_id(&format!("app:{id}")),
        )
        .is_none()
    {
        Err("This application's identifier is too long or contains unsupported characters for an app menu. You can still launch it.".into())
    } else {
        Ok(())
    }
}

pub(super) fn finished(state: &mut ShellState, request: Request, result: Result<(), String>) {
    let Err(error) = result else {
        return;
    };
    // A late failure cannot reopen a dismissed panel or steal another source's focus.
    if !request.alive.load(Ordering::Acquire)
        || request.popup_epoch != state.frame_layout.popup_epoch
    {
        return;
    }
    if let Some(output) = state
        .outputs
        .iter_mut()
        .find(|output| output.output_name == request.target.output_name)
    {
        if request.target.source_app_id == output.launcher_app_id() && !output.show_search {
            return;
        }
        output.launch_error = Some(format!("Could not open app menu: {error}"));
        output.show_search = true;
    }
}

pub(super) fn new_request(target: Target, anchor: Rect, alive: Arc<AtomicBool>) -> Request {
    Request {
        target,
        anchor,
        alive,
        deadline: Instant::now() + Duration::from_millis(750),
        popup_epoch: 0,
        intent: Intent::Ordinary,
    }
}
