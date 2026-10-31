use std::{collections::HashSet, path::PathBuf};

use tracing::debug;

mod desktop_entry;
mod exec;
pub(crate) mod gpu_preference;
mod matching;
mod paths;
mod search;
mod spawn;
mod terminal;

pub use gpu_preference::set_app_prefers_non_default_gpu;
pub use spawn::launch_command;
pub use spawn::{
    reap_child, spawn_argv, spawn_argv_with_env, spawn_argv_with_options,
    spawn_argv_with_wayland_display,
};

/// Resolve current XDG precedence without GIO's process-wide directory cache.
/// MIME handlers can have NoDisplay=true, so this must not use the visible catalog.
pub(crate) fn desktop_file_by_id(id: &str) -> Option<PathBuf> {
    for directory in paths::xdg_data_dirs() {
        let applications = directory.join("applications");
        let Ok(files) = desktop_entry::desktop_files_under(&applications) else {
            continue;
        };
        if let Some(path) = files.into_iter().find(|path| {
            desktop_entry::desktop_entry_id(&applications, path).as_deref() == Some(id)
        }) {
            return Some(path);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub keywords: Vec<String>,
    pub exec: String,
    pub dbus_activatable: bool,
    /// A next-start hint; false leaves ordinary application GPU selection intact.
    pub prefers_non_default_gpu: bool,
    pub icon: Option<String>,
    pub terminal: bool,
    pub path: PathBuf,
    pub working_directory: Option<PathBuf>,
    pub categories: Vec<String>,
    pub terminal_arg_exec: Option<String>,
    pub snap_instance_name: Option<String>,
    pub startup_wm_class: Option<String>,
    pub snap_app_name: Option<String>,
    pub snap_common_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AppCatalog {
    apps: Vec<DesktopApp>,
}

impl AppCatalog {
    pub fn load() -> Self {
        Self::load_from_data_dirs(paths::xdg_data_dirs())
    }

    pub fn load_from_data_dirs(data_dirs: Vec<PathBuf>) -> Self {
        let data_dir_count = data_dirs.len();
        let mut apps = Vec::new();
        let mut seen_ids = HashSet::new();

        for data_dir in data_dirs {
            let applications_dir = data_dir.join("applications");
            let Ok(paths) = desktop_entry::desktop_files_under(&applications_dir) else {
                continue;
            };

            for path in paths {
                let Some(id) = desktop_entry::desktop_entry_id(&applications_dir, &path) else {
                    continue;
                };
                if seen_ids.contains(&id) {
                    continue;
                }
                seen_ids.insert(id.clone());

                let Ok(Some(contents)) = desktop_entry::read_desktop_file(&path) else {
                    continue;
                };
                let Some(app) = DesktopApp::from_desktop_entry(id, path, &contents) else {
                    continue;
                };
                apps.push(app);
            }
        }

        apps.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        debug!(
            data_dir_count,
            app_count = apps.len(),
            "loaded desktop app catalog"
        );
        Self { apps }
    }

    pub fn apps(&self) -> &[DesktopApp] {
        &self.apps
    }

    pub fn app_by_id(&self, id: &str) -> Option<&DesktopApp> {
        self.apps.iter().find(|app| app.id == id)
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<DesktopApp> {
        search::search_apps(&self.apps, query, limit)
    }

    pub fn terminal_command_for(&self, command: Vec<String>) -> Result<Vec<String>, String> {
        terminal::terminal_command_for(self, command)
    }

    fn preferred_terminal(&self) -> Option<&DesktopApp> {
        terminal::preferred_terminal(self)
    }
}

impl DesktopApp {
    pub fn launch_argv(&self) -> Result<Vec<String>, String> {
        exec::launch_argv(self)
    }
}

#[cfg(test)]
mod tests;
