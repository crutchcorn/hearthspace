use crate::{hex_decode, hex_encode, shell_app_id_for_output, shell_output_name_from_app_id};

const FIELD_ID: &str = "dev.hearthspace.canvas-search";
const RESULT_ID: &str = "dev.hearthspace.canvas-search-result";

pub fn canvas_search_app_id(session: u64, output: Option<&str>) -> String {
    let base = output.map_or_else(
        || FIELD_ID.to_owned(),
        |name| shell_app_id_for_output(FIELD_ID, name),
    );
    format!("{base}.session-{session}")
}

pub fn canvas_search_from_app_id(app_id: &str) -> Option<(u64, Option<String>)> {
    let (base, session) = app_id.rsplit_once(".session-")?;
    let session = session.parse::<u64>().ok().filter(|id| *id > 0)?;
    let output = shell_output_name_from_app_id(FIELD_ID, base)?;
    if output
        .as_ref()
        .is_some_and(|name| name.is_empty() || name.len() > 256)
    {
        return None;
    }
    Some((session, output))
}

/// Session zero reserves a persistent, initially hidden control/tool preview.
pub fn canvas_search_result_app_id(session: u64, id: &str) -> String {
    format!(
        "{RESULT_ID}.session-{session}.item-{}",
        hex_encode(id.as_bytes())
    )
}

pub fn canvas_search_result_from_app_id(app_id: &str) -> Option<(u64, String)> {
    let suffix = app_id.strip_prefix(RESULT_ID)?.strip_prefix(".session-")?;
    let (session, id) = suffix.split_once(".item-")?;
    let session = session.parse::<u64>().ok()?;
    if id.is_empty() || id.len() > 2048 {
        return None;
    }
    let id = String::from_utf8(hex_decode(id)?).ok()?;
    if session == 0
        && !["control:", "tool:"]
            .iter()
            .any(|prefix| id.strip_prefix(prefix).is_some_and(|name| !name.is_empty()))
    {
        return None;
    }
    (!id.chars().any(char::is_control)).then_some((session, id))
}
