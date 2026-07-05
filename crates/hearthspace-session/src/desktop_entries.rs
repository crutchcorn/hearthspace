use std::{fs, path::Path};

use tracing::info;

use crate::session_env::{SessionPaths, repo_root};

pub(crate) fn generate_dev_desktop_entries(
    paths: &SessionPaths,
) -> Result<(), Box<dyn std::error::Error>> {
    let exec = repo_root().join("scripts/dev-run-app");
    let desktop_entry = format!(
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
        escape_exec_path(&exec)
    );
    let path = paths
        .applications_dir
        .join("dev.hearthspace.Settings.desktop");
    fs::write(&path, desktop_entry)?;
    info!(path = %path.display(), "generated development desktop entry");
    Ok(())
}

fn escape_exec_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
