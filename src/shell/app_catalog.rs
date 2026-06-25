use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const CURRENT_DESKTOP: &str = "Hearthspace";

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
        Self::load_from_data_dirs(xdg_data_dirs())
    }

    fn load_from_data_dirs(data_dirs: Vec<PathBuf>) -> Self {
        let mut apps = Vec::new();
        let mut seen_ids = HashSet::new();

        for data_dir in data_dirs {
            let applications_dir = data_dir.join("applications");
            let Ok(paths) = desktop_files_under(&applications_dir) else {
                continue;
            };

            for path in paths {
                let Some(id) = desktop_entry_id(&applications_dir, &path) else {
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
        Self { apps }
    }

    pub fn apps(&self) -> &[DesktopApp] {
        &self.apps
    }

    pub fn app_by_id(&self, id: &str) -> Option<&DesktopApp> {
        self.apps.iter().find(|app| app.id == id)
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<DesktopApp> {
        let query_tokens = tokenize_search(query);
        if query_tokens.is_empty() {
            return self.apps.iter().take(limit).cloned().collect();
        }

        let mut scored = self
            .apps
            .iter()
            .filter_map(|app| search_score(app, &query_tokens).map(|score| (score, app)))
            .collect::<Vec<_>>();
        scored.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then(left.name.cmp(&right.name))
                .then(left.id.cmp(&right.id))
        });

        scored
            .into_iter()
            .take(limit)
            .map(|(_, app)| app.clone())
            .collect()
    }

    pub fn terminal_command_for(&self, command: Vec<String>) -> Result<Vec<String>, String> {
        if command.is_empty() {
            return Err("cannot wrap an empty terminal command".to_string());
        }

        if executable_in_path("xdg-terminal-exec") {
            let mut terminal_command = vec!["xdg-terminal-exec".to_string()];
            terminal_command.extend(command);
            return Ok(terminal_command);
        }

        let terminal = self
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

    fn preferred_terminal(&self) -> Option<&DesktopApp> {
        for id in terminal_preference_ids() {
            if let Some(app) = self.app_by_id(&id).filter(|app| {
                app.categories
                    .iter()
                    .any(|category| category == "TerminalEmulator")
            }) {
                return Some(app);
            }
        }

        self.apps.iter().find(|app| {
            app.categories
                .iter()
                .any(|category| category == "TerminalEmulator")
        })
    }
}

impl DesktopApp {
    fn from_desktop_entry(id: String, path: PathBuf, contents: &str) -> Option<Self> {
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

    pub fn launch_argv(&self) -> Result<Vec<String>, String> {
        let argv = parse_exec_argv(self)?;
        if argv.is_empty() {
            Err(format!("desktop entry {} produced no command", self.id))
        } else {
            Ok(argv)
        }
    }
}

fn xdg_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(xdg_data_home) = env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(xdg_data_home));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }

    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    dirs.extend(data_dirs);
    dirs
}

fn desktop_files_under(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
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

fn desktop_entry_id(base: &Path, path: &Path) -> Option<String> {
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

fn unescape_desktop_value(value: &str) -> String {
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

fn parse_semicolon_list(value: &str) -> Vec<String> {
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
        .any(|desktop| desktop == CURRENT_DESKTOP)
}

fn not_show_in_allows(value: Option<&String>) -> bool {
    let Some(value) = value else {
        return true;
    };

    !parse_semicolon_list(value)
        .iter()
        .any(|desktop| desktop == CURRENT_DESKTOP)
}

fn try_exec_resolves(try_exec: &str) -> bool {
    let path = Path::new(try_exec);
    if path.is_absolute() || try_exec.contains('/') {
        path.exists()
    } else {
        executable_in_path(try_exec)
    }
}

fn executable_in_path(name: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|path| {
            let candidate = path.join(name);
            candidate.exists() && !candidate.is_dir()
        })
    })
}

fn tokenize_search(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|token| !token.is_empty())
        .collect()
}

fn search_score(app: &DesktopApp, query_tokens: &[String]) -> Option<u32> {
    let mut total = 0;
    for token in query_tokens {
        let score = token_score(app, token)?;
        total += score;
    }
    Some(total)
}

fn token_score(app: &DesktopApp, token: &str) -> Option<u32> {
    let name = app.name.to_lowercase();
    if name == token {
        return Some(1000);
    }
    if name.starts_with(token) {
        return Some(900);
    }
    if name.split_whitespace().any(|word| word.starts_with(token)) {
        return Some(800);
    }
    if app
        .keywords
        .iter()
        .any(|keyword| keyword.to_lowercase().contains(token))
    {
        return Some(700);
    }
    if name.contains(token) {
        return Some(600);
    }
    if app
        .generic_name
        .as_ref()
        .is_some_and(|value| value.to_lowercase().contains(token))
        || app
            .comment
            .as_ref()
            .is_some_and(|value| value.to_lowercase().contains(token))
    {
        return Some(500);
    }
    if app.id.to_lowercase().contains(token)
        || app.exec.to_lowercase().contains(token)
        || app
            .categories
            .iter()
            .any(|category| category.to_lowercase().contains(token))
    {
        return Some(400);
    }
    None
}

fn parse_exec_argv(app: &DesktopApp) -> Result<Vec<String>, String> {
    let args = split_exec(&app.exec)?;
    let mut expanded = Vec::new();
    for arg in args {
        if let Some(arg) = expand_exec_field_codes(app, &arg)? {
            expanded.push(arg);
        }
    }
    Ok(expanded)
}

fn split_exec(exec: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = exec.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '\\' => match chars.next() {
                Some(next) => current.push(next),
                None => current.push('\\'),
            },
            ch if ch.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if in_quotes {
        return Err(format!("unterminated quote in Exec={exec}"));
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

fn expand_exec_field_codes(app: &DesktopApp, arg: &str) -> Result<Option<String>, String> {
    let mut output = String::new();
    let mut chars = arg.chars();

    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }

        let Some(code) = chars.next() else {
            return Err(format!("dangling field code in {}", app.id));
        };
        match code {
            'f' | 'F' | 'u' | 'U' | 'i' => return Ok(None),
            'c' => output.push_str(&app.name),
            'k' => output.push_str(&app.path.to_string_lossy()),
            '%' => output.push('%'),
            other => return Err(format!("unsupported field code %{other} in {}", app.id)),
        }
    }

    Ok((!output.is_empty()).then_some(output))
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

fn xdg_config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME") {
        dirs.push(PathBuf::from(xdg_config_home));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".config"));
    }

    let config_dirs = env::var_os("XDG_CONFIG_DIRS")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")]);
    dirs.extend(config_dirs);
    dirs
}

pub fn spawn_argv(argv: &[String]) -> Result<(), String> {
    spawn_argv_with_wayland_display(argv, crate::config::WAYLAND_DISPLAY_NAME)
}

pub fn spawn_argv_with_wayland_display(
    argv: &[String],
    wayland_display: &str,
) -> Result<(), String> {
    spawn_argv_with_env(argv, wayland_display, &[])
}

pub fn spawn_argv_with_env(
    argv: &[String],
    wayland_display: &str,
    envs: &[(&str, &str)],
) -> Result<(), String> {
    let Some((program, args)) = argv.split_first() else {
        return Err("cannot spawn an empty command".to_string());
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .env("WAYLAND_DISPLAY", wayland_display)
        .env_remove("DISPLAY");
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to spawn {program}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

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
}
