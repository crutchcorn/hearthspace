use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use super::{DesktopApp, paths::executable_in_path};

impl DesktopApp {
    pub(super) fn from_desktop_entry(id: String, path: PathBuf, contents: &str) -> Option<Self> {
        let fields = parse_desktop_entry_fields(contents);
        if fields.get("Type")? != "Application" {
            return None;
        }
        if bool_field(&fields, "Hidden") || bool_field(&fields, "NoDisplay") {
            return None;
        }
        if !only_show_in_allows(fields.get("OnlyShowIn")) {
            return None;
        }
        if !not_show_in_allows(fields.get("NotShowIn")) {
            return None;
        }
        if fields
            .get("TryExec")
            .is_some_and(|try_exec| !try_exec_resolves(try_exec))
        {
            return None;
        }

        Some(Self {
            id,
            name: fields.get("Name")?.clone(),
            generic_name: fields.get("GenericName").cloned(),
            comment: fields.get("Comment").cloned(),
            keywords: fields
                .get("Keywords")
                .map(|value| parse_semicolon_list(value))
                .unwrap_or_default(),
            exec: fields.get("Exec")?.clone(),
            icon: fields.get("Icon").cloned(),
            terminal: bool_field(&fields, "Terminal"),
            path,
            categories: fields
                .get("Categories")
                .map(|value| parse_semicolon_list(value))
                .unwrap_or_default(),
            terminal_arg_exec: fields
                .get("TerminalArgExec")
                .or_else(|| fields.get("X-TerminalArgExec"))
                .or_else(|| fields.get("X-ExecArg"))
                .cloned(),
            snap_instance_name: fields.get("X-SnapInstanceName").cloned(),
        })
    }
}

pub(super) fn desktop_files_under(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_desktop_files(dir, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_desktop_files(dir: &Path, paths: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_desktop_files(&path, paths)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "desktop")
        {
            paths.push(path);
        }
    }
    Ok(())
}

pub(super) fn desktop_entry_id(base: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    let id = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("-");
    (!id.is_empty()).then_some(id)
}

fn parse_desktop_entry_fields(contents: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let mut in_desktop_entry = false;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.contains('[') {
            continue;
        }
        fields.insert(key.to_string(), unescape_desktop_value(value));
    }

    fields
}

pub(super) fn unescape_desktop_value(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('s') => output.push(' '),
            Some('n') => output.push('\n'),
            Some('t') => output.push('\t'),
            Some('r') => output.push('\r'),
            Some('\\') => output.push('\\'),
            Some(other) => output.push(other),
            None => output.push('\\'),
        }
    }
    output
}

fn bool_field(fields: &HashMap<String, String>, key: &str) -> bool {
    fields
        .get(key)
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

pub(super) fn parse_semicolon_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn only_show_in_allows(value: Option<&String>) -> bool {
    let Some(value) = value else {
        return true;
    };

    parse_semicolon_list(value)
        .iter()
        .any(|desktop| desktop == hearthspace_common::CURRENT_DESKTOP)
}

fn not_show_in_allows(value: Option<&String>) -> bool {
    let Some(value) = value else {
        return true;
    };

    !parse_semicolon_list(value)
        .iter()
        .any(|desktop| desktop == hearthspace_common::CURRENT_DESKTOP)
}

fn try_exec_resolves(try_exec: &str) -> bool {
    let path = Path::new(try_exec);
    if path.is_absolute() || try_exec.contains('/') {
        path.exists()
    } else {
        executable_in_path(try_exec)
    }
}
