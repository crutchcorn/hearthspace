//! Shell adapter: connector names keep each panel bound to its existing output.
use super::state::{OutputNamesChanged, ShellState};
use hearthspace_wayland_client::{OutputInfo, connect_wayland, initial_outputs, watch_outputs};
use std::io;
use tracing::error;
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

pub(super) fn output_watcher_task()
-> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<OutputNamesChanged>, _state: &mut ShellState| async move {
            let result = async {
                let connection = connect_wayland().await?;
                let mut previous = None;
                watch_outputs(connection, |outputs| {
                    let names = output_names(outputs);
                    if previous.as_ref() != Some(&names) {
                        previous = Some(names.clone());
                        proxy
                            .message(OutputNamesChanged { names })
                            .map_err(|error| io::Error::other(error.to_string()))?;
                    }
                    Ok(())
                })
                .await
            }
            .await;
            if let Err(error) = result {
                error!(%error, "Wayland output watcher stopped");
            }
        },
        |state: &mut ShellState, message: OutputNamesChanged| {
            state.apply_output_names(message.names)
        },
    )
}

pub(super) fn initial_wayland_output_names() -> io::Result<Vec<String>> {
    initial_outputs().map(output_names)
}

fn output_names(outputs: Vec<OutputInfo>) -> Vec<String> {
    outputs
        .into_iter()
        .filter_map(|output| output.name)
        .collect()
}

#[cfg(test)]
mod tests;
