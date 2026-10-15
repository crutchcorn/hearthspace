use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

const SCHEMA: &str = "org.gnome.desktop.wm.preferences";
const SCHEMA_FILE: &str = "org.gnome.desktop.wm.preferences.gschema.xml";
const ENUM_FILE: &str = "org.gnome.desktop.enums.xml";
const MAX_SOURCE_BYTES: u64 = 1024 * 1024;

pub(super) fn prepare(runtime: &Path, data_dirs: &str) -> io::Result<()> {
    let target = runtime.join("share/glib-2.0/schemas");
    let source = env::split_paths(data_dirs)
        .map(|directory| directory.join("glib-2.0/schemas"))
        .find(|directory| directory != &target && directory.join(SCHEMA_FILE).is_file())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "optional GNOME window settings schema is not installed",
            )
        })?;
    fs::create_dir_all(&target)?;
    // Copy only this installed schema and its enum definitions, not the rest
    // of the desktop schema catalog. Preserve all its keys and translations.
    for name in [SCHEMA_FILE, ENUM_FILE] {
        fs::write(target.join(name), read_source(&source.join(name))?)?;
    }
    let mut overrides: Vec<PathBuf> = fs::read_dir(&source)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "override")
        })
        .collect();
    overrides.sort();
    if overrides.len() > 256 {
        return Err(io::Error::other("too many GNOME schema override files"));
    }
    for (index, path) in overrides.iter().enumerate() {
        let contents = read_source(path)?;
        let retained = window_overrides(&contents);
        if !retained.is_empty() {
            fs::write(
                target.join(format!("{index:03}.gschema.override")),
                retained,
            )?;
        }
    }
    fs::write(
        target.join("999-hearthspace.gschema.override"),
        format!(
            "[{SCHEMA}:{}]\nbutton-layout=':maximize,close'\n",
            hearthspace_common::CURRENT_DESKTOP
        ),
    )?;
    let result = crate::bounded_command::output(
        Command::new("glib-compile-schemas")
            .arg("--strict")
            .arg(&target),
        Instant::now() + Duration::from_secs(2),
        &AtomicBool::new(false),
    )?;
    if !result.status.success() {
        return Err(io::Error::other(format!(
            "optional GTK schema compilation failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )));
    }
    Ok(())
}

fn read_source(path: &Path) -> io::Result<String> {
    if fs::metadata(path)?.len() > MAX_SOURCE_BYTES {
        return Err(io::Error::other("GNOME schema source exceeds 1 MiB"));
    }
    fs::read_to_string(path)
}

fn window_overrides(contents: &str) -> String {
    let mut selected = false;
    let mut output = String::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(section) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            selected = section == SCHEMA
                || section
                    .strip_prefix(SCHEMA)
                    .is_some_and(|suffix| suffix.starts_with(':'));
        }
        if selected {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrelated_schema_sections_are_not_copied() {
        let source = "[unrelated]\nkey=1\n[org.gnome.desktop.wm.preferences]\nfocus-mode='sloppy'\n[org.gnome.desktop.wm.preferences:other]\nbutton-layout='close:'\n[org.gnome.desktop.wm.preferences.extra]\nkey=2\n";
        assert_eq!(
            window_overrides(source),
            "[org.gnome.desktop.wm.preferences]\nfocus-mode='sloppy'\n[org.gnome.desktop.wm.preferences:other]\nbutton-layout='close:'\n"
        );
    }
}
