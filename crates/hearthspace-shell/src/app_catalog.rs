use std::{collections::HashSet, fs, path::PathBuf};

use tracing::debug;

mod desktop_entry;
mod exec;
mod paths;
mod search;
mod spawn;
mod terminal;

pub use spawn::{
    reap_child, spawn_argv, spawn_argv_with_env, spawn_argv_with_options,
    spawn_argv_with_wayland_display,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub keywords: Vec<String>,
    pub exec: String,
    pub icon: Option<String>,
    pub terminal: bool,
    pub path: PathBuf,
    pub working_directory: Option<PathBuf>,
    pub categories: Vec<String>,
    pub terminal_arg_exec: Option<String>,
    pub snap_instance_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AppCatalog {
    apps: Vec<DesktopApp>,
}

impl AppCatalog {
    pub fn load() -> Self {
        Self::load_from_data_dirs(paths::xdg_data_dirs())
    }

    fn load_from_data_dirs(data_dirs: Vec<PathBuf>) -> Self {
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

                let Ok(contents) = fs::read_to_string(&path) else {
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
mod tests {
    use super::*;
    use super::{
        desktop_entry::{desktop_entry_id, parse_semicolon_list, unescape_desktop_value},
        exec::split_exec,
        paths::source_tree_data_dir,
        search::token_score,
        spawn::session_environment_keys,
    };
    use rstest::rstest;
    use std::{fs, path::Path};

    fn app_with(name: &str, contents: &str) -> DesktopApp {
        DesktopApp::from_desktop_entry(
            format!("{name}.desktop"),
            PathBuf::from(format!("{name}.desktop")),
            contents,
        )
        .unwrap()
    }

    #[test]
    fn hidden_and_no_display_apps_are_filtered() {
        assert!(
            DesktopApp::from_desktop_entry(
                "hidden.desktop".to_string(),
                PathBuf::from("hidden.desktop"),
                "[Desktop Entry]\nType=Application\nName=Hidden\nExec=hidden\nHidden=true\n"
            )
            .is_none()
        );
        assert!(
            DesktopApp::from_desktop_entry(
                "nodisplay.desktop".to_string(),
                PathBuf::from("nodisplay.desktop"),
                "[Desktop Entry]\nType=Application\nName=Hidden\nExec=hidden\nNoDisplay=true\n"
            )
            .is_none()
        );
    }

    #[test]
    fn only_show_in_must_include_hearthspace() {
        assert!(
            DesktopApp::from_desktop_entry(
                "gnome.desktop".to_string(),
                PathBuf::from("gnome.desktop"),
                "[Desktop Entry]\nType=Application\nName=GNOME\nExec=gnome\nOnlyShowIn=GNOME;\n"
            )
            .is_none()
        );
        assert!(DesktopApp::from_desktop_entry(
            "hearthspace.desktop".to_string(),
            PathBuf::from("hearthspace.desktop"),
            "[Desktop Entry]\nType=Application\nName=Hearthspace\nExec=app\nOnlyShowIn=Hearthspace;\n"
        )
        .is_some());
    }

    #[test]
    fn search_matches_partial_tokens() {
        let app = DesktopApp::from_desktop_entry(
            "org.gnome.Calculator.desktop".to_string(),
            PathBuf::from("org.gnome.Calculator.desktop"),
            "[Desktop Entry]\nType=Application\nName=Calculator\nExec=gnome-calculator\nKeywords=calculation;arithmetic;\n",
        )
        .unwrap();
        let catalog = AppCatalog { apps: vec![app] };
        assert_eq!(catalog.search("calc", 10)[0].name, "Calculator");
        assert_eq!(catalog.search("arith", 10)[0].name, "Calculator");
    }

    #[test]
    fn search_with_no_matches_still_returns_apps_for_launcher_dropdown() {
        let catalog = AppCatalog {
            apps: vec![
                app_with(
                    "calc",
                    "[Desktop Entry]\nType=Application\nName=Calculator\nExec=gnome-calculator\n",
                ),
                app_with(
                    "files",
                    "[Desktop Entry]\nType=Application\nName=Files\nExec=nautilus\n",
                ),
            ],
        };

        let results = catalog.search("does-not-match", 10);

        assert_eq!(
            results
                .iter()
                .map(|app| app.name.as_str())
                .collect::<Vec<_>>(),
            ["Calculator", "Files"]
        );
    }

    #[test]
    fn exec_field_codes_are_expanded_or_removed() {
        let app = DesktopApp::from_desktop_entry(
            "code.desktop".to_string(),
            PathBuf::from("/usr/share/applications/code.desktop"),
            "[Desktop Entry]\nType=Application\nName=Code\nExec=code --name %c --desktop %k %F %%\n",
        )
        .unwrap();
        assert_eq!(
            app.launch_argv().unwrap(),
            vec![
                "code".to_string(),
                "--name".to_string(),
                "Code".to_string(),
                "--desktop".to_string(),
                "/usr/share/applications/code.desktop".to_string(),
                "%".to_string(),
            ]
        );
    }

    #[rstest]
    #[case("foo bar", &["foo", "bar"])]
    #[case("  spaced   out  ", &["spaced", "out"])]
    #[case("\"foo bar\" baz", &["foo bar", "baz"])]
    #[case("foo\\ bar", &["foo bar"])]
    #[case("", &[])]
    fn split_exec_handles_quotes_and_escapes(#[case] input: &str, #[case] expected: &[&str]) {
        assert_eq!(split_exec(input).unwrap(), expected);
    }

    #[test]
    fn split_exec_rejects_unterminated_quotes() {
        assert!(split_exec("\"never closed").is_err());
    }

    #[rstest]
    #[case("a\\sb", "a b")]
    #[case("line\\nbreak", "line\nbreak")]
    #[case("tab\\there", "tab\there")]
    #[case("back\\\\slash", "back\\slash")]
    #[case("trailing\\", "trailing\\")]
    #[case("plain", "plain")]
    fn unescape_desktop_value_expands_known_escapes(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(unescape_desktop_value(input), expected);
    }

    #[test]
    fn parse_semicolon_list_drops_empty_entries() {
        assert_eq!(
            parse_semicolon_list("a;b;;c;"),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert!(parse_semicolon_list("").is_empty());
    }

    #[test]
    fn desktop_entry_id_joins_nested_components() {
        let base = Path::new("/apps");
        assert_eq!(
            desktop_entry_id(base, Path::new("/apps/org/gnome/Foo.desktop")).as_deref(),
            Some("org-gnome-Foo.desktop")
        );
        assert_eq!(
            desktop_entry_id(base, Path::new("/apps/Foo.desktop")).as_deref(),
            Some("Foo.desktop")
        );
        assert_eq!(
            desktop_entry_id(base, Path::new("/other/Foo.desktop")),
            None
        );
    }

    #[test]
    fn token_score_ranks_more_specific_matches_higher() {
        let app = app_with(
            "calc",
            "[Desktop Entry]\nType=Application\nName=Calculator\nExec=gnome-calculator\nKeywords=arithmetic;\nComment=Do sums\n",
        );

        // Exact name beats prefix beats keyword beats substring.
        assert_eq!(token_score(&app, "calculator"), Some(1000));
        assert_eq!(token_score(&app, "calc"), Some(900));
        assert_eq!(token_score(&app, "arith"), Some(700));
        assert_eq!(token_score(&app, "lcul"), Some(600));
        assert_eq!(token_score(&app, "sums"), Some(500));
        assert_eq!(token_score(&app, "missing"), None);
    }

    #[test]
    fn token_score_matches_prefix_of_a_later_word() {
        let app = app_with(
            "files",
            "[Desktop Entry]\nType=Application\nName=GNOME Files\nExec=nautilus\n",
        );
        assert_eq!(token_score(&app, "fil"), Some(800));
    }

    #[test]
    fn load_from_data_dirs_filters_and_dedups() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();

        let write = |dir: &Path, name: &str, contents: &str| {
            let apps_dir = dir.join("applications");
            fs::create_dir_all(&apps_dir).unwrap();
            fs::write(apps_dir.join(name), contents).unwrap();
        };
        write(
            first.path(),
            "editor.desktop",
            "[Desktop Entry]\nType=Application\nName=Editor (first)\nExec=editor\n",
        );
        write(
            first.path(),
            "hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=hidden\nNoDisplay=true\n",
        );
        // Same id in a lower-priority dir must not override the first one.
        write(
            second.path(),
            "editor.desktop",
            "[Desktop Entry]\nType=Application\nName=Editor (second)\nExec=editor\n",
        );
        write(
            second.path(),
            "viewer.desktop",
            "[Desktop Entry]\nType=Application\nName=Viewer\nExec=viewer\n",
        );

        let catalog = AppCatalog::load_from_data_dirs(vec![
            first.path().to_path_buf(),
            second.path().to_path_buf(),
        ]);

        let names: Vec<&str> = catalog.apps().iter().map(|app| app.name.as_str()).collect();
        assert_eq!(names, vec!["Editor (first)", "Viewer"]);
        assert!(catalog.app_by_id("hidden.desktop").is_none());
        assert_eq!(
            catalog
                .app_by_id("editor.desktop")
                .map(|app| app.name.as_str()),
            Some("Editor (first)")
        );
    }

    #[test]
    fn source_tree_settings_desktop_file_is_searchable_and_launchable() {
        let catalog = AppCatalog::load_from_data_dirs(vec![source_tree_data_dir()]);

        let app = catalog
            .app_by_id("dev.hearthspace.Settings.desktop")
            .unwrap();
        assert_eq!(app.name, "Hearthspace Settings");
        assert_eq!(catalog.search("settings", 1)[0].id, app.id);
        assert_eq!(app.launch_argv().unwrap(), vec!["hearthspace-settings"]);
    }

    #[test]
    fn session_settings_desktop_file_is_searchable_and_launchable() {
        let dir = tempfile::tempdir().unwrap();
        let apps_dir = dir.path().join("share/applications");
        fs::create_dir_all(&apps_dir).unwrap();
        fs::write(
            apps_dir.join("dev.hearthspace.Settings.desktop"),
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Hearthspace Settings\n\
             Comment=Configure Hearthspace\n\
             Exec=\"/repo/scripts/dev-run-app\" hearthspace-settings\n\
             Categories=Settings;System;\n\
             Keywords=hearthspace;settings;config;preferences;\n\
             OnlyShowIn=Hearthspace;\n\
             X-Hearthspace-Dev=true\n",
        )
        .unwrap();

        let catalog = AppCatalog::load_from_data_dirs(vec![dir.path().join("share")]);

        let app = catalog
            .app_by_id("dev.hearthspace.Settings.desktop")
            .unwrap();
        assert_eq!(app.name, "Hearthspace Settings");
        assert_eq!(catalog.search("hearthspace", 1)[0].id, app.id);
        assert_eq!(catalog.search("preferences", 1)[0].id, app.id);
        assert_eq!(
            app.launch_argv().unwrap(),
            vec!["/repo/scripts/dev-run-app", "hearthspace-settings"]
        );
    }

    #[test]
    fn session_settings_entry_overrides_source_tree_entry() {
        let session = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let write = |dir: &Path, exec: &str| {
            let apps_dir = dir.join("applications");
            fs::create_dir_all(&apps_dir).unwrap();
            fs::write(
                apps_dir.join("dev.hearthspace.Settings.desktop"),
                format!(
                    "[Desktop Entry]\n\
                     Type=Application\n\
                     Name=Hearthspace Settings\n\
                     Exec={exec}\n\
                     Categories=Settings;\n"
                ),
            )
            .unwrap();
        };
        write(
            session.path(),
            "\"/repo/scripts/dev-run-app\" hearthspace-settings",
        );
        write(source.path(), "hearthspace-settings");

        let catalog = AppCatalog::load_from_data_dirs(vec![
            session.path().to_path_buf(),
            source.path().to_path_buf(),
        ]);

        assert_eq!(
            catalog.search("settings", 1)[0].launch_argv().unwrap(),
            vec!["/repo/scripts/dev-run-app", "hearthspace-settings"]
        );
    }

    #[test]
    fn launched_apps_receive_compositor_command_socket_env() {
        assert!(session_environment_keys().contains(&hearthspace_common::SHELL_COMMAND_SOCKET_ENV));
    }
}
