use std::{fs, io, path::Path};

use tracing::info;

use crate::{
    cli::RunMode,
    session_env::{SessionPaths, repo_root},
};

pub(crate) fn generate_desktop_entries(
    paths: &SessionPaths,
    mode: RunMode,
    binary_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if mode == RunMode::InstalledBinaries {
        let executable = match binary_dir {
            Some(directory) => std::path::absolute(directory)?.join("hearthspace-settings"),
            None => "hearthspace-settings".into(),
        };
        let entry = include_str!("../../../data/applications/dev.hearthspace.Settings.desktop")
            .replace(
                "Exec=hearthspace-settings\n",
                &format!("Exec=\"{}\"\n", escape_exec_path(&executable)?),
            );
        let path = paths
            .applications_dir
            .join("dev.hearthspace.Settings.desktop");
        fs::write(&path, entry)?;
        info!(path = %path.display(), "generated session Settings desktop entry");
        return Ok(());
    }
    let exec = repo_root().join("scripts/dev-run-app");
    let settings_entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Hearthspace Settings\n\
         Comment=Configure Hearthspace\n\
         Exec=\"{}\" hearthspace-settings\n\
         Icon=hearthspace\n\
         Categories=Settings;System;\n\
         Keywords=hearthspace;settings;config;preferences;\n\
         OnlyShowIn=Hearthspace;\n\
         X-Hearthspace-Dev=true\n",
        escape_exec_path(&exec)?
    );
    let settings_path = paths
        .applications_dir
        .join("dev.hearthspace.Settings.desktop");
    fs::write(&settings_path, settings_entry)?;
    info!(path = %settings_path.display(), "generated development desktop entry");

    let storybook_entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Hearthspace UI Storybook\n\
         Comment=Browse and tune Hearthspace UI components\n\
         Exec=\"{}\" hearthspace-ui-storybook\n\
         Icon=hearthspace\n\
         Categories=Development;GUIDesigner;\n\
         Keywords=hearthspace;ui;components;storybook;design;\n\
         OnlyShowIn=Hearthspace;\n\
         X-Hearthspace-Dev=true\n",
        escape_exec_path(&exec)?
    );
    let storybook_path = paths
        .applications_dir
        .join("dev.hearthspace.UIStorybook.desktop");
    fs::write(&storybook_path, storybook_entry)?;
    info!(path = %storybook_path.display(), "generated development desktop entry");
    Ok(())
}

fn escape_exec_path(path: &Path) -> io::Result<String> {
    let path = path
        .to_str()
        .ok_or_else(|| io::Error::other("desktop executable path must be UTF-8"))?;
    if path.chars().any(char::is_control) {
        return Err(io::Error::other(
            "desktop executable path cannot contain control characters",
        ));
    }
    // Exec has two escaping layers: desktop-entry values, then quoted argv.
    // Literal percent signs must also survive desktop-entry field expansion.
    Ok(path
        .replace('%', "%%")
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('\\', "\\\\"))
}

#[cfg(test)]
mod tests;
