//! Bounded compositor feedback and explicit reloads, off the UI thread.
use crate::state::SettingsApp;
use std::time::Duration;
use xilem::{
    ViewCtx,
    core::{NoElement, View},
    tokio::sync::watch,
    view::task_raw,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Request {
    path: Option<String>,
    generation: u64,
}
#[derive(Debug)]
struct Response {
    request: Request,
    result: Result<String, String>,
}

#[derive(Default)]
pub(crate) struct ShaderStatus {
    request: Request,
    sender: Option<watch::Sender<Request>>,
    pub(crate) message: String,
    pub(crate) failed: bool,
}

impl ShaderStatus {
    pub(crate) fn synchronize_path(&mut self, path: Option<&str>) {
        if self.request.path.as_deref() != path {
            self.request.path = path.map(str::to_owned);
            self.reload();
        }
    }
    pub(crate) fn reload(&mut self) {
        self.request.generation = self.request.generation.wrapping_add(1);
        self.message = "Applying GLSL wallpaper…".into();
        self.failed = false;
        if let Some(sender) = &self.sender {
            sender.send_replace(self.request.clone());
        }
    }
    fn receive(&mut self, response: Response) {
        if response.request != self.request {
            return;
        }
        self.failed = response.result.is_err();
        self.message = response.result.unwrap_or_else(|error| error);
    }
}

pub(crate) fn task() -> impl View<SettingsApp, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy, state: &mut SettingsApp| {
            let (sender, mut receiver) =
                watch::channel(state.background_shader_status.request.clone());
            state.background_shader_status.sender = Some(sender);
            async move {
                let mut previous = None;
                loop {
                    let request = receiver.borrow_and_update().clone();
                    if request.path.is_some() {
                        let reload = previous.as_ref() != Some(&request);
                        previous = Some(request.clone());
                        let work = request.clone();
                        let result = crate::background_preview::blocking_load(move || {
                            observe(&work, reload)
                        })
                        .await
                        .unwrap_or_else(|| Err("Could not read GLSL wallpaper status".into()));
                        if proxy.message(Response { request, result }).is_err() {
                            break;
                        }
                        xilem::tokio::select! {
                            changed = receiver.changed() => { if changed.is_err() { break; } },
                            _ = xilem::tokio::time::sleep(Duration::from_secs(1)) => {},
                        }
                    } else if receiver.changed().await.is_err() {
                        break;
                    }
                }
            }
        },
        |state: &mut SettingsApp, response| state.background_shader_status.receive(response),
    )
}

fn observe(request: &Request, reload: bool) -> Result<String, String> {
    let socket =
        crate::settings_backend::command_socket_path().map_err(|error| error.to_string())?;
    observe_at(&socket, request, reload)
}

fn observe_at(socket: &std::path::Path, request: &Request, reload: bool) -> Result<String, String> {
    if reload {
        let reply = hearthspace_ipc::transport::request_line(
            socket,
            "reload-settings\n",
            Duration::from_millis(1500),
        )
        .map_err(|error| error.to_string())?;
        if reply.trim() != "ok" {
            return Err(reply.trim().trim_start_matches("err ").into());
        }
    }
    let bytes = hearthspace_ipc::transport::request_payload(
        socket,
        "status",
        Duration::from_millis(500),
        1024 * 1024,
    )
    .map_err(|error| error.to_string())?;
    let status: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let shader = &status["background_shader"];
    if shader["path"].as_str() != request.path.as_deref() {
        return Ok("Waiting for the desktop to apply GLSL…".into());
    }
    if let Some(error) = shader["error"].as_str() {
        return Err(format!("{error}\nShowing Canvas dots instead."));
    }
    if shader["ready"] != true {
        return Ok("Waiting for the desktop to apply GLSL…".into());
    }
    Ok("GLSL wallpaper is active on the desktop.".into())
}

#[cfg(test)]
mod tests;
