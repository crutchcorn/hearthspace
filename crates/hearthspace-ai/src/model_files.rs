use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{MODEL_FILE, MODEL_REPOSITORY, model::DevicePreference};

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct SavedModel {
    pub device: DevicePreference,
    pub registered: bool,
}

pub(crate) fn directory() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .context("Cannot locate the Hearthspace model cache")?;
    Ok(base.join("hearthspace/ai"))
}

pub(crate) fn lock(directory: &Path) -> Result<File> {
    fs::create_dir_all(directory)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("model.lock"))?;
    file.try_lock()
        .context("Another D1 operation is in progress; retry when it finishes")?;
    Ok(file)
}

pub(crate) fn saved(directory: &Path) -> Result<SavedModel> {
    match fs::read(directory.join("model.json")) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).context("Invalid D1 model settings")?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SavedModel::default()),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn save(directory: &Path, settings: &SavedModel) -> Result<()> {
    fs::create_dir_all(directory)?;
    let temporary = directory.join("model.json.tmp");
    fs::write(&temporary, serde_json::to_vec(settings)?)?;
    fs::rename(temporary, directory.join("model.json"))?;
    Ok(())
}

pub(crate) fn cached(directory: &Path) -> Option<PathBuf> {
    hf_hub::Cache::new(directory.join("hub"))
        .model(MODEL_REPOSITORY.into())
        .get(MODEL_FILE)
}

pub(crate) fn cache_present(directory: &Path) -> bool {
    directory.join("hub").exists()
}

pub(crate) fn remove_cache(directory: &Path) -> Result<()> {
    match fs::remove_dir_all(directory.join("hub")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
