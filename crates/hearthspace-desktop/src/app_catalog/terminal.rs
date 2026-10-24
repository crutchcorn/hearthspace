use std::{fs, path::PathBuf};

use super::{
    AppCatalog, DesktopApp,
    paths::{executable_in_path, xdg_config_dirs, xdg_data_dirs},
};

pub(super) fn terminal_command_for(
    catalog: &AppCatalog,
    command: Vec<String>,
) -> Result<Vec<String>, String> {
    if command.is_empty() {
        return Err("cannot wrap an empty terminal command".to_string());
    }

    if executable_in_path("xdg-terminal-exec") {
        let mut terminal_command = vec!["xdg-terminal-exec".to_string()];
        terminal_command.extend(command);
        return Ok(terminal_command);
    }

    let terminal = catalog
        .preferred_terminal()
        .ok_or_else(|| "no terminal emulator found".to_string())?;
    let mut terminal_command = terminal.launch_argv()?;
    match terminal.terminal_arg_exec.as_deref() {
        Some("") => {}
        Some(arg) => terminal_command.push(arg.to_string()),
        None => terminal_command.push("-e".to_string()),
    }
    terminal_command.extend(command);
    Ok(terminal_command)
}

pub(super) fn preferred_terminal(catalog: &AppCatalog) -> Option<&DesktopApp> {
    for id in terminal_preference_ids() {
        if let Some(app) = catalog.app_by_id(&id).filter(|app| {
            app.categories
                .iter()
                .any(|category| category == "TerminalEmulator")
        }) {
            return Some(app);
        }
    }

    catalog.apps.iter().find(|app| {
        app.categories
            .iter()
            .any(|category| category == "TerminalEmulator")
    })
}

fn terminal_preference_ids() -> Vec<String> {
    terminal_preference_files()
        .into_iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .flat_map(|contents| {
            contents
                .lines()
                .filter_map(|line| {
                    let line = line.trim();
                    if line.is_empty()
                        || line.starts_with('#')
                        || line.starts_with('/')
                        || line.starts_with('-')
                        || line.starts_with('+')
                    {
                        return None;
                    }
                    Some(
                        line.split_once(':')
                            .map_or(line, |(entry_id, _)| entry_id)
                            .to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn terminal_preference_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let config_dirs = xdg_config_dirs();
    for config_dir in config_dirs {
        files.push(config_dir.join("hearthspace-xdg-terminals.list"));
        files.push(config_dir.join("xdg-terminals.list"));
    }
    for data_dir in xdg_data_dirs().into_iter().skip(1) {
        files.push(data_dir.join("xdg-terminal-exec/hearthspace-xdg-terminals.list"));
        files.push(data_dir.join("xdg-terminal-exec/xdg-terminals.list"));
    }
    files
}
