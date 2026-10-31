use super::WIDGETS;
use crate::{hex_decode, hex_encode, shell_app_id_for_output, shell_output_name_from_app_id};

const WIDGET: &str = "dev.hearthspace.frame-widget";
const GROUP: &str = "dev.hearthspace.frame-group";
const GRIP: &str = "dev.hearthspace.frame-group-grip";
const EDITOR: &str = "dev.hearthspace.layout-editor";
const CONFIRM: &str = "dev.hearthspace.layout-confirm";
const TOOLTIP: &str = "dev.hearthspace.layout-tooltip";
const REVEAL: &str = "dev.hearthspace.pinned-window-reveal.window-";

pub fn frame_widget_app_id(output: Option<&str>, id: &str) -> String {
    target_app_id(WIDGET, output, id)
}

pub fn frame_widget_from_app_id(app_id: &str) -> Option<(Option<String>, String)> {
    let (output, id) = parse_target_app_id(WIDGET, app_id)?;
    WIDGETS.contains(&id.as_str()).then_some((output, id))
}

pub fn frame_group_app_id(output: Option<&str>, id: &str) -> String {
    target_app_id(GROUP, output, id)
}

pub fn frame_group_from_app_id(app_id: &str) -> Option<(Option<String>, String)> {
    parse_target_app_id(GROUP, app_id)
}

pub fn frame_group_grip_app_id(output: Option<&str>, id: &str) -> String {
    target_app_id(GRIP, output, id)
}

pub fn frame_group_grip_from_app_id(app_id: &str) -> Option<(Option<String>, String)> {
    parse_target_app_id(GRIP, app_id)
}

pub fn layout_editor_app_id(session: u64, output: Option<&str>) -> String {
    format!("{}.session-{session}", output_base(EDITOR, output))
}

pub fn layout_editor_from_app_id(app_id: &str) -> Option<(u64, Option<String>)> {
    parse_session_app_id(EDITOR, app_id)
}

pub fn layout_confirm_app_id(session: u64, output: Option<&str>) -> String {
    format!("{}.session-{session}", output_base(CONFIRM, output))
}

pub fn layout_confirm_from_app_id(app_id: &str) -> Option<(u64, Option<String>)> {
    parse_session_app_id(CONFIRM, app_id)
}

pub fn layout_tooltip_app_id(session: u64, output: Option<&str>) -> String {
    format!("{}.session-{session}", output_base(TOOLTIP, output))
}

pub fn layout_tooltip_from_app_id(app_id: &str) -> Option<(u64, Option<String>)> {
    parse_session_app_id(TOOLTIP, app_id)
}

fn parse_session_app_id(prefix: &str, app_id: &str) -> Option<(u64, Option<String>)> {
    // Reject other surface families before scanning their encoded output names.
    app_id.strip_prefix(prefix)?.strip_prefix('.')?;
    let (base, session) = app_id.rsplit_once(".session-")?;
    let session = session.parse::<u64>().ok().filter(|id| *id > 0)?;
    Some((session, parse_output(prefix, base)?))
}

pub fn pinned_window_reveal_app_id(window_id: u64) -> String {
    format!("{REVEAL}{window_id}")
}

pub fn pinned_window_reveal_from_app_id(app_id: &str) -> Option<u64> {
    app_id
        .strip_prefix(REVEAL)?
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
}

fn target_app_id(base: &str, output: Option<&str>, id: &str) -> String {
    format!(
        "{}.target-{}",
        output_base(base, output),
        hex_encode(id.as_bytes())
    )
}

fn parse_target_app_id(base: &str, app_id: &str) -> Option<(Option<String>, String)> {
    // The delimiter also distinguishes frame-group from frame-group-grip.
    app_id.strip_prefix(base)?.strip_prefix('.')?;
    let (prefix, id) = app_id.rsplit_once(".target-")?;
    if id.is_empty() || id.len() > 2048 {
        return None;
    }
    let id = String::from_utf8(hex_decode(id)?).ok()?;
    if id.chars().any(char::is_control) {
        return None;
    }
    Some((parse_output(base, prefix)?, id))
}

fn output_base(base: &str, output: Option<&str>) -> String {
    output.map_or_else(
        || base.to_owned(),
        |output| shell_app_id_for_output(base, output),
    )
}

fn parse_output(base: &str, app_id: &str) -> Option<Option<String>> {
    let output = shell_output_name_from_app_id(base, app_id)?;
    if output.as_ref().is_some_and(|name| {
        name.is_empty() || name.len() > 256 || name.chars().any(char::is_control)
    }) {
        return None;
    }
    Some(output)
}
