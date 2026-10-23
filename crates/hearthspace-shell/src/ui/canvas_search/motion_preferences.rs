//! Honor the host's animation preference without blocking native UI or requiring
//! GNOME services. Missing schemas/tools leave animations enabled.
use crate::ui::state::ShellState;
use std::{process::Stdio, time::Duration};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

pub(in crate::ui) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<>
{
    task_raw(
        |proxy: MessageProxy<bool>, _: &mut ShellState| async move {
            loop {
                let enabled = animation_preference().await.unwrap_or(true);
                // Resend after a disconnect or lock; an identical preference
                // is idempotent and does not repaint or restart animation.
                if proxy.message(enabled).is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        },
        |state, enabled| super::submit_command(state, format!("canvas-search-motion {enabled}")),
    )
}

async fn animation_preference() -> Option<bool> {
    let mut command = tokio::process::Command::new("gsettings");
    command
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let result = tokio::time::timeout(Duration::from_millis(750), command.output())
        .await
        .ok()?
        .ok()?;
    result
        .status
        .success()
        .then(|| parse(&result.stdout))
        .flatten()
}

fn parse(bytes: &[u8]) -> Option<bool> {
    match std::str::from_utf8(bytes).ok()?.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    #[test]
    fn respects_disabled_animations_and_ignores_missing_schema_errors() {
        assert_eq!(parse(b"false\n"), Some(false));
        assert_eq!(parse(b"true\n"), Some(true));
        assert_eq!(parse(b"No such schema"), None);
    }
}
