//! Keep the tray and Battery dropdown in sync with system battery readings.
use crate::battery::BatteryStatus;
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use super::state::ShellState;

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Option<BatteryStatus>>, _: &mut ShellState| {
            let address = std::env::var("HEARTHSPACE_SESSION_BUS_ADDRESS").ok();
            async move {
                let (sender, mut snapshots) = tokio::sync::mpsc::unbounded_channel();
                let render = async move {
                    while let Some(snapshot) = snapshots.recv().await {
                        if proxy.message(snapshot).is_err() {
                            break;
                        }
                    }
                };
                tokio::select! {
                    () = crate::battery::monitor(sender, address) => {},
                    () = render => {},
                }
            }
        },
        |state: &mut ShellState, battery| state.battery = battery,
    )
}
