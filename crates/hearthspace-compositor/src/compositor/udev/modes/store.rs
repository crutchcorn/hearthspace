use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};

use super::super::device::KmsOutputTarget;

#[derive(Default)]
pub(in crate::compositor::udev) struct State {
    pub selected: BTreeMap<String, String>,
    pub pending: Option<super::Pending>,
    pub serial: u64,
    pub message: Option<String>,
    path: Option<PathBuf>,
}

impl State {
    pub fn load() -> Self {
        let path = hearthspace_common::config_path()
            .ok()
            .map(|p| p.with_file_name("display-modes.json"));
        Self::load_path(path)
    }

    pub(super) fn load_path(path: Option<PathBuf>) -> Self {
        let selected = path
            .as_ref()
            .and_then(|p| fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            selected,
            path,
            serial: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            ..Self::default()
        }
    }

    pub fn select(&self, targets: &mut [KmsOutputTarget]) {
        for target in targets {
            if let Some(id) = self.selected.get(target.connector_name())
                && let Some(mode) = target
                    .modes
                    .iter()
                    .find(|m| KmsOutputTarget::describe_mode(**m).id == *id)
            {
                target.mode = *mode;
            }
        }
    }

    pub(super) fn confirm(&mut self, token: u64, now: std::time::Instant) -> Result<(), String> {
        if self
            .pending
            .as_ref()
            .is_none_or(|p| !p.can_keep(token, now))
        {
            return Err("The display change is not ready to keep or has expired.".into());
        }
        self.save()
            .map_err(|e| format!("Could not save display mode: {e}"))?;
        self.pending = None;
        self.message = Some("Display mode saved.".into());
        Ok(())
    }

    fn save(&self) -> io::Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| io::Error::other("display configuration path unavailable"))?;
        fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let bytes = serde_json::to_vec_pretty(&self.selected)?;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}
