use hearthspace_ai::{DevicePreference, ModelManager, ModelStatus};
use xilem::{
    ViewCtx,
    core::{NoElement, View},
    view::worker,
};

use crate::{
    ai::{Operation, Response},
    state::SettingsApp,
};

/// Lives above the page switcher, so changing categories never drops a download.
pub(crate) fn worker_view() -> impl View<SettingsApp, (), ViewCtx, Element = NoElement> {
    worker(
        |proxy, mut rx| async move {
            while let Some(operation) = rx.recv().await {
                let progress_proxy = proxy.clone();
                let result = xilem::tokio::task::spawn_blocking(move || {
                    execute(operation, move |progress| {
                        drop(progress_proxy.message(Response::Progress(progress)));
                    })
                })
                .await
                .unwrap_or_else(|error| Err(format!("Local AI worker failed: {error}")));
                let (snapshot, error) = match result {
                    Ok((snapshot, error)) => (Some(snapshot), error),
                    Err(error) => (None, Some(error)),
                };
                drop(proxy.message(Response::Finished {
                    operation,
                    snapshot,
                    error,
                }));
            }
        },
        |state: &mut SettingsApp, sender| {
            state.ai.sender = Some(sender);
            state.ai.request(Operation::Refresh);
        },
        |state: &mut SettingsApp, response| state.ai.receive(response),
    )
}

fn execute(
    operation: Operation,
    progress: impl Fn(hearthspace_ai::InstallProgress) + Send + Sync + 'static,
) -> Result<(ModelStatus, Option<String>), String> {
    let manager = ModelManager::from_env().map_err(|error| error.to_string())?;
    let result = match operation {
        Operation::Refresh => Ok(()),
        Operation::Install => {
            let device = manager
                .status()
                .map(|status| status.device)
                .unwrap_or(DevicePreference::Automatic);
            manager.install(device, progress)
        }
        Operation::Remove => manager.remove(),
        Operation::EnableInstalled => manager.enable_installed(),
        Operation::Warm => manager.warm(),
    };
    let error = result.err().map(|error| error.to_string());
    match manager.status() {
        Ok(status) => Ok((status, error)),
        Err(status_error) => Err(error.unwrap_or_else(|| status_error.to_string())),
    }
}
