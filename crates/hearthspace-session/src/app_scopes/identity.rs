use std::{io, path::Path};

pub const CLOSING_FILE: &str = "app-scopes-closing";
pub const READY_FILE: &str = "app-scope-guard-ready";

pub fn owner_unit(runtime: &Path) -> io::Result<String> {
    let name = runtime
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if name.is_empty()
        || name.len() > 100
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    {
        return Err(io::Error::other(
            "session runtime basename cannot form an app scope owner",
        ));
    }
    Ok(format!("hearthspace-apps-{name}.service"))
}

pub fn valid_scope_name(name: &str) -> bool {
    if name.len() > 255
        || !name.ends_with(".scope")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        return false;
    }
    if name.starts_with("snap.") {
        return true;
    }
    name.strip_prefix("ptyxis-spawn-")
        .and_then(|name| name.strip_suffix(".scope"))
        .is_some_and(|uuid| {
            uuid.len() == 36
                && uuid.bytes().enumerate().all(|(index, byte)| {
                    if [8, 13, 18, 23].contains(&index) {
                        byte == b'-'
                    } else {
                        byte.is_ascii_hexdigit()
                    }
                })
        })
}

pub fn process_start_time(stat: &str) -> Option<&str> {
    // comm may contain spaces or parentheses. Field 22 follows the final ')'.
    stat.rsplit_once(')')?.1.split_whitespace().nth(19)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_identity_survives_spaces_and_parentheses_in_comm() {
        let suffix = (3..=30)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            process_start_time(&format!("42 (weird ) name) {suffix}")),
            Some("22")
        );
        assert_eq!(process_start_time("invalid"), None);
    }
}
