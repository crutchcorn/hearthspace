//! Toolkit-owned masked credentials and pairing confirmation, only on user actions.
use super::Result;
use std::process::Stdio;

async fn dialog(arguments: &[&str]) -> Result<String> {
    let child = tokio::process::Command::new("zenity")
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            "The authentication dialog is unavailable. Install Zenity and try again.".to_owned()
        })?;
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| "Authentication timed out.".to_owned())?
    .map_err(|_| "Authentication was cancelled.".to_owned())?;
    if !output.status.success() {
        return Err("Authentication was cancelled.".into());
    }
    let mut text =
        String::from_utf8(output.stdout).map_err(|_| "Authentication failed.".to_owned())?;
    while text.ends_with(['\n', '\r']) {
        text.pop();
    }
    Ok(text)
}
pub(super) async fn password() -> Result<String> {
    dialog(&["--password", "--title=Wi-Fi password"]).await
}
pub(super) async fn pin() -> Result<String> {
    dialog(&[
        "--entry",
        "--hide-text",
        "--title=Bluetooth pairing",
        "--text=Enter the code shown on your Bluetooth device.",
        "--no-markup",
    ])
    .await
}
pub(super) async fn confirm(text: &str) -> Result<()> {
    dialog(&[
        "--question",
        "--title=Bluetooth pairing",
        "--text",
        text,
        "--no-markup",
    ])
    .await
    .map(|_| ())
}

pub(super) async fn info(text: &str) -> Result<()> {
    dialog(&[
        "--info",
        "--title=Bluetooth pairing",
        "--text",
        text,
        "--no-markup",
    ])
    .await
    .map(|_| ())
}
