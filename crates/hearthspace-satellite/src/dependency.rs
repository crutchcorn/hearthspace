//! Exact local-build provenance required by the stable X11 backing geometry.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use rustix::fs::OFlags;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const REQUIREMENTS: &str = include_str!("../../../data/xwayland-requirements.json");
const BINARY_NAME: &str = "hearthspace-Xwayland";
const RECEIPT_NAME: &str = "hearthspace-Xwayland.json";
const RECEIPT_LIMIT: usize = 64 * 1024;
const PREPARATION: &str = "Prepare the pinned Xwayland dependency as your normal user before starting Hearthspace (see docs/DEPENDENCIES.md), or reinstall the complete Hearthspace release. No system Xwayland fallback is permitted.";

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Requirements {
    format_version: u32,
    version: String,
    source_archive_sha256: String,
    surface_input_targets_sha256: String,
}

#[derive(Deserialize)]
struct Receipt {
    #[serde(flatten)]
    requirements: Requirements,
    sha256: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Dependency {
    pub(crate) binary: PathBuf,
    receipt: PathBuf,
}

impl Dependency {
    pub(crate) fn current() -> io::Result<Self> {
        let executable = std::env::current_exe().map_err(|error| {
            io::Error::other(format!(
                "cannot locate actual Satellite executable: {error}; {PREPARATION}"
            ))
        })?;
        Self::beside(&executable)
    }

    fn beside(executable: &Path) -> io::Result<Self> {
        let parent = executable
            .parent()
            .filter(|_| executable.is_absolute())
            .ok_or_else(|| {
                io::Error::other("actual Satellite executable must have an absolute parent")
            })?;
        Ok(Self {
            binary: parent.join(BINARY_NAME),
            receipt: parent.join(RECEIPT_NAME),
        })
    }

    pub(crate) fn validate(&self) -> io::Result<()> {
        self.validate_files().map_err(|error| self.error(error))
    }

    fn validate_files(&self) -> io::Result<()> {
        let trusted: Requirements = serde_json::from_str(REQUIREMENTS)?;
        let receipt_file = regular_file(&self.receipt, false)?;
        let mut bytes = Vec::new();
        receipt_file
            .take((RECEIPT_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > RECEIPT_LIMIT {
            return Err(io::Error::other("Xwayland receipt exceeds 65536 bytes"));
        }
        let receipt: Receipt = serde_json::from_slice(&bytes)?;
        if receipt.requirements != trusted {
            return Err(io::Error::other(
                "receipt does not match the exact embedded Xwayland requirements",
            ));
        }
        let mut binary = regular_file(&self.binary, true)?;
        let mut digest = Sha256::new();
        let mut bytes = [0; 64 * 1024];
        loop {
            let len = binary.read(&mut bytes)?;
            if len == 0 {
                break;
            }
            digest.update(&bytes[..len]);
        }
        if format!("{:x}", digest.finalize()) != receipt.sha256 {
            return Err(io::Error::other(
                "Xwayland binary SHA-256 does not match its receipt",
            ));
        }
        Ok(())
    }

    pub(super) fn error(&self, reason: impl std::fmt::Display) -> io::Error {
        io::Error::other(format!(
            "Xwayland startup failed: {reason}; binary: {}; receipt: {}; {PREPARATION}",
            self.binary.display(),
            self.receipt.display()
        ))
    }
}

fn regular_file(path: &Path, executable: bool) -> io::Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || (executable && metadata.permissions().mode() & 0o111 == 0) {
        return Err(io::Error::other(format!(
            "{} must be a regular nonsymlink {}file",
            path.display(),
            if executable { "executable " } else { "" }
        )));
    }
    // Reject symlink replacement between inspection and open; NONBLOCK avoids a
    // replaced FIFO hanging startup. Hash the opened regular file, never run it.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file()
        || metadata.dev() != opened.dev()
        || metadata.ino() != opened.ino()
        || (executable && opened.permissions().mode() & 0o111 == 0)
    {
        return Err(io::Error::other(
            "Xwayland companion changed while being opened",
        ));
    }
    Ok(file)
}

#[cfg(test)]
mod tests;
